//! Fixed-origin real HTTP executor. Each command includes auth wait/renewal,
//! bounded response/body admission and the same absolute STOP/run deadline.
use super::{RestFailure, RestTicket};
use crate::{
    auth::{AuthLifecycle, PrimaryAuthClass},
    transport::StopHandle,
};
use broker_finam::{
    sparse_m10::{AdmittedClosedM1Snapshot, ClosedM1RequestPlan},
    PrimaryGetFailure, Stage8bP1fO2GetOnlyClientV1,
};
use serde::Serialize;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

#[derive(Debug, Clone, Serialize)]
pub struct HttpReceipt {
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
    pub deadline_ms: i64,
    /// Attempts recorded inside the fixed client immediately before send().
    /// This is not proof that the provider received/accepted the request.
    pub get_attempts: u64,
    pub auth_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyReceipt>,
    #[serde(skip_serializing_if = "completed")]
    pub executor_completed: bool,
}

fn completed(value: &bool) -> bool {
    *value
}

#[derive(Debug, Clone, Serialize)]
pub struct BodyReceipt {
    pub sha256: String,
    pub bytes: usize,
    pub received_at_ms: i64,
    pub intrinsic_admission_valid: bool,
    pub raw_body_included: bool,
}
pub struct HttpOutcome {
    pub result: Result<AdmittedClosedM1Snapshot, RestFailure>,
    pub receipt: HttpReceipt,
}

impl HttpOutcome {
    /// No admitted grant: a known auth/binding rejection is not a bad market
    /// data response. Do not disguise it as optional REST observation failure.
    pub(super) fn authority_rejected(&self) -> bool {
        self.receipt.auth_generation.is_none()
            && matches!(
                self.result,
                Err(RestFailure::Authentication | RestFailure::Integrity | RestFailure::Unknown)
            )
    }
}

struct AuthCancellation {
    stop: StopHandle,
    armed: bool,
}
impl Drop for AuthCancellation {
    fn drop(&mut self) {
        if self.armed {
            self.stop.stop();
        }
    }
}

fn get_failure(e: PrimaryGetFailure) -> RestFailure {
    match e {
        PrimaryGetFailure::Timeout => RestFailure::Timeout,
        PrimaryGetFailure::Unavailable => RestFailure::Unavailable,
        PrimaryGetFailure::Authentication => RestFailure::Authentication,
        PrimaryGetFailure::Integrity => RestFailure::Integrity,
        PrimaryGetFailure::Unknown => RestFailure::Unknown,
    }
}

pub async fn execute_ticket(
    auth: &AuthLifecycle,
    client: &Stage8bP1fO2GetOnlyClientV1,
    ticket: &RestTicket,
    run_deadline_ms: i64,
    stop: &StopHandle,
) -> HttpOutcome {
    match ticket.request_plan() {
        Ok(plan) => {
            execute(
                auth,
                client,
                &plan,
                ticket.deadline_ms().min(run_deadline_ms),
                stop,
            )
            .await
        }
        Err(_) => HttpOutcome {
            result: Err(RestFailure::Integrity),
            receipt: HttpReceipt {
                started_at_ms: chrono::Utc::now().timestamp_millis(),
                finished_at_ms: chrono::Utc::now().timestamp_millis(),
                deadline_ms: ticket.deadline_ms(),
                get_attempts: 0,
                auth_generation: None,
                body: None,
                executor_completed: true,
            },
        },
    }
}

/// Also used for the independent single-GET bootstrap plan. No arbitrary
/// endpoint/account fallback or retry can be supplied by the caller.
pub async fn execute(
    auth: &AuthLifecycle,
    client: &Stage8bP1fO2GetOnlyClientV1,
    plan: &ClosedM1RequestPlan,
    deadline_ms: i64,
    stop: &StopHandle,
) -> HttpOutcome {
    execute_with_deadline(
        auth,
        client,
        plan,
        Deadline {
            at_ms: deadline_ms,
            late_as_timeout: false,
            evidence_only_late: false,
        },
        stop,
    )
    .await
}

pub(super) async fn execute_ws_first_proof(
    auth: &AuthLifecycle,
    client: &Stage8bP1fO2GetOnlyClientV1,
    plan: &ClosedM1RequestPlan,
    deadline_ms: i64,
    stop: &StopHandle,
) -> HttpOutcome {
    execute_with_deadline(
        auth,
        client,
        plan,
        Deadline {
            at_ms: deadline_ms,
            late_as_timeout: true,
            evidence_only_late: false,
        },
        stop,
    )
    .await
}

pub(super) async fn execute_nonblocking_comparison(
    auth: &AuthLifecycle,
    client: &Stage8bP1fO2GetOnlyClientV1,
    plan: &ClosedM1RequestPlan,
    deadline_ms: i64,
    stop: &StopHandle,
) -> HttpOutcome {
    execute_with_deadline(
        auth,
        client,
        plan,
        Deadline {
            at_ms: deadline_ms,
            late_as_timeout: true,
            evidence_only_late: true,
        },
        stop,
    )
    .await
}

#[derive(Clone, Copy)]
struct Deadline {
    at_ms: i64,
    late_as_timeout: bool,
    evidence_only_late: bool,
}

impl Deadline {
    fn check_response(
        self,
        started: chrono::DateTime<chrono::Utc>,
        finished: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), RestFailure> {
        if finished < started {
            return Err(RestFailure::Integrity);
        }
        if finished.timestamp_millis() >= self.at_ms {
            return Err(if self.late_as_timeout {
                RestFailure::Timeout
            } else {
                RestFailure::Integrity
            });
        }
        Ok(())
    }
}

async fn execute_with_deadline(
    auth: &AuthLifecycle,
    client: &Stage8bP1fO2GetOnlyClientV1,
    plan: &ClosedM1RequestPlan,
    deadline: Deadline,
    stop: &StopHandle,
) -> HttpOutcome {
    let started = chrono::Utc::now().timestamp_millis();
    let attempts = AtomicU64::new(0);
    let deadline = Deadline {
        at_ms: deadline.at_ms.min(started.saturating_add(5000)),
        ..deadline
    };
    let deadline_ms = deadline.at_ms;
    let mut receipt = HttpReceipt {
        started_at_ms: started,
        finished_at_ms: started,
        deadline_ms,
        get_attempts: 0,
        auth_generation: None,
        body: None,
        executor_completed: true,
    };
    let result = execute_inner(auth, client, plan, deadline, stop, &attempts, &mut receipt).await;
    receipt.finished_at_ms = chrono::Utc::now().timestamp_millis();
    receipt.get_attempts = attempts.load(Ordering::SeqCst);
    if matches!(
        result,
        Err(RestFailure::Authentication
            | RestFailure::Integrity
            | RestFailure::Unknown
            | RestFailure::Cancelled)
    ) {
        stop.stop();
    }
    HttpOutcome { result, receipt }
}

async fn execute_inner(
    auth: &AuthLifecycle,
    client: &Stage8bP1fO2GetOnlyClientV1,
    plan: &ClosedM1RequestPlan,
    deadline: Deadline,
    stop: &StopHandle,
    attempts: &AtomicU64,
    receipt: &mut HttpReceipt,
) -> Result<AdmittedClosedM1Snapshot, RestFailure> {
    let deadline_ms = deadline.at_ms;
    if !auth.binds(client.account_id()) || plan.ranges().count() != 1 {
        return Err(RestFailure::Integrity);
    }
    let start = chrono::Utc::now();
    plan.preflight(start).map_err(|_| RestFailure::Integrity)?;
    let remaining = deadline_ms
        .checked_sub(start.timestamp_millis())
        .filter(|v| *v > 1)
        .ok_or(RestFailure::Timeout)?;
    let end = tokio::time::Instant::now() + Duration::from_millis(remaining as u64);
    let mut cancel = AuthCancellation {
        stop: stop.clone(),
        armed: true,
    };
    let grant = tokio::select! {
        biased;
        _=stop.cancelled()=>return Err(RestFailure::Cancelled),
        _=tokio::time::sleep_until(end)=>return Err(RestFailure::Cancelled),
        result=auth.grant_primary_budget(Duration::from_millis(remaining as u64-1))=>{
            match result{
                Ok(grant)=>grant,
                Err(error)=>{
                    cancel.armed=false;
                    return Err(match error.class{
                        PrimaryAuthClass::Unavailable=>RestFailure::Unavailable,
                        PrimaryAuthClass::Authentication=>RestFailure::Authentication,
                        PrimaryAuthClass::CancelledOrHeld=>RestFailure::Cancelled,
                        PrimaryAuthClass::Unknown=>RestFailure::Unknown,_=>RestFailure::Integrity,
                    });
                }
            }
        }
    };
    cancel.armed = false;
    receipt.auth_generation = Some(grant.generation());
    if stop.is_stopped() {
        return Err(RestFailure::Cancelled);
    }
    if !grant
        .grant
        .valid(chrono::Utc::now(), std::time::Instant::now())
    {
        return Err(RestFailure::Authentication);
    }
    let evidence = tokio::select! {
        biased;
        _=stop.cancelled()=>return Err(RestFailure::Cancelled),
        _=tokio::time::sleep_until(end)=>return Err(RestFailure::Timeout),
        value=client.fetch_closed_m1_primary(grant.grant.lease.token(),plan,attempts)=>value.map_err(get_failure)?,
    };
    classify_completed_body_for(
        client.shadow_instrument(),
        evidence,
        plan,
        start,
        chrono::Utc::now(),
        deadline,
        receipt,
    )
}

/// A completed body is intrinsically admitted BEFORE timing disposition. A
/// timer that wins without a complete body never enters this classifier.
#[cfg(test)]
fn classify_completed_body(
    evidence: broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1,
    plan: &ClosedM1RequestPlan,
    started: chrono::DateTime<chrono::Utc>,
    finished: chrono::DateTime<chrono::Utc>,
    deadline: Deadline,
    receipt: &mut HttpReceipt,
) -> Result<AdmittedClosedM1Snapshot, RestFailure> {
    classify_completed_body_for(
        broker_finam::ShadowInstrument::Imoexf,
        evidence,
        plan,
        started,
        finished,
        deadline,
        receipt,
    )
}
fn classify_completed_body_for(
    instrument: broker_finam::ShadowInstrument,
    evidence: broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1,
    plan: &ClosedM1RequestPlan,
    started: chrono::DateTime<chrono::Utc>,
    finished: chrono::DateTime<chrono::Utc>,
    deadline: Deadline,
    receipt: &mut HttpReceipt,
) -> Result<AdmittedClosedM1Snapshot, RestFailure> {
    if finished < started {
        return Err(RestFailure::Integrity);
    }
    let metadata = evidence.parts.first().map(|p| BodyReceipt {
        sha256: crate::seed::digest(p.raw_body.as_bytes()),
        bytes: p.raw_body.len(),
        received_at_ms: p.received_at.timestamp_millis(),
        intrinsic_admission_valid: false,
        raw_body_included: false,
    });
    let admitted = AdmittedClosedM1Snapshot::admit_for(instrument, evidence, plan, finished);
    if deadline.late_as_timeout || deadline.evidence_only_late {
        receipt.body = metadata.map(|mut m| {
            m.intrinsic_admission_valid = admitted.is_ok();
            m
        });
    }
    let source = admitted.map_err(|_| RestFailure::Integrity)?;
    if !deadline.evidence_only_late {
        deadline.check_response(started, finished)?;
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::ReadOnlyCollector;
    #[test]
    fn nonblocking_rest_completed_body_validation_precedes_deadline_disposition() {
        use broker_finam::sparse_m10::{
            ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1, OBSERVED_M1_POLICY_V1,
        };
        use serde_json::json;
        let label = 1793678400000;
        let at = |t| chrono::DateTime::from_timestamp_millis(t).unwrap();
        let start = at(label);
        let end = at(label + 600000);
        let plan = ClosedM1RequestPlan::new(start, end, vec![end]).unwrap();
        for v3 in [false, true] {
            for delay in [1850, 1900, 1901] {
                for mutation in ["valid", "malformed", "symbol", "ohlc"] {
                    let mut raw = json!({"symbol":"IMOEXF@RTSX","bars":(0..10).map(|i|crate::session::primary_tests::raw_row(label+i*60000)).collect::<Vec<_>>()});
                    if mutation == "symbol" {
                        raw["symbol"] = json!("OTHER@RTSX");
                    }
                    if mutation == "ohlc" {
                        raw["bars"][0]["high"] = json!({"value":"1"});
                    }
                    let raw = if mutation == "malformed" {
                        "{".to_owned()
                    } else {
                        raw.to_string()
                    };
                    let hash = crate::seed::digest(raw.as_bytes());
                    let evidence = ClosedM1SnapshotEvidenceV1 {
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
                            requested_at: at(label + 601000),
                            received_at: at(label + 600000 + delay),
                            declared_body_bytes: Some(raw.len() as u64),
                            response_sha256: hash.clone(),
                            raw_body: raw,
                            status: 200,
                            transport_complete: true,
                        }],
                    };
                    let mut receipt = HttpReceipt {
                        started_at_ms: label + 601000,
                        finished_at_ms: label + 600000 + delay,
                        deadline_ms: label + 601900,
                        get_attempts: 1,
                        auth_generation: Some(1),
                        body: None,
                        executor_completed: true,
                    };
                    let result = classify_completed_body(
                        evidence,
                        &plan,
                        at(label + 601000),
                        at(label + 600000 + delay),
                        Deadline {
                            at_ms: label + 601900,
                            late_as_timeout: true,
                            evidence_only_late: v3,
                        },
                        &mut receipt,
                    );
                    let expected = if mutation != "valid" {
                        Some(RestFailure::Integrity)
                    } else if delay >= 1900 && !v3 {
                        Some(RestFailure::Timeout)
                    } else {
                        None
                    };
                    assert_eq!(result.err(), expected, "v3={v3} {mutation} {delay}");
                    let body = receipt.body.unwrap();
                    assert_eq!(body.sha256, hash);
                    assert_eq!(body.intrinsic_admission_valid, mutation == "valid");
                    assert!(!body.raw_body_included);
                }
            }
        }
    }
    #[test]
    fn ws_first_deadline_classification_is_opt_in_and_clock_rollback_stays_integrity() {
        let at = |ms| chrono::DateTime::from_timestamp_millis(ms).unwrap();
        for late_as_timeout in [false, true] {
            let d = Deadline {
                at_ms: 1900,
                late_as_timeout,
                evidence_only_late: false,
            };
            assert_eq!(d.check_response(at(1000), at(1899)), Ok(()));
            for finished in [1900, 1901, 2000] {
                assert_eq!(
                    d.check_response(at(1000), at(finished)),
                    Err(if late_as_timeout {
                        RestFailure::Timeout
                    } else {
                        RestFailure::Integrity
                    })
                );
            }
            assert_eq!(
                d.check_response(at(1000), at(999)),
                Err(RestFailure::Integrity)
            );
            assert_eq!(
                d.check_response(
                    at(1000) + chrono::Duration::microseconds(900),
                    at(1000) + chrono::Duration::microseconds(100)
                ),
                Err(RestFailure::Integrity)
            );
        }
    }
    #[tokio::test]
    async fn binding_and_stop_refuse_before_auth_or_get() {
        let auth = AuthLifecycle::new(
            broker_finam::SecretToken::new("not-real"),
            "selected".into(),
        )
        .unwrap();
        let client = Stage8bP1fO2GetOnlyClientV1::new("different").unwrap();
        let now = chrono::Utc::now();
        let end = chrono::DateTime::from_timestamp(now.timestamp().div_euclid(60) * 60, 0).unwrap();
        let plan =
            ClosedM1RequestPlan::new(end - chrono::Duration::minutes(10), end, vec![end]).unwrap();
        let (stop, _) =
            ReadOnlyCollector::new(now + chrono::Duration::seconds(60), Duration::from_secs(1))
                .unwrap();
        let result = execute(&auth, &client, &plan, now.timestamp_millis() + 1000, &stop).await;
        assert!(matches!(result.result, Err(RestFailure::Integrity)));
        assert_eq!(result.receipt.get_attempts, 0);
    }
    #[test]
    fn classification_preserves_security_integrity_and_unknown() {
        assert_eq!(
            get_failure(PrimaryGetFailure::Authentication),
            RestFailure::Authentication
        );
        assert_eq!(
            get_failure(PrimaryGetFailure::Integrity),
            RestFailure::Integrity
        );
        assert_eq!(
            get_failure(PrimaryGetFailure::Unknown),
            RestFailure::Unknown
        );
        assert_eq!(
            get_failure(PrimaryGetFailure::Timeout),
            RestFailure::Timeout
        );
    }

    #[tokio::test]
    async fn stopped_or_expired_command_never_starts_auth_or_http() {
        for stopped in [false, true] {
            let auth = AuthLifecycle::new(
                broker_finam::SecretToken::new("not-real"),
                "selected".into(),
            )
            .unwrap();
            let client = Stage8bP1fO2GetOnlyClientV1::new("selected").unwrap();
            let now = chrono::Utc::now();
            let end =
                chrono::DateTime::from_timestamp(now.timestamp().div_euclid(60) * 60, 0).unwrap();
            let plan =
                ClosedM1RequestPlan::new(end - chrono::Duration::minutes(10), end, vec![end])
                    .unwrap();
            let (stop, _) =
                ReadOnlyCollector::new(now + chrono::Duration::seconds(60), Duration::from_secs(1))
                    .unwrap();
            if stopped {
                stop.stop();
            }
            let result = execute(
                &auth,
                &client,
                &plan,
                now.timestamp_millis() + if stopped { 1000 } else { 0 },
                &stop,
            )
            .await;
            assert_eq!(
                result.result.err().unwrap(),
                if stopped {
                    RestFailure::Cancelled
                } else {
                    RestFailure::Timeout
                }
            );
            assert_eq!(result.receipt.get_attempts, 0);
            assert_eq!(result.receipt.auth_generation, None);
            assert_eq!(
                auth.primary_readiness().await.status,
                crate::auth::PrimaryAuthStatus::NotIssued
            );
        }
    }
}
