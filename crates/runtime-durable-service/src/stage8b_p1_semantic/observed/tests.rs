use super::*;
use broker_core::{event::Bar, MarketDataSourceKind};
use chrono::{Duration, Utc};

fn open() -> DateTime<Utc> {
    "2026-10-01T10:00:00Z".parse().unwrap()
}
fn receipt(minutes: &[i64]) -> ObservedM1Receipt {
    let bars = minutes
        .iter()
        .map(|i| {
            let ts = open() + Duration::minutes(*i);
            let price = Decimal::from(2200) + Decimal::new(*i * 5, 1);
            Bar {
                instrument: p1_instrument(),
                source_kind: MarketDataSourceKind::HistoricalPoll,
                timeframe_sec: 60,
                open_ts: ts,
                close_ts: ts + Duration::minutes(1),
                open: price,
                high: price + Decimal::ONE,
                low: price - Decimal::ONE,
                close: price + Decimal::new(5, 1),
                volume: Decimal::ONE,
                is_final: true,
            }
        })
        .collect();
    from_rows(bars)
}
fn from_rows(bars: Vec<Bar>) -> ObservedM1Receipt {
    ObservedM1Receipt::from_admitted_source(
        "aa".repeat(32),
        open(),
        open() + Duration::minutes(10),
        open() + Duration::minutes(10) + Duration::seconds(2),
        p1_instrument(),
        bars,
    )
    .unwrap()
}
fn encoded(source: &ObservedM1Receipt) -> Vec<u8> {
    build_stage8b_p1_observed_canonical_m10(&"11".repeat(32), open().timestamp_millis(), source)
        .unwrap()
}
fn parsed(
    bytes: &[u8],
    source: &ObservedM1Receipt,
) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
    parse_stage8b_p1_observed_canonical_m10(bytes, &"11".repeat(32), source, source.sha256())
}

fn bind(source: ObservedM1Receipt) -> Stage8bP1ObservedM10Binding {
    let hash = source.sha256().to_string();
    Stage8bP1ObservedM10Binding::new(&"11".repeat(32), source, &hash).unwrap()
}

fn next_receipt(old: &ObservedM1Receipt) -> ObservedM1Receipt {
    let mut bars = old.bars().to_vec();
    let mut next = bars[0].clone();
    next.open_ts += Duration::minutes(10);
    next.close_ts += Duration::minutes(10);
    bars.push(next);
    ObservedM1Receipt::from_admitted_source(
        "bb".repeat(32),
        old.start(),
        old.end() + Duration::minutes(10),
        old.received_at() + Duration::minutes(10),
        p1_instrument(),
        bars,
    )
    .unwrap()
}

#[test]
fn observed_published_window_is_exact_bounded_and_cannot_rebase_initial() {
    let initial = bind(receipt(&[1, 4, 7]));
    let decision = bind(next_receipt(initial.source()));
    let mut rows = decision.source().bars().to_vec();
    let mut tail = rows[0].clone();
    tail.open_ts = decision.source().end() + Duration::minutes(1);
    tail.close_ts = tail.open_ts + Duration::minutes(1);
    rows.push(tail);
    let successor = bind(
        ObservedM1Receipt::from_admitted_source(
            "cc".repeat(32),
            decision.source().start(),
            decision.source().end() + Duration::minutes(10),
            decision.source().received_at() + Duration::minutes(10),
            p1_instrument(),
            rows,
        )
        .unwrap(),
    );
    let bytes = |binding: &Stage8bP1ObservedM10Binding| {
        build_stage8b_p1_observed_canonical_m10(
            binding.operational_identity_sha256(),
            binding.source().end().timestamp_millis() - 600_000,
            binding.source(),
        )
        .unwrap()
    };
    let old_bytes = bytes(&initial);
    let decision_bytes = bytes(&decision);
    let successor_bytes = bytes(&successor);
    let window = Stage8bP1ObservedPublishedWindow::new(
        decision.clone(),
        decision_bytes.clone(),
        successor.clone(),
        successor_bytes.clone(),
    )
    .unwrap();
    let mut context = Stage8bP1ObservedRecoveryContext {
        initial: initial.clone(),
        retained_parent: Default::default(),
        fresh_window: None,
    };
    let op = initial.operational_identity_sha256();
    assert!(context.parse_fresh_exact(&decision_bytes, op).is_err());
    context.admit_fresh_window(window.clone()).unwrap();
    for exact in [&old_bytes, &decision_bytes, &successor_bytes] {
        assert!(context.parse_fresh_exact(exact, op).is_ok());
    }
    assert!(context.admit_fresh_window(window).is_err());
    assert!(context
        .parse_fresh_exact(&successor_bytes, &"ff".repeat(32))
        .is_err());
    // Fresh admission is not a shortcut around retained evidence at restart.
    // No receipt file exists here; only the original initial binding can be
    // restored without disk even when the fresh window contains this candle.
    let decision_value = decision.parse_exact(&decision_bytes, op).unwrap();
    // Same prices and nominal bucket, but rehashed against a later snapshot.
    let rebound_old = build_stage8b_p1_observed_canonical_m10(
        op,
        initial.source().start().timestamp_millis(),
        successor.source(),
    )
    .unwrap();
    let rebound_decision = build_stage8b_p1_observed_canonical_m10(
        op,
        decision.source().end().timestamp_millis() - 600_000,
        successor.source(),
    )
    .unwrap();
    for unbound in [rebound_old, rebound_decision] {
        assert!(context.parse_fresh_exact(&unbound, op).is_err());
    }
    assert!(Stage8bP1ObservedPublishedWindow::new(
        decision.clone(),
        successor_bytes.clone(),
        successor.clone(),
        decision_bytes.clone(),
    )
    .is_err());
    assert!(Stage8bP1ObservedPublishedWindow::new(
        decision.clone(),
        decision_bytes.clone(),
        decision.clone(),
        decision_bytes.clone(),
    )
    .is_err());
    let mut corrupt = successor_bytes;
    corrupt.push(b' ');
    assert!(Stage8bP1ObservedPublishedWindow::new(
        decision.clone(),
        bytes(&decision),
        successor,
        corrupt,
    )
    .is_err());
    assert!(ObservedM10Context::Recovery(context)
        .parse_sealed_exact(
            &decision_bytes,
            op,
            decision_value.redis_id(),
            decision_value.semantic_id_sha256(),
            decision_value.payload_sha256(),
        )
        .is_err());
}

#[test]
fn observed_window_pair_preserves_old_bytes_and_refuses_overlap_rebinding() {
    let old = bind(receipt(&[1, 4, 7]));
    let next = bind(next_receipt(old.source()));
    let pair = Stage8bP1ObservedM10WindowPair::new(old.clone(), next.clone()).unwrap();
    let identity = "11".repeat(32);
    let before = encoded(old.source());
    let after = build_stage8b_p1_observed_canonical_m10(
        &identity,
        old.source().end().timestamp_millis(),
        next.source(),
    )
    .unwrap();
    assert_eq!(
        pair.parse_exact(&before, &identity)
            .unwrap()
            .canonical_bytes(),
        before
    );
    assert_eq!(
        pair.parse_exact(&after, &identity)
            .unwrap()
            .canonical_bytes(),
        after
    );
    // Identical old OHLCV, but a different receipt may not rewrite a pending M10.
    let rebound = encoded(next.source());
    assert_ne!(before, rebound);
    assert!(pair.parse_exact(&rebound, &identity).is_err());
    assert!(pair.parse_exact(&after, &"22".repeat(32)).is_err());
    assert!(Stage8bP1ObservedM10WindowPair::new(next.clone(), old.clone()).is_err());
    assert!(Stage8bP1ObservedM10WindowPair::new(old.clone(), old.clone()).is_err());
    for lost_minute in [false, true] {
        let mut bars = next.source().bars().to_vec();
        if lost_minute {
            bars.remove(1);
        } else {
            bars[1].close += Decimal::new(5, 1);
        }
        let changed = ObservedM1Receipt::from_admitted_source(
            "cc".repeat(32),
            next.source().start(),
            next.source().end(),
            next.source().received_at(),
            p1_instrument(),
            bars,
        )
        .unwrap();
        assert!(Stage8bP1ObservedM10WindowPair::new(old.clone(), bind(changed)).is_err());
    }
    let foreign = Stage8bP1ObservedM10Binding::new(
        &"22".repeat(32),
        next_receipt(old.source()),
        next.source().sha256(),
    )
    .unwrap();
    assert!(Stage8bP1ObservedM10WindowPair::new(old, foreign).is_err());
}

#[test]
fn observed_retained_receipts_restore_exact_sealed_m10_without_replacing_prior_source() {
    use std::os::unix::fs::DirBuilderExt;
    let directory = std::env::temp_dir().join(format!(
        "observed-receipt-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let dir = &directory;
    let old = bind(receipt(&[1, 4, 7]));
    let next = bind(next_receipt(old.source()));
    old.persist_retained_receipt(dir).unwrap();
    next.persist_retained_receipt(dir).unwrap();
    old.persist_retained_receipt(dir).unwrap();
    assert_eq!(std::fs::read_dir(dir).unwrap().count(), 2);
    let bytes = encoded(old.source());
    let validated = old.parse_exact(&bytes, &"11".repeat(32)).unwrap();
    let restore = |input: &[u8]| {
        Stage8bP1ObservedM10Binding::restore_for_exact_m10(
            dir,
            input,
            &"11".repeat(32),
            validated.redis_id(),
            validated.semantic_id_sha256(),
            validated.payload_sha256(),
        )
    };
    assert_eq!(
        restore(&bytes).unwrap().source().retained_bytes(),
        old.source().retained_bytes()
    );
    assert!(restore(&encoded(next.source())).is_err());
    let mut forged: EnvelopeV2 = serde_json::from_slice(&encoded(next.source())).unwrap();
    // Preserve independently sealed outer IDs while replacing the payload.
    forged.m10_semantic_id_sha256 = validated.semantic_id_sha256().into();
    forged.m10_payload_sha256 = validated.payload_sha256().into();
    assert!(restore(&serde_json::to_vec(&forged).unwrap()).is_err());
    let target = dir.join(format!(
        "stage8b-observed-receipt-{}.json",
        old.source().sha256()
    ));
    std::fs::write(&target, b"corrupt").unwrap();
    assert!(restore(&bytes).is_err());
    assert!(old.persist_retained_receipt(dir).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"corrupt");
    std::fs::remove_file(&target).unwrap();
    assert!(restore(&bytes).is_err());
    let other = dir.join("other.json");
    std::fs::write(&other, old.source().retained_bytes()).unwrap();
    std::os::unix::fs::symlink(&other, &target).unwrap();
    assert!(restore(&bytes).is_err());
    assert!(old.persist_retained_receipt(dir).is_err());
    std::fs::remove_file(&target).unwrap();
    std::fs::hard_link(&other, &target).unwrap();
    assert!(restore(&bytes).is_err());
    assert!(Stage8bP1ObservedM10Binding::restore_retained_receipt(
        dir,
        &"11".repeat(32),
        "../other.json",
    )
    .is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn observed_local_delivery_reclaims_same_context_and_rejects_missing_or_swapped_context() {
    let source = receipt(&[1, 4, 7]);
    let bytes = encoded(&source);
    let mut stream = Stage8bP1LocalM10Stream::new(STAGE8B_P1_LOCAL_M10_MIN_RETENTION).unwrap();
    stream.create_canonical_group_mkstream();
    stream
        .publish_exact(parsed(&bytes, &source).unwrap())
        .unwrap();
    let delivery = stream.read_next_pending().unwrap();
    let reclaimed = stream
        .reclaim_exact_pending(
            delivery.redis_id(),
            delivery.semantic_id_sha256(),
            delivery.payload_sha256(),
        )
        .unwrap();
    assert_eq!(
        reclaimed
            .parse_exact(&"11".repeat(32))
            .unwrap()
            .canonical_bytes(),
        bytes
    );
    assert!(reclaimed.parse_exact(&"22".repeat(32)).is_err());
    let mut missing = delivery;
    missing.observed_binding = None;
    assert!(missing.parse_exact(&"11".repeat(32)).is_err());
    let changed = receipt(&[1, 7]);
    let hash = changed.sha256().to_string();
    missing.observed_binding =
        Some(Stage8bP1ObservedM10Binding::new(&"11".repeat(32), changed, &hash).unwrap());
    assert!(missing.parse_exact(&"11".repeat(32)).is_err());
    assert_eq!(stream.pending_count(), 1);
    assert_eq!(stream.acknowledged_count(), 0);
}

#[test]
fn sparse_canonical_v2_roundtrip_preserves_bounds_policy_and_exact_source() {
    let source = receipt(&[1, 4, 7]);
    let bytes = encoded(&source);
    let restored = ObservedM1Receipt::restore(source.retained_bytes(), source.sha256()).unwrap();
    assert_eq!(bytes, encoded(&restored));
    let v: EnvelopeV2 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v.payload.actual_minute_bitmap, 0b0010010010);
    assert_eq!(v.payload.source_m1.len(), 3);
    assert_eq!(v.payload.open, "2200.5");
    assert_eq!(v.payload.close, "2204");
    assert_eq!(v.payload.volume, "3");
    assert!(!String::from_utf8_lossy(&bytes).contains("raw_body"));
    let canonical = parsed(&bytes, &restored).unwrap();
    assert_eq!(canonical.open_ts_utc_ms(), open().timestamp_millis());
    assert_eq!(
        canonical.close_ts_utc_ms(),
        (open() + Duration::minutes(10)).timestamp_millis()
    );
    assert_eq!(canonical.canonical_bytes(), bytes);
    assert_eq!(canonical.source_policy(), OBSERVED_M1_POLICY_V1);
    assert_eq!(
        canonical.semantic_m10_identity().source_kind,
        OBSERVED_M1_POLICY_V1
    );
    assert!(canonical.into_stage5c_semantic_bar().is_ok());
}

#[test]
fn sparse_canonical_v2_all_counts_and_legacy_contract_are_distinct() {
    for count in 1..=10 {
        let source = receipt(&(0..count).collect::<Vec<_>>());
        let bytes = encoded(&source);
        assert!(parsed(&bytes, &source).is_ok());
        assert!(parse_stage8b_p1_canonical_m10(&bytes, &"11".repeat(32)).is_err());
        let v: EnvelopeV2 = serde_json::from_slice(&bytes).unwrap();
        let strict = build_stage8b_p1_canonical_m10(Stage8bP1CanonicalM10BuildInput {
            operational_identity_sha256: "11".repeat(32),
            open_ts_utc_ms: v.payload.open_ts_utc_ms,
            close_ts_utc_ms: v.payload.close_ts_utc_ms,
            open: v.payload.open,
            high: v.payload.high,
            low: v.payload.low,
            close: v.payload.close,
            volume: v.payload.volume,
            source_m1: v.payload.source_m1,
        });
        assert_eq!(strict.is_ok(), count == 10);
        if let Ok(strict) = strict {
            assert!(parse_stage8b_p1_canonical_m10(&strict, &"11".repeat(32)).is_ok());
            assert!(parsed(&strict, &source).is_err());
        }
    }
    assert!(build_stage8b_p1_observed_canonical_m10(
        &"11".repeat(32),
        open().timestamp_millis(),
        &receipt(&[])
    )
    .is_err());
}

#[test]
fn sparse_canonical_v2_cannot_rehash_lost_or_repriced_m1_with_old_binding() {
    let source = receipt(&[1, 4, 7]);
    for reprice in [false, true] {
        let mut rows = source.bars().to_vec();
        if reprice {
            rows[1].close += Decimal::new(5, 1);
        } else {
            rows.remove(1);
        }
        let changed = from_rows(rows);
        // Same raw reference, but a different full normalized receipt.
        assert_eq!(
            source.source_snapshot_sha256(),
            changed.source_snapshot_sha256()
        );
        assert_ne!(source.sha256(), changed.sha256());
        let changed_bytes = encoded(&changed); // Coherent aggregate and all hashes.
        assert!(parsed(&changed_bytes, &source).is_err());
        assert!(parse_stage8b_p1_observed_canonical_m10(
            &changed_bytes,
            &"11".repeat(32),
            &changed,
            source.sha256()
        )
        .is_err());
        assert!(ObservedM1Receipt::restore(changed.retained_bytes(), source.sha256()).is_err());
    }
}

#[test]
fn sparse_canonical_v2_rejects_rehashed_policy_identity_inventory_and_price_mutations() {
    let source = receipt(&[1, 4, 7]);
    let bytes = encoded(&source);
    let mutations: &[fn(&mut EnvelopeV2)] = &[
        |v| v.schema_version = 1,
        |v| v.identity_domain = "other".into(),
        |v| v.payload.policy = "silence-is-coverage".into(),
        |v| v.payload.source_snapshot_sha256 = "bb".repeat(32),
        |v| v.payload.source_receipt_sha256 = "bb".repeat(32),
        |v| v.payload.operational_identity_sha256 = "22".repeat(32),
        |v| v.payload.actual_minute_bitmap = 1023,
        |v| {
            v.payload.source_m1.pop();
        },
        |v| v.payload.source_m1.swap(0, 1),
        |v| v.payload.source_m1[0].payload_sha256 = "00".repeat(32),
        |v| v.payload.source_m1[0].semantic_id_sha256 = "00".repeat(32),
        |v| v.payload.source_m1[0].open_ts_utc_ms += 60000,
        |v| v.payload.source_range_end_utc_ms -= 60000,
        |v| v.payload.source_range_start_utc_ms += 60000,
        |v| v.payload.close_ts_utc_ms += 60000,
        |v| v.payload.high = "2300".into(),
        |v| v.payload.volume = "2".into(),
        |v| v.redis_id = "1-0".into(),
        |v| v.payload.is_final = false,
    ];
    for (i, mutate) in mutations.iter().enumerate() {
        let mut v: EnvelopeV2 = serde_json::from_slice(&bytes).unwrap();
        mutate(&mut v);
        let payload = serde_json::to_vec(&v.payload).unwrap();
        v.m10_payload_sha256 = sha256_hex(&payload);
        v.m10_semantic_id_sha256 =
            domain_sha256(OBSERVED_CANONICAL_M10_DOMAIN.as_bytes(), &payload);
        assert!(
            parsed(&serde_json::to_vec(&v).unwrap(), &source).is_err(),
            "mutation {i}"
        );
    }
    let mut extra = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
    extra["payload"]["gap_absence_proven"] = serde_json::json!(true);
    assert!(parsed(&serde_json::to_vec(&extra).unwrap(), &source).is_err());
    assert!(parse_stage8b_p1_observed_canonical_m10(
        &bytes,
        &"22".repeat(32),
        &source,
        source.sha256()
    )
    .is_err());
    assert!(parsed(&vec![b' '; 16 * 1024 + 1], &source).is_err());
}
