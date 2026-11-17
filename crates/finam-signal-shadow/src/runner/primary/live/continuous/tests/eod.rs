//! Synthetic compressed-day local peers, not a 17-hour/provider/cgroup soak.
use super::*;
use crate::primary::{
    calendar::tests::{at, Dir, Fixture},
    verify::{self, EvidenceBinding},
};
mod r1;
mod tail;
mod ws_historical;
#[derive(Clone, Copy, PartialEq, Eq)]
enum R1Scenario {
    None,
    StaleReference,
    RepeatedWitness,
    RestTail,
    WsHistorical,
    RestThenWs,
    RestThenRest,
    WsThenWs,
}
impl R1Scenario {
    // Synthetic source competition at two different labels, same generation.
    fn historical_at(self, close_minute: i64) -> Option<bool> {
        match (self, close_minute) {
            (Self::WsHistorical, 30)
            | (Self::RestThenWs | Self::WsThenWs, 40)
            | (Self::WsThenWs, 30) => Some(false), // WS winner
            (Self::RestThenWs | Self::RestThenRest, 30) | (Self::RestThenRest, 40) => Some(true),
            _ => None,
        }
    }
}

#[derive(Clone)]
struct Artifact {
    config: crate::primary::WsPrimaryConfig,
    journal: Vec<u8>,
    completion: Vec<u8>,
    calendar_sha: String,
    now: i64,
}
impl Artifact {
    fn binding(&self) -> EvidenceBinding {
        EvidenceBinding {
            schema_version: 1,
            config_sha256: self.config.fingerprint().unwrap(),
            calendar_sha256: self.calendar_sha.clone(),
            journal_sha256: crate::seed::digest(&self.journal),
            completion_sha256: crate::seed::digest(&self.completion),
        }
    }
    fn verify(&self) -> Result<verify::VerifiedEod, verify::VerifyError> {
        verify::verify(
            &self.config,
            &self.binding(),
            &self.journal,
            &self.completion,
            self.now,
        )
    }
    fn change_journal(&mut self, change: impl FnOnce(&mut Vec<Value>)) {
        let mut rows: Vec<Value> = std::str::from_utf8(&self.journal)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        change(&mut rows);
        self.journal = rows
            .iter()
            .flat_map(|r| {
                let mut b = serde_json::to_vec(r).unwrap();
                b.push(b'\n');
                b
            })
            .collect();
        let mut completion: Value = serde_json::from_slice(&self.completion).unwrap();
        completion["journal"]["sha256"] = json!(crate::seed::digest(&self.journal));
        completion["journal"]["bytes"] = json!(self.journal.len());
        completion["journal"]["last_sequence"] = rows.last().unwrap()["sequence"].clone();
        self.completion = serde_json::to_vec(&completion).unwrap();
    }
}
async fn day(
    f: Fixture,
    audit_failure: Option<RestFailure>,
    early_stop: bool,
) -> (RunReport, Artifact) {
    day_with_recovery(f, audit_failure, early_stop, None, false).await
}
async fn day_with_recovery(
    f: Fixture,
    audit_failure: Option<RestFailure>,
    early_stop: bool,
    fault_minute: Option<i64>,
    after_data: bool,
) -> (RunReport, Artifact) {
    day_with_proof_failure(f, audit_failure, early_stop, fault_minute, after_data, None).await
}
async fn day_with_proof_failure(
    f: Fixture,
    audit_failure: Option<RestFailure>,
    early_stop: bool,
    fault_minute: Option<i64>,
    after_data: bool,
    proof_failure: Option<(RestFailure, bool)>,
) -> (RunReport, Artifact) {
    day_scenario(
        f,
        audit_failure,
        early_stop,
        fault_minute,
        after_data,
        proof_failure,
        R1Scenario::None,
    )
    .await
}
async fn day_scenario(
    mut f: Fixture,
    audit_failure: Option<RestFailure>,
    early_stop: bool,
    fault_minute: Option<i64>,
    after_data: bool,
    proof_failure: Option<(RestFailure, bool)>,
    scenario: R1Scenario,
) -> (RunReport, Artifact) {
    let startup_recovery = fault_minute == Some(0);
    let d = Dir::new();
    f.config.base.previous_segment_id = None;
    let output = d.0.join("output");
    let path = output.join("journal.jsonl");
    let f = Arc::new(f);
    let prepared = crate::primary::entry::tests::fresh(&f);
    let root = StopHandle::default();
    let crate::primary::entry::Opened {
        mut owner,
        calendar,
        artifacts,
        config: _,
    } = prepared.open(&output, root.clone()).unwrap();
    let reference = at(calendar.reference_start_ms());
    let calendar_sha = calendar.sha256().to_owned();
    owner.bind_calendar(calendar).unwrap();
    let (factory, mut peers, mut progress, _) = setup().await;
    let source_f = f.clone();
    let proof_release = Arc::new(tokio::sync::Notify::new());
    let fetch_release = proof_release.clone();
    let fetch = move |job: Job, _stop: StopHandle| {
        let f = source_f.clone();
        let release = fetch_release.clone();
        async move {
            if scenario == R1Scenario::StaleReference
                && matches!(&job, Job::Bootstrap(_) | Job::Recovery(_))
            {
                let bootstrap = matches!(&job, Job::Bootstrap(_));
                if bootstrap {
                    release.notified().await;
                }
                let mut row = f.row(f.open() - 120000);
                if !bootstrap {
                    row["close"] = json!({"value":"2200"});
                }
                return source(&job, vec![row]);
            }
            if let Some((failure, _)) = proof_failure {
                if matches!(&job, Job::Proof(t) if t.label_ms() == f.open() && t.reason() != Some(ExceptionReason::SessionEnd))
                {
                    release.notified().await;
                    return HttpOutcome {
                        result: Err(failure),
                        receipt: HttpReceipt {
                            started_at_ms: crate::timing::now_ms(),
                            finished_at_ms: crate::timing::now_ms(),
                            deadline_ms: job.deadline_ms(),
                            get_attempts: 1,
                            auth_generation: Some(1),
                            body: None,
                            executor_completed: true,
                        },
                    };
                }
            }
            if matches!(job, Job::Audit(_)) {
                HttpOutcome {
                    result: match audit_failure {
                        Some(failure) => Err(failure),
                        None => {
                            let plan = job.plan().unwrap();
                            let original = f.rest(&plan);
                            if f.config.primary.observed_bo() {
                                // Test-only provider revision. Immutable canonical
                                // source/model hashes must stay as originally used.
                                let mut e = original.evidence().clone();
                                let p = &mut e.parts[0];
                                let mut raw: Value = serde_json::from_str(&p.raw_body).unwrap();
                                raw["bars"][9]["volume"] = json!({"value":"1548"});
                                if f.config.primary.rest_nonblocking() {
                                    if (plan.start().timestamp_millis() - f.open()) / 600000 % 12
                                        == 5
                                    {
                                        raw["bars"][9]["low"] = json!({"value":"2199.5"});
                                        raw["bars"][9]["close"] = json!({"value":"2200"});
                                    } else {
                                        raw["bars"].as_array_mut().unwrap().remove(9);
                                    }
                                }
                                p.raw_body = raw.to_string();
                                p.declared_body_bytes = Some(p.raw_body.len() as u64);
                                p.response_sha256 = crate::seed::digest(p.raw_body.as_bytes());
                                Ok(broker_finam::sparse_m10::AdmittedClosedM1Snapshot::admit(
                                    e,
                                    &plan,
                                    at(crate::timing::now_ms()),
                                )
                                .unwrap())
                            } else {
                                Ok(original)
                            }
                        }
                    },
                    receipt: HttpReceipt {
                        started_at_ms: crate::timing::now_ms(),
                        finished_at_ms: crate::timing::now_ms(),
                        deadline_ms: job.deadline_ms(),
                        get_attempts: u64::from(audit_failure.is_none()),
                        auth_generation: Some(1),
                        body: None,
                        executor_completed: true,
                    },
                }
            } else {
                let historical_winner = match &job {
                    Job::Recovery(p) => scenario
                        .historical_at((p.request().end().timestamp_millis() - f.open()) / 60000),
                    Job::Proof(t) => {
                        scenario.historical_at((t.label_ms() + 600000 - f.open()) / 60000)
                    }
                    _ => None,
                };
                if let Some(rest_winner) = historical_winner {
                    let plan = job.plan().unwrap();
                    let mut evidence = f.rest(&plan).evidence().clone();
                    let part = &mut evidence.parts[0];
                    let mut raw: Value = serde_json::from_str(&part.raw_body).unwrap();
                    let n = raw["bars"].as_array().unwrap().len();
                    if !rest_winner || matches!(&job, Job::Proof(_)) {
                        raw["bars"].as_array_mut().unwrap().truncate(n - 8);
                    }
                    part.raw_body = raw.to_string();
                    part.declared_body_bytes = Some(part.raw_body.len() as u64);
                    part.response_sha256 = crate::seed::digest(part.raw_body.as_bytes());
                    let admitted = broker_finam::sparse_m10::AdmittedClosedM1Snapshot::admit(
                        evidence,
                        &plan,
                        at(crate::timing::now_ms()),
                    )
                    .unwrap();
                    return HttpOutcome {
                        result: Ok(admitted),
                        receipt: HttpReceipt {
                            started_at_ms: crate::timing::now_ms(),
                            finished_at_ms: crate::timing::now_ms(),
                            deadline_ms: job.deadline_ms(),
                            get_attempts: 1,
                            auth_generation: Some(1),
                            body: None,
                            executor_completed: true,
                        },
                    };
                }
                assert!(
                    matches!(&job,Job::Proof(t) if t.reason()==Some(ExceptionReason::SessionEnd))
                        || (f.config.primary.admitted_rest_source()
                            && matches!(&job,Job::Proof(t) if t.reason()==Some(ExceptionReason::BoundaryUnproven)))
                        || (f.config.primary.consistent_sources()
                            && matches!(&job,Job::Proof(t) if matches!(t.reason(),Some(ExceptionReason::WsGracePending|ExceptionReason::ProvisionalRevision))))
                        || (fault_minute.is_some()
                            && matches!(&job,Job::Recovery(p) if p.startup_anchor()==startup_recovery))
                        || ((fault_minute.is_some() || scenario == R1Scenario::StaleReference)
                            && matches!(&job,Job::Proof(t) if t.reason()==Some(ExceptionReason::StartupPartial))),
                    "no regular GETs on this dense fixture"
                );
                let plan = job.plan().unwrap();
                let mut admitted = f.rest(&plan);
                if f.config.primary.consistent_sources()
                    && matches!(&job,Job::Recovery(p) if !p.startup_anchor())
                {
                    let mut evidence = admitted.evidence().clone();
                    let part = &mut evidence.parts[0];
                    let mut raw: Value = serde_json::from_str(&part.raw_body).unwrap();
                    if scenario == R1Scenario::RepeatedWitness {
                        raw["bars"][0]["high"] = json!({"value":"2200.5"});
                    } else {
                        raw["bars"][0]["close"] = json!({"value":"2200"});
                        let n = raw["bars"].as_array().unwrap().len();
                        raw["bars"][n - 1]["close"] = json!({"value":"2200"});
                        raw["bars"][n - 1]["volume"] = json!({"value":"1"});
                    }
                    part.raw_body = raw.to_string();
                    part.declared_body_bytes = Some(part.raw_body.len() as u64);
                    part.response_sha256 = crate::seed::digest(part.raw_body.as_bytes());
                    admitted = broker_finam::sparse_m10::AdmittedClosedM1Snapshot::admit(
                        evidence,
                        &plan,
                        at(crate::timing::now_ms()),
                    )
                    .unwrap();
                }
                if f.config.primary.admitted_rest_source()
                    && matches!(&job,Job::Proof(t) if t.reason()==Some(ExceptionReason::BoundaryUnproven))
                {
                    // Synthetic same failure shape: close changes and volume
                    // decreases, before the unpublished non-startup callback.
                    let mut evidence = admitted.evidence().clone();
                    let part = &mut evidence.parts[0];
                    let mut raw: Value = serde_json::from_str(&part.raw_body).unwrap();
                    raw["bars"][9]["close"] = json!({"value":"2200"});
                    raw["bars"][9]["volume"] = json!({"value":"1"});
                    part.raw_body = raw.to_string();
                    part.declared_body_bytes = Some(part.raw_body.len() as u64);
                    part.response_sha256 = crate::seed::digest(part.raw_body.as_bytes());
                    admitted = broker_finam::sparse_m10::AdmittedClosedM1Snapshot::admit(
                        evidence,
                        &plan,
                        at(crate::timing::now_ms()),
                    )
                    .unwrap();
                }
                if scenario == R1Scenario::RestTail
                    && matches!(&job,Job::Recovery(p) if p.scope().is_some_and(|s|s.ordinal==1))
                {
                    let mut evidence = admitted.evidence().clone();
                    let part = &mut evidence.parts[0];
                    let mut raw: Value = serde_json::from_str(&part.raw_body).unwrap();
                    let n = raw["bars"].as_array().unwrap().len();
                    raw["bars"].as_array_mut().unwrap().truncate(n - 3);
                    part.raw_body = raw.to_string();
                    part.declared_body_bytes = Some(part.raw_body.len() as u64);
                    part.response_sha256 = crate::seed::digest(part.raw_body.as_bytes());
                    admitted = broker_finam::sparse_m10::AdmittedClosedM1Snapshot::admit(
                        evidence,
                        &plan,
                        at(crate::timing::now_ms()),
                    )
                    .unwrap();
                }
                HttpOutcome {
                    result: Ok(admitted),
                    receipt: HttpReceipt {
                        started_at_ms: crate::timing::now_ms(),
                        finished_at_ms: crate::timing::now_ms(),
                        deadline_ms: job.deadline_ms(),
                        get_attempts: 1,
                        auth_generation: Some(1),
                        body: None,
                        executor_completed: true,
                    },
                }
            }
        }
    };
    let task = tokio::spawn(drive(owner, reference, factory, fetch));
    let mut accepted_data = progress.recv().await.unwrap();
    let mut peer = peers.remove(0);
    handshake(&mut peer).await;
    until(|| {
        trace(&path)
            .iter()
            .any(|r| r["event"]["event"] == "primary_subscription_sent")
    })
    .await;
    if scenario == R1Scenario::StaleReference {
        go(f.open() + 100);
        let mut history = f.row(f.open() - 120000);
        history.as_object_mut().unwrap().remove("isDataSnapshot");
        peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[history,f.row(f.open())]}}).to_string())).await.unwrap();
        until(|| {
            trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_bootstrap_proof_required")
        })
        .await;
        go(f.open() + 2201);
        proof_release.notify_one();
        accepted_data = tokio::time::timeout(Duration::from_secs(5), progress.recv())
            .await
            .unwrap()
            .unwrap();
        go(f.open() + 2301);
        peer = peers.remove(0);
        handshake(&mut peer).await;
        until(|| {
            trace(&path).iter().any(|r| {
                r["event"]["event"] == "primary_subscription_sent" && r["event"]["generation"] == 2
            })
        })
        .await;
        go(f.open() + 2401);
        peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(f.open())]}}).to_string())).await.unwrap();
        accepted(&mut accepted_data, f.open() + 2401).await;
        until(|| {
            trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
        })
        .await;
        accepted_data.changed().await.unwrap();
        accepted_data.borrow_and_update();
        assert!(!trace(&path).iter().any(|r| r["kind"] == "model_step"));
    }
    if startup_recovery {
        if after_data {
            go(f.open() + 100);
            peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(f.open())]}}).to_string())).await.unwrap();
            accepted(&mut accepted_data, f.open() + 100).await;
        }
        peer.close(None).await.unwrap();
        accepted_data = tokio::time::timeout(Duration::from_secs(5), progress.recv())
            .await
            .unwrap()
            .unwrap();
        go(f.open() + 200);
        peer = peers.remove(0);
        handshake(&mut peer).await;
        until(|| {
            trace(&path).iter().any(|r| {
                r["event"]["event"] == "primary_subscription_sent" && r["event"]["generation"] == 2
            })
        })
        .await;
        go(f.open() + 300);
        peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(f.open())]}}).to_string())).await.unwrap();
        accepted(&mut accepted_data, f.open() + 300).await;
        until(|| {
            trace(&path)
                .iter()
                .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
        })
        .await;
        accepted_data.changed().await.unwrap();
        accepted_data.borrow_and_update();
        assert_eq!(
            trace(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count(),
            0
        );
    }
    let minutes = if early_stop { 21 } else { 1010 };
    for minute in i64::from(startup_recovery || scenario == R1Scenario::StaleReference)..minutes {
        let label = f.open() + minute * 60000;
        if let Some(rest_winner) = scenario.historical_at(minute) {
            go(label + 1000);
            peer.send(data(label - 60000)).await.unwrap();
            until(|| {
                trace(&path).iter().any(|r| {
                    r["event"]["event"] == "primary_rest_observation"
                        && r["event"]["label_ms"] == label - 600000
                })
            })
            .await;
            go(label + 1364);
            peer.send(data(label)).await.unwrap();
            accepted(&mut accepted_data, label + 1364).await;
            go(label + 1904);
            peer.send(data(label - 60000)).await.unwrap();
            if !rest_winner {
                until(|| {
                    trace(&path).iter().any(|r| {
                        r["event"]["event"] == "primary_recovery_data_not_ready"
                            && r["event"]["assessment"]["label_ms"] == label - 600000
                    })
                })
                .await;
                go(label + 2364);
                peer.send(data(label)).await.unwrap();
            }
            until(|| {
                trace(&path)
                    .iter()
                    .filter(|r| r["event"]["event"] == "primary_recovery_episode_ready")
                    .count()
                    == if minute == 40 { 2 } else { 1 }
            })
            .await;
            assert_eq!(
                trace(&path)
                    .iter()
                    .filter(|r| r["kind"] == "canonical_commit")
                    .count(),
                minute as usize / 10
            );
            continue;
        }
        if (f.config.primary.admitted_rest_source() && minute == 50)
            || (f.config.primary.consistent_sources() && minute == 60)
        {
            // No boundary row before the proof: the whole REST exception must
            // commit, then later DATA resumes in the same physical generation.
            if minute == 60 {
                go(label + 950);
                peer.send(data(label)).await.unwrap();
                accepted(&mut accepted_data, label + 950).await;
            }
            go(label + 1000);
            // Wake the fixture-clock driver with a duplicate of the previous
            // minute, not a row from the new minute or a fabricated boundary.
            peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(label-60000)]}}).to_string())).await.unwrap();
            until(|| {
                trace(&path)
                    .iter()
                    .filter(|r| r["kind"] == "canonical_commit")
                    .count()
                    == minute as usize / 10
            })
            .await;
            accepted_data.changed().await.unwrap();
            accepted_data.borrow_and_update();
            if f.config.primary.consistent_sources() && minute == 50 {
                go(label + 1100);
                peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(label-60000)]}}).to_string())).await.unwrap();
                until(|| {
                    trace(&path).iter().any(|r| {
                        r["event"]["change"]["context"] == "WS_SUPERSEDED_REPEAT_DISCARDED"
                    })
                })
                .await;
            }
            go(label + 1200);
            peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(label)]}}).to_string())).await.unwrap();
            if minute == 50 {
                accepted(&mut accepted_data, label + 1200).await;
            }
            continue;
        }
        if let Some((_, early)) = proof_failure.filter(|_| minute == 10) {
            go(label + 3000);
            peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(label-60000)]}}).to_string())).await.unwrap();
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_rest_proof_scheduled")
            })
            .await;
            if early {
                go(label + 3050);
                proof_release.notify_one();
                until(|| {
                    trace(&path)
                        .iter()
                        .any(|r| r["event"]["event"] == "primary_rest_proof_failed")
                })
                .await;
            }
            if fault_minute != Some(10) {
                go(label + 3100);
                peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(label)]}}).to_string())).await.unwrap();
                accepted(&mut accepted_data, label + 3100).await;
                go(label + 4100);
                until(|| {
                    trace(&path)
                        .iter()
                        .filter(|r| r["kind"] == "canonical_commit")
                        .count()
                        == 1
                })
                .await;
                accepted_data.changed().await.unwrap();
                accepted_data.borrow_and_update();
                if !early {
                    go(label + 4200);
                    proof_release.notify_one();
                    until(|| {
                        trace(&path)
                            .iter()
                            .any(|r| r["event"]["event"] == "primary_rest_proof_failed")
                    })
                    .await;
                }
                continue;
            }
        }
        if fault_minute == Some(minute) && minute > 0 {
            let offset = if minute == 10 && proof_failure.is_some() {
                3300
            } else {
                100
            };
            go(label + offset);
            peer.close(None).await.unwrap();
            accepted_data = tokio::time::timeout(Duration::from_secs(5), progress.recv())
                .await
                .unwrap()
                .unwrap();
            go(label + offset + 100);
            peer = peers.remove(0);
            handshake(&mut peer).await;
            until(|| {
                trace(&path).iter().any(|r| {
                    r["event"]["event"] == "primary_subscription_sent"
                        && r["event"]["generation"] == 2
                })
            })
            .await;
            go(label + offset + 200);
            peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(label)]}}).to_string())).await.unwrap();
            accepted(&mut accepted_data, label + offset + 200).await;
            if scenario == R1Scenario::RestTail {
                until(|| {
                    trace(&path)
                        .iter()
                        .any(|r| r["event"]["event"] == "primary_recovery_data_not_ready")
                })
                .await;
                // Deliberately cross the OLD 5 s ticket. Only a new request can
                // authorize the synthetic positive continuation, not retiming.
                go(label + 6500);
                peer.send(data(label)).await.unwrap();
                // An unchanged forming row need not refresh the DATA stamp.
                // Keep the peer live while the REST-only backoff completes.
            }
            until(|| {
                trace(&path)
                    .iter()
                    .any(|r| r["event"]["event"] == "primary_recovery_episode_ready")
            })
            .await;
            accepted_data.changed().await.unwrap();
            accepted_data.borrow_and_update();
            if scenario == R1Scenario::RepeatedWitness {
                let before: Vec<_> = trace(&path)
                    .into_iter()
                    .filter(|r| {
                        matches!(
                            r["kind"].as_str(),
                            Some("canonical_source" | "model_step" | "canonical_commit")
                        )
                    })
                    .collect();
                assert_eq!(before.len(), 3);
                go(label + 400);
                peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(f.open())]}}).to_string())).await.unwrap();
                until(|| {
                    trace(&path).iter().any(|r| {
                        r["event"]["change"]["context"] == "WS_SUPERSEDED_REPEAT_DISCARDED"
                    })
                })
                .await;
                assert_eq!(
                    accepted_data.borrow().as_ref().unwrap().received_at_ms,
                    label + offset + 200
                );
                let after: Vec<_> = trace(&path)
                    .into_iter()
                    .filter(|r| {
                        matches!(
                            r["kind"].as_str(),
                            Some("canonical_source" | "model_step" | "canonical_commit")
                        )
                    })
                    .collect();
                assert_eq!(before, after);
            }
            continue;
        }
        go(label + 100);
        let wire=json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(label)]}}).to_string();
        peer.send(Message::Text(wire.clone())).await.unwrap();
        accepted(&mut accepted_data, label + 100).await;
        if scenario == R1Scenario::RepeatedWitness && minute == 0 {
            for (delay, high) in [(200, "2200.5"), (300, "2201")] {
                go(label + delay);
                let mut row = f.row(label);
                row["high"] = json!({"value":high});
                peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[row]}}).to_string())).await.unwrap();
                accepted(&mut accepted_data, label + delay).await;
            }
        }
        if f.config.primary.consistent_sources() && minute == 71 {
            go(label + 200);
            let mut revised = f.row(label);
            revised["high"] = json!({"value":"2200.5"});
            peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[revised]}}).to_string())).await.unwrap();
            accepted(&mut accepted_data, label + 200).await;
        }
        if minute > 0 && minute % 10 == 0 {
            go(label
                + if fault_minute == Some(minute - 10) && !f.config.primary.consistent_sources() {
                    3100
                } else {
                    1100
                });
            peer.send(Message::Text(wire)).await.unwrap();
            until(|| {
                trace(&path)
                    .iter()
                    .filter(|r| r["kind"] == "canonical_commit")
                    .count()
                    == minute as usize / 10
            })
            .await;
            accepted_data.changed().await.unwrap();
            accepted_data.borrow_and_update();
        }
    }
    if early_stop {
        root.stop();
        drop(peer);
    } else {
        // Let the real codec answer Pings while only the test clocks jump.
        let responder = tokio::spawn(async move {
            while let Some(Ok(message)) = peer.next().await {
                if let Message::Ping(p) = message {
                    if peer.send(Message::Pong(p)).await.is_err() {
                        break;
                    }
                }
            }
        });
        go(f.close() + f.config.primary.proof_wait_ms() + 100);
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(64)).await;
        tokio::time::resume();
        until(|| {
            trace(&path)
                .iter()
                .filter(|r| r["kind"] == "canonical_commit")
                .count()
                == 101
        })
        .await;
        accepted_data.changed().await.unwrap();
        accepted_data.borrow_and_update();
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        go(f.binding.eod_check_at_ms);
        let _running_clock = crate::timing::run_fixture_clock();
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(60)).await;
        tokio::time::resume();
        let report = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(1), responder).await;
        if scenario != R1Scenario::None {
            eprintln!(
                "R1_RUNTIME_EOD commits={} recovery={} model_prepared={}",
                report.canonical_commits,
                report.recovery.len(),
                report.completion.as_ref().unwrap().semantic_eod_passed
            );
        }
        return export(report, &f, &artifacts, &output, &calendar_sha);
    }
    let report = task.await.unwrap().unwrap();
    export(report, &f, &artifacts, &output, &calendar_sha)
}

#[test]
fn admitted_rest_v4_full_day_boundary_exception_and_independent_verifier_agree() {
    let mut f = Fixture::new();
    f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBoAdmittedRestV4;
    f.config.primary.audit_every_commits = Some(6);
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, artifact) = day(f, None, false).await;
            assert_eq!(report.canonical_commits, 101);
            assert!(report.recovery.is_empty());
            let verified = artifact.verify().unwrap();
            assert_eq!(verified.rest_exception_labels, 2);
            assert_eq!(verified.ws_live_labels, 99);
            assert!(
                !verified.parity_accepted
                    && !verified.network_qualified
                    && !verified.cgroup_qualified
            );
            let rows: Vec<Value> = std::str::from_utf8(&artifact.journal)
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
            assert_eq!(
                rows.iter()
                    .filter(|r| r["event"]["event"] == "primary_subscription_sent")
                    .count(),
                1
            );
            let canonical = rows
                .iter()
                .find(|r| {
                    r["kind"] == "canonical_source"
                        && r["event"]["receipt"]["reason"] == "BoundaryUnproven"
                })
                .unwrap();
            assert_eq!(
                canonical["event"]["receipt"]["actual_m1"][9]["close"],
                "2200"
            );
            assert_eq!(canonical["event"]["receipt"]["actual_m1"][9]["volume"], "1");
            for mutation in ["provenance", "price"] {
                let mut bad = artifact.clone();
                bad.change_journal(|rows| {
                    let r = rows
                        .iter_mut()
                        .find(|r| {
                            r["kind"] == "canonical_source"
                                && r["event"]["receipt"]["reason"] == "BoundaryUnproven"
                        })
                        .unwrap();
                    if mutation == "provenance" {
                        r["event"]["receipt"]["origin"] = json!("WS_LIVE");
                    } else {
                        r["event"]["receipt"]["actual_m1"][9]["close"] = json!("2200.5");
                    }
                });
                assert!(bad.verify().is_err());
            }
        })
    });
}

#[test]
fn consistent_v5_full_day_audit_six_repeats_and_recovery_reader_agree() {
    for (failure, recover) in [
        (None, false),
        (Some(RestFailure::Unavailable), false),
        (None, true),
    ] {
        // Use the existing historical synthetic calendar so the exported file
        // fixture also verifies against actual UTC now, not a future test clock.
        let mut f = Fixture::historical();
        f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBoConsistentSourcesV5;
        f.config.primary.audit_every_commits = Some(6);
        with_fixture_clock(f.open(), || {
            runtime(async {
                let (report, artifact) = if recover {
                    day_with_recovery(f, failure, false, Some(30), false).await
                } else {
                    day(f, failure, false).await
                };
                assert_eq!(report.canonical_commits, 101);
                assert_eq!(report.recovery.len(), usize::from(recover));
                let verified = artifact.verify().unwrap();
                assert!(verified.semantic_eod_passed);
                assert!(!verified.parity_accepted);
                let rows: Vec<Value> = std::str::from_utf8(&artifact.journal)
                    .unwrap()
                    .lines()
                    .map(|s| serde_json::from_str(s).unwrap())
                    .collect();
                assert_eq!(
                    rows.iter()
                        .filter(|r| r["event"]["event"] == "primary_audit_scheduled")
                        .count(),
                    16
                );
                assert!(rows
                    .iter()
                    .any(|r| r["event"]["change"]["context"] == "WS_SUPERSEDED_REPEAT_DISCARDED"));
                for reason in ["WsGracePending", "ProvisionalRevision"] {
                    assert!(rows.iter().any(|r| r["kind"] == "canonical_source"
                        && r["event"]["receipt"]["reason"] == reason));
                }
                if recover {
                    assert!(rows
                        .iter()
                        .any(|r| r["event"]["change"]["context"] == "RECOVERY_OVERLAP_REVISION"));
                }
                // Optional local review fixture export; never used by runtime.
                // All data/account identifiers/clocks here are synthetic.
                if let Some(root) = std::env::var_os("FINAM_V5_REVIEW_EVIDENCE_DIR") {
                    let dir = std::path::PathBuf::from(root).join(if recover {
                        "recovery-price-overlap"
                    } else if failure.is_some() {
                        "audit-unavailable"
                    } else {
                        "audit-differences"
                    });
                    std::fs::create_dir(&dir).unwrap();
                    for (name,bytes) in [
                        ("config.json",serde_json::to_vec(&artifact.config).unwrap()),
                        ("evidence-binding.json",serde_json::to_vec(&artifact.binding()).unwrap()),
                        ("journal.jsonl",artifact.journal.clone()),
                        ("completion.json",artifact.completion.clone()),
                        ("synthetic-result.json",serde_json::to_vec(&json!({"synthetic_only":true,"provider_calls":false,"verified":verified,"commits":report.canonical_commits,"recovery_episodes":report.recovery.len(),"audit_every_commits":6,"parity_accepted":false})).unwrap()),
                    ] {std::fs::write(dir.join(name),bytes).unwrap();}
                }
                for target in [
                    "superseded_witness",
                    "canonical_hash",
                    "original_price",
                    "overlap_witness",
                    "grace_boundary",
                    "quarantine_evidence",
                ] {
                    if target == "overlap_witness" && !recover {
                        continue;
                    }
                    let mut bad = artifact.clone();
                    bad.change_journal(|rows| {
                        if target == "grace_boundary" {
                            let row = rows
                                .iter_mut()
                                .find(|r| {
                                    r["kind"] == "canonical_source"
                                        && r["event"]["receipt"]["reason"] == "WsGracePending"
                                })
                                .unwrap();
                            row["event"]["receipt"]["boundary_received_at_ms"] = Value::Null;
                        } else if target == "quarantine_evidence" {
                            let row = rows
                                .iter_mut()
                                .find(|r| {
                                    r["event"]["change"]["context"] == "WS_PROVISIONAL_QUARANTINED"
                                })
                                .unwrap();
                            row["event"]["change"]["context"] = json!("REST_AUDIT");
                        } else if target == "original_price" {
                            let row = rows
                                .iter_mut()
                                .find(|r| r["kind"] == "canonical_source")
                                .unwrap();
                            row["event"]["receipt"]["actual_m1"][0]["close"] = json!("2201");
                        } else {
                            let ctx = if target == "overlap_witness" {
                                "RECOVERY_OVERLAP_REVISION"
                            } else {
                                "WS_SUPERSEDED_REPEAT_DISCARDED"
                            };
                            let row = rows
                                .iter_mut()
                                .find(|r| r["event"]["change"]["context"] == ctx)
                                .unwrap();
                            if target == "superseded_witness" {
                                row["event"]["change"]["superseded_ws_version"]["frame_sha256"] =
                                    json!("f".repeat(64));
                            } else {
                                row["event"]["change"]["original_canonical_source_sha256"] =
                                    json!("f".repeat(64));
                            }
                        }
                    });
                    assert!(bad.verify().is_err(), "forgery accepted: {target}");
                }
            })
        });
    }
}

#[test]
fn nonblocking_rest_full_synthetic_day_evidence_is_recomputed_and_never_called_parity() {
    for failure in [
        None,
        Some(RestFailure::Integrity),
        Some(RestFailure::Authentication),
    ] {
        let mut f = Fixture::new();
        f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBoNonBlockingRestV3;
        f.config.primary.audit_every_commits = Some(6);
        with_fixture_clock(f.open(), || {
            runtime(async {
                let (report, mut artifact) = day(f, failure, false).await;
                assert_eq!(report.canonical_commits, 101, "{failure:?}: {report:?}");
                let verified = artifact.verify().unwrap();
                assert!(verified.semantic_eod_passed);
                assert!(
                    !verified.network_qualified
                        && !verified.cgroup_qualified
                        && !verified.parity_accepted
                );
                if failure.is_none() {
                    let text = std::str::from_utf8(&artifact.journal).unwrap();
                    assert!(
                        text.contains("PriceDifference") && text.contains("InventoryDifference")
                    );
                    artifact.change_journal(|rows| {
                        let row = rows
                            .iter_mut()
                            .find(|r| r["event"]["event"] == "primary_audit_result")
                            .unwrap();
                        row["event"]["assessment"]["kind"] = json!("Match");
                    });
                } else {
                    artifact.change_journal(|rows| {
                        rows.retain(|r| r["event"]["event"] != "primary_rest_nonblocking_failed");
                    });
                }
                assert!(artifact.verify().is_err());
            })
        });
    }
}

#[test]
fn nonblocking_rest_committed_proof_basis_is_bound_to_its_preceding_real_commit() {
    let mut f = Fixture::new();
    f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBoNonBlockingRestV3;
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (_, mut artifact) = day(f, None, false).await;
            artifact.verify().unwrap();
            artifact.change_journal(|rows| {
                let at = rows
                    .iter()
                    .position(|r| r["event"]["event"] == "primary_rest_observation")
                    .unwrap();
                let mut proof = rows.remove(at);
                let label = proof["event"]["label_ms"].as_i64().unwrap();
                let receipt = &rows
                    .iter()
                    .find(|r| {
                        r["kind"] == "canonical_source"
                            && r["event"]["receipt"]["label_ms"] == label
                    })
                    .unwrap()["event"]["receipt"];
                let expected = serde_json::from_value(receipt["actual_m1"].clone()).unwrap();
                let actual =
                    serde_json::from_value(proof["event"]["assessment"]["observed_m1"].clone())
                        .unwrap();
                proof["event"]["assessment"] =
                    serde_json::to_value(crate::primary::rest_observation::assess(
                        label,
                        "COMMITTED_MODEL_INPUT",
                        expected,
                        actual,
                    ))
                    .unwrap();
                let after = rows
                    .iter()
                    .position(|r| {
                        r["kind"] == "canonical_commit" && r["event"]["model_label_utc_ms"] == label
                    })
                    .unwrap();
                proof["event"]["recorded_at_ms"] = json!(
                    rows[after]["event"]["publication_checked_at_ms"]
                        .as_i64()
                        .unwrap()
                        + 1
                );
                rows.insert(after + 1, proof);
                for (i, row) in rows.iter_mut().enumerate() {
                    row["sequence"] = json!(i);
                }
            });
            // Correct diagnostic after commit remains valid, including any
            // disagreement with REST. This does not re-run a model callback.
            artifact.verify().unwrap();
            let mut forged = artifact.clone();
            forged.change_journal(|rows| {
                let a = &mut rows
                    .iter_mut()
                    .find(|r| r["event"]["event"] == "primary_rest_observation")
                    .unwrap()["event"]["assessment"];
                let mut expected: Vec<broker_core::event::Bar> =
                    serde_json::from_value(a["expected_m1"].clone()).unwrap();
                expected[0].volume += rust_decimal::Decimal::ONE;
                let actual = serde_json::from_value(a["observed_m1"].clone()).unwrap();
                *a = serde_json::to_value(crate::primary::rest_observation::assess(
                    a["label_ms"].as_i64().unwrap(),
                    "COMMITTED_MODEL_INPUT",
                    expected,
                    actual,
                ))
                .unwrap();
            });
            assert_eq!(forged.verify().unwrap_err(), verify::VerifyError::Source);
            artifact.change_journal(|rows| {
                rows.iter_mut()
                    .find(|r| r["event"]["event"] == "primary_rest_observation")
                    .unwrap()["event"]["assessment"]["known_basis"] = json!("PROVISIONAL_WS");
            });
            assert_eq!(artifact.verify().unwrap_err(), verify::VerifyError::Source);
        })
    });
}

#[test]
fn ws_first_policy_full_synthetic_day_passes_independent_reader_without_live_claims() {
    let mut f = Fixture::new();
    f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBoRestPrecommitWsFirstV2;
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, artifact) = day(f, None, false).await;
            assert_eq!(report.canonical_commits, 101);
            let verified = artifact.verify().unwrap();
            assert!(verified.semantic_eod_passed);
            assert!(
                !verified.network_qualified
                    && !verified.cgroup_qualified
                    && !verified.parity_accepted
            );
        })
    });
}

#[test]
fn resilience_observed_delivery_full_day_still_passes_independent_eod_verifier() {
    let mut f = Fixture::new();
    f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBo1sVolumeAuditV1;
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, artifact) = day(f, None, false).await;
            assert_eq!(report.canonical_commits, 101);
            let verified = artifact.verify().unwrap();
            assert!(verified.semantic_eod_passed);
            assert!(!verified.network_qualified);
        })
    });
}

#[test]
fn resilience_observed_volume_audits_are_independently_verified_not_relabelled_match() {
    let mut f = Fixture::new();
    f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBo1sVolumeAuditV1;
    f.config.primary.audit_every_commits = Some(6);
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, mut artifact) = day(f, None, false).await;
            assert_eq!(report.canonical_commits, 101);
            assert!(artifact.verify().unwrap().semantic_eod_passed);
            let mut changed = false;
            artifact.change_journal(|rows| {
                let row = rows
                    .iter_mut()
                    .find(|r| {
                        r["event"]["event"] == "primary_audit_result"
                            && r["event"]["status"] == "VolumeRevision"
                    })
                    .unwrap();
                row["event"]["status"] = json!("Match");
                changed = true;
            });
            assert!(changed);
            assert!(artifact.verify().is_err());
        })
    });
}

#[test]
fn r4_known_proof_outage_before_or_after_ws_commit_allows_verified_101_label_eod() {
    for failure in [RestFailure::Timeout, RestFailure::Unavailable] {
        for early in [false, true] {
            let f = Fixture::new();
            with_fixture_clock(f.open(), || {
                runtime(async {
                    let (report, artifact) =
                        day_with_proof_failure(f, None, false, None, false, Some((failure, early)))
                            .await;
                    assert_eq!(report.canonical_commits, 101);
                    assert_eq!(report.http.proof.scheduled, 1);
                    assert_eq!(report.http.proof.completed, 1);
                    assert!(
                        report.completion.as_ref().unwrap().semantic_eod_passed,
                        "{report:?}"
                    );
                    let verified = artifact.verify().unwrap();
                    assert_eq!(
                        (verified.ws_live_labels, verified.rest_exception_labels),
                        (100, 1)
                    );
                    for mutation in [
                        "Authentication",
                        "Integrity",
                        "Unknown",
                        "Cancelled",
                        "missing-schedule",
                        "unclosed-label",
                        "duplicate-failure",
                    ] {
                        let mut bad = artifact.clone();
                        bad.change_journal(|rows| {
                            if mutation == "missing-schedule" {
                                rows.retain(|r| {
                                    !(r["event"]["event"] == "primary_rest_proof_scheduled"
                                        && r["event"]["reason"] != "SessionEnd")
                                });
                            } else if mutation == "duplicate-failure" {
                                let idx = rows
                                    .iter()
                                    .position(|r| {
                                        r["event"]["event"] == "primary_rest_proof_failed"
                                    })
                                    .unwrap();
                                rows.insert(idx + 1, rows[idx].clone());
                            } else {
                                let ev = &mut rows
                                    .iter_mut()
                                    .find(|r| r["event"]["event"] == "primary_rest_proof_failed")
                                    .unwrap()["event"];
                                if mutation == "unclosed-label" {
                                    ev["label_ms"] = json!(0);
                                } else {
                                    ev["class"] = json!(mutation);
                                }
                            }
                            for (i, row) in rows.iter_mut().enumerate() {
                                row["sequence"] = json!(i);
                            }
                        });
                        assert!(bad.verify().is_err(), "{mutation}");
                    }
                })
            });
        }
    }
}

#[test]
fn r4_failed_mandatory_proof_requires_source_bound_resolved_recovery() {
    let f = Fixture::new();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, artifact) = day_with_proof_failure(
                f,
                None,
                false,
                Some(10),
                false,
                Some((RestFailure::Unavailable, true)),
            )
            .await;
            assert_eq!(report.canonical_commits, 101);
            assert!(
                report.completion.as_ref().unwrap().semantic_eod_passed,
                "{report:?}"
            );
            let verified = artifact.verify().unwrap();
            assert_eq!(verified.historical_labels, 1);
            for mutation in ["missing-resolution", "unbound-source"] {
                let mut bad = artifact.clone();
                bad.change_journal(|rows| {
                    if mutation == "missing-resolution" {
                        rows.retain(|r| {
                            r["event"]["event"] != "primary_canonical_recovery_resolved"
                        });
                        for (i, row) in rows.iter_mut().enumerate() {
                            row["sequence"] = json!(i);
                        }
                    } else {
                        rows.iter_mut()
                            .find(|r| r["event"]["scope"] == "MODEL_GAP")
                            .unwrap()["event"]["source_sha256"] = json!("0".repeat(64));
                    }
                });
                assert!(bad.verify().is_err(), "{mutation}");
            }
        })
    });
}
fn export(
    report: RunReport,
    f: &Fixture,
    artifacts: &crate::primary::entry::artifacts::Artifacts,
    output: &std::path::Path,
    calendar_sha: &str,
) -> (RunReport, Artifact) {
    let outcome = artifacts
        .finish(&f.config, calendar_sha, Ok(&report))
        .unwrap();
    assert_eq!(
        outcome.semantic_eod_passed,
        report.completion.as_ref().unwrap().semantic_eod_passed,
        "file verifier: {}",
        std::fs::read_to_string(output.join("verification.json")).unwrap()
    );
    let manifest = std::fs::read(output.join("manifest.json")).unwrap();
    let value: Value = serde_json::from_slice(&manifest).unwrap();
    assert_eq!(value["files_sha256"].as_object().unwrap().len(), 6);
    for (name, sha) in value["files_sha256"].as_object().unwrap() {
        assert_eq!(
            sha.as_str().unwrap(),
            crate::seed::digest(&std::fs::read(output.join(name)).unwrap())
        );
    }
    assert!(artifacts
        .finish(&f.config, calendar_sha, Ok(&report))
        .is_err());
    assert_eq!(
        manifest,
        std::fs::read(output.join("manifest.json")).unwrap()
    );
    (
        report,
        Artifact {
            config: f.config.clone(),
            journal: std::fs::read(output.join("journal.jsonl")).unwrap(),
            completion: std::fs::read(output.join("completion.json")).unwrap(),
            calendar_sha: calendar_sha.into(),
            now: crate::timing::now_ms(),
        },
    )
}

#[test]
fn historical_eod_fixture_verifies_with_actual_now_without_opening_qualification() {
    let f = Fixture::historical();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, artifact) = day(f, None, false).await;
            assert!(report.completion.as_ref().unwrap().semantic_eod_passed);
            let actual_now = Utc::now().timestamp_millis();
            let checked = verify::verify(
                &artifact.config,
                &artifact.binding(),
                &artifact.journal,
                &artifact.completion,
                actual_now,
            )
            .unwrap();
            assert!(checked.semantic_eod_passed);
            assert!(
                !checked.network_qualified && !checked.cgroup_qualified && !checked.parity_accepted
            );
            if let Some(path) = std::env::var_os("WS_PRIMARY_T40_FIXTURE_DIR") {
                use std::io::Write;
                let path = std::path::PathBuf::from(path);
                std::fs::create_dir(&path).unwrap();
                let files = [
                    (
                        "config.json",
                        serde_json::to_vec_pretty(&artifact.config).unwrap(),
                    ),
                    (
                        "evidence-binding.json",
                        serde_json::to_vec_pretty(&artifact.binding()).unwrap(),
                    ),
                    ("journal.jsonl", artifact.journal),
                    ("completion.json", artifact.completion),
                ];
                let mut pins = std::collections::BTreeMap::new();
                for (name, bytes) in files {
                    pins.insert(name, crate::seed::digest(&bytes));
                    let mut out = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path.join(name))
                        .unwrap();
                    out.write_all(&bytes).unwrap();
                    out.sync_all().unwrap();
                }
                let metadata = json!({"schema_version":1,"scope":"SYNTHETIC_HISTORICAL_FILE_ONLY_NOT_FINAM_NOT_LIVE_PARITY",
                "session_date":"2026-09-28","source":"generated fake calendar/M1 and actual shadow model, not re-timestamped provider data",
                "files_sha256":pins,"canonical_labels":101,"ws_live_labels":100,"rest_exception_labels":1,
                "full_session_qualified":false,"network_qualified":false,"cgroup_qualified":false,"parity_accepted":false});
                let mut out = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path.join("fixture.json"))
                    .unwrap();
                out.write_all(&serde_json::to_vec_pretty(&metadata).unwrap())
                    .unwrap();
                out.sync_all().unwrap();
            }
        })
    });
}

#[test]
fn public_export_fault_matrix_uses_a_genuine_verified_eod_without_false_pass() {
    let mut f = Fixture::new();
    f.config.base.previous_segment_id = None;
    with_fixture_clock(f.open(), || {
        runtime(async {
            let entry = crate::primary::entry::tests::fresh(&f).receipt().unwrap();
            let (report, artifact) = day(f, None, false).await;
            artifact.verify().unwrap();
            crate::primary::entry::artifacts::tests::export_fault_matrix(
                &artifact.config,
                &entry,
                &report,
                &artifact.journal,
            );
        })
    });
}

#[test]
fn calendar_eod_joins_workers_then_seals_a_separately_verifiable_semantic_result() {
    let f = Fixture::new();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, artifact) = day(f, None, false).await;
            assert!(matches!(report.cause, Cause::EndOfSession), "{report:?}");
            assert!(report.all_workers_drained, "{report:?}");
            assert!(
                report.completion.as_ref().unwrap().semantic_eod_passed,
                "{report:?}"
            );
            assert!(!report.full_session_qualified);
            assert_eq!(report.canonical_commits, 101);
            assert_eq!(report.http.session_end.known_get_send_attempts, 1);
            assert_eq!(report.http.proof.scheduled, 0);
            let verified = artifact.verify().unwrap();
            assert_eq!(verified.ws_live_labels, 100);
            assert_eq!(verified.rest_exception_labels, 1);
            assert_eq!(verified.historical_labels, 0);
            assert!(
                !verified.network_qualified
                    && !verified.cgroup_qualified
                    && !verified.parity_accepted
            );
        })
    });
}

#[test]
fn known_audit_unavailability_does_not_corrupt_a_complete_eod() {
    let mut f = Fixture::new();
    f.config.primary.audit_every_commits = Some(6);
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, artifact) = day(f, Some(RestFailure::Timeout), false).await;
            assert!(report.http.audit.completed > 0);
            assert!(
                report.completion.as_ref().unwrap().semantic_eod_passed,
                "{report:?}"
            );
            artifact.verify().unwrap();
        })
    });
}

#[test]
fn operator_stop_cannot_be_relabelled_as_calendar_eod() {
    let f = Fixture::new();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, artifact) = day(f, None, true).await;
            assert!(!report.completion.as_ref().unwrap().semantic_eod_passed);
            assert!(artifact.verify().is_err());
        })
    });
}

#[test]
fn verifier_rejects_rebound_receipts_counts_latches_partial_commits_and_origins() {
    let f = Fixture::new();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (_, artifact) = day(f, None, false).await;
            artifact.verify().unwrap();
            for field in [
                "writer_drained",
                "fault_free",
                "continuity_resolved",
                "recovery_resolved",
                "calendar_eod_trigger",
                "model_prepared",
            ] {
                let mut c: Value = serde_json::from_slice(&artifact.completion).unwrap();
                c[field] = json!(false);
                let bytes = serde_json::to_vec(&c).unwrap();
                let mut binding = artifact.binding();
                binding.completion_sha256 = crate::seed::digest(&bytes);
                assert!(
                    verify::verify(
                        &artifact.config,
                        &binding,
                        &artifact.journal,
                        &bytes,
                        artifact.now
                    )
                    .is_err(),
                    "{field}"
                );
            }
            for case in [
                "gap",
                "pending",
                "self_join",
                "source",
                "commit",
                "origin",
                "reason",
                "late",
                "terminal",
                "calendar",
            ] {
                let mut a = Artifact {
                    config: artifact.config.clone(),
                    journal: artifact.journal.clone(),
                    completion: artifact.completion.clone(),
                    calendar_sha: artifact.calendar_sha.clone(),
                    now: artifact.now,
                };
                a.change_journal(|rows| match case {
                    "gap" => {
                        rows.iter_mut()
                            .find(|r| r["event"]["event"] == "primary_async_shutdown")
                            .unwrap()["event"]["continuity_lost"] = json!(true)
                    }
                    "pending" => {
                        rows.iter_mut()
                            .find(|r| r["event"]["event"] == "primary_eod_prepared")
                            .unwrap()["event"]["model"]["pending"] = json!(true)
                    }
                    "self_join" => {
                        rows.iter_mut()
                            .find(|r| r["event"]["event"] == "primary_eod_prepared")
                            .unwrap()["event"]["writer_join_attested"] = json!(true)
                    }
                    "source" => {
                        rows.iter_mut()
                            .find(|r| r["kind"] == "canonical_source")
                            .unwrap()["event"]["receipt"]["actual_m1"][0]["volume"] = json!("99")
                    }
                    "commit" => {
                        rows.iter_mut()
                            .find(|r| r["kind"] == "canonical_commit")
                            .unwrap()["event"]["feedback_complete"] = json!(false)
                    }
                    "origin" => {
                        rows.iter_mut().find(|r| r["kind"] == "model_step").unwrap()["event"]
                            ["mode"] = json!("BACKFILL_REPLAY")
                    }
                    "reason" => {
                        rows.iter_mut()
                            .rfind(|r| r["kind"] == "canonical_source")
                            .unwrap()["event"]["receipt"]["reason"] = json!("BoundaryUnproven")
                    }
                    "late" => {
                        rows.iter_mut()
                            .rfind(|r| r["kind"] == "canonical_commit")
                            .unwrap()["event"]["publication_checked_at_ms"] =
                            json!(artifact.now + 20000)
                    }
                    "terminal" => {
                        rows.last_mut().unwrap()["event"]["reason"] =
                            json!("INCOMPLETE_primary_bounded_stop")
                    }
                    "calendar" => {
                        rows.iter_mut()
                            .find(|r| r["event"]["event"] == "primary_calendar_bound")
                            .unwrap()["event"]["evidence"]["model_day_raw_json"] = json!("{}")
                    }
                    _ => unreachable!(),
                });
                assert!(a.verify().is_err(), "{case}");
            }
        })
    });
}

#[test]
fn audit_match_is_recomputed_from_retained_raw_and_committed_m1_not_the_status_flag() {
    let mut f = Fixture::new();
    f.config.primary.audit_every_commits = Some(6);
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, mut artifact) = day(f, None, false).await;
            assert!(report.http.audit.known_get_send_attempts > 0);
            artifact.verify().unwrap();
            artifact.change_journal(|rows| {
                rows.iter_mut()
                    .find(|r| r["event"]["event"] == "primary_audit_result")
                    .unwrap()["event"]["source_sha256"] = json!("0".repeat(64))
            });
            assert_eq!(artifact.verify().unwrap_err(), verify::VerifyError::Source);
        })
    });
}

#[test]
fn startup_close_requires_calendar_anchor_then_partial_proof_and_survives_to_eod() {
    for after_data in [false, true] {
        let f = Fixture::new();
        with_fixture_clock(f.open(), || {
            runtime(async {
                let (report, artifact) =
                    day_with_recovery(f, None, false, Some(0), after_data).await;
                assert!(
                    report.completion.as_ref().unwrap().semantic_eod_passed,
                    "{report:?}"
                );
                assert_eq!(report.http.recovery.scheduled, 1);
                assert_eq!(report.http.proof.scheduled, 1);
                assert_eq!(report.http.session_end.scheduled, 1);
                assert_eq!(report.recovery.len(), 1);
                assert!(
                    !report.recovery[0].canonical_unresolved
                        && !report.recovery[0].transport_unresolved
                );
                let verified = artifact.verify().unwrap();
                assert_eq!(
                    (
                        verified.ws_live_labels,
                        verified.rest_exception_labels,
                        verified.historical_labels
                    ),
                    (99, 2, 0)
                );
                for case in ["range", "scope", "missing"] {
                    let mut changed = artifact.clone();
                    changed.change_journal(|rows| {
                        if case == "missing" {
                            rows.retain(|r| {
                                r["event"]["event"] != "primary_recovery_source_validated"
                            });
                            for (i, row) in rows.iter_mut().enumerate() {
                                row["sequence"] = json!(i);
                            }
                        } else {
                            let event = &mut rows
                                .iter_mut()
                                .find(|r| r["event"]["scope"] == "STARTUP_ANCHOR")
                                .unwrap()["event"];
                            if case == "range" {
                                event["start_ms"] = json!(0);
                            } else {
                                event["scope"] = json!("MODEL_GAP");
                            }
                        }
                    });
                    assert!(changed.verify().is_err(), "{case}");
                }
            })
        });
    }
}

#[test]
fn complete_day_with_overlap_recovery_reverifies_retained_proof_and_exact_m1() {
    let f = Fixture::new();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, mut artifact) = day_with_recovery(f, None, false, Some(30), false).await;
            assert!(
                report.completion.as_ref().unwrap().semantic_eod_passed,
                "{report:?}"
            );
            let result = artifact.verify().unwrap();
            assert_eq!(
                (
                    result.ws_live_labels,
                    result.rest_exception_labels,
                    result.historical_labels
                ),
                (98, 2, 1)
            );
            assert_eq!(report.http.recovery.completed, 1);
            assert_eq!(report.canonical_commits, 101);
            artifact.change_journal(|rows| {
                rows.iter_mut()
                    .find(|r| r["event"]["scope"] == "MODEL_GAP")
                    .unwrap()["event"]["overlap_labels"] = json!(0)
            });
            assert_eq!(artifact.verify().unwrap_err(), verify::VerifyError::Source);
        })
    });
}
