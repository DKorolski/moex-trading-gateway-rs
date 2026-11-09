use super::*;
use crate::{
    primary::{
        calendar::tests::{Dir, Fixture},
        PrimaryProcessor,
    },
    seed::digest,
    segment::SeededSegment,
    timing::{advance_fixture_clock, now_ms, with_fixture_clock},
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(crate) fn members(
    f: &Fixture,
    d: &Dir,
    count: usize,
    root: &StopHandle,
) -> (Vec<ObservedSession>, Vec<PathBuf>) {
    let paths: Vec<_> = (0..count)
        .map(|i| d.0.join(format!("runtime-{i}.jsonl")))
        .collect();
    let sessions = paths
        .iter()
        .map(|path| {
            let segment = SeededSegment::from_inputs(
                ValidatedConfig::from_ws_primary(&f.config).unwrap(),
                &f.materials[0],
                &f.materials[1],
                &f.materials[2],
                &f.materials[3],
            )
            .unwrap();
            ObservedSession::create(segment, path, root.clone()).unwrap()
        })
        .collect();
    (sessions, paths)
}
pub(crate) fn group(f: &Fixture, d: &Dir, count: usize) -> (Coordinator, StopHandle, Vec<PathBuf>) {
    let root = StopHandle::default();
    let (sessions, paths) = members(f, d, count, &root);
    let processor = PrimaryProcessor::from_config(
        &f.config,
        crate::primary::calendar::tests::instrument(),
        f.model.required_m10_labels_ms.clone(),
        1,
        now_ms(),
    )
    .unwrap();
    let owner = Coordinator::new_group(processor, sessions).unwrap();
    (owner, root, paths)
}
pub(crate) fn trace(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .split_inclusive('\n')
        .filter(|s| s.ends_with('\n'))
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
pub(crate) fn count(path: &Path, kind: &str) -> usize {
    trace(path).iter().filter(|r| r["kind"] == kind).count()
}
fn go(ms: i64) {
    advance_fixture_clock((ms - now_ms()).try_into().unwrap());
}
fn bind(f: &Fixture, owner: &mut Coordinator) -> crate::socket::primary::Subscription {
    owner.bind_calendar(f.cap(now_ms()).unwrap()).unwrap();
    let sub = crate::socket::primary::fixture_subscription(1, now_ms());
    owner.bind_subscription(&sub).unwrap();
    sub
}
fn frame(
    f: &Fixture,
    owner: &mut Coordinator,
    sub: &crate::socket::primary::Subscription,
    label: i64,
) {
    let text = json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"cohort",
        "payload":{"symbol":"IMOEXF@RTSX","timeframe":"TIME_FRAME_M1","bars":[f.row(label)]}})
    .to_string();
    let mut frame = crate::primary::WireFrame::normalize(
        &text,
        1,
        "cohort",
        chrono::DateTime::from_timestamp_millis(now_ms()).unwrap(),
    )
    .unwrap();
    frame.owner = Some(sub.owner.clone());
    owner.on_frame(frame).unwrap();
}
fn ready(f: &Fixture, owner: &mut Coordinator, sub: &crate::socket::primary::Subscription) {
    for i in 0..=10 {
        let label = f.open() + i * 60000;
        go(label + 100);
        frame(f, owner, sub, label);
    }
    go(f.open() + 601100);
}

#[test]
fn shared_feed_delivers_once_to_each_model_and_schedules_proof_and_audit_once() {
    let mut f = Fixture::new();
    f.config.primary.audit_every_commits = Some(6);
    let d = Dir::new();
    with_fixture_clock(f.open(), || {
        let (mut owner, _, paths) = group(&f, &d, 3);
        let sub = bind(&f, &mut owner);
        // Six dense M10, one common audit slot, then one exception proof.
        for minute in 0..=60 {
            let label = f.open() + minute * 60000;
            go(label + 100);
            frame(&f, &mut owner, &sub, label);
            frame(&f, &mut owner, &sub, label); // retransmission does not fan out callbacks
            if minute > 0 && minute % 10 == 0 {
                go(label + 1100);
                assert!(matches!(
                    owner.tick().unwrap(),
                    Tick::Published {
                        origin: "WS_LIVE",
                        ..
                    }
                ));
                assert!(matches!(owner.tick().unwrap(), Tick::Waiting));
            }
        }
        let audit = owner.take_audit().unwrap().unwrap();
        assert!(owner.take_audit().unwrap().is_none());
        let audit_source = f.rest(&audit.request_plan().unwrap());
        assert_eq!(
            owner.complete_audit(audit, Ok(audit_source)).unwrap(),
            AuditStatus::Match
        );
        for i in 1..10 {
            let label = f.open() + 3600000 + i * 60000;
            go(label + 100);
            frame(&f, &mut owner, &sub, label);
        }
        go(f.open() + 4203100);
        let Tick::ProofRequired(ticket) = owner.tick().unwrap() else {
            panic!("one proof")
        };
        assert!(matches!(owner.tick().unwrap(), Tick::AwaitingProof));
        let source = f.rest(&ticket.request_plan().unwrap());
        assert!(matches!(
            owner.complete_proof(ticket.clone(), Ok(source)).unwrap(),
            Tick::Published {
                origin: "REST_EXCEPTION",
                ..
            }
        ));
        let source = f.rest(&ticket.request_plan().unwrap());
        assert_eq!(
            owner.complete_proof(ticket, Ok(source)).unwrap_err(),
            CoordinatorError::Ticket
        );
        assert_eq!(owner.processor().counters().committed, 7);
        assert_eq!(owner.processor().next_label_ms(), Some(f.open() + 4200000));
        assert_eq!(owner.role_counts().proof_scheduled, 1);
        assert_eq!(owner.role_counts().audit_scheduled, 1);
        for path in &paths {
            assert_eq!(count(path, "model_step"), 7);
            assert_eq!(count(path, "canonical_commit"), 7);
            let barriers: Vec<_> = trace(path)
                .into_iter()
                .filter(|r| r["event"]["event"] == "primary_runtime_cohort_barrier")
                .collect();
            assert_eq!(barriers.len(), 7);
            assert!(barriers.iter().all(|r| r["event"]["consumer_commits"]
                .as_array()
                .unwrap()
                .len()
                == 3));
        }
        let states: Vec<_> = paths
            .iter()
            .map(|p| {
                trace(p)
                    .into_iter()
                    .filter(|r| r["kind"] == "model_step")
                    .map(|r| r["event"]["after"].clone())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(states[0], states[1]);
        assert_eq!(states[1], states[2]);
        // No primary-only seal or restart may silently stand in for the cohort.
        assert!(!owner.session.eod_model_ready());
        assert!(owner.session.journal_seal().is_none());
        owner
            .session
            .finish("BOUNDED_COHORT_NOT_AGGREGATE_EOD")
            .unwrap();
        let bytes = std::fs::read(&paths[0]).unwrap();
        let mut next = f.config.clone();
        next.base.previous_segment_id = Some(f.config.base.segment_id.clone());
        next.base.segment_id = "cohort-next".into();
        let binding = crate::primary::restart::RestartBinding {
            schema_version: 1,
            config_sha256: next.fingerprint().unwrap(),
            previous_config_sha256: f.config.fingerprint().unwrap(),
            previous_journal_sha256: digest(&bytes),
            previous_segment_id: f.config.base.segment_id.clone(),
            required_labels_sha256: crate::primary::restart::inventory_sha256(
                &f.model.required_m10_labels_ms,
            ),
            committed_labels: 7,
            max_reconstruction_ms: 10000,
        };
        assert!(matches!(
            crate::primary::restart::admit(
                &next,
                binding,
                &bytes,
                &f.model.required_m10_labels_ms,
                now_ms()
            ),
            Err(crate::primary::restart::RestartError::Incomplete)
        ));
    });
}

#[test]
fn any_consumer_write_failure_holds_shared_frontier_without_callback_retry() {
    for (kind, steps) in [
        ("canonical_source", [1, 0, 0]),
        ("canonical_commit", [1, 1, 0]),
    ] {
        let f = Fixture::new();
        let d = Dir::new();
        with_fixture_clock(f.open(), || {
            let (mut owner, root, paths) = group(&f, &d, 3);
            let sub = bind(&f, &mut owner);
            owner.session.members[1].fail_primary_evidence(kind);
            ready(&f, &mut owner, &sub);
            assert!(owner.tick().is_err());
            assert!(root.is_stopped());
            assert_eq!(owner.processor().counters().committed, 0);
            assert_eq!(owner.processor().next_label_ms(), Some(f.open()));
            assert_eq!(owner.tick().unwrap_err(), CoordinatorError::Held);
            for (path, expected) in paths.iter().zip(steps) {
                assert_eq!(count(path, "model_step"), expected);
            }
            // Terminal export still visits runtime 2 after runtime 1 fails.
            assert!(owner.session.finish("INCOMPLETE_cohort_failure").is_err());
            assert_eq!(count(&paths[2], "segment_end"), 1);
        });
    }
}

#[test]
fn shared_publication_barrier_preserves_original_lmax_and_root_stop() {
    for fault in [(9000, false), (0, true)] {
        let f = Fixture::new();
        let d = Dir::new();
        with_fixture_clock(f.open(), || {
            let (mut owner, root, paths) = group(&f, &d, 2);
            let sub = bind(&f, &mut owner);
            ready(&f, &mut owner, &sub);
            owner.session.barrier_fault = Some(fault);
            assert!(owner.tick().is_err());
            assert!(root.is_stopped());
            assert_eq!(owner.processor().counters().committed, 0);
            assert_eq!(owner.tick().unwrap_err(), CoordinatorError::Held);
            for path in &paths {
                assert_eq!(count(path, "model_step"), 1);
            }
        });
    }
}

#[test]
fn cohort_rejects_empty_oversized_foreign_stop_and_mismatched_config_or_frontier() {
    assert!(Runtimes::group(vec![]).is_err());
    for mode in ["size", "stop", "config", "frontier"] {
        let mut f = Fixture::new();
        let d = Dir::new();
        with_fixture_clock(f.open(), || {
            let root = StopHandle::default();
            if mode == "size" {
                let (sessions, _) = members(&f, &d, 9, &root);
                assert!(Runtimes::group(sessions).is_err());
                return;
            }
            let (mut sessions, _) = members(&f, &d, 1, &root);
            let other = Dir::new();
            if mode == "config" {
                f.config.base.run_id = "different-config".into();
            }
            let other_root = if mode == "stop" {
                StopHandle::default()
            } else {
                root.clone()
            };
            let (mut follower, _) = members(&f, &other, 1, &other_root);
            if mode == "frontier" {
                // Advance this otherwise identical follower through a real
                // callback, never a state/frontier setter.
                let p = PrimaryProcessor::from_config(
                    &f.config,
                    crate::primary::calendar::tests::instrument(),
                    f.model.required_m10_labels_ms.clone(),
                    1,
                    now_ms(),
                )
                .unwrap();
                let mut one = Coordinator::new(p, follower.pop().unwrap()).unwrap();
                let sub = bind(&f, &mut one);
                ready(&f, &mut one, &sub);
                assert!(matches!(one.tick().unwrap(), Tick::Published { .. }));
                follower.push(one.session.members.remove(0));
            }
            sessions.extend(follower);
            assert!(Runtimes::group(sessions).is_err(), "{mode}");
        });
    }
}
