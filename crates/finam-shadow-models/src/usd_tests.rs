use crate::{context::*, session::*, usdrubf, BarEvent, DataOrigin};
use chrono::{Duration, NaiveDate};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
fn day() -> NaiveDate {
    date("2026-10-26")
}
fn config() -> Config {
    serde_json::from_value(json!({"engine":"usdrubf","settings":{
        "model_id":"previous_day07_h4_w4","family":"previous_day","context_hour":7,
        "clock_hour":7,"mr_horizon":4,"bo_wait":4,"mr_start_delay":0}}))
    .unwrap()
}
fn identity(policy: Policy) -> Identity {
    Identity {
        profile: Profile::UsdrubfPreviousDay07,
        symbol: "USDRUBF".into(),
        model_config_sha256: config().fingerprint().unwrap(),
        receipt_stream: "usd-fixture".into(),
        policy,
        excluded_dates: BTreeSet::new(),
    }
}
fn bar(ts: i64, price: f64) -> BarEvent {
    BarEvent {
        symbol: "USDRUBF".into(),
        close_time_utc: ts,
        o: price,
        h: price,
        l: price,
        close: price,
        v: 1.,
        origin: DataOrigin::Replay,
    }
}
fn session(policy: Policy) -> (Store, NumericalSession) {
    let id = identity(policy);
    let mut c = Store::new(id.clone()).unwrap();
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
        for slot in 0..if weekend { 55 } else { 101 } {
            let ts = start + slot * 600;
            let mut b = bar(ts, if weekend { 103. } else { 100. });
            b.h += 2.;
            b.l -= 2.;
            b.origin = DataOrigin::History;
            let seq = c.last_sequence() + 1;
            c.ingest(
                Receipt {
                    sequence: seq,
                    source_id: format!("usd-{seq}"),
                    bar: b,
                    received_at_utc: Some(ts + 600),
                    durable_at_utc: Some(ts + 600),
                },
                ts + 600,
            )
            .unwrap();
        }
    }
    let now = id.cutoff(day());
    c.freeze(
        day(),
        now,
        &Barrier {
            receipt_stream: id.receipt_stream.clone(),
            through_utc: now,
            last_sequence: c.last_sequence(),
            ingress_sha256: c.ingress_sha256().unwrap(),
            attested_at_utc: now,
        },
    )
    .unwrap();
    let s = NumericalSession::new(&c, day(), config()).unwrap();
    (c, s)
}
fn apply(s: &mut NumericalSession, slot: i64, price: f64) {
    let ts = identity(Policy::WeekdayOnly).cutoff(day()) + slot * 600;
    s.observe(bar(ts, price), ts + 600).unwrap();
}
fn model(s: &NumericalSession) -> Value {
    s.state().unwrap()["model"]["state"].clone()
}

#[test]
fn only_h4_w4_previous_day07_is_selected() {
    let (c, _) = session(Policy::WeekdayOnly);
    for (field, value) in [
        ("model_id", json!("previous_day07_h3_w2")),
        ("family", json!("vwap")),
        ("context_hour", json!(9)),
        ("clock_hour", json!(9)),
        ("mr_horizon", json!(3)),
        ("bo_wait", json!(2)),
        ("mr_start_delay", json!(1)),
    ] {
        let mut cfg = serde_json::to_value(config()).unwrap();
        cfg["settings"][field] = value;
        let cfg: Config = serde_json::from_value(cfg).unwrap();
        assert!(NumericalSession::new(&c, day(), cfg).is_err());
    }
}

#[test]
fn usd_context_arms_use_their_own_frozen_prior_days() {
    for (policy, expected) in [
        (Policy::WeekdayOnly, "2026-10-23"),
        (Policy::WeekendContextNoTrades, "2026-10-25"),
    ] {
        let (c, _) = session(policy);
        assert_eq!(
            c.frozen(day())
                .unwrap()
                .previous
                .as_ref()
                .unwrap()
                .session_date,
            date(expected)
        );
    }
    let mut id = identity(Policy::WeekdayOnly);
    id.symbol = "IMOEXF".into();
    assert!(Store::new(id).is_err());
}

#[test]
fn mr_stop_take_are_entry_open_relative_and_rounded_half_up() {
    let (_, mut s) = session(Policy::WeekdayOnly);
    apply(&mut s, 0, 100.1);
    assert!(model(&s)["position"].is_null());
    apply(&mut s, 1, 100.);
    let p = &model(&s)["position"];
    assert_eq!(p["price"], 100.);
    assert_eq!(p["stop"], 101.72);
    assert_eq!(p["take"], 99.36);
    assert_eq!(usdrubf::round_tick(100.125), 100.13);
}

#[test]
fn ambiguous_mr_touch_selects_stop_but_fills_next_open() {
    let (_, mut s) = session(Policy::WeekdayOnly);
    apply(&mut s, 0, 100.1);
    apply(&mut s, 1, 100.);
    let ts = identity(Policy::WeekdayOnly).cutoff(day()) + 1200;
    let mut b = bar(ts, 100.);
    b.h = 102.;
    b.l = 99.;
    s.observe(b, ts + 600).unwrap();
    assert_eq!(model(&s)["pending"]["Exit"]["reason"], "mr_stop");
    assert_eq!(model(&s)["pending"]["Exit"]["ambiguous"], true);
    apply(&mut s, 3, 100.4);
    let t = &model(&s)["trades"][0];
    assert_eq!(t["exit_price"], 100.4);
    assert_eq!(t["ambiguous_mr_bar"], true);
}

#[test]
fn mr_h4_cutoff_is_1050_signal_1100_action() {
    let (_, mut s) = session(Policy::WeekdayOnly);
    for slot in 0..24 {
        apply(&mut s, slot, 100.1);
    }
    assert_eq!(
        model(&s)["pending"]["Exit"]["signal"],
        "2026-10-26T10:50:00"
    );
    assert_eq!(
        model(&s)["pending"]["Exit"]["action"],
        "2026-10-26T11:00:00"
    );
    apply(&mut s, 24, 100.1);
    assert!(model(&s)["position"].is_null());
}

#[test]
fn bo_wait4_starts_at_1100_not_imoexf_wait3() {
    let (_, mut s) = session(Policy::WeekdayOnly);
    for slot in 0..24 {
        apply(&mut s, slot, 104.);
        assert!(model(&s)["pending"].is_null());
    }
    apply(&mut s, 24, 104.);
    assert_eq!(
        model(&s)["pending"]["Entry"]["signal"],
        "2026-10-26T11:00:00"
    );
    assert_eq!(
        model(&s)["pending"]["Entry"]["action"],
        "2026-10-26T11:10:00"
    );
}

#[test]
fn bo_stop2_uses_touch_and_precedes_eod() {
    let (_, mut s) = session(Policy::WeekdayOnly);
    for slot in 0..99 {
        apply(&mut s, slot, 104.);
    }
    let ts = identity(Policy::WeekdayOnly).cutoff(day()) + 99 * 600;
    let mut b = bar(ts, 104.);
    b.l = 98.;
    s.observe(b, ts + 600).unwrap();
    assert_eq!(model(&s)["pending"]["Exit"]["reason"], "bo_stop2_long");
    apply(&mut s, 100, 104.5);
    s.complete().unwrap();
    assert_eq!(model(&s)["trades"][0]["exit_price"], 104.5);
    assert_eq!(model(&s)["trades"][0]["exit_reason"], "bo_stop2_long");
}

#[test]
fn mr_scheduled_counter_is_not_a_max2_quota() {
    let (_, mut s) = session(Policy::WeekdayOnly);
    for slot in 0..12 {
        let ts = identity(Policy::WeekdayOnly).cutoff(day()) + slot * 600;
        let mut b = bar(ts, 100.1);
        if slot % 3 == 2 {
            b.h = 102.;
        }
        s.observe(b, ts + 600).unwrap();
    }
    assert!(model(&s)["mr_scheduled"].as_u64().unwrap() > 2);
}

#[test]
fn overflow_and_negative_volume_are_atomic() {
    let (_, mut s) = session(Policy::WeekdayOnly);
    apply(&mut s, 0, 100.1);
    let before = s.checkpoint_bytes().unwrap();
    let ts = identity(Policy::WeekdayOnly).cutoff(day()) + 600;
    for volume in [f64::MAX, -1., f64::NAN] {
        let mut b = bar(ts, 100.1);
        b.v = volume;
        assert!(s.observe(b, ts + 600).is_err());
        assert_eq!(s.checkpoint_bytes().unwrap(), before);
    }
}

#[test]
fn usd_pending_position_metrics_and_kahan_restore_without_reexecution() {
    let (c, mut s) = session(Policy::WeekdayOnly);
    for slot in 0..5 {
        let ts = c.identity().cutoff(day()) + slot * 600;
        let mut b = bar(ts, 100.1);
        b.v = 1.125 + slot as f64 / 10.;
        s.observe(b, ts + 600).unwrap();
        let before = s.state().unwrap();
        s = NumericalSession::restore(
            c.identity(),
            &config(),
            &s.checkpoint_bytes().unwrap(),
            ts + 600,
        )
        .unwrap();
        assert_eq!(s.state().unwrap(), before);
    }
    assert_eq!(model(&s)["events"].as_array().unwrap().len(), 1);
}

#[test]
fn usd_cannot_restore_into_imoexf_or_another_policy() {
    let (c, mut s) = session(Policy::WeekdayOnly);
    apply(&mut s, 0, 100.1);
    let now = c.identity().cutoff(day()) + 600;
    let bytes = s.checkpoint_bytes().unwrap();
    let mut id = c.identity().clone();
    id.profile = Profile::ImoexfReaction07;
    id.symbol = "IMOEXF".into();
    assert!(NumericalSession::restore(&id, &config(), &bytes, now).is_err());
    let mut id = c.identity().clone();
    id.policy = Policy::WeekendContextNoTrades;
    assert!(NumericalSession::restore(&id, &config(), &bytes, now).is_err());
}

#[test]
fn usd_missing_terminal_and_duplicate_do_not_fake_completion() {
    let (_, mut s) = session(Policy::WeekdayOnly);
    for slot in 0..100 {
        apply(&mut s, slot, 104.);
    }
    assert!(s.complete().is_err());
    let before = s.checkpoint_bytes().unwrap();
    let ts = identity(Policy::WeekdayOnly).cutoff(day()) + 99 * 600;
    assert!(s.observe(bar(ts, 104.), ts + 600).is_err());
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    apply(&mut s, 100, 104.);
    s.complete().unwrap();
    assert!(model(&s)["position"].is_null());
    assert!(model(&s)["pending"].is_null());
}
