//! Test-only deterministic Published-window -> runtime barrier. No production
//! environment/clock/source override. The Linux harness uses real Unix signals.
use super::*;
use std::{os::unix::net::UnixDatagram, path::PathBuf};

pub(super) struct HandoffBarrier {
    gate: Option<tokio::net::UnixDatagram>,
    events: Option<UnixDatagram>,
}

impl HandoffBarrier {
    pub(super) fn new() -> Self {
        let Some(directory) = std::env::var_os("STAGE8B_O301_BARRIER_DIRECTORY") else {
            return Self {
                gate: None,
                events: None,
            };
        };
        let directory = PathBuf::from(directory);
        let gate = tokio::net::UnixDatagram::bind(directory.join("release.sock")).unwrap();
        let events = UnixDatagram::unbound().unwrap();
        events.connect(directory.join("events.sock")).unwrap();
        // Called only AFTER obtaining the real typed Published pair and BEFORE
        // constructing/polling the ordinary runtime entrypoint.
        events
            .send(
                serde_json::json!({
                    "event": "PUBLISHED_WINDOW_READY", "pid": std::process::id(),
                })
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        Self {
            gate: Some(gate),
            events: Some(events),
        }
    }

    pub(super) async fn release(&self) {
        if let Some(gate) = &self.gate {
            let mut bytes = [0; 32];
            let count = gate.recv(&mut bytes).await.unwrap();
            assert_eq!(&bytes[..count], b"RELEASE");
            self.events
                .as_ref()
                .unwrap()
                .send(b"{\"event\":\"RUNTIME_RELEASED\"}")
                .unwrap();
        }
    }

    pub(super) fn accepted(&self, latch: &durable::Stage8bP1eShutdownLatchV1, signal: &str) {
        if let Some(events) = &self.events {
            let intent = latch
                .intent()
                .expect("P1-O301: stop accepted without retained runtime intent");
            assert_eq!(
                intent.cause(),
                durable::Stage8bP1eShutdownCauseV1::ExternalSignal
            );
            events
                .send(
                    serde_json::json!({
                        "event": "STOP_ACCEPTED", "signal": signal,
                        "first_sequence": intent.first_request_sequence(),
                        "grace_deadline_utc_ms": intent.grace_deadline_utc_ms(),
                        "cause": "ExternalSignal",
                    })
                    .to_string()
                    .as_bytes(),
                )
                .unwrap();
        }
    }
}

pub(super) fn disconnect_latch_mutation() -> bool {
    std::env::var("STAGE8B_O301_DISCONNECT_LATCH").as_deref() == Ok("MUTATION")
}

fn early_signal_child() {
    let phase = std::env::var("STAGE8B_O301_PHASE_SHA256").unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(8 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(run_fixed(&phase));
    assert!(
        matches!(
            result,
            Err(O3Error::Runtime(
                durable::Stage8bP1eProcessErrorV1::DurableRestart
            ))
        ),
        "early stop must reject before runtime admission: {result:?}"
    );
    println!("O301_ZERO_ADMISSION_STOP runtime_exit_class=66");
}

#[test]
#[ignore = "isolated Linux real-Published-window process harness"]
fn o3_early_sigterm_process_child() {
    early_signal_child();
}

#[test]
#[ignore = "isolated Linux real-Published-window process harness"]
fn o3_early_sigint_process_child() {
    early_signal_child();
}

#[test]
fn o3_shutdown_handoff_preserves_first_cause_and_deadline() {
    let coordinator = durable::Stage8bP1eCoordinatorV1::new();
    let latch = coordinator.shutdown_latch();
    retain_shutdown(
        &latch,
        30_000,
        durable::Stage8bP1eShutdownCauseV1::ExternalSignal,
    );
    let first = latch.intent().unwrap().clone();
    retain_shutdown(
        &latch,
        90_000,
        durable::Stage8bP1eShutdownCauseV1::OwnerFailure,
    );
    assert_eq!(latch.intent(), Some(&first));
    assert_eq!(coordinator.shutdown_intent(), Some(&first));
    assert_eq!(first.first_request_sequence(), 1);
}

// Root-only Linux harness around a nonroot, real run_fixed child. These exact
// selectors are invoked explicitly by the isolated local qualification script.
// No production binary contains this barrier or the disconnected-latch mutant.
#[cfg(target_os = "linux")]
mod process {
    use super::*;
    use std::{
        collections::BTreeMap,
        os::unix::{fs::PermissionsExt, process::CommandExt},
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    fn durable_snapshot(parent: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut result: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for entry in fs::read_dir(parent).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap();
            if name.starts_with("stage7b-") && path.is_dir() {
                for entry in fs::read_dir(&path).unwrap() {
                    let p = entry.unwrap().path();
                    assert!(p.is_file(), "unexpected durable-root member");
                    result.insert(
                        p.strip_prefix(parent).unwrap().to_str().unwrap().into(),
                        fs::read(&p).unwrap(),
                    );
                }
            }
        }
        for name in [
            ".stage8b-p1-first-boot-receipt-v2.json",
            ".stage8b-p1-first-boot-transaction-v4.json",
        ] {
            result.insert(name.into(), fs::read(parent.join(name)).unwrap());
        }
        assert!(result.keys().any(|n| n.ends_with("stage7b-recovery.seal")));
        assert!(result.keys().any(|n| n.ends_with("stage6.journal")));
        result
    }

    async fn redis_snapshot() -> BTreeMap<String, String> {
        let mut db = redis::Client::open("redis://127.0.0.1:6379/15")
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .unwrap();
        let ns = durable::stage8b_p1_redis_namespace();
        let mut result = BTreeMap::new();
        for (label, stream) in [
            ("m10", ns.canonical_m10_stream.as_str()),
            ("commands", ns.canonical_command_stream.as_str()),
        ] {
            let rows: redis::Value = redis::cmd("XRANGE")
                .arg(stream)
                .arg("-")
                .arg("+")
                .query_async(&mut db)
                .await
                .unwrap();
            let groups: redis::Value = redis::cmd("XINFO")
                .arg("GROUPS")
                .arg(stream)
                .query_async(&mut db)
                .await
                .unwrap();
            result.insert(label.into(), format!("{rows:?}"));
            result.insert(format!("{label}-groups"), format!("{groups:?}"));
        }
        let info: String = redis::cmd("INFO")
            .arg("commandstats")
            .query_async(&mut db)
            .await
            .unwrap();
        // Only calls, not server CPU microseconds: prove no runtime source
        // acquisition/XACK command ran even if it would have returned no rows.
        for cmd in ["xack", "xreadgroup", "xautoclaim"] {
            let prefix = format!("cmdstat_{cmd}:calls=");
            let count = info
                .lines()
                .find_map(|line| line.strip_prefix(&prefix))
                .map(|s| s.split(',').next().unwrap())
                .unwrap_or("0");
            result.insert(format!("{cmd}-calls"), count.into());
        }
        result
    }

    fn event(socket: &UnixDatagram, child: &mut Child) -> serde_json::Value {
        let mut b = [0; 2048];
        let deadline = Instant::now() + Duration::from_secs(1000);
        let n = loop {
            match socket.recv(&mut b) {
                Ok(n) => break n,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    assert!(
                        child.try_wait().unwrap().is_none(),
                        "child exited before barrier event"
                    );
                    assert!(Instant::now() < deadline, "barrier event deadline expired");
                }
                Err(error) => panic!("barrier event: {error}"),
            }
        };
        let value: serde_json::Value = serde_json::from_slice(&b[..n]).unwrap();
        println!("O301_EVENT={value}");
        value
    }

    async fn case(signal: i32, mutation: bool) {
        assert_eq!(
            unsafe { libc::geteuid() },
            0,
            "isolated Linux root harness only"
        );
        let signal_name = if signal == libc::SIGTERM {
            "SIGTERM"
        } else {
            "SIGINT"
        };
        let label = format!(
            "{}-{}",
            signal_name,
            if mutation { "disconnected" } else { "retained" }
        );
        let base = PathBuf::from("/run/o3-public/early-stop-evidence");
        fs::create_dir_all(&base).unwrap();
        let directory = base.join(&label);
        fs::create_dir(&directory).unwrap();
        use std::os::unix::fs::chown;
        chown(&directory, Some(987), Some(987)).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let events = UnixDatagram::bind(directory.join("events.sock")).unwrap();
        fs::set_permissions(
            directory.join("events.sock"),
            fs::Permissions::from_mode(0o666),
        )
        .unwrap();
        events
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        let name = if signal == libc::SIGTERM {
            "o3_early_sigterm_process_child"
        } else {
            "o3_early_sigint_process_child"
        };
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                &format!("stage8b_p1f_o3_composition::shutdown_tests::{name}"),
                "--nocapture",
            ])
            .env("STAGE8B_O301_BARRIER_DIRECTORY", &directory)
            .env_remove("STAGE8B_O301_DISCONNECT_LATCH")
            .env("RUST_MIN_STACK", "33554432")
            .stdin(Stdio::null())
            .stdout(fs::File::create(directory.join("child.stdout")).unwrap())
            .stderr(fs::File::create(directory.join("child.stderr")).unwrap());
        if mutation {
            command.env("STAGE8B_O301_DISCONNECT_LATCH", "MUTATION");
        }
        unsafe {
            command.pre_exec(|| {
                if libc::setgroups(0, std::ptr::null()) != 0
                    || libc::setgid(987) != 0
                    || libc::setuid(987) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let initial = durable_snapshot(Path::new("/var/lib/moex-finam-p1-paper/state"));
        let mut child = OwnedChild(command.spawn().unwrap());
        let ready = event(&events, &mut child.0);
        assert_eq!(ready["event"], "PUBLISHED_WINDOW_READY");
        assert_eq!(ready["pid"], child.0.id());
        let published = redis_snapshot().await;
        let producer_before =
            fs::read("/var/lib/moex-finam-p1-paper-o3-producer/schedule-high-water.json").unwrap();
        let durable_before = durable_snapshot(Path::new("/var/lib/moex-finam-p1-paper/state"));
        assert_eq!(
            initial, durable_before,
            "window preparation changed runtime/first-boot state"
        );
        assert_eq!(unsafe { libc::kill(child.0.id() as i32, signal) }, 0);
        let mut accepted = serde_json::Value::Null;
        if !mutation {
            accepted = event(&events, &mut child.0);
            assert_eq!(accepted["event"], "STOP_ACCEPTED");
            assert_eq!(accepted["signal"], signal_name);
            assert_eq!(accepted["first_sequence"], 1);
            // Ordering oracle is receipt of STOP_ACCEPTED, not elapsed time.
            events
                .send_to(b"RELEASE", directory.join("release.sock"))
                .unwrap();
            assert_eq!(event(&events, &mut child.0)["event"], "RUNTIME_RELEASED");
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "child did not stop without force-kill"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        let stdout = fs::read_to_string(directory.join("child.stdout")).unwrap();
        let stderr = fs::read_to_string(directory.join("child.stderr")).unwrap();
        if mutation {
            assert_eq!(status.code(), Some(101));
            assert!(stderr.contains("P1-O301: stop accepted without retained runtime intent"));
        } else {
            assert_eq!(status.code(), Some(0));
            assert!(stdout.contains("O301_ZERO_ADMISSION_STOP runtime_exit_class=66"));
        }
        let after = durable_snapshot(Path::new("/var/lib/moex-finam-p1-paper/state"));
        assert_eq!(durable_before, after, "root/journal/bootstrap mutation");
        let after_redis = redis_snapshot().await;
        assert_eq!(
            published, after_redis,
            "source acquisition/command/XACK mutation"
        );
        assert_eq!(
            producer_before,
            fs::read("/var/lib/moex-finam-p1-paper-o3-producer/schedule-high-water.json").unwrap(),
            "published producer checkpoint reset or changed"
        );
        let hashes: BTreeMap<_, _> = after.iter().map(|(n, b)| (n, inputs::hash(b))).collect();
        let evidence = serde_json::json!({
            "signal": signal_name, "disconnected_latch_mutation": mutation,
            "child_exit_code": status.code(), "stop_accepted_before_release": !mutation,
            "accepted": accepted, "durable_root_and_bootstrap_unchanged": true,
            "durable_sha256": hashes, "redis_before": published, "redis_after": after_redis,
            "producer_checkpoint_sha256": inputs::hash(&producer_before),
            "producer_checkpoint_retained": true,
            "fresh_runtime_admission": false, "callback_effects": 0, "force_kill": false,
            "scope": "LOCAL_TEST_PROCESS_REAL_PUBLISHED_WINDOW_NOT_VPS",
        });
        fs::write(
            directory.join("result.json"),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        println!("O301_CASE_PASS {label} child_exit={:?}", status.code());
    }

    #[tokio::test]
    #[ignore = "isolated Linux harness"]
    async fn o3_sigterm_before_runtime_start_is_retained() {
        case(libc::SIGTERM, false).await;
    }
    #[tokio::test]
    #[ignore = "isolated Linux harness"]
    async fn o3_sigint_before_runtime_start_is_retained() {
        case(libc::SIGINT, false).await;
    }
    #[tokio::test]
    #[ignore = "isolated Linux harness"]
    async fn o3_sigterm_disconnected_latch_mutation_is_detected() {
        case(libc::SIGTERM, true).await;
    }
    #[tokio::test]
    #[ignore = "isolated Linux harness"]
    async fn o3_sigint_disconnected_latch_mutation_is_detected() {
        case(libc::SIGINT, true).await;
    }
}
