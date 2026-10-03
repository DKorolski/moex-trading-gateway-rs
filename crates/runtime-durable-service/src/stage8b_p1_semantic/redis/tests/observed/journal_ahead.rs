use super::*;

#[tokio::test]
async fn observed_journal_ahead_resolves_new_receipt_from_protected_source_digest() {
    case("valid").await;
}

#[tokio::test]
async fn observed_journal_ahead_refuses_unbound_source_before_claim() {
    for fault in [
        "missing",
        "corrupt",
        "alternate-source",
        "copied-hashes",
        "wrong-tuple",
    ] {
        case(fault).await;
    }
}

#[tokio::test]
async fn observed_journal_ahead_preset_latch_prevents_callback_and_s1() {
    case("latch").await;
}

async fn case(fault: &str) {
    let redis = RedisServer::start().await;
    let parent = temp_directory("observed-journal-ahead");
    let (owner, key, fresh, op, initial) = observed_first_boot(&parent);
    let config = observed_bootstrap(&parent, fresh.stage5c_config_fingerprint());
    let decision = extend(&initial, 2220);
    decision.persist_retained_receipt(&parent).unwrap();
    let bytes = build_stage8b_p1_observed_canonical_m10(
        &op,
        initial.source().end().timestamp_millis(),
        decision.source(),
    )
    .unwrap();
    assert!(initial.parse_exact(&bytes, &op).is_err());
    let canonical = decision.parse_exact(&bytes, &op).unwrap();
    let binding = strategy_runtime_core::Stage5gP1SemanticBindingInput {
        operational_identity_sha256: op.clone(),
        m10_redis_id: canonical.redis_id().into(),
        m10_semantic_id_sha256: canonical.semantic_id_sha256().into(),
        m10_payload_sha256: canonical.payload_sha256().into(),
    };
    let id = binding.m10_redis_id.clone();
    let before_crash = crate::recovery::stage8b_p1_test_stop_after_request_accepted(
        owner,
        canonical.into_stage5c_semantic_bar().unwrap(),
        binding.clone(),
        &key,
    )
    .unwrap();
    let root = config.durable_parent().join(config.expected_root_name());
    let journal_path = root.join(crate::STAGE7B_JOURNAL_FILE);
    let seal_path = root.join(crate::STAGE7B_RECOVERY_SEAL_FILE);
    let journal_before = std::fs::read(&journal_path).unwrap();
    let seal_before = std::fs::read(&seal_path).unwrap();

    let restart = restart_stage8b_p1(
        observed_bootstrap(&parent, fresh.stage5c_config_fingerprint()),
        &key,
        fresh,
    )
    .unwrap();
    let Stage7bRestartOutcome::P1SemanticPrepublicationPending(pending) = restart else {
        panic!("RequestAccepted without S1 must remain journal-ahead pending");
    };
    assert!(pending.matches_source_probe(&binding));
    for field in ["identity", "id", "semantic", "payload"] {
        let mut changed = binding.clone();
        match field {
            "identity" => changed.operational_identity_sha256 = "01".repeat(32),
            "id" => changed.m10_redis_id = "1-0".into(),
            "semantic" => changed.m10_semantic_id_sha256 = "02".repeat(32),
            "payload" => changed.m10_payload_sha256 = "03".repeat(32),
            _ => unreachable!(),
        }
        assert!(!pending.matches_source_probe(&changed), "{field}");
    }

    let mut supplied = bytes.clone();
    let retained_path = parent.join(format!(
        "stage8b-observed-receipt-{}.json",
        decision.source().sha256()
    ));
    match fault {
        "missing" => std::fs::remove_file(&retained_path).unwrap(),
        "corrupt" => std::fs::write(&retained_path, b"{}").unwrap(),
        "alternate-source" => {
            // Fully valid, rehashed receipt/candle at the same close. It does
            // NOT match RequestAccepted even though it can parse in isolation.
            let alternate = extend(&initial, 2221);
            alternate.persist_retained_receipt(&parent).unwrap();
            supplied = build_stage8b_p1_observed_canonical_m10(
                &op,
                initial.source().end().timestamp_millis(),
                alternate.source(),
            )
            .unwrap();
            assert!(alternate.parse_exact(&supplied, &op).is_ok());
        }
        "copied-hashes" => {
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["payload"]["volume"] = serde_json::json!("999");
            supplied = serde_json::to_vec(&value).unwrap();
        }
        "wrong-tuple" => {
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["m10_payload_sha256"] = serde_json::json!("ef".repeat(32));
            supplied = serde_json::to_vec(&value).unwrap();
        }
        "valid" | "latch" => {}
        _ => unreachable!(),
    }
    drop(decision);
    let mut probe = initialize_stage8b_p1_redis_namespace(&redis.url, reclaim_config())
        .await
        .unwrap();
    // Raw injection is fault instrumentation only: the protected journal was
    // formed from the original canonical input, independently of these bytes.
    let namespace = stage8b_p1_redis_namespace();
    let mut connection = redis.connection().await;
    let _: String = redis::cmd("XADD")
        .arg(&namespace.canonical_m10_stream)
        .arg(&id)
        .arg("payload")
        .arg(supplied)
        .query_async(&mut connection)
        .await
        .unwrap();
    let _: StreamReadReply = redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg(&namespace.m10_consumer_group)
        .arg("pre-crash-consumer")
        .arg("COUNT")
        .arg(1)
        .arg("STREAMS")
        .arg(&namespace.canonical_m10_stream)
        .arg(">")
        .query_async(&mut connection)
        .await
        .unwrap();
    let before = probe.backend.pending_entries("-", "+", 2).await.unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    let transport = attach_stage8b_p1_observed_recovery_redis(
        &redis.url,
        reclaim_config(),
        Stage8bP1ObservedRecoveryContext::new(initial, &config).unwrap(),
    )
    .await
    .unwrap();
    p1e_i1_begin_direct_effect_audit();
    let acquired = acquire_stage8b_p1_journal_ahead_with_redis(*pending, transport).await;
    let pre_permit = p1e_i1_take_direct_effect_audit();
    assert_eq!(pre_permit.callback_total, 0, "{fault}");
    assert_eq!(pre_permit.provider_total, 0, "{fault}");
    assert_eq!(pre_permit.publication_total, 0, "{fault}");
    assert_eq!(pre_permit.xack_total, 0, "{fault}");
    if fault != "valid" && fault != "latch" {
        assert!(acquired.is_err(), "{fault}");
        assert_eq!(pre_permit.claim_total, 0, "{fault}");
        let after = probe.backend.pending_entries("-", "+", 2).await.unwrap();
        assert_eq!(before.ids.len(), 1);
        assert_eq!(after.ids.len(), 1);
        assert_eq!(before.ids[0].id, after.ids[0].id);
        assert_eq!(before.ids[0].consumer, after.ids[0].consumer);
        assert_eq!(before.ids[0].times_delivered, after.ids[0].times_delivered);
    } else {
        assert_eq!(pre_permit.claim_total, 1);
        let acquired = acquired.unwrap();
        if fault == "latch" {
            let latch = Stage8bP1eShutdownLatchV1::new();
            assert!(latch.request(Stage8bP1eShutdownIntentV1::new(
                Stage8bP1eShutdownCauseV1::ExternalSignal,
                20_000,
                41,
            )));
            p1e_i1_begin_direct_effect_audit();
            p1e_i0_begin_effect_audit();
            assert!(matches!(
                decide_stage8b_p1e_post_acquisition_latch(acquired, &latch),
                Stage8bP1ePostAcquisitionDecisionV1::RetainForRestart(_)
            ));
            assert_eq!(
                p1e_i1_take_direct_effect_audit(),
                P1eI1DirectEffectCountersV1::default()
            );
            assert_eq!(
                p1e_i0_take_effect_audit(),
                P1eI0ObservedEffectCountersV1::default()
            );
        } else {
            let permit = p1e_clear_permit(acquired);
            let resumed = resume_stage8b_p1_journal_ahead_with_redis(permit, &key)
                .await
                .unwrap();
            assert_eq!(resumed.evidence(), &before_crash);
            let published = resumed.publish_exact_command().await.unwrap();
            assert_eq!(
                published.receipt().disposition,
                Stage8bP1RedisCommandPublicationDisposition::Published
            );
            assert!(!published.m10_xack_allowed());
            drop(published);
        }
    }
    let count: usize = redis::cmd("XLEN")
        .arg(&namespace.canonical_command_stream)
        .query_async(&mut connection)
        .await
        .unwrap();
    assert_eq!(count, usize::from(fault == "valid"));
    assert_eq!(
        std::fs::read(&journal_path).unwrap(),
        journal_before,
        "replaying RequestAccepted must not append a second journal record"
    );
    let seal_after = std::fs::read(&seal_path).unwrap();
    if fault == "valid" {
        assert_ne!(
            seal_before, seal_after,
            "only successful post-permit replay commits S1"
        );
    } else {
        assert_eq!(seal_before, seal_after, "{fault}");
    }
    assert_eq!(
        probe
            .backend
            .pending_entries("-", "+", 2)
            .await
            .unwrap()
            .ids
            .len(),
        1
    );
    std::fs::remove_dir_all(parent).unwrap();
}
