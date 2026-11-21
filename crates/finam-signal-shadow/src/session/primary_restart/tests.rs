use super::*;
use crate::{
    primary::{self, restart::*},
    runner::primary::{Coordinator, Tick},
    timing::{advance_fixture_clock, now_ms, with_fixture_clock},
};
use serde_json::{json, Value};
use std::path::PathBuf;

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "primary-restart-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Fixture {
    config: WsPrimaryConfig,
    materials: [Vec<u8>; 4],
    required: Vec<i64>,
}
impl Fixture {
    fn new() -> Self {
        let (mut base, materials) = crate::intraday::tests::primary_restart_materials();
        base.lmax_ms = 10000;
        base.clock_uncertainty_ms = 100;
        base.segment_id = "original".into();
        let required = (0..101)
            .map(|i| base.observation_start_utc_ms + i * 600000)
            .collect();
        Self {
            config: WsPrimaryConfig {
                schema_version: 2,
                market_data_mode: "ws_primary_v1".into(),
                source_policy: primary::WS_PRIMARY_POLICY.into(),
                base,
                primary: primary::PrimaryPolicy {
                    audit_every_commits: None,
                    ..Default::default()
                },
            },
            materials,
            required,
        }
    }
    fn segment(&self, c: &WsPrimaryConfig) -> SeededSegment {
        SeededSegment::from_inputs(
            ValidatedConfig::from_ws_primary(c).unwrap(),
            &self.materials[0],
            &self.materials[1],
            &self.materials[2],
            &self.materials[3],
        )
        .unwrap()
    }
    fn successor(&self, previous: &WsPrimaryConfig, id: &str) -> WsPrimaryConfig {
        let mut next = previous.clone();
        next.base.segment_id = id.into();
        next.base.previous_segment_id = Some(previous.base.segment_id.clone());
        next
    }
    fn binding(
        &self,
        c: &WsPrimaryConfig,
        old: &WsPrimaryConfig,
        bytes: &[u8],
        count: usize,
    ) -> RestartBinding {
        RestartBinding {
            schema_version: 1,
            config_sha256: c.fingerprint().unwrap(),
            previous_config_sha256: old.fingerprint().unwrap(),
            previous_journal_sha256: crate::seed::digest(bytes),
            previous_segment_id: old.base.segment_id.clone(),
            required_labels_sha256: inventory_sha256(&self.required),
            committed_labels: count,
            max_reconstruction_ms: 10000,
        }
    }
}
fn stop(_c: &WsPrimaryConfig) -> StopHandle {
    // Only obtain the independent STOP latch; the collector is never executed.
    crate::transport::ReadOnlyCollector::new(
        Utc::now() + chrono::Duration::hours(1),
        std::time::Duration::from_secs(1),
    )
    .unwrap()
    .0
}
fn go(ms: i64) {
    assert!(ms >= now_ms());
    advance_fixture_clock((ms - now_ms()) as u64);
}
fn bars(f: &Fixture, minute: i64, side: i64) -> String {
    let label = f.required[0] + minute * 60000;
    let (o, h, l, c) = if minute >= 180 {
        if side < 0 {
            ("2195", "2196", "2190", "2195")
        } else {
            ("2205", "2210", "2204", "2205")
        }
    } else {
        ("2200", "2201", "2199", "2200.5")
    };
    json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","timeframe":"TIME_FRAME_M1",
        "bars":[{"timestamp":DateTime::<Utc>::from_timestamp_millis(label).unwrap().to_rfc3339(),"isDataSnapshot":false,
        "open":{"value":o},"high":{"value":h},"low":{"value":l},"close":{"value":c},"volume":{"value":"2"}}]}}).to_string()
}
fn journal(f: &Fixture, path: &Path, side: i64, pause: Option<(&'static str, bool, PathBuf)>) {
    with_fixture_clock(f.required[0] - 100, || {
        let mut session =
            ObservedSession::create(f.segment(&f.config), path, stop(&f.config)).unwrap();
        let mut processor = PrimaryProcessor::from_fixture(
            &f.config,
            broker_core::InstrumentId {
                symbol: "IMOEXF".into(),
                venue_symbol: Some("IMOEXF@RTSX".into()),
                exchange: broker_core::Exchange::Moex,
                market: broker_core::Market::Futures,
            },
            f.required.clone(),
            1,
            now_ms(),
        )
        .unwrap();
        for minute in 0..=190 {
            if minute == 181 {
                if let Some((kind, before, marker)) = &pause {
                    session
                        .observer
                        .pause_for_crash(kind, *before, marker.clone());
                }
            }
            let at = f.required[0] + minute * 60000;
            go(at + 100);
            processor
                .ingest(
                    primary::WireFrame::normalize(
                        &bars(f, minute, side),
                        1,
                        "fixture",
                        DateTime::from_timestamp_millis(at + 100).unwrap(),
                    )
                    .unwrap(),
                    now_ms(),
                )
                .unwrap();
            if minute > 0 && minute % 10 == 0 {
                go(at + 1100);
                let primary::CanonicalStatus::Ready(input) = processor.prepare(now_ms()).unwrap()
                else {
                    panic!("dense input");
                };
                let publication = session.canonical(&input).unwrap();
                processor
                    .committed(*input, publication.committed_at_ms)
                    .unwrap();
            }
        }
        assert_eq!(session.model.position_lots(), side);
    });
}
fn rows(bytes: &[u8]) -> Vec<Value> {
    std::str::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
fn encode(rows: &[Value]) -> Vec<u8> {
    rows.iter()
        .flat_map(|r| {
            let mut b = serde_json::to_vec(r).unwrap();
            b.push(b'\n');
            b
        })
        .collect()
}
fn prefix(
    f: &Fixture,
    c: &WsPrimaryConfig,
    old: &WsPrimaryConfig,
    bytes: &[u8],
    count: usize,
) -> Result<CommittedPrefix, RestartError> {
    primary::restart::admit(
        c,
        f.binding(c, old, bytes, count),
        bytes,
        &f.required,
        now_ms(),
    )
}

fn rest(
    f: &Fixture,
    plan: &broker_finam::sparse_m10::ClosedM1RequestPlan,
) -> AdmittedClosedM1Snapshot {
    rest_side(f, plan, 1)
}
fn rest_side(
    f: &Fixture,
    plan: &broker_finam::sparse_m10::ClosedM1RequestPlan,
    side: i64,
) -> AdmittedClosedM1Snapshot {
    use broker_finam::sparse_m10::{
        ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1, OBSERVED_M1_POLICY_V1,
    };
    let start = plan.start();
    let end = plan.end();
    let actual = DateTime::from_timestamp_millis(now_ms()).unwrap();
    let values: Vec<_> = (start.timestamp_millis()..end.timestamp_millis())
        .step_by(60000)
        .map(|label| {
            let raw: Value =
                serde_json::from_str(&bars(f, (label - f.required[0]) / 60000, side)).unwrap();
            raw["payload"]["bars"][0].clone()
        })
        .collect();
    let raw = json!({"symbol":"IMOEXF@RTSX","bars":values}).to_string();
    AdmittedClosedM1Snapshot::admit(
        ClosedM1SnapshotEvidenceV1 {
            policy: OBSERVED_M1_POLICY_V1.into(),
            start,
            end,
            parts: vec![ClosedM1RestPartV1 {
                method: "GET".into(),
                endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
                symbol: "IMOEXF@RTSX".into(),
                timeframe: "TIME_FRAME_M1".into(),
                start,
                end,
                requested_at: actual,
                received_at: actual,
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(raw.len() as u64),
                response_sha256: crate::seed::digest(raw.as_bytes()),
                raw_body: raw,
            }],
        },
        plan,
        actual,
    )
    .unwrap()
}

#[test]
fn restored_inventory_supports_overlap_recovery_and_rest_exception_without_reentry() {
    let f = Fixture::new();
    let d = Dir::new();
    journal(&f, &d.0.join("original"), 1, None);
    let bytes = std::fs::read(d.0.join("original")).unwrap();
    let c = f.successor(&f.config, "second");
    with_fixture_clock(f.required[19] + 5000, || {
        let cap = prefix(&f, &c, &f.config, &bytes, 19).unwrap();
        let (session, mut p) =
            ObservedSession::resume_primary(f.segment(&c), cap, &c, &d.0.join("second"), stop(&c))
                .unwrap();
        go(f.required[20] + 100);
        p.break_continuity();
        p.begin_generation(2, now_ms()).unwrap();
        let subscription = crate::socket::primary::fixture_subscription(2, now_ms());
        let mut owner = Coordinator::new(p, session).unwrap();
        owner.bind_subscription(&subscription).unwrap();
        go(f.required[20] + 200);
        let mut frame = primary::WireFrame::normalize(
            &bars(&f, 200, 1),
            2,
            "fixture",
            DateTime::from_timestamp_millis(now_ms()).unwrap(),
        )
        .unwrap();
        frame.owner = Some(subscription.owner.clone());
        owner.on_frame(frame).unwrap();
        let plan = owner.plan_recovery(f.required[20]).unwrap();
        assert_eq!(plan.overlap_labels(), 2);
        assert_eq!(plan.missed_labels(), 1);
        owner
            .complete_recovery(&plan, &rest(&f, plan.request()))
            .unwrap();
        assert!(owner.resolve_recovery().unwrap().is_some());
        assert_eq!(owner.processor().counters().committed, 20);
        assert_eq!(owner.processor().counters().restart_committed, 19);
        go(f.required[21] + 3100);
        let Tick::ProofRequired(ticket) = owner.tick().unwrap() else {
            panic!("partial fresh bucket needs proof");
        };
        let outcome = rest(&f, &ticket.request_plan().unwrap());
        owner.complete_proof(ticket, Ok(outcome)).unwrap();
        assert_eq!(owner.processor().counters().committed, 21);
        let current = std::fs::read(d.0.join("second")).unwrap();
        // Stored responses are historical evidence, not new LIVE observations.
        // Both the recovery source and inline exception are now >15min old.
        go(f.required[23] + 100);
        let c3 = f.successor(&c, "third");
        let cap = prefix(&f, &c3, &c, &current, 21).unwrap();
        let (s3, p3) = ObservedSession::resume_primary(
            f.segment(&c3),
            cap,
            &c3,
            &d.0.join("third"),
            stop(&c3),
        )
        .unwrap();
        assert_eq!(s3.model.position_lots(), 1);
        assert_eq!(p3.next_label_ms(), Some(f.required[21]));
        assert_eq!(
            rows(&current)
                .iter()
                .filter(|r| r["kind"] == "hypothetical_candle_feedback")
                .count(),
            1
        );
    });
}

#[test]
fn warmed_long_and_short_reconstruct_and_can_restart_again_without_live_effects() {
    check_warmed_restarts(primary::PrimaryPolicy::default().delivery);
}

#[test]
fn tail_v6_r1_second_restart_long_short_and_subsequent_overlap_admission() {
    check_warmed_restarts(primary::DeliveryPolicy::ObservedBoRestTailRecoveryV6);
}

fn check_warmed_restarts(delivery: primary::DeliveryPolicy) {
    for side in [-1, 1] {
        let mut f = Fixture::new();
        f.config.primary.delivery = delivery;
        let d = Dir::new();
        let original = d.0.join("original");
        journal(&f, &original, side, None);
        let bytes = std::fs::read(&original).unwrap();
        let c = f.successor(&f.config, "second");
        with_fixture_clock(f.required[19] + 5000, || {
            let admitted = prefix(&f, &c, &f.config, &bytes, 19).unwrap();
            let (session, p) = ObservedSession::resume_primary(
                f.segment(&c),
                admitted,
                &c,
                &d.0.join("second"),
                stop(&c),
            )
            .unwrap();
            assert_eq!(session.model.position_lots(), side);
            assert!(session.model.pending().is_none());
            assert_eq!(p.next_label_ms(), Some(f.required[19]));
            assert!(!p.subscription_data_ready());
            assert!(p.last_live_data().is_none());
            assert_eq!(p.counters().restart_committed, 19);
            assert_eq!(p.state(), primary::PipelineState::Bootstrap);
            let state = session.model.observation();
            let _owner = Coordinator::new(p, session).unwrap();
            let second = std::fs::read(d.0.join("second")).unwrap();
            let log = rows(&second);
            assert_eq!(
                log.iter().filter(|r| r["kind"] == "restart_source").count(),
                19
            );
            assert!(
                log.iter()
                    .filter(|r| r["kind"] == "model_step"
                        || r["kind"] == "hypothetical_candle_feedback")
                    .all(|r| r["event"]["mode"] == "BACKFILL_REPLAY")
            );
            let c3 = f.successor(&c, "third");
            let first_commit = log
                .iter()
                .position(|r| r["kind"] == "canonical_commit")
                .unwrap();
            assert!(
                prefix(&f, &c3, &c, &encode(&log[..=first_commit]), 1).is_err(),
                "unfinished reconstruction is not a smaller committed prefix"
            );
            let mut broken_anchor = log.clone();
            broken_anchor[1]["event"]["restart_binding"]["previous_journal_sha256"] =
                json!("0".repeat(64));
            assert!(prefix(&f, &c3, &c, &encode(&broken_anchor), 19).is_err());
            let cap = prefix(&f, &c3, &c, &second, 19).unwrap();
            let (mut s3, mut p3) = ObservedSession::resume_primary(
                f.segment(&c3),
                cap,
                &c3,
                &d.0.join("third"),
                stop(&c3),
            )
            .unwrap();
            assert_eq!(s3.model.position_lots(), side);
            assert_eq!(s3.model.observation(), state);
            assert!(!p3.subscription_data_ready());
            assert_eq!(std::fs::read(&original).unwrap(), bytes);
            if delivery == primary::DeliveryPolicy::ObservedBoRestTailRecoveryV6 {
                // The processor's reconstructed inventory must remain usable
                // for exact committed overlap, not just prefix readability.
                go(f.required[20] + 100);
                p3.break_continuity();
                p3.begin_generation(2, now_ms()).unwrap();
                let subscription = crate::socket::primary::fixture_subscription(2, now_ms());
                p3.bind_subscription(&subscription, now_ms()).unwrap();
                let plan = p3
                    .issue_recovery_request(
                        f.required[20],
                        now_ms(),
                        primary::RecoveryRequestScope {
                            episode: 1,
                            ordinal: 1,
                            deadline_ms: now_ms() + 45000,
                        },
                        None,
                    )
                    .unwrap();
                assert_eq!(plan.overlap_labels(), 2);
                assert_eq!(plan.missed_labels(), 1);
                let source = rest_side(&f, plan.request(), side);
                let primary::RecoveryOutcome::Ready(batch) = p3
                    .prepare_recovery_request(&plan, &source, now_ms())
                    .unwrap()
                else {
                    panic!("complete overlap");
                };
                assert_eq!(batch.steps(), 1);
                for input in batch.inputs {
                    let publication = s3.recovery(&input, &source).unwrap();
                    p3.recovered(input, publication.committed_at_ms).unwrap();
                }
                assert_eq!(s3.model.position_lots(), side);
                assert_eq!(p3.counters().committed, 20);
                assert_eq!(p3.counters().restart_committed, 19);
                assert_eq!(p3.counters().recovery_committed, 1);
            }
        });
    }
}

#[test]
fn partial_source_step_and_fill_tails_hold_but_committed_frontiers_resume() {
    let f = Fixture::new();
    let d = Dir::new();
    let path = d.0.join("original");
    journal(&f, &path, 1, None);
    let bytes = std::fs::read(path).unwrap();
    let log = rows(&bytes);
    let c = f.successor(&f.config, "restart");
    let source = log
        .iter()
        .rposition(|r| r["kind"] == "canonical_source")
        .unwrap();
    let step = log.iter().rposition(|r| r["kind"] == "model_step").unwrap();
    let fill = log
        .iter()
        .rposition(|r| r["kind"] == "hypothetical_candle_feedback")
        .unwrap();
    let commit = log
        .iter()
        .rposition(|r| r["kind"] == "canonical_commit")
        .unwrap();
    with_fixture_clock(f.required[19] + 5000, || {
        for (end, count, ok) in [
            (source, 18, true),
            (source + 1, 18, false),
            (step + 1, 18, false),
            (fill + 1, 18, false),
            (commit + 1, 19, true),
        ] {
            let bytes = encode(&log[..end]);
            assert_eq!(
                prefix(&f, &c, &f.config, &bytes, count).is_ok(),
                ok,
                "frontier {end}"
            );
        }
        // A saved pending intent is never promoted by adding a commit boolean.
        let mut partial = log[..fill].to_vec();
        partial.push(log[commit].clone());
        let n = partial.len();
        partial[n - 1]["sequence"] = json!(n - 1);
        assert!(prefix(&f, &c, &f.config, &encode(&partial), 19).is_err());
    });
}

#[test]
fn identity_order_numeric_and_state_mutations_do_not_mint_a_resumable_model() {
    let f = Fixture::new();
    let d = Dir::new();
    let original = d.0.join("original");
    journal(&f, &original, 1, None);
    let bytes = std::fs::read(original).unwrap();
    let c = f.successor(&f.config, "restart");
    with_fixture_clock(f.required[19] + 5000, || {
        let mut binding = f.binding(&c, &f.config, &bytes, 19);
        binding.previous_journal_sha256 = "0".repeat(64);
        assert!(admit(&c, binding, &bytes, &f.required, now_ms()).is_err());
        for kind in [
            "sequence", "source", "step", "commit", "bitmap", "policy", "future",
        ] {
            let mut log = rows(&bytes);
            match kind {
                "sequence" => log[1]["sequence"] = json!(500),
                "source" => {
                    let r = log
                        .iter_mut()
                        .find(|r| r["kind"] == "canonical_source")
                        .unwrap();
                    r["event"]["receipt"]["actual_m1"][0]["volume"] = json!("99");
                }
                "step" => {
                    let r = log.iter_mut().find(|r| r["kind"] == "model_step").unwrap();
                    r["event"]["canonical_input_sha256"] = json!("0".repeat(64));
                }
                "commit" => {
                    let r = log
                        .iter_mut()
                        .find(|r| r["kind"] == "canonical_commit")
                        .unwrap();
                    r["event"]["feedback_complete"] = json!(false);
                }
                "bitmap" => {
                    let r = log
                        .iter_mut()
                        .find(|r| r["kind"] == "canonical_source")
                        .unwrap();
                    r["event"]["receipt"]["minute_bitmap"] = json!(1);
                }
                "policy" => log[0]["observation"]["semantic_config_sha256"] = json!("a".repeat(64)),
                "future" => {
                    let r = log
                        .iter_mut()
                        .find(|r| r["kind"] == "canonical_source")
                        .unwrap();
                    r["event"]["receipt"]["boundary_received_at_ms"] = json!(i64::MAX);
                }
                _ => unreachable!(),
            }
            assert!(
                prefix(&f, &c, &f.config, &encode(&log), 19).is_err(),
                "{kind}"
            );
        }
        let mut bad = bytes.clone();
        bad.pop();
        assert!(prefix(&f, &c, &f.config, &bad, 19).is_err());
        let duplicate = String::from_utf8(bytes.clone()).unwrap().replacen(
            "\"sequence\":0",
            "\"sequence\":0,\"sequence\":0",
            1,
        );
        assert!(prefix(&f, &c, &f.config, duplicate.as_bytes(), 19).is_err());
        let mut drift = rows(&bytes);
        let step = drift
            .iter_mut()
            .find(|r| r["kind"] == "model_step")
            .unwrap();
        step["event"]["after"]["current_day_high"] = json!(9999);
        // Cross-row state chain detects a changed saved observation, even with a new byte pin.
        assert!(prefix(&f, &c, &f.config, &encode(&drift), 19).is_err());
        // Internally consistent saved observations still are not executable state.
        let mut drift = rows(&bytes);
        drift
            .iter_mut()
            .rfind(|r| r["kind"] == "model_step")
            .unwrap()["event"]["after"]["current_day_high"] = json!(9999);
        drift
            .iter_mut()
            .rfind(|r| r["kind"] == "hypothetical_candle_feedback")
            .unwrap()["event"]["state_after"]["current_day_high"] = json!(9999);
        let drift = encode(&drift);
        let cap = prefix(&f, &c, &f.config, &drift, 19).unwrap();
        let latch = stop(&c);
        assert!(matches!(
            ObservedSession::resume_primary(
                f.segment(&c),
                cap,
                &c,
                &d.0.join("drift"),
                latch.clone()
            ),
            Err(RestartError::Model)
        ));
        assert!(latch.is_stopped());
        let cap = prefix(&f, &c, &f.config, &bytes, 19).unwrap();
        let stopped = stop(&c);
        stopped.stop();
        assert!(ObservedSession::resume_primary(
            f.segment(&c),
            cap,
            &c,
            &d.0.join("stopped"),
            stopped
        )
        .is_err());
        assert!(!d.0.join("stopped").exists());
        let cap = prefix(&f, &c, &f.config, &bytes, 19).unwrap();
        assert!(ObservedSession::resume_primary(
            f.segment(&c),
            cap,
            &c,
            &d.0.join("original"),
            stop(&c)
        )
        .is_err());
        assert_eq!(std::fs::read(d.0.join("original")).unwrap(), bytes);
    });
}

#[cfg(unix)]
#[test]
fn crash_worker() {
    let Some(root) = std::env::var_os("PRIMARY_RESTART_CRASH_ROOT") else {
        return;
    };
    let phase = std::env::var("PRIMARY_RESTART_CRASH_PHASE").unwrap();
    let root = PathBuf::from(root);
    let (kind, before) = match phase.as_str() {
        "before_source" => ("canonical_source", true),
        "source" => ("canonical_source", false),
        "step" => ("model_step", false),
        "fill" => ("hypothetical_candle_feedback", false),
        "commit" => ("canonical_commit", false),
        _ => panic!("phase"),
    };
    journal(
        &Fixture::new(),
        &root.join("journal"),
        1,
        Some((kind, before, root.join("marker"))),
    );
    panic!("crash pause not reached");
}

#[cfg(unix)]
#[test]
fn five_sigkill_frontiers_use_real_durable_journal_and_preserve_pending_hold() {
    use std::{
        os::unix::process::ExitStatusExt,
        process::{Command, Stdio},
        time::Duration,
    };
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let f = Fixture::new();
    for (phase, count, ok) in [
        ("before_source", 18, true),
        ("source", 18, false),
        ("step", 18, false),
        ("fill", 18, false),
        ("commit", 19, true),
    ] {
        let d = Dir::new();
        let mut child = Child(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "session::primary_restart::tests::crash_worker",
                    "--nocapture",
                ])
                .env("PRIMARY_RESTART_CRASH_ROOT", &d.0)
                .env("PRIMARY_RESTART_CRASH_PHASE", phase)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let began = Instant::now();
        while !d.0.join("marker").exists() {
            assert!(
                began.elapsed() < Duration::from_secs(15),
                "child phase {phase}"
            );
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited before marker {phase}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        child.0.kill().unwrap();
        let status = child.0.wait().unwrap();
        assert_eq!(status.signal(), Some(9));
        let bytes = std::fs::read(d.0.join("journal")).unwrap();
        let c = f.successor(&f.config, "restart");
        with_fixture_clock(f.required[19] + 5000, || {
            let cap = prefix(&f, &c, &f.config, &bytes, count);
            assert_eq!(cap.is_ok(), ok, "SIGKILL {phase}");
            if let Ok(cap) = cap {
                let (session, p) = ObservedSession::resume_primary(
                    f.segment(&c),
                    cap,
                    &c,
                    &d.0.join("restart"),
                    stop(&c),
                )
                .unwrap();
                assert_eq!(
                    session.model.position_lots(),
                    if count == 19 { 1 } else { 0 }
                );
                assert!(session.model.pending().is_none());
                assert!(!p.subscription_data_ready());
                assert_eq!(p.next_label_ms(), Some(f.required[count]));
            }
            assert_eq!(std::fs::read(d.0.join("journal")).unwrap(), bytes);
        });
    }
}
