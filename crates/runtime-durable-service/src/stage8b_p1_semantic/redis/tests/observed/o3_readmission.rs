use super::*;
use broker_core::*;
use rust_decimal::Decimal;
use strategy_runtime_core::stage8b_o3_readmission::*;

mod sync_retry;

pub(super) fn paper_source(op: &str, fingerprint: &str) -> Stage8bO3PaperSource {
    let now = chrono::Utc::now();
    let fixture = BrokerTruthSnapshot {
        account_id: BrokerAccountId::new("ACC_TEST_0001"),
        orders: vec![],
        positions: vec![],
        trades: vec![],
        cash: None,
        instruments: vec![BrokerInstrumentSpec {
            instrument: InstrumentMapEntry {
                internal_symbol: InternalSymbol("IMOEXF".into()),
                broker: BrokerKind::Finam,
                broker_symbol: BrokerSymbol("IMOEXF@RTSX".into()),
                exchange: Exchange::Moex,
                market: Market::Futures,
                price_step: Decimal::new(5, 1),
                qty_step: Decimal::ONE,
                lot_size: Decimal::ONE,
                min_qty: Decimal::ONE,
                step_value: Decimal::new(5, 0),
                currency: "RUB".into(),
                schedule_id: "RTSX".into(),
                expiration_date: None,
                is_tradable: true,
            },
            broker_asset_id: Some("SYNTHETIC-O3".into()),
            board: Some("RTSX".into()),
            long_initial_margin: Some(Money::new(5000, 0)),
            short_initial_margin: Some(Money::new(5000, 0)),
        }],
        received_ts: now - chrono::Duration::days(1),
    };
    Stage8bO3PaperSource::new(Stage8bO3PaperScope {
        phase_id:"local-o3-source-witness".into(),phase_sha256:"11".repeat(32),
        config_sha256:"22".repeat(32),plan_sha256:"33".repeat(32),
        fixture_sha256:format!("{:x}",sha2::Sha256::digest(serde_json::to_vec(&fixture).unwrap())),
        operational_identity_sha256:op.into(),runtime_config_fingerprint_sha256:fingerprint.into(),
        strategy_id:crate::STAGE8B_P1_STRATEGY_ID.into(),
        not_before:now-chrono::Duration::seconds(1),deadline:now+chrono::Duration::seconds(1799),
        catchup_coverage: serde_json::from_value(serde_json::json!({
            "source_mode":"config-bound-explicit-session-windows-v1",
            "sessions":(["2026-09-28","2026-09-29"].map(|date| {
                let start = chrono::DateTime::parse_from_rfc3339(&format!("{date}T04:00:00Z")).unwrap().timestamp();
                serde_json::json!({"session_date":date,"windows":[{"first_close_time_utc":start,"last_close_time_utc":start+600*100}]})
            }))
        })).unwrap(),
    },fixture).unwrap()
}

#[tokio::test]
async fn o3_fresh_real_stage4_stage5c_long_gap_market_ack_truth_xack_restart() {
    rolling_receipts_market_restart(true, true).await;
}

#[tokio::test]
#[ignore = "real ten-minute M10 interval; run explicitly for O3 source acceptance"]
async fn o3_real_ten_minute_market_successor_preserves_position_and_stops() {
    rolling_market_with_delay(true, true, Duration::from_secs(601)).await;
}

pub(super) fn historical_bytes(parent: &std::path::Path) -> Vec<Vec<u8>> {
    [
        crate::STAGE8B_P1E_TRANSACTION_MARKER_FILE,
        crate::STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE,
    ]
    .map(|name| std::fs::read(parent.join(name)).unwrap())
    .to_vec()
}

pub(super) fn next_session(old: &Stage8bP1ObservedM10Binding) -> Stage8bP1ObservedM10Binding {
    next_session_price(old, 2220)
}

fn next_session_price(
    old: &Stage8bP1ObservedM10Binding,
    decision_price: i64,
) -> Stage8bP1ObservedM10Binding {
    let candidate = chrono::DateTime::parse_from_rfc3339("2026-09-29T07:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let end = candidate + chrono::Duration::minutes(10);
    // A bounded rolling receipt may retire the oldest History day; the exact
    // shared prefix is still checked against authenticated V4 source.
    let start = old.source().start() + chrono::Duration::days(1);
    let mut bars = old
        .source()
        .bars()
        .iter()
        .filter(|b| b.open_ts >= start)
        .cloned()
        .collect::<Vec<_>>();
    for label in (old.source().end().timestamp()..end.timestamp()).step_by(600) {
        use chrono::Timelike;
        let open = chrono::DateTime::from_timestamp(label, 0).unwrap();
        let minute = open.hour() * 60 + open.minute();
        if !(240..1250).contains(&minute) {
            continue;
        }
        for offset in [1, 4, 7] {
            let mut bar = bars[0].clone();
            let price = if open == candidate {
                decision_price
            } else {
                2200
            };
            bar.open_ts = open + chrono::Duration::minutes(offset);
            bar.close_ts = bar.open_ts + chrono::Duration::minutes(1);
            bar.open = price.into();
            bar.close = price.into();
            bar.high = (price + 1).into();
            bar.low = (price - 1).into();
            bars.push(bar);
        }
    }
    let receipt = ObservedM1Receipt::from_admitted_source(
        "ed".repeat(32),
        start,
        end,
        end + chrono::Duration::seconds(30),
        old.source().instrument().clone(),
        bars,
    )
    .unwrap();
    let sha = receipt.sha256().to_string();
    Stage8bP1ObservedM10Binding::new(old.operational_identity_sha256(), receipt, &sha).unwrap()
}

pub(super) fn context(
    initial: &Stage8bP1ObservedM10Binding,
    decision: &Stage8bP1ObservedM10Binding,
    successor: &Stage8bP1ObservedM10Binding,
    config: &crate::Stage8bP1ValidatedBootstrapConfig,
    source: Stage8bO3PaperSource,
) -> Stage8bP1ObservedRecoveryContext {
    let bytes = |b: &Stage8bP1ObservedM10Binding| {
        build_stage8b_p1_observed_canonical_m10(
            b.operational_identity_sha256(),
            b.source().end().timestamp_millis() - 600_000,
            b.source(),
        )
        .unwrap()
    };
    let window = crate::Stage8bP1ObservedPublishedWindow::new(
        decision.clone(),
        bytes(decision),
        successor.clone(),
        bytes(successor),
    )
    .unwrap()
    .with_o3_paper_source(source)
    .unwrap();
    let mut c = Stage8bP1ObservedRecoveryContext::new(initial.clone(), config).unwrap();
    c.admit_fresh_window(window).unwrap();
    c
}

#[tokio::test]
async fn o3_retained_preappend_attempt_restarts_exactly_and_rejects_foreign_seed() {
    for fault in [
        "valid",
        "seal",
        "profile",
        "source",
        "checkpoint",
        "frontier",
    ] {
        let parent = temp_directory("o3-preappend");
        let (owner, key, fresh, op, initial) = observed_first_boot(&parent);
        let config = observed_bootstrap(&parent, fresh.stage5c_config_fingerprint());
        let decision = next_session(&initial);
        let successor = extend(&decision, 2221);
        let source = paper_source(&op, &fresh.stage5c_config_fingerprint());
        let c = super::super::super::super::observed::ObservedM10Context::Recovery(context(
            &initial,
            &decision,
            &successor,
            &config,
            source.clone(),
        ));
        let bytes = build_stage8b_p1_observed_canonical_m10(
            &op,
            decision.source().end().timestamp_millis() - 600_000,
            decision.source(),
        )
        .unwrap();
        let bar = decision.parse_exact(&bytes, &op).unwrap();
        let binding = strategy_runtime_core::Stage5gP1SemanticBindingInput {
            operational_identity_sha256: op,
            m10_redis_id: bar.redis_id().into(),
            m10_semantic_id_sha256: bar.semantic_id_sha256().into(),
            m10_payload_sha256: bar.payload_sha256().into(),
        };
        let exact = owner
            .o3_decision_binding(&binding, decision.source().sha256())
            .unwrap();
        let p = c.o3().unwrap().fresh(&binding, exact.clone()).unwrap();
        let retained = p.retained_bytes().to_vec();
        drop(p);
        drop(owner); // pre-append crash; no journal or S1 has been written.
        let root = config.durable_parent().join(config.expected_root_name());
        let before = [
            crate::STAGE7B_JOURNAL_FILE,
            crate::STAGE7B_RECOVERY_SEAL_FILE,
        ]
        .map(|n| std::fs::read(root.join(n)).unwrap());
        let Stage7bRestartOutcome::Ready(owner) = restart_stage8b_p1(config, &key, fresh).unwrap()
        else {
            panic!("pre-append authenticated Ready");
        };
        let p = c.o3().unwrap().fresh(&binding, exact.clone()).unwrap();
        assert_eq!(p.retained_bytes(), retained);
        let p = if fault == "valid" {
            p
        } else {
            let mut changed = exact;
            let mut scope = source.scope().clone();
            match fault {
                "seal" => changed.source_seal_commitment_sha256 = "01".repeat(32),
                "source" => changed.observed_source_sha256 = "02".repeat(32),
                "checkpoint" => changed.source_checkpoint_sha256 = "03".repeat(32),
                "frontier" => changed.source_frontier_sha256 = "04".repeat(32),
                "profile" => scope.runtime_config_fingerprint_sha256 = "05".repeat(32),
                _ => unreachable!(),
            }
            // Typed production constructor still cannot grant authority for a
            // different authenticated seed/profile/source.
            let v: serde_json::Value = serde_json::from_slice(&retained).unwrap();
            let fixture = serde_json::from_value(v["fixture"].clone()).unwrap();
            Stage8bO3PaperSource::new(scope, fixture)
                .unwrap()
                .issue(changed)
                .unwrap()
        };
        take_o3_callback_attempts();
        let result = owner.commit_stage8b_p1_semantic_o3(
            bar.into_stage5c_semantic_bar().unwrap(),
            binding,
            &key,
            Some(p),
        );
        if fault == "valid" {
            assert!(result.is_ok());
            assert_eq!(&take_o3_callback_attempts()[1..], &[1, 0]);
            drop(result);
        } else {
            assert!(result.is_err(), "{fault}");
            assert_eq!(take_o3_callback_attempts(), [0, 0, 0], "{fault}");
            assert_eq!(
                [
                    crate::STAGE7B_JOURNAL_FILE,
                    crate::STAGE7B_RECOVERY_SEAL_FILE
                ]
                .map(|n| std::fs::read(root.join(n)).unwrap()),
                before,
                "{fault}"
            );
        }
        std::fs::remove_dir_all(parent).unwrap();
    }
}

#[tokio::test]
#[ignore = "real ten-minute M10 interval; run explicitly for O3 source acceptance"]
async fn o3_real_ten_minute_successor_refresh_no_effect_and_bounded_stop() {
    zero_successor(Duration::from_secs(601)).await;
}

#[tokio::test]
async fn o3_zero_intent_restart_then_successor_uses_distinct_proof() {
    zero_successor(Duration::ZERO).await;
}

async fn zero_successor(delay: Duration) {
    let redis = RedisServer::start().await;
    let parent = temp_directory("o3-real-successor");
    let (owner, key, fresh, op, initial) = observed_first_boot(&parent);
    let historical = historical_bytes(&parent);
    let config = observed_bootstrap(&parent, fresh.stage5c_config_fingerprint());
    let root = config.durable_parent().join(config.expected_root_name());
    let journal_path = root.join(crate::STAGE7B_JOURNAL_FILE);
    let journal_before = std::fs::read(&journal_path).unwrap();
    drop(owner);
    let Stage7bRestartOutcome::Ready(owner) = restart_stage8b_p1(
        observed_bootstrap(&parent, fresh.stage5c_config_fingerprint()),
        &key,
        fresh.clone(),
    )
    .unwrap() else {
        panic!("V5 public authenticated restart");
    };
    let decision = next_session_price(&initial, 2200);
    let successor = extend(&decision, 2200);
    for b in [&decision, &successor] {
        b.persist_retained_receipt(&parent).unwrap();
    }
    let source = paper_source(&op, &fresh.stage5c_config_fingerprint());
    let initial_context = context(&initial, &decision, &successor, &config, source.clone());
    initialize_stage8b_p1_redis_namespace(&redis.url, reclaim_config())
        .await
        .unwrap();
    let mut transport =
        attach_stage8b_p1_observed_recovery_redis(&redis.url, reclaim_config(), initial_context)
            .await
            .unwrap();
    for b in [&decision, &successor] {
        let bytes = build_stage8b_p1_observed_canonical_m10(
            &op,
            b.source().end().timestamp_millis() - 600_000,
            b.source(),
        )
        .unwrap();
        transport.publish_canonical_m10(&bytes, &op).await.unwrap();
    }
    take_o3_callback_attempts();
    p1e_i1_begin_direct_effect_audit();
    let Stage8bP1RedisSemanticOutcome::Ready { owner, receipt, .. } =
        Stage8bP1RedisSemanticCompositionOwner::new(*owner, transport)
            .process_next(&key)
            .await
            .unwrap()
    else {
        panic!("first O3 decision must have zero intents");
    };
    assert_eq!(receipt.evidence.intent_count, 0);
    let first_proof = std::fs::read_dir(&parent)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("stage8b-o3-decision-")
        })
        .unwrap();
    let proof_before = std::fs::read(&first_proof).unwrap();
    let started = std::time::Instant::now();
    tokio::time::sleep(delay).await;
    assert!(started.elapsed() >= delay);
    if delay >= Duration::from_secs(60) {
        assert!(Stage8bO3PaperPermit::restore_pending(&proof_before, source.scope()).is_err());
    }
    // Persisted S1 is the only restart authority; no second first boot.
    drop(owner);
    let Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(pending) = restart_stage8b_p1(
        observed_bootstrap(&parent, fresh.stage5c_config_fingerprint()),
        &key,
        fresh.clone(),
    )
    .unwrap() else {
        panic!("zero intent S1 restart");
    };
    let transport = attach_stage8b_p1_observed_recovery_redis(
        &redis.url,
        reclaim_config(),
        context(&initial, &decision, &successor, &config, source),
    )
    .await
    .unwrap();
    let resolved = p1e_test_resolve_zero_intent(*pending, transport)
        .await
        .unwrap();
    assert_eq!(
        resolved.disposition(),
        Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged
    );
    let owner = resolved.into_ready_owner();
    let Stage8bP1RedisSemanticOutcome::Ready { owner, receipt, .. } =
        owner.process_next(&key).await.unwrap()
    else {
        panic!("successor must remain a zero-intent decision");
    };
    assert_eq!(receipt.evidence.intent_count, 0);
    assert_eq!(std::fs::read(first_proof).unwrap(), proof_before);
    assert_eq!(std::fs::read(journal_path).unwrap(), journal_before);
    let effects = p1e_i1_take_direct_effect_audit();
    assert_eq!(
        (
            effects.callback_total,
            effects.provider_total,
            effects.publication_total,
            effects.xack_total
        ),
        (2, 0, 0, 2)
    );
    let counts = take_o3_callback_attempts();
    assert!(counts[0] > 50);
    assert_eq!(&counts[1..], &[2, 0]);
    assert_eq!(
        std::fs::read_dir(&parent)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("stage8b-o3-decision-"))
            .count(),
        2
    );
    drop(owner); // bounded witness ends; no unattended loop.
    assert_eq!(historical_bytes(&parent), historical);
    std::fs::remove_dir_all(parent).unwrap();
}
