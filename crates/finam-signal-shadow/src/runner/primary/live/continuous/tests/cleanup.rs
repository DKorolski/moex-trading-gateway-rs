//! Concrete stop/drain/finalizer faults. No provider, credentials or real units.
use super::*;
use crate::primary::calendar::tests::{at, Dir, Fixture};

#[derive(Default)]
struct Counts {
    started: [AtomicUsize; 3],
    exited: [AtomicUsize; 3],
}
struct Exit(Arc<Counts>, usize);
impl Exit {
    fn enter(c: Arc<Counts>, i: usize) -> Self {
        c.started[i].fetch_add(1, Ordering::SeqCst);
        Self(c, i)
    }
}
impl Drop for Exit {
    fn drop(&mut self) {
        self.0.exited[self.1].fetch_add(1, Ordering::SeqCst);
    }
}
async fn unused_fetch(_job: Job, _stop: StopHandle) -> HttpOutcome {
    panic!("cleanup must not dispatch a new GET")
}
fn ticket(f: &Fixture, audit: bool) -> RestTicket {
    RestTicket {
        owner: Arc::new(()),
        label_ms: f.open(),
        reason: if audit {
            None
        } else {
            Some(ExceptionReason::BoundaryUnproven)
        },
        issued_at_ms: f.open(),
        deadline_ms: f.open() + 5000,
    }
}
fn cancelled(job: &Job) -> HttpOutcome {
    HttpOutcome {
        result: Err(RestFailure::Cancelled),
        receipt: HttpReceipt {
            started_at_ms: crate::timing::now_ms(),
            finished_at_ms: crate::timing::now_ms(),
            deadline_ms: job.deadline_ms(),
            get_attempts: 0,
            auth_generation: Some(1),
            body: None,
            executor_completed: true,
        },
    }
}

#[test]
fn stop_faults_still_visit_http_socket_and_writer_and_never_make_unknown_eod_pass() {
    for mode in [
        "normal",
        "http-panic",
        "no-receipt",
        "http-timeout",
        "socket-panic",
        "final-receipt-write",
    ] {
        let mut f = Fixture::new();
        f.config.base.stop_timeout_ms = if mode == "http-timeout" { 100 } else { 1000 };
        let d = Dir::new();
        let path = d.0.join("journal");
        with_fixture_clock(f.open(), || {
            runtime(async {
                let counts = Arc::new(Counts::default());
                let mut owner = f.owner(&path, false);
                let cal = f.cap(crate::timing::now_ms()).unwrap();
                let reference = at(cal.reference_start_ms());
                let calendar_sha = cal.sha256().to_owned();
                owner.bind_calendar(cal).unwrap();
                let (config, root) = owner.session.primary_runtime().unwrap();
                let config_sha = config.fingerprint().to_owned();
                if mode == "final-receipt-write" {
                    owner.session.fail_primary_evidence("transport_event");
                }
                let gate = ContinuityGate::new(1);
                let faults = Faults::default();
                let slot = FailureSlot::with_gate(gate.clone(), root.clone());
                let writer = Writer::spawn_mode(
                    owner,
                    reference,
                    root.clone(),
                    faults.clone(),
                    slot.clone(),
                    Some(gate.clone()),
                )
                .unwrap();
                let c = counts.clone();
                let connection = Connection::open(
                    1,
                    f.config.base.run_deadline_utc_ms,
                    f.config.base.run_deadline_utc_ms,
                    root.clone(),
                    slot,
                    &f.config.primary,
                    move |request: Open| {
                        let c = c.clone();
                        async move {
                            let _exit = Exit::enter(c, 2);
                            request.stop.cancelled().await;
                            if mode == "socket-panic" {
                                panic!("fixture-only socket stop fault");
                            }
                            Err(Failure {
                                generation: 1,
                                kind: crate::socket::primary::Kind::Cancelled,
                                liveness: None,
                            })
                        }
                    },
                )
                .unwrap();
                let mut scope = Scope::new(root.clone(), faults.clone());
                scope.socket = connection.worker.as_ref().map(|h| h.abort_handle());
                let critical_job = Job::Proof(ticket(&f, false));
                let audit_job = Job::Audit(ticket(&f, true));
                let child = connection.stop.clone();
                let c = counts.clone();
                let request = critical_job.clone();
                let critical = tokio::spawn(async move {
                    let _exit = Exit::enter(c, 0);
                    if mode == "http-timeout" {
                        std::future::pending::<()>().await;
                    }
                    child.cancelled().await;
                    if mode == "http-panic" {
                        panic!("fixture-only HTTP stop fault");
                    }
                    if mode == "no-receipt" {
                        None
                    } else {
                        Some(cancelled(&request))
                    }
                });
                let child = connection.stop.clone();
                let c = counts.clone();
                let request = audit_job.clone();
                let audit = tokio::spawn(async move {
                    let _exit = Exit::enter(c, 1);
                    child.cancelled().await;
                    Some(cancelled(&request))
                });
                scope.critical = Some(critical.abort_handle());
                scope.audit = Some(audit.abort_handle());
                let mut totals = HttpTotals::default();
                totals.proof.scheduled = 1;
                totals.audit.scheduled = 1;
                let engine = Engine {
                    writer,
                    connection,
                    fetch: unused_fetch,
                    root: root.clone(),
                    faults,
                    gate,
                    scope,
                    policy: f.config.primary.clone(),
                    deadline: Instant::now() + Duration::from_secs(60),
                    run_deadline_ms: f.config.base.run_deadline_utc_ms,
                    eod_at: None,
                    eod_ms: Some(f.binding.eod_check_at_ms),
                    config_sha256: config_sha,
                    calendar_sha256: Some(calendar_sha),
                    clock_uncertainty_ms: f.config.base.clock_uncertainty_ms,
                    critical: Some(HttpJob {
                        job: critical_job,
                        worker: critical,
                    }),
                    audit: Some(HttpJob {
                        job: audit_job,
                        worker: audit,
                    }),
                    totals,
                    data: None,
                    bootstrap_pending: false,
                    wake: Instant::now() + Duration::from_secs(1),
                    committed: 0,
                    recovery: vec![],
                    socket_report: None,
                    all_connections_drained: true,
                    drained_http: vec![],
                };
                until(|| counts.started.iter().all(|c| c.load(Ordering::SeqCst) == 1)).await;
                let result = engine.finish(Cause::RequestedStop).await;
                assert!(root.is_stopped());
                // A timed-out/aborted worker is NOT upgraded to drained after the
                // fact. Still wait here to avoid leaking fixture tasks/threads.
                until(|| counts.exited.iter().all(|c| c.load(Ordering::SeqCst) == 1)).await;
                if mode == "final-receipt-write" {
                    // A journal whose write failed is terminal/unknown; it
                    // cannot mint a synthetic successful segment_end receipt.
                    assert!(!trace(&path).iter().any(|r| r["kind"] == "segment_end"));
                } else {
                    until(|| trace(&path).iter().any(|r| r["kind"] == "segment_end")).await;
                }
                assert_eq!(result.canonical_commits, 0);
                assert_eq!(result.http.audit.completed, 1);
                assert_eq!(result.http.audit.cancelled_or_unaccounted, 0);
                assert_eq!(
                    result.http.proof.completed + result.http.proof.cancelled_or_unaccounted,
                    1
                );
                assert!(!result.completion.as_ref().unwrap().semantic_eod_passed);
                assert!(!result.full_session_qualified);
                if mode == "http-timeout" {
                    assert!(!result.http_drained && !result.all_workers_drained);
                } else if mode == "socket-panic" {
                    assert!(!result.socket_drained && !result.all_workers_drained);
                    assert!(result.http_drained && result.writer_drained);
                } else {
                    assert!(result.http_drained && result.socket_drained && result.writer_drained);
                }
                if mode == "final-receipt-write" {
                    assert!(!result.session_finalized);
                }
                if matches!(mode, "http-panic" | "no-receipt" | "http-timeout") {
                    assert_eq!(result.http.proof.cancelled_or_unaccounted, 1);
                }
                emit(mode, &result, &counts);
            })
        });
    }
}
fn emit(mode: &str, report: &RunReport, counts: &Counts) {
    if let Some(base) = std::env::var_os("WS_PRIMARY_T39_STOP_EVIDENCE_DIR") {
        use std::io::Write;
        let dir = std::path::Path::new(&base).join(format!("run-{}-{mode}", std::process::id()));
        std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
        std::fs::create_dir(&dir).unwrap();
        let value = json!({"scenario":"T39","mode":mode,"report":report,
            "started":counts.started.iter().map(|c|c.load(Ordering::SeqCst)).collect::<Vec<_>>(),
            "exited":counts.exited.iter().map(|c|c.load(Ordering::SeqCst)).collect::<Vec<_>>(),
            "source_callbacks":0,"scope":"LOCAL_STOP_WORKERS_NOT_FINAM_OR_CGROUP","operational_qualified":false});
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join("result.json"))
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&value).unwrap())
            .unwrap();
        file.sync_all().unwrap();
        println!("T39_STOP_EVIDENCE_DIR={}", dir.display());
    }
}
