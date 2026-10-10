use super::*;
use crate::stage8b_p1_semantic::observed::sync_test::{self, Point};

#[tokio::test]
async fn o3_existing_proof_sync_retry_orders_durability_before_request_accepted() {
    witness(None).await;
}

#[tokio::test]
async fn o3_existing_proof_sync_retry_file_failure_preserves_pending() {
    witness(Some(Point::BeforeFileSync)).await;
}

#[tokio::test]
async fn o3_existing_proof_sync_retry_parent_failure_preserves_pending() {
    witness(Some(Point::BeforeParentSync)).await;
}

#[tokio::test]
async fn o3_existing_proof_sync_retry_reread_failure_preserves_pending() {
    witness(Some(Point::BeforeReread)).await;
}

async fn witness(fault: Option<Point>) {
    let redis = RedisServer::start().await;
    let parent = temp_directory("o3-sync-retry");
    let (owner, key, fresh, op, initial) = observed_first_boot(&parent);
    let config = observed_bootstrap(&parent, fresh.stage5c_config_fingerprint());
    let root = config.durable_parent().join(config.expected_root_name());
    let history = historical_bytes(&parent);
    let durable_bytes = || {
        [
            crate::STAGE7B_JOURNAL_FILE,
            crate::STAGE7B_RECOVERY_SEAL_FILE,
        ]
        .map(|n| std::fs::read(root.join(n)).unwrap())
    };
    let before = durable_bytes();
    drop(owner);
    let reopen = || {
        let Stage7bRestartOutcome::Ready(owner) = restart_stage8b_p1(
            observed_bootstrap(&parent, fresh.stage5c_config_fingerprint()),
            &key,
            fresh.clone(),
        )
        .unwrap() else {
            panic!("authenticated pre-append Ready restart");
        };
        *owner
    };
    let decision = next_session(&initial);
    let successor = extend(&decision, 2221);
    decision.persist_retained_receipt(&parent).unwrap();
    let source = paper_source(&op, &fresh.stage5c_config_fingerprint());
    let context = || super::context(&initial, &decision, &successor, &config, source.clone());
    let mut probe = initialize_stage8b_p1_redis_namespace(&redis.url, reclaim_config())
        .await
        .unwrap();
    let mut transport =
        attach_stage8b_p1_observed_recovery_redis(&redis.url, reclaim_config(), context())
            .await
            .unwrap();
    let bytes = build_stage8b_p1_observed_canonical_m10(
        &op,
        decision.source().end().timestamp_millis() - 600_000,
        decision.source(),
    )
    .unwrap();
    transport.publish_canonical_m10(&bytes, &op).await.unwrap();

    // Stop INSIDE fresh(), immediately after the full write, before either
    // file/parent sync. A returned error is the deterministic barrier, not a
    // claim to have reproduced physical loss of page cache in a power failure.
    let guard = sync_test::arm(Some(Point::AfterWrite));
    take_o3_callback_attempts();
    p1e_i1_begin_direct_effect_audit();
    assert!(
        Stage8bP1RedisSemanticCompositionOwner::new(reopen(), transport)
            .process_next(&key)
            .await
            .is_err()
    );
    assert_eq!(sync_test::trace(), vec![Point::AfterWrite]);
    assert_eq!(take_o3_callback_attempts(), [0, 0, 0]);
    let effects = p1e_i1_take_direct_effect_audit();
    assert_eq!(
        (
            effects.callback_total,
            effects.publication_attempt_total,
            effects.publication_total,
            effects.provider_total,
            effects.xack_attempt_total,
            effects.xack_total
        ),
        (0, 0, 0, 0, 0, 0)
    );
    assert_eq!(durable_bytes(), before);
    drop(guard);
    let proof_files = || {
        std::fs::read_dir(&parent)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("stage8b-o3-decision-")
            })
            .collect::<Vec<_>>()
    };
    let files = proof_files();
    assert_eq!(files.len(), 1);
    let proof_path = &files[0];
    let original = std::fs::read(proof_path).unwrap();
    assert!(Stage8bO3PaperPermit::restore_pending(&original, source.scope()).is_ok());
    let pending_before = probe.backend.pending_entries("-", "+", 2).await.unwrap();
    assert_eq!(pending_before.ids.len(), 1);
    tokio::time::sleep(Duration::from_millis(5)).await;

    // Public restart reopens the same authenticated seal/frontier. Existing-file
    // retry must complete durability without reissuing or changing the proof.
    let transport =
        attach_stage8b_p1_observed_recovery_redis(&redis.url, reclaim_config(), context())
            .await
            .unwrap();
    let guard = sync_test::arm(fault);
    p1e_i1_begin_direct_effect_audit();
    let result = Stage8bP1RedisSemanticCompositionOwner::new(reopen(), transport)
        .process_next(&key)
        .await;
    let effects = p1e_i1_take_direct_effect_audit();
    let attempts = take_o3_callback_attempts();
    let expected = vec![
        Point::BeforeFileSync,
        Point::FileSynced,
        Point::BeforeParentSync,
        Point::ParentSynced,
        Point::BeforeReread,
        Point::ExactReread,
        Point::CommitEntered,
        Point::CommitReturned,
    ];
    if let Some(fault) = fault {
        assert!(result.is_err(), "{fault:?}");
        let count = expected.iter().position(|p| *p == fault).unwrap() + 1;
        assert_eq!(sync_test::trace(), expected[..count]);
        assert_eq!(attempts, [0, 0, 0]);
        assert_eq!(effects.callback_total, 0);
        assert_eq!(durable_bytes(), before);
        drop(reopen()); // refusal did not move the authenticated Ready frontier
    } else {
        assert!(matches!(
            &result,
            Ok(Stage8bP1RedisSemanticOutcome::Prepublication(_))
        ));
        assert_eq!(sync_test::trace(), expected);
        assert!(attempts[0] > 50);
        assert_eq!(&attempts[1..], &[1, 0]);
        assert_eq!(effects.callback_total, 1);
        assert_ne!(durable_bytes()[0], before[0]); // RequestAccepted is durable
        assert_ne!(durable_bytes()[1], before[1]); // ordinary S1 follows it
    }
    assert_eq!(
        (
            effects.publication_attempt_total,
            effects.publication_total,
            effects.provider_total,
            effects.xack_attempt_total,
            effects.xack_total
        ),
        (0, 0, 0, 0, 0)
    );
    assert_eq!(std::fs::read(proof_path).unwrap(), original); // includes all times/identity
    assert_eq!(proof_files(), files); // no replacement proof or newly issued attempt
    assert_eq!(historical_bytes(&parent), history);
    let pending_after = probe.backend.pending_entries("-", "+", 2).await.unwrap();
    assert_eq!(pending_after.ids.len(), 1);
    assert_eq!(pending_after.ids[0].id, pending_before.ids[0].id);
    eprintln!("O3_PROOF_SYNC_RETRY fault={fault:?} trace={:?} callbacks={} publication={} provider={} xack={} pending=1 proof_unchanged=true", sync_test::trace(), effects.callback_total, effects.publication_total, effects.provider_total, effects.xack_total);
    drop(result);
    drop(guard);
    std::fs::remove_dir_all(parent).unwrap();
}
