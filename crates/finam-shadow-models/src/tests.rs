use crate::{context::*, session::*, BarEvent, DataOrigin};
use chrono::{Duration, NaiveDate};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
fn day() -> NaiveDate {
    date("2026-10-26")
}
fn config() -> Config {
    serde_json::from_value(json!({"engine":"imoexf","sleeve":"combo","costs":{
        "point_value_rub":10.0,"tick_value_rub":5.0,
        "rate_per_component_per_side":0.00006575119054451348,"components":2}}))
    .unwrap()
}
fn identity(policy: Policy) -> Identity {
    Identity {
        profile: Profile::ImoexfReaction07,
        symbol: "IMOEXF".into(),
        model_config_sha256: config().fingerprint().unwrap(),
        receipt_stream: "fixture-only".into(),
        policy,
        excluded_dates: BTreeSet::new(),
    }
}
fn bar(ts: i64, price: f64) -> BarEvent {
    BarEvent {
        symbol: "IMOEXF".into(),
        close_time_utc: ts,
        o: price,
        h: price,
        l: price,
        close: price,
        v: 1.,
        origin: DataOrigin::Replay,
    }
}
fn context(policy: Policy) -> Store {
    let id = identity(policy);
    let mut store = Store::new(id.clone()).unwrap();
    for (d, weekend) in [
        ("2026-10-22", false),
        ("2026-10-23", false),
        ("2026-10-24", true),
        ("2026-10-25", true),
    ] {
        let d = date(d);
        let start = if weekend {
            (d.and_hms_opt(9, 50, 0).unwrap() - Duration::hours(3))
                .and_utc()
                .timestamp()
        } else {
            id.cutoff(d)
        };
        let count = if weekend { 55 } else { 101 };
        for slot in 0..count {
            let ts = start + slot * 600;
            let mut b = bar(ts, if weekend { 103. } else { 100. });
            b.h += 2.;
            b.l -= 2.;
            b.origin = DataOrigin::History;
            let seq = store.last_sequence() + 1;
            store
                .ingest(
                    Receipt {
                        sequence: seq,
                        source_id: format!("fixture-{seq}"),
                        bar: b,
                        received_at_utc: Some(ts + 600),
                        durable_at_utc: Some(ts + 600),
                    },
                    ts + 600,
                )
                .unwrap();
        }
    }
    store
}
fn freeze(store: &mut Store) -> Frozen {
    let cutoff = store.identity().cutoff(day());
    store
        .freeze(
            day(),
            cutoff,
            &Barrier {
                receipt_stream: store.identity().receipt_stream.clone(),
                through_utc: cutoff,
                last_sequence: store.last_sequence(),
                ingress_sha256: store.ingress_sha256().unwrap(),
                attested_at_utc: cutoff,
            },
        )
        .unwrap()
}
fn session() -> (Store, NumericalSession) {
    let mut c = context(Policy::WeekdayOnly);
    freeze(&mut c);
    let s = NumericalSession::new(&c, day(), config()).unwrap();
    (c, s)
}
fn model(s: &NumericalSession) -> Value {
    s.state().unwrap()["model"]["state"].clone()
}
fn apply(s: &mut NumericalSession, slot: i64, price: f64) {
    let ts = identity(Policy::WeekdayOnly).cutoff(day()) + slot * 600;
    s.observe(bar(ts, price), ts + 600).unwrap();
}

#[test]
fn context_policies_freeze_distinct_independent_anchors() {
    for (policy, previous, previous2) in [
        (Policy::WeekdayOnly, "2026-10-23", "2026-10-22"),
        (Policy::WeekendContextNoTrades, "2026-10-25", "2026-10-24"),
    ] {
        let mut c = context(policy);
        let f = freeze(&mut c);
        assert_eq!(f.previous.unwrap().session_date, date(previous));
        assert_eq!(f.previous2.unwrap().session_date, date(previous2));
    }
}

#[test]
fn selected_model_and_cost_policy_are_not_silently_replaceable() {
    let (c, _) = session();
    for (key, value) in [
        ("point_value_rub", json!(11.)),
        ("tick_value_rub", json!(6.)),
        ("rate_per_component_per_side", json!(0.001)),
        ("components", json!(1)),
    ] {
        let mut cfg = serde_json::to_value(config()).unwrap();
        cfg["costs"][key] = value;
        let cfg: Config = serde_json::from_value(cfg).unwrap();
        assert!(NumericalSession::new(&c, day(), cfg).is_err());
    }
    let mut cfg = serde_json::to_value(config()).unwrap();
    cfg["sleeve"] = json!("bo");
    assert!(NumericalSession::new(&c, day(), serde_json::from_value(cfg).unwrap()).is_err());
}

#[test]
fn reaction_fill_uses_next_open_not_signal_close_or_exit_threshold() {
    let (_, mut s) = session();
    apply(&mut s, 0, 100.1);
    assert!(model(&s)["position"].is_null());
    assert_eq!(model(&s)["pending"]["at"], "2026-10-26T07:10:00");
    let ts = identity(Policy::WeekdayOnly).cutoff(day()) + 600;
    let mut b = bar(ts, 99.9);
    b.o = 99.5;
    b.l = 99.5;
    b.h = 100.;
    s.observe(b, ts + 600).unwrap();
    assert_eq!(model(&s)["position"]["price"], 99.5);
    assert_eq!(model(&s)["pending"]["exit_reason"], "take_author_close");
    apply(&mut s, 2, 100.2);
    assert_eq!(model(&s)["trades"][0]["exit_price"], 100.2);
    assert_eq!(model(&s)["trades"][0]["entry_event"], "2026-10-26T07:10:00");
}

#[test]
fn observer_continues_while_mr_owns_the_slot() {
    let (_, mut s) = session();
    for slot in 0..6 {
        apply(&mut s, slot, 100.1);
    }
    assert_eq!(model(&s)["observation"]["count"], 6);
    assert_eq!(model(&s)["position"]["entry"]["owner"], "MR");
    assert_eq!(model(&s)["mr_entries"], 1);
    assert_eq!(s.state().unwrap()["order_emission_allowed"], false);
}

#[test]
fn wrong_contract_origin_grid_clock_and_ohlcv_are_atomic_rejections() {
    let (_, s) = session();
    let ts = identity(Policy::WeekdayOnly).cutoff(day());
    let mut samples = Vec::new();
    let b = bar(ts, 100.1);
    let mut x = b.clone();
    x.symbol = "USDRUBF".into();
    samples.push((x, ts + 600));
    let mut x = b.clone();
    x.origin = DataOrigin::Live;
    samples.push((x, ts + 600));
    let mut x = b.clone();
    x.close_time_utc += 600;
    samples.push((x, ts + 1200));
    let mut x = b.clone();
    x.h = 99.;
    samples.push((x, ts + 600));
    let mut x = b.clone();
    x.v = -1.;
    samples.push((x, ts + 600));
    let mut x = b.clone();
    x.close = f64::NAN;
    samples.push((x, ts + 600));
    samples.push((b, ts + 599));
    for (b, now) in samples {
        let mut copy = s.clone();
        let before = copy.checkpoint_bytes().unwrap();
        assert!(copy.observe(b, now).is_err());
        assert_eq!(copy.checkpoint_bytes().unwrap(), before);
    }
}

#[test]
fn duplicate_execution_never_applies_a_second_fill() {
    let (_, mut s) = session();
    apply(&mut s, 0, 100.1);
    apply(&mut s, 1, 100.1);
    let before = s.checkpoint_bytes().unwrap();
    let ts = identity(Policy::WeekdayOnly).cutoff(day()) + 600;
    assert!(s.observe(bar(ts, 100.1), ts + 600).is_err());
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
}

#[test]
fn pending_and_open_position_restore_exactly_without_new_events() {
    let (c, mut s) = session();
    for slot in 0..4 {
        apply(&mut s, slot, 100.1);
        let state = s.state().unwrap();
        let bytes = s.checkpoint_bytes().unwrap();
        s = NumericalSession::restore(
            c.identity(),
            &config(),
            &bytes,
            c.identity().cutoff(day()) + (slot + 1) * 600,
        )
        .unwrap();
        assert_eq!(s.state().unwrap(), state);
    }
}

#[test]
fn checkpoint_rejects_identity_rebind_tampering_and_clock_regression() {
    let (c, mut s) = session();
    apply(&mut s, 0, 100.1);
    let now = c.identity().cutoff(day()) + 600;
    let bytes = s.checkpoint_bytes().unwrap();
    let mut id = c.identity().clone();
    id.policy = Policy::WeekendContextNoTrades;
    assert!(NumericalSession::restore(&id, &config(), &bytes, now).is_err());
    assert!(NumericalSession::restore(c.identity(), &config(), &bytes, now - 1).is_err());
    let mut value: Value = serde_json::from_slice(&bytes).unwrap();
    value["payload"]["witness"]["model"]["state"]["mr_entries"] = json!(2);
    assert!(NumericalSession::restore(
        c.identity(),
        &config(),
        &serde_json::to_vec(&value).unwrap(),
        now
    )
    .is_err());
    // A recomputed checksum cannot make a false reconstructed witness valid.
    // Typed payload ordering is obtained from the original serialized prefix.
    let payload = String::from_utf8(bytes.clone()).unwrap();
    let changed = payload.replace("\"mr_entries\":0", "\"mr_entries\":2");
    let start = changed.find("\"payload\":").unwrap() + 10;
    let end = changed.rfind(",\"sha256\":").unwrap();
    let checksum = format!("{:x}", Sha256::digest(&changed.as_bytes()[start..end]));
    let old: Value = serde_json::from_slice(&bytes).unwrap();
    let changed = changed.replace(old["sha256"].as_str().unwrap(), &checksum);
    let error = NumericalSession::restore(c.identity(), &config(), changed.as_bytes(), now)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("reconstructed decision state differs"),
        "{error}"
    );
}

#[test]
fn duplicate_and_conflicting_context_are_append_only_without_anchor_rewrite() {
    let id = identity(Policy::WeekdayOnly);
    let ts = id.cutoff(date("2026-10-23"));
    let mut c = Store::new(id).unwrap();
    let mut b = bar(ts, 100.);
    b.origin = DataOrigin::History;
    let first = Receipt {
        sequence: 1,
        source_id: "original".into(),
        bar: b,
        received_at_utc: Some(ts + 600),
        durable_at_utc: Some(ts + 600),
    };
    assert_eq!(
        c.ingest(first.clone(), ts + 600).unwrap(),
        Route::ContextOnly
    );
    let mut duplicate = first.clone();
    duplicate.sequence = 2;
    assert_eq!(c.ingest(duplicate, ts + 600).unwrap(), Route::Duplicate);
    let mut conflict = first;
    conflict.sequence = 3;
    conflict.source_id = "revision".into();
    conflict.bar.v = 2.;
    assert_eq!(
        c.ingest(conflict, ts + 600).unwrap(),
        Route::Quarantined(Reject::ConflictingQuote)
    );
    assert_eq!(c.accepted_count(), 1);
    assert_eq!(c.last_sequence(), 3);
}

#[test]
fn missing_terminal_bar_is_incomplete_not_fabricated_flat() {
    let (_, mut s) = session();
    for slot in 0..100 {
        apply(&mut s, slot, 104.);
    }
    assert!(s.complete().is_err());
    assert!(!model(&s)["pending"].is_null());
    apply(&mut s, 100, 104.);
    s.complete().unwrap();
    assert!(model(&s)["position"].is_null());
    assert!(model(&s)["pending"].is_null());
}

#[test]
fn late_and_conflicting_context_cannot_rewrite_frozen_model() {
    let (mut c, s) = session();
    let before = s.state().unwrap();
    let frozen = c.frozen(day()).unwrap().clone();
    let ts = c.identity().cutoff(date("2026-10-23"));
    let now = c.identity().cutoff(day());
    let mut b = bar(ts, 99.);
    b.origin = DataOrigin::History;
    let r = Receipt {
        sequence: c.last_sequence() + 1,
        source_id: "late-correction".into(),
        bar: b,
        received_at_utc: Some(now),
        durable_at_utc: Some(now),
    };
    assert!(matches!(c.ingest(r, now).unwrap(), Route::Quarantined(_)));
    assert_eq!(c.frozen(day()), Some(&frozen));
    assert_eq!(s.state().unwrap(), before);
}

#[test]
fn cutoff_requires_exact_ingress_barrier_and_two_anchors() {
    let mut c = context(Policy::WeekdayOnly);
    let now = c.identity().cutoff(day());
    let bad = Barrier {
        receipt_stream: c.identity().receipt_stream.clone(),
        through_utc: now,
        last_sequence: c.last_sequence() - 1,
        ingress_sha256: c.ingress_sha256().unwrap(),
        attested_at_utc: now,
    };
    assert!(c.freeze(day(), now, &bad).is_err());
    assert!(c.frozen(day()).is_none());
    let mut empty = Store::new(identity(Policy::WeekdayOnly)).unwrap();
    freeze(&mut empty);
    assert!(NumericalSession::new(&empty, day(), config()).is_err());
}

#[test]
fn fixture_rejects_empty_wrong_evidence_and_expected_output_injection() {
    assert!(crate::fixture::run(b"{}", "test").is_err());
    for kind in [
        "synthetic_numerical_reaction_not_production",
        "observed_runtime",
    ] {
        let bytes = serde_json::to_vec(&json!({"schema_version":"weekend_reaction_fixture_v1",
            "evidence_kind":kind,"cases":[]}))
        .unwrap();
        assert!(crate::fixture::run(&bytes, "test").is_err());
    }
    let bytes = serde_json::to_vec(&json!({"schema_version":"weekend_reaction_fixture_v1",
        "evidence_kind":"synthetic_numerical_reaction_not_production","cases":[],"expected":[]}))
    .unwrap();
    assert!(crate::fixture::run(&bytes, "test").is_err());
}
