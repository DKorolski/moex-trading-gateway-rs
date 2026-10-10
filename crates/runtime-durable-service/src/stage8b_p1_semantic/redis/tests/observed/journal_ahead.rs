use super::*;

#[tokio::test]
async fn o3_journal_ahead_replays_exact_expired_proof_without_new_observation() {
    case("o3-valid").await;
}

#[tokio::test]
async fn o3_journal_ahead_missing_and_corrupt_proofs_do_not_claim_or_erase_request() {
    case("o3-proof-missing").await;
    case("o3-proof-corrupt").await;
}

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
    strategy_runtime_core::stage8b_o3_readmission::take_o3_callback_attempts();
    let (o3, fault) = fault
        .strip_prefix("o3-")
        .map_or((false, fault), |s| (true, s));
    let redis = RedisServer::start().await;
    let parent = temp_directory("observed-journal-ahead");
    let (owner, key, fresh, op, initial) = observed_first_boot(&parent);
    let config = observed_bootstrap(&parent, fresh.stage5c_config_fingerprint());
    let decision = if o3 {
        o3_readmission::next_session(&initial)
    } else {
        extend(&initial, 2220)
    };
    let successor = extend(&decision, 2221);
    let paper_source =
        o3.then(|| o3_readmission::paper_source(&op, &fresh.stage5c_config_fingerprint()));
    decision.persist_retained_receipt(&parent).unwrap();
    let bytes = build_stage8b_p1_observed_canonical_m10(
        &op,
        decision.source().end().timestamp_millis() - 600_000,
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
    let context = paper_source.clone().map(|source| {
        super::super::super::super::observed::ObservedM10Context::Recovery(o3_readmission::context(
            &initial, &decision, &successor, &config, source,
        ))
    });
    let permit = context.as_ref().map(|c| {
        c.o3()
            .unwrap()
            .fresh(
                &binding,
                owner
                    .o3_decision_binding(&binding, decision.source().sha256())
                    .unwrap(),
            )
            .unwrap()
    });
    let retained_proof = permit.as_ref().map(|p| p.retained_bytes().to_vec());
    let before_crash = crate::recovery::stage8b_o3_test_stop_after_request_accepted(
        owner,
        canonical.into_stage5c_semantic_bar().unwrap(),
        binding.clone(),
        &key,
        permit,
    )
    .unwrap();
    let root = config.durable_parent().join(config.expected_root_name());
    if o3 {
        let attempts = strategy_runtime_core::stage8b_o3_readmission::take_o3_callback_attempts();
        assert!(attempts[0] > 50);
        assert_eq!(&attempts[1..], &[1, 0]);
    }
    let journal_path = root.join(crate::STAGE7B_JOURNAL_FILE);
    let seal_path = root.join(crate::STAGE7B_RECOVERY_SEAL_FILE);
    let journal_before = std::fs::read(&journal_path).unwrap();
    let seal_before = std::fs::read(&seal_path).unwrap();

    let restart = restart_stage8b_p1(
        observed_bootstrap(&parent, fresh.stage5c_config_fingerprint()),
        &key,
        fresh.clone(),
    )
    .unwrap();
    let Stage7bRestartOutcome::P1SemanticPrepublicationPending(pending) = restart else {
        panic!("RequestAccepted without S1 must remain journal-ahead pending");
    };
    assert_eq!(pending.matches_source_probe(&binding), !o3);
    if o3 && fault == "valid" {
        // Real elapsed wall time, no timestamp rewriting or test clock compression.
        tokio::time::sleep(Duration::from_secs(61)).await;
        assert!(
            strategy_runtime_core::stage8b_o3_readmission::Stage8bO3PaperPermit::restore_pending(
                retained_proof.as_ref().unwrap(),
                paper_source.as_ref().unwrap().scope()
            )
            .is_err()
        );
    }
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
        "proof-missing" | "proof-corrupt" => {
            let file = std::fs::read_dir(&parent)
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|p| {
                    p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("stage8b-o3-decision-")
                })
                .unwrap();
            if fault == "proof-missing" {
                std::fs::remove_file(file).unwrap();
            } else {
                std::fs::write(file, b"{}").unwrap();
            }
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
    let recovery_context = || {
        if let Some(source) = paper_source.clone() {
            let restored = Stage8bP1ObservedM10Binding::restore_retained_receipt(
                &parent,
                &op,
                serde_json::from_slice::<serde_json::Value>(retained_proof.as_ref().unwrap())
                    .unwrap()["binding"]["observed_source_sha256"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            o3_readmission::context(&initial, &restored, &successor, &config, source)
        } else {
            Stage8bP1ObservedRecoveryContext::new(initial.clone(), &config).unwrap()
        }
    };
    let transport =
        attach_stage8b_p1_observed_recovery_redis(&redis.url, reclaim_config(), recovery_context())
            .await
            .unwrap();
    p1e_i1_begin_direct_effect_audit();
    let acquired = acquire_stage8b_p1_journal_ahead_with_redis(*pending, transport).await;
    let pre_permit = p1e_i1_take_direct_effect_audit();
    assert_eq!(pre_permit.callback_total, 0, "{fault}");
    assert_eq!(pre_permit.provider_total, 0, "{fault}");
    assert_eq!(pre_permit.publication_total, 0, "{fault}");
    assert_eq!(pre_permit.xack_total, 0, "{fault}");
    let mut finish_paper = None;
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
            if o3 {
                p1e_i1_begin_direct_effect_audit();
            }
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
            if o3 {
                finish_paper = Some(published);
            } else {
                drop(published);
            }
        }
    }
    let count: usize = redis::cmd("XLEN")
        .arg(&namespace.canonical_command_stream)
        .query_async(&mut connection)
        .await
        .unwrap();
    assert_eq!(count, usize::from(fault == "valid"));
    if o3 {
        let attempts = strategy_runtime_core::stage8b_o3_readmission::take_o3_callback_attempts();
        if fault == "valid" {
            assert!(attempts[0] > 50);
            assert_eq!(
                &attempts[1..],
                &[0, 1],
                "one replay computation, no new decision"
            );
        } else {
            assert_eq!(attempts, [0, 0, 0]);
        }
    }
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
    if let Some(published) = finish_paper {
        // Same exact journal decision now traverses the ordinary provider,
        // persisted ACK/truth and XACK-last chain. No new observation/callback.
        let successor_bytes = build_stage8b_p1_observed_canonical_m10(
            &op,
            successor.source().end().timestamp_millis() - 600_000,
            successor.source(),
        )
        .unwrap();
        let mut publisher = attach_stage8b_p1_observed_recovery_redis(
            &redis.url,
            reclaim_config(),
            recovery_context(),
        )
        .await
        .unwrap();
        publisher
            .publish_canonical_m10(&successor_bytes, &op)
            .await
            .unwrap();
        drop(publisher);
        let successor_close = successor.source().end().timestamp_millis();
        let ack = published
            .execute_next_canonical_market(
                p1d2_test_schedule_authority_for(successor_close - 600_000, successor_close),
                &key,
            )
            .await
            .unwrap();
        let truth = ack.commit_truth(&key).unwrap();
        let audit = truth.audit_evidence().unwrap();
        assert_eq!(audit.core.seq_truth, audit.core.seq_ack + 1);
        drop(truth);
        for expected in [
            Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending,
            Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged,
        ] {
            let Stage7bRestartOutcome::P1d2TruthCommitted(truth) = restart_stage8b_p1(
                observed_bootstrap(&parent, fresh.stage5c_config_fingerprint()),
                &key,
                fresh.clone(),
            )
            .unwrap() else {
                panic!("replayed decision S_truth restart");
            };
            let transport = attach_stage8b_p1_observed_recovery_redis(
                &redis.url,
                reclaim_config(),
                recovery_context(),
            )
            .await
            .unwrap();
            let resolved = p1e_test_resume_p1d2_truth(*truth, transport).await.unwrap();
            assert_eq!(resolved.disposition(), expected);
            assert_eq!(resolved.audit_evidence(), &audit);
            drop(resolved);
        }
        let effects = p1e_i1_take_direct_effect_audit();
        assert_eq!(
            (
                effects.callback_total,
                effects.provider_total,
                effects.publication_total,
                effects.xack_total
            ),
            (0, 1, 1, 1)
        ); // Existing direct callback counter counts new decisions only.
           // The separate core counter above proves ONE replay computation.
        assert_eq!(
            strategy_runtime_core::stage8b_o3_readmission::take_o3_callback_attempts(),
            [0, 0, 0]
        );
        assert!(probe
            .backend
            .pending_entries("-", "+", 2)
            .await
            .unwrap()
            .ids
            .is_empty());
    }
    std::fs::remove_dir_all(parent).unwrap();
}
