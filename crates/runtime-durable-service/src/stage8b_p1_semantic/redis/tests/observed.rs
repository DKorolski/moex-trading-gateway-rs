use super::*;
use crate::stage8b_p1_semantic::observed::Stage8bP1ObservedRecoveryContext;
use crate::stage8b_p1e_first_boot_source::observed::tests::{fixture, observed_bootstrap};
use crate::{build_stage8b_p1_observed_canonical_m10, Stage8bP1ObservedM10Binding};
use broker_core::{event::Bar, observed_m1::ObservedM1Receipt, MarketDataSourceKind};

mod journal_ahead;

fn observed_first_boot(
    parent: &Path,
) -> (
    Stage7bRecoveryReadyOwner,
    Stage5gLifecycleCommitmentKey,
    strategy_runtime_core::HybridIntradayRuntimeStrategy,
    String,
    Stage8bP1ObservedM10Binding,
) {
    let (fresh, fp) = crate::Stage8bP1RuntimeProfileV2::build_hybrid_runtime().unwrap();
    let bootstrap = observed_bootstrap(parent, fp);
    let op = bootstrap.operational_identity_sha256().to_string();
    let (mut value, captured) = fixture(&op);
    // As in the existing adjacent-M10 first-boot regression, bootstrap just
    // before the next close. Do not widen the production truth TTL to make a
    // continuation pass; a long-running loop needs its own fresh-truth path.
    let now = captured + chrono::Duration::seconds(569);
    value["captured_at_utc"] =
        serde_json::json!(now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    value["broker_truth"]["checked_at_utc"] = value["captured_at_utc"].clone();
    let bytes = serde_json::to_vec(&value).unwrap();
    let source = crate::validate_stage8b_p1e_observed_first_boot_source_v4(
        &bytes,
        &format!("{:x}", sha2::Sha256::digest(&bytes)),
        &op,
        "ACC_TEST_0001",
        now,
    )
    .unwrap();
    let receipt = source.observed_receipt().unwrap();
    let binding = Stage8bP1ObservedM10Binding::new(
        &op,
        ObservedM1Receipt::restore(receipt.retained_bytes(), receipt.sha256()).unwrap(),
        receipt.sha256(),
    )
    .unwrap();
    let admin =
        authorize_stage8b_p1_first_boot(&bootstrap, STAGE8B_P1_FIRST_BOOT_CONFIRMATION).unwrap();
    let prepared = crate::stage8b_p1e_first_boot_source::observed::tests::prepare_fixture(
        bootstrap,
        fresh.clone(),
        source,
    );
    let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x9b; 32]).unwrap();
    let owner = crate::first_boot_stage8b_p1e_transaction_v5(prepared, admin, 1, &key)
        .unwrap()
        .into_owner();
    (owner, key, fresh, op, binding)
}

fn extend(old: &Stage8bP1ObservedM10Binding, price: i64) -> Stage8bP1ObservedM10Binding {
    let mut bars = old.source().bars().to_vec();
    for minute in [1, 4, 7] {
        let mut bar = bars[0].clone();
        bar.open_ts = old.source().end() + chrono::Duration::minutes(minute);
        bar.close_ts = bar.open_ts + chrono::Duration::minutes(1);
        bar.open = price.into();
        bar.close = price.into();
        bar.high = (price + 1).into();
        bar.low = (price - 1).into();
        bars.push(bar);
    }
    let source = ObservedM1Receipt::from_admitted_source(
        "cd".repeat(32),
        old.source().start(),
        old.source().end() + chrono::Duration::minutes(10),
        old.source().received_at() + chrono::Duration::minutes(10),
        crate::stage8b_p1_semantic::p1_instrument(),
        bars,
    )
    .unwrap();
    let hash = source.sha256().to_string();
    Stage8bP1ObservedM10Binding::new(old.operational_identity_sha256(), source, &hash).unwrap()
}

#[tokio::test]
async fn observed_redis_policy_cannot_cross_legacy_or_observed_durable_identity() {
    for observed_root in [false, true] {
        let redis = RedisServer::start().await;
        let parent = temp_directory("observed-policy-refusal");
        let (owner, key, op) = if observed_root {
            let (owner, key, _, op, _) = observed_first_boot(&parent);
            (owner, key, op)
        } else {
            let (owner, key, _, op) = first_boot(&parent);
            (owner, key, op)
        };
        let close = owner
            .stage8b_p1e_test_continuation_checkpoint_ts_utc_ms()
            .unwrap()
            + 600_000;
        let mut strict = initialize_stage8b_p1_redis_namespace(&redis.url, reclaim_config())
            .await
            .unwrap();
        let (transport, bytes) = if observed_root {
            let bytes = canonical_m10(op.clone(), close, 2650);
            strict.publish_canonical_m10(&bytes, &op).await.unwrap();
            (strict, bytes)
        } else {
            let binding = binding_at(
                &op,
                chrono::DateTime::from_timestamp_millis(close - 600_000).unwrap(),
                &[1, 4, 7],
                2650,
            );
            let bytes =
                build_stage8b_p1_observed_canonical_m10(&op, close - 600_000, binding.source())
                    .unwrap();
            let mut transport =
                attach_stage8b_p1_observed_redis(&redis.url, reclaim_config(), binding)
                    .await
                    .unwrap();
            transport.publish_canonical_m10(&bytes, &op).await.unwrap();
            (transport, bytes)
        };
        assert!(!bytes.is_empty());
        p1e_i1_begin_direct_effect_audit();
        assert!(
            Stage8bP1RedisSemanticCompositionOwner::new(owner, transport)
                .process_next(&key)
                .await
                .is_err()
        );
        let effects = p1e_i1_take_direct_effect_audit();
        assert_eq!(effects.callback_total, 0);
        assert_eq!(effects.provider_total, 0);
        assert_eq!(effects.publication_total, 0);
        assert_eq!(effects.xack_total, 0);
        let mut probe = attach_stage8b_p1_redis(&redis.url, reclaim_config())
            .await
            .unwrap();
        assert_eq!(
            probe
                .backend
                .pending_entries("-", "+", 10)
                .await
                .unwrap()
                .ids
                .len(),
            1
        );
        std::fs::remove_dir_all(parent).unwrap();
    }
}

/// Linked synthetic sparse source -> V4 History/first boot -> decision ->
/// next-source paper fill -> persisted ACK/truth -> XACK last -> exact restart.
/// Real broker-history/price parity remains the separate gateway/replay proof.
#[tokio::test]
async fn observed_redis_rolling_receipts_market_ack_truth_xack_and_restart_once() {
    rolling_receipts_market_restart(false).await;
}

#[tokio::test]
async fn observed_redis_sealed_recovery_loads_old_receipt_without_repeated_effects() {
    rolling_receipts_market_restart(true).await;
}

async fn rolling_receipts_market_restart(sealed_recovery: bool) {
    let redis = RedisServer::start().await;
    let parent = temp_directory("observed-rolling-market");
    let (owner, key, fresh, op, initial) = observed_first_boot(&parent);
    let receipt_dir = parent.clone();
    let decision = extend(&initial, 2220);
    let successor = extend(&decision, 2221);
    decision.persist_retained_receipt(&receipt_dir).unwrap();
    successor.persist_retained_receipt(&receipt_dir).unwrap();
    let decision_bytes = build_stage8b_p1_observed_canonical_m10(
        &op,
        initial.source().end().timestamp_millis(),
        decision.source(),
    )
    .unwrap();
    let successor_bytes = build_stage8b_p1_observed_canonical_m10(
        &op,
        decision.source().end().timestamp_millis(),
        successor.source(),
    )
    .unwrap();
    let canonical = decision.parse_exact(&decision_bytes, &op).unwrap();
    let successor_hash = successor.source().sha256().to_string();
    let predecessor_close = decision.source().end().timestamp_millis();
    let successor_close = successor.source().end().timestamp_millis();
    let pair = || {
        let old = Stage8bP1ObservedM10Binding::restore_for_exact_m10(
            &receipt_dir,
            &decision_bytes,
            &op,
            canonical.redis_id(),
            canonical.semantic_id_sha256(),
            canonical.payload_sha256(),
        )
        .unwrap();
        let next = Stage8bP1ObservedM10Binding::restore_retained_receipt(
            &receipt_dir,
            &op,
            &successor_hash,
        )
        .unwrap();
        crate::Stage8bP1ObservedM10WindowPair::new(old, next).unwrap()
    };
    // Keep only the authenticated initial context, which cannot parse the
    // later decision. Recovery must select its retained receipt by sealed IDs.
    assert!(initial.parse_exact(&decision_bytes, &op).is_err());
    drop(decision);
    drop(successor);
    initialize_stage8b_p1_redis_namespace(&redis.url, reclaim_config())
        .await
        .unwrap();
    let mut transport =
        crate::attach_stage8b_p1_observed_redis_pair(&redis.url, reclaim_config(), pair())
            .await
            .unwrap();
    transport
        .publish_canonical_m10(&decision_bytes, &op)
        .await
        .unwrap();
    transport
        .publish_canonical_m10(&successor_bytes, &op)
        .await
        .unwrap();
    p1e_i1_begin_direct_effect_audit();
    let outcome = Stage8bP1RedisSemanticCompositionOwner::new(owner, transport)
        .process_next(&key)
        .await
        .unwrap();
    let Stage8bP1RedisSemanticOutcome::Prepublication(pending) = outcome else {
        panic!("sparse breakout must emit one Market intent");
    };
    let published = pending.publish_exact_command().await.unwrap();
    let ack = published
        .execute_next_canonical_market(
            p1d2_test_schedule_authority_for(predecessor_close, successor_close),
            &key,
        )
        .await
        .unwrap();
    assert!(!ack.m10_xack_allowed());
    let before_restart = p1e_i1_take_direct_effect_audit();
    assert_eq!(before_restart.callback_total, 1);
    assert_eq!(before_restart.provider_total, 1);
    assert_eq!(before_restart.publication_total, 1);
    assert_eq!(before_restart.xack_total, 0);
    drop(ack);
    p1e_i1_begin_direct_effect_audit();
    let restart = || {
        restart_stage8b_p1(
            observed_bootstrap(&parent, fresh.stage5c_config_fingerprint()),
            &key,
            fresh.clone(),
        )
        .unwrap()
    };
    let Stage7bRestartOutcome::P1d2AckCommitted(ack) = restart() else {
        panic!("S_ack must restore truth-only route");
    };
    let recovery_context = || {
        Stage8bP1ObservedRecoveryContext::new(
            initial.clone(),
            &observed_bootstrap(&parent, fresh.stage5c_config_fingerprint()),
        )
        .unwrap()
    };
    let transport = if sealed_recovery {
        attach_stage8b_p1_observed_recovery_redis(&redis.url, reclaim_config(), recovery_context())
            .await
            .unwrap()
    } else {
        crate::attach_stage8b_p1_observed_redis_pair(&redis.url, reclaim_config(), pair())
            .await
            .unwrap()
    };
    let ack = p1e_test_resume_p1d2_ack(*ack, transport).await.unwrap();
    let ack_generation = ack.recovery_seal_generation();
    let mut probe =
        crate::attach_stage8b_p1_observed_redis_pair(&redis.url, reclaim_config(), pair())
            .await
            .unwrap();
    assert_eq!(
        probe
            .backend
            .pending_entries("-", "+", 10)
            .await
            .unwrap()
            .ids
            .len(),
        1
    );
    let truth = ack.commit_truth(&key).unwrap();
    assert_eq!(truth.recovery_seal_generation(), ack_generation + 1);
    assert!(truth.m10_xack_allowed());
    assert_eq!(
        probe
            .backend
            .pending_entries("-", "+", 10)
            .await
            .unwrap()
            .ids
            .len(),
        1
    );
    let audit = truth.audit_evidence().unwrap();
    assert_eq!(audit.core.seq_truth, audit.core.seq_ack + 1);
    drop(truth);
    for expected in [
        Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending,
        Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged,
    ] {
        let Stage7bRestartOutcome::P1d2TruthCommitted(truth) = restart() else {
            panic!("S_truth must restore XACK-only route");
        };
        let transport = if sealed_recovery {
            attach_stage8b_p1_observed_recovery_redis(
                &redis.url,
                reclaim_config(),
                recovery_context(),
            )
            .await
            .unwrap()
        } else {
            crate::attach_stage8b_p1_observed_redis_pair(&redis.url, reclaim_config(), pair())
                .await
                .unwrap()
        };
        let resolved = p1e_test_resume_p1d2_truth(*truth, transport).await.unwrap();
        assert_eq!(resolved.disposition(), expected);
        assert_eq!(resolved.audit_evidence(), &audit);
        drop(resolved);
    }
    let after_restart = p1e_i1_take_direct_effect_audit();
    assert_eq!(after_restart.callback_total, 0);
    assert_eq!(after_restart.provider_total, 0);
    assert_eq!(after_restart.publication_total, 0);
    assert_eq!(after_restart.xack_total, 1);
    assert_eq!(
        probe
            .backend
            .pending_entries("-", "+", 10)
            .await
            .unwrap()
            .ids
            .len(),
        0
    );
    assert_eq!(probe.retained_m10_count().await.unwrap(), 2);
    assert_eq!(
        std::fs::read_dir(&receipt_dir)
            .unwrap()
            .filter(|entry| entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("stage8b-observed-receipt-"))
            .count(),
        2
    );
    drop(probe);
    std::fs::remove_dir_all(parent).unwrap();
}

#[tokio::test]
async fn observed_redis_retained_recovery_refuses_bad_evidence_before_reclaim() {
    for fault in ["missing", "corrupt", "swapped", "wrong-sealed-payload"] {
        let redis = RedisServer::start().await;
        let parent = temp_directory("observed-retained-refusal");
        let (_, fp) = crate::Stage8bP1RuntimeProfileV2::build_hybrid_runtime().unwrap();
        let config = observed_bootstrap(&parent, fp);
        let op = config.operational_identity_sha256();
        let initial = binding(op, &[1, 4, 7]);
        let decision = extend(&initial, 2220);
        let successor = extend(&decision, 2221);
        let bytes = build_stage8b_p1_observed_canonical_m10(
            op,
            initial.source().end().timestamp_millis(),
            decision.source(),
        )
        .unwrap();
        let exact = decision.parse_exact(&bytes, op).unwrap();
        decision.persist_retained_receipt(&parent).unwrap();
        // A newer valid receipt remains available in every fault case. It must
        // not repair or replace the evidence for the already sealed decision.
        successor.persist_retained_receipt(&parent).unwrap();
        initialize_stage8b_p1_redis_namespace(&redis.url, reclaim_config())
            .await
            .unwrap();
        let mut writer =
            attach_stage8b_p1_observed_redis(&redis.url, reclaim_config(), decision.clone())
                .await
                .unwrap();
        writer.publish_canonical_m10(&bytes, op).await.unwrap();
        writer.backend.read_next_fresh().await.unwrap();
        let pending_before = writer.backend.pending_entries("-", "+", 2).await.unwrap();
        let path = parent.join(format!(
            "stage8b-observed-receipt-{}.json",
            decision.source().sha256()
        ));
        match fault {
            "missing" => std::fs::remove_file(&path).unwrap(),
            "corrupt" => std::fs::write(&path, b"{}").unwrap(),
            "swapped" => std::fs::write(&path, successor.source().retained_bytes()).unwrap(),
            "wrong-sealed-payload" => {}
            _ => unreachable!(),
        }
        let mut recovery = attach_stage8b_p1_observed_recovery_redis(
            &redis.url,
            reclaim_config(),
            Stage8bP1ObservedRecoveryContext::new(initial, &config).unwrap(),
        )
        .await
        .unwrap();
        let wrong_payload = "ef".repeat(32);
        let expected_payload = if fault == "wrong-sealed-payload" {
            &wrong_payload
        } else {
            exact.payload_sha256()
        };
        p1e_i1_begin_direct_effect_audit();
        assert!(
            recovery
                .backend
                .reclaim_exact_binding(
                    exact.redis_id(),
                    exact.semantic_id_sha256(),
                    expected_payload
                )
                .await
                .is_err(),
            "{fault}"
        );
        let effects = p1e_i1_take_direct_effect_audit();
        assert_eq!(effects.claim_total, 0, "{fault}");
        assert_eq!(effects.callback_total, 0, "{fault}");
        assert_eq!(effects.provider_total, 0, "{fault}");
        assert_eq!(effects.publication_total, 0, "{fault}");
        assert_eq!(effects.xack_total, 0, "{fault}");
        let pending_after = writer.backend.pending_entries("-", "+", 2).await.unwrap();
        assert_eq!(pending_before.ids.len(), 1);
        assert_eq!(pending_after.ids.len(), 1);
        assert_eq!(pending_after.ids[0].id, pending_before.ids[0].id);
        assert_eq!(
            pending_after.ids[0].consumer,
            pending_before.ids[0].consumer
        );
        assert_eq!(
            pending_after.ids[0].times_delivered,
            pending_before.ids[0].times_delivered
        );
        std::fs::remove_dir_all(parent).unwrap();
    }
}

#[tokio::test]
async fn observed_redis_fresh_context_cannot_discover_receipt_from_redis_hash() {
    let redis = RedisServer::start().await;
    let parent = temp_directory("observed-no-fresh-discovery");
    let (_, fp) = crate::Stage8bP1RuntimeProfileV2::build_hybrid_runtime().unwrap();
    let config = observed_bootstrap(&parent, fp);
    let op = config.operational_identity_sha256();
    let initial = binding(op, &[1, 4, 7]);
    let decision = extend(&initial, 2220);
    let bytes = build_stage8b_p1_observed_canonical_m10(
        op,
        initial.source().end().timestamp_millis(),
        decision.source(),
    )
    .unwrap();
    decision.persist_retained_receipt(&parent).unwrap();
    initialize_stage8b_p1_redis_namespace(&redis.url, reclaim_config())
        .await
        .unwrap();
    let mut writer = attach_stage8b_p1_observed_redis(&redis.url, reclaim_config(), decision)
        .await
        .unwrap();
    writer.publish_canonical_m10(&bytes, op).await.unwrap();
    let mut recovery = attach_stage8b_p1_observed_recovery_redis(
        &redis.url,
        reclaim_config(),
        Stage8bP1ObservedRecoveryContext::new(initial, &config).unwrap(),
    )
    .await
    .unwrap();
    p1e_i1_begin_direct_effect_audit();
    assert!(recovery.backend.read_next_fresh().await.is_err());
    let effects = p1e_i1_take_direct_effect_audit();
    assert_eq!(effects.callback_total, 0);
    assert_eq!(effects.provider_total, 0);
    assert_eq!(effects.publication_total, 0);
    assert_eq!(effects.xack_total, 0);
    // XREADGROUP puts the rejected fresh entry in PEL; it is not silently
    // consumed, acknowledged or trusted just because a file exists on disk.
    assert_eq!(
        writer
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

fn binding(op: &str, minutes: &[i64]) -> Stage8bP1ObservedM10Binding {
    let open: chrono::DateTime<chrono::Utc> = "2026-10-01T10:00:00Z".parse().unwrap();
    binding_at(op, open, minutes, 2200)
}

fn binding_at(
    op: &str,
    open: chrono::DateTime<chrono::Utc>,
    minutes: &[i64],
    price: i64,
) -> Stage8bP1ObservedM10Binding {
    let instrument = crate::stage8b_p1_semantic::p1_instrument();
    let source = ObservedM1Receipt::from_admitted_source(
        "aa".repeat(32),
        open,
        open + chrono::Duration::minutes(20),
        open + chrono::Duration::minutes(20),
        instrument.clone(),
        minutes
            .iter()
            .map(|m| Bar {
                instrument: instrument.clone(),
                source_kind: MarketDataSourceKind::HistoricalPoll,
                timeframe_sec: 60,
                open_ts: open + chrono::Duration::minutes(*m),
                close_ts: open + chrono::Duration::minutes(*m + 1),
                open: price.into(),
                high: (price + 1).into(),
                low: (price - 1).into(),
                close: price.into(),
                volume: 1.into(),
                is_final: true,
            })
            .collect(),
    )
    .unwrap();
    let hash = source.sha256().to_string();
    Stage8bP1ObservedM10Binding::new(op, source, &hash).unwrap()
}

/// Synthetic source, real V4 transaction/seal and isolated Redis; no live data.
#[tokio::test]
async fn observed_redis_zero_intent_durable_commit_xack_then_ack_only_restart() {
    let redis = RedisServer::start().await;
    let parent = temp_directory("observed-zero-intent");
    let (owner, key, fresh, op, initial) = observed_first_boot(&parent);
    initialize_stage8b_p1_redis_namespace(&redis.url, reclaim_config())
        .await
        .unwrap();
    let open = initial.source().end();
    let b = extend(&initial, 2200);
    let bytes =
        build_stage8b_p1_observed_canonical_m10(&op, open.timestamp_millis(), b.source()).unwrap();
    let mut transport = attach_stage8b_p1_observed_redis(&redis.url, reclaim_config(), b.clone())
        .await
        .unwrap();
    transport.publish_canonical_m10(&bytes, &op).await.unwrap();
    let outcome = Stage8bP1RedisSemanticCompositionOwner::new(owner, transport)
        .process_next(&key)
        .await
        .unwrap();
    let Stage8bP1RedisSemanticOutcome::Ready {
        owner,
        receipt,
        ack_disposition,
    } = outcome
    else {
        panic!("zero-intent must settle after durable commit");
    };
    assert_eq!(receipt.evidence.intent_count, 0);
    assert_eq!(
        ack_disposition,
        Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending
    );
    drop(owner);
    let restart = restart_stage8b_p1(
        observed_bootstrap(&parent, fresh.stage5c_config_fingerprint()),
        &key,
        fresh,
    )
    .unwrap();
    let Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(pending) = restart else {
        panic!("restart must retain ACK-only route");
    };
    let callbacks = pending.stage5c_callback_count();
    let source =
        ObservedM1Receipt::restore(b.source().retained_bytes(), b.source().sha256()).unwrap();
    let b = Stage8bP1ObservedM10Binding::new(&op, source, b.source().sha256()).unwrap();
    let transport = attach_stage8b_p1_observed_redis(&redis.url, reclaim_config(), b)
        .await
        .unwrap();
    let resolved = p1e_test_resolve_zero_intent(*pending, transport)
        .await
        .unwrap();
    assert_eq!(
        resolved.disposition(),
        Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged
    );
    assert_eq!(resolved.stage5c_callback_count(), callbacks);
    drop(resolved);
    fs::remove_dir_all(parent).unwrap();
}

#[tokio::test]
async fn observed_redis_exact_publish_pending_restart_and_successor_require_independent_source() {
    let redis = RedisServer::start().await;
    let op = "11".repeat(32);
    let b = binding(&op, &[1, 4, 7, 12, 18]);
    let open = b.source().start().timestamp_millis();
    let bytes = build_stage8b_p1_observed_canonical_m10(&op, open, b.source()).unwrap();
    let second = build_stage8b_p1_observed_canonical_m10(&op, open + 600_000, b.source()).unwrap();
    let id = b.parse_exact(&bytes, &op).unwrap().redis_id().to_string();
    let mut strict = initialize_stage8b_p1_redis_namespace(&redis.url, reclaim_config())
        .await
        .unwrap();
    assert!(strict.publish_canonical_m10(&bytes, &op).await.is_err());
    assert_eq!(strict.retained_m10_count().await.unwrap(), 0);
    let mut transport = attach_stage8b_p1_observed_redis(&redis.url, reclaim_config(), b.clone())
        .await
        .unwrap();
    assert_eq!(
        transport.publish_canonical_m10(&bytes, &op).await.unwrap(),
        Stage8bP1RedisM10PublishDisposition::Published
    );
    assert_eq!(
        transport.publish_canonical_m10(&bytes, &op).await.unwrap(),
        Stage8bP1RedisM10PublishDisposition::IdempotentExisting
    );
    transport
        .verify_exact_canonical_m10(&id, &bytes, &op)
        .await
        .unwrap();
    assert!(transport
        .verify_exact_canonical_m10(&id, &bytes, &"22".repeat(32))
        .await
        .is_err());
    let pending = transport.backend.read_next_fresh().await.unwrap();
    assert_eq!(pending.parse_exact(&op).unwrap().canonical_bytes(), bytes);
    let semantic = pending.semantic_id_sha256().to_string();
    let payload = pending.payload_sha256().to_string();
    drop(pending);
    drop(transport);
    // Restart context is restored from separately retained receipt/hash, not the
    // payload in Redis. Real XAUTOCLAIM acquires the same PEL item for a new owner.
    let restored =
        ObservedM1Receipt::restore(b.source().retained_bytes(), b.source().sha256()).unwrap();
    let restored = Stage8bP1ObservedM10Binding::new(&op, restored, b.source().sha256()).unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    let mut restart = attach_stage8b_p1_observed_redis(&redis.url, reclaim_config(), restored)
        .await
        .unwrap();
    let pending = restart
        .backend
        .reclaim_exact_binding(&id, &semantic, &payload)
        .await
        .unwrap();
    assert_eq!(pending.parse_exact(&op).unwrap().canonical_bytes(), bytes);
    let inspected = restart
        .backend
        .exact_delivery_for_binding(&id, &semantic, &payload, &op)
        .await
        .unwrap();
    assert_eq!(inspected.parse_exact(&op).unwrap().canonical_bytes(), bytes);
    assert!(restart
        .backend
        .first_successor_m10_if_present(&id, &op)
        .await
        .unwrap()
        .is_none());
    restart.publish_canonical_m10(&second, &op).await.unwrap();
    assert_eq!(
        restart
            .backend
            .first_successor_m10_if_present(&id, &op)
            .await
            .unwrap()
            .unwrap()
            .canonical_bytes(),
        second
    );
    let mut wrong = attach_stage8b_p1_observed_redis(
        &redis.url,
        reclaim_config(),
        binding(&op, &[1, 7, 12, 18]),
    )
    .await
    .unwrap();
    assert!(wrong
        .backend
        .exact_delivery_for_binding(&id, &semantic, &payload, &op)
        .await
        .is_err());
    assert!(wrong
        .verify_exact_canonical_m10(&id, &bytes, &op)
        .await
        .is_err());
    assert!(strict
        .backend
        .exact_delivery_for_binding(&id, &semantic, &payload, &op)
        .await
        .is_err());
    let dense_v1 = canonical_m10(op.clone(), open + 600_000, 2200);
    assert!(restart.publish_canonical_m10(&dense_v1, &op).await.is_err());
    let pel = restart.backend.pending_entries("-", "+", 10).await.unwrap();
    assert_eq!(pel.ids.len(), 1); // No XACK granted by admission/publication.
    assert_eq!(pel.ids[0].id, id);
}
