use crate::ri::{selected_identity, RiSession, Step};
use crate::ri_context::*;
use crate::{BarEvent, DataOrigin};
use chrono::{Datelike, Duration, NaiveDate};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
mod bootstrap;

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
fn day() -> NaiveDate {
    date("2026-10-13")
}
fn epoch(d: NaiveDate, h: u32, m: u32) -> i64 {
    (d.and_hms_opt(h, m, 0).unwrap() - Duration::hours(3))
        .and_utc()
        .timestamp()
}
fn bar(ts: i64, price: f64) -> BarEvent {
    BarEvent {
        symbol: "RIZ6".into(),
        close_time_utc: ts,
        o: price,
        h: price,
        l: price,
        close: price,
        v: 1.,
        origin: DataOrigin::Live,
    }
}

fn context(d: NaiveDate, policy: ContextPolicy) -> RiContextStore {
    let id = selected_identity(policy, "ri-unit-fixture".into()).unwrap();
    let mut c = RiContextStore::new(id.clone()).unwrap();
    let mut dates = vec![];
    let mut prev = d;
    while dates.len() < 2 {
        prev = prev.pred_opt().unwrap();
        if prev.weekday().num_days_from_monday() < 5 && !id.excluded_dates.contains(&prev) {
            dates.push(prev);
        }
    }
    dates.reverse();
    for d in dates {
        for ts in (id.cutoff(d)..=epoch(d, 23, 30)).step_by(600) {
            let mut b = bar(ts, 100.);
            b.h = 104.;
            b.l = 96.;
            b.origin = DataOrigin::History;
            c.ingest(
                ContextInput {
                    sequence: c.last_sequence() + 1,
                    bar: b,
                    received_at_utc: Some(ts + 600),
                    durable_at_utc: Some(ts + 600),
                },
                ts + 600,
            )
            .unwrap();
        }
    }
    c
}
fn freeze(c: &mut RiContextStore, d: NaiveDate, at: i64) {
    let id = c.identity();
    let barrier = ReceiptBarrier {
        receipt_stream: id.receipt_stream.clone(),
        through_utc: id.cutoff(d),
        last_sequence: c.last_sequence(),
        ingress_sha256: c.ingress_sha256().unwrap(),
        attested_at_utc: at,
    };
    c.freeze(d, at, &barrier).unwrap();
}
fn setup(d: NaiveDate) -> (RiContextStore, RiSession) {
    let mut c = context(d, ContextPolicy::RuntimeAcceptedOnly);
    let at = c.identity().cutoff(d);
    freeze(&mut c, d, at);
    let s = RiSession::new(&c, d, "unit-generation".into()).unwrap();
    (c, s)
}
fn process(c: &mut RiContextStore, s: &mut RiSession, b: BarEvent) -> Step {
    let now = b.close_time_utc + 600;
    let input = ContextInput {
        sequence: c.last_sequence() + 1,
        bar: b,
        received_at_utc: Some(now),
        durable_at_utc: Some(now),
    };
    c.ingest(input.clone(), now).unwrap();
    s.process(c, input, now).unwrap()
}
fn at(c: &mut RiContextStore, s: &mut RiSession, h: u32, m: u32, price: f64) -> Step {
    process(c, s, bar(epoch(day(), h, m), price))
}
fn bo_pending() -> (RiContextStore, RiSession) {
    let (mut c, mut s) = setup(day());
    for m in (0..60).step_by(10) {
        at(&mut c, &mut s, 7, m, 103.);
    }
    let out = at(&mut c, &mut s, 8, 50, 104.);
    assert_eq!(out.state["pending"]["side"], "long");
    (c, s)
}

#[test]
fn ri_profile_calendar_contract_and_generation_are_bound() {
    let id = selected_identity(ContextPolicy::RuntimeAcceptedOnly, "ri-profile".into()).unwrap();
    assert_eq!(id.symbol, "RIZ6");
    assert!(id.excluded_dates.contains(&date("2026-11-04")));
    let mut wrong = id;
    wrong.symbol = "RIH7".into();
    let mut c = RiContextStore::new(wrong).unwrap();
    let at = c.identity().cutoff(day());
    freeze(&mut c, day(), at);
    assert!(RiSession::new(&c, day(), "unit".into()).is_err());
    let (c, _) = setup(day());
    assert!(RiSession::new(&c, day(), " ".into()).is_err());
}

#[test]
fn ri_closing_recovery_changes_context_only_in_the_selected_policy() {
    for (policy, want, reason) in [
        (
            ContextPolicy::RuntimeAcceptedOnly,
            100.,
            Some(QuarantineReason::RecoveryDisabled),
        ),
        (ContextPolicy::RiConfirmedClosingContextV1, 102., None),
    ] {
        let mut c = context(day(), policy);
        let d = day().pred_opt().unwrap();
        let ts = epoch(d, 23, 40);
        let mut b = bar(ts, 102.);
        b.origin = DataOrigin::HistoryGap;
        let route = c
            .ingest(
                ContextInput {
                    sequence: c.last_sequence() + 1,
                    bar: b,
                    received_at_utc: Some(ts + 600),
                    durable_at_utc: Some(ts + 600),
                },
                ts + 600,
            )
            .unwrap();
        assert_eq!(
            route,
            reason.map_or(ContextRoute::ContextOnly, ContextRoute::Quarantined)
        );
        freeze(&mut c, day(), epoch(day(), 7, 0));
        assert_eq!(
            c.frozen(day()).unwrap().previous.as_ref().unwrap().close,
            want
        );
        let s = RiSession::new(&c, day(), "closing-context".into()).unwrap();
        assert_eq!(s.state()["mr_total"], 0);
        assert_eq!(s.state()["bo_total"], 0);
    }
}

#[test]
fn ri_nonclosing_recovery_and_weekend_cannot_be_anchors() {
    let id = selected_identity(
        ContextPolicy::RiConfirmedClosingContextV1,
        "ri-origin".into(),
    )
    .unwrap();
    let mut c = RiContextStore::new(id).unwrap();
    for (ts, origin, reason) in [
        (
            epoch(date("2026-10-09"), 23, 30),
            DataOrigin::HistoryGap,
            QuarantineReason::NonClosingRecovery,
        ),
        (
            epoch(date("2026-10-10"), 10, 0),
            DataOrigin::History,
            QuarantineReason::NonModelBar,
        ),
    ] {
        let mut b = bar(ts, 100.);
        b.origin = origin;
        let route = c
            .ingest(
                ContextInput {
                    sequence: c.last_sequence() + 1,
                    bar: b,
                    received_at_utc: Some(ts + 600),
                    durable_at_utc: Some(ts + 600),
                },
                ts + 600,
            )
            .unwrap();
        assert_eq!(route, ContextRoute::Quarantined(reason));
    }
}

#[test]
fn ri_missing_previous2_stays_not_ready_without_synthetic_anchor() {
    let id = selected_identity(ContextPolicy::RuntimeAcceptedOnly, "ri-empty".into()).unwrap();
    let mut c = RiContextStore::new(id).unwrap();
    freeze(&mut c, day(), epoch(day(), 7, 0));
    let mut s = RiSession::new(&c, day(), "not-ready".into()).unwrap();
    let out = at(&mut c, &mut s, 7, 0, 100.5);
    assert_eq!(out.disposition, "context_not_ready");
    assert_eq!(out.state["mr_total"], 0);
    assert_eq!(out.state["bo_total"], 0);
    assert!(out.model_marks.is_empty());
}

#[test]
fn ri_mr_uses_same_bar_close_then_stop_level_not_next_open() {
    let (mut c, mut s) = setup(day());
    let entry = at(&mut c, &mut s, 7, 0, 100.5);
    assert_eq!(entry.state["mr"]["entry_price"], 100.5);
    assert_eq!(entry.model_marks[0].model_mark_price, 100.5);
    let mut b = bar(epoch(day(), 7, 10), 99.);
    b.o = 100.;
    b.h = 106.;
    b.l = 99.;
    let out = process(&mut c, &mut s, b);
    assert_eq!(out.state["actions"][0]["reason"], "stop");
    assert_eq!(out.model_marks[0].model_mark_price, 106.);
    assert_eq!(out.state["actions"][0]["pnl"], -7.5);
    assert_eq!(out.state["phase"], "flat");
    assert!(out.state["mr"].is_null());
}

#[test]
fn ri_mr_take_is_strict_and_max2_is_shared_across_sides() {
    let (mut c, mut s) = setup(day());
    at(&mut c, &mut s, 7, 0, 99.5);
    let out = at(&mut c, &mut s, 7, 10, 100.04);
    assert!(out.model_marks.is_empty());
    at(&mut c, &mut s, 7, 20, 100.05);
    at(&mut c, &mut s, 7, 30, 100.5);
    at(&mut c, &mut s, 7, 40, 99.);
    let out = at(&mut c, &mut s, 7, 50, 99.5);
    assert_eq!(out.state["mr_total"], 2);
    assert!(out.state["mr"].is_null());
    assert!(out.state["branches"]
        .as_array()
        .unwrap()
        .contains(&json!("mr:entry_quota")));
}

#[test]
fn ri_inclusive_10_entry_has_no_forced_10_exit() {
    let (mut c, mut s) = setup(day());
    let out = at(&mut c, &mut s, 10, 0, 100.5);
    assert_eq!(out.state["mr_total"], 1);
    let out = at(&mut c, &mut s, 10, 10, 100.5);
    assert!(out.model_marks.is_empty());
    assert_eq!(out.state["phase"], "live_in_position");
}

#[test]
fn ri_observer_and_sixth_levels_advance_while_mr_owns_slot() {
    let (mut c, mut s) = setup(day());
    for m in (0..60).step_by(10) {
        at(&mut c, &mut s, 7, m, 100.5);
    }
    let state = s.state();
    assert_eq!(state["observation_count"], 6);
    assert_eq!(state["mr_total"], 1);
    assert_eq!(state["long_level"], 103.36);
    assert_eq!(state["short_level"], 96.64);
    assert_eq!(state["lifecycle_owner"], "author41_mr");
}

#[test]
fn ri_friday_and_seasonal_bans_do_not_disable_mr() {
    for d in [date("2026-10-16"), date("2026-05-21")] {
        let (mut c, mut s) = setup(d);
        let ts = c.identity().cutoff(d);
        let out = process(&mut c, &mut s, bar(ts, 100.5));
        assert_eq!(out.state["mr_total"], 1);
        assert_eq!(out.state["trade_allowed"], false);
    }
}

#[test]
fn ri_bo_pending_entry_and_exit_use_next_observed_open() {
    let (mut c, mut s) = bo_pending();
    let mut b = bar(epoch(day(), 9, 0), 104.5);
    b.o = 105.;
    b.h = 105.;
    let out = process(&mut c, &mut s, b);
    assert_eq!(out.state["bo"]["entry_price"], 105.);
    assert_eq!(out.model_marks[0].model_mark_price, 105.);
    assert_eq!(out.state["bo_total"], 1);
    at(&mut c, &mut s, 9, 10, 101.);
    assert_eq!(
        s.state()["pending"]["reason"],
        "stop_emergency_long_next_open"
    );
    let mut b = bar(epoch(day(), 9, 20), 102.5);
    b.o = 102.;
    b.l = 102.;
    let out = process(&mut c, &mut s, b);
    assert_eq!(out.model_marks[0].model_mark_price, 102.);
    assert_eq!(out.state["actions"][0]["pnl"], -3.);
    assert_eq!(out.state["phase"], "flat");
}

#[test]
fn ri_mr_priority_over_pending_bo_preserves_no_overlap() {
    let (mut c, mut s) = bo_pending();
    let mut b = bar(epoch(day(), 9, 0), 100.5);
    b.o = 104.;
    b.h = 104.;
    let out = process(&mut c, &mut s, b);
    assert_eq!(out.state["mr_total"], 1);
    assert_eq!(out.state["bo_total"], 0);
    assert_eq!(out.state["pending"]["kind"], "entry");
    let out = at(&mut c, &mut s, 9, 10, 100.5);
    assert!(out.state["pending"].is_null());
    assert!(out.state["bo"].is_null());
    assert_eq!(out.state["lifecycle_owner"], "author41_mr");
}

#[test]
fn ri_bo_eod_is_same_bar_close_and_prefix_is_not_a_full_day() {
    let (mut c, mut s) = bo_pending();
    at(&mut c, &mut s, 9, 0, 105.);
    let mut b = bar(epoch(day(), 23, 0), 106.);
    b.o = 105.;
    b.l = 105.;
    let out = process(&mut c, &mut s, b);
    assert_eq!(
        out.state["actions"][0]["reason"],
        "time_exit_same_bar_close"
    );
    assert_eq!(out.model_marks[0].model_mark_price, 106.);
    assert_eq!(out.state["actions"][0]["pnl"], 1.);
    at(&mut c, &mut s, 23, 40, 106.);
    assert!(s.complete().is_err());
}

#[test]
fn ri_exact_full_calendar_can_complete_but_missing_or_quarantined_slot_cannot() {
    for corrupt in [false, true] {
        let (mut c, mut s) = setup(day());
        let start = c.identity().cutoff(day());
        for slot in 0..101 {
            let mut b = bar(start + slot * 600, 100.);
            if corrupt && slot == 10 {
                b.h = 99.;
            }
            process(&mut c, &mut s, b);
        }
        assert_eq!(s.complete().is_ok(), !corrupt);
        assert_eq!(s.state()["phase"], "flat");
    }
}

#[test]
fn ri_duplicate_receipt_cannot_reapply_decision_or_mark() {
    let (mut c, mut s) = setup(day());
    let ts = epoch(day(), 7, 0);
    let i = ContextInput {
        sequence: c.last_sequence() + 1,
        bar: bar(ts, 100.5),
        received_at_utc: Some(ts + 600),
        durable_at_utc: Some(ts + 600),
    };
    c.ingest(i.clone(), ts + 600).unwrap();
    s.process(&c, i.clone(), ts + 600).unwrap();
    let before = s.checkpoint_bytes().unwrap();
    assert!(s.process(&c, i, ts + 600).is_err());
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
}

#[test]
fn ri_early_receipt_and_excluded_date_are_not_model_inputs() {
    let (mut c, mut s) = setup(day());
    let ts = epoch(day(), 7, 0);
    let i = ContextInput {
        sequence: c.last_sequence() + 1,
        bar: bar(ts, 100.5),
        received_at_utc: Some(ts + 599),
        durable_at_utc: Some(ts + 599),
    };
    assert_eq!(
        c.ingest(i.clone(), ts + 600).unwrap(),
        ContextRoute::Quarantined(QuarantineReason::BarNotClosed)
    );
    let out = s.process(&c, i, ts + 600).unwrap();
    assert!(out.model_marks.is_empty());
    assert_eq!(s.state()["mr_total"], 0);
    let mut c = context(date("2026-11-04"), ContextPolicy::RuntimeAcceptedOnly);
    let ts = epoch(date("2026-11-04"), 7, 0);
    let i = ContextInput {
        sequence: c.last_sequence() + 1,
        bar: bar(ts, 100.),
        received_at_utc: Some(ts + 600),
        durable_at_utc: Some(ts + 600),
    };
    assert_eq!(
        c.ingest(i, ts + 600).unwrap(),
        ContextRoute::Quarantined(QuarantineReason::NonModelBar)
    );
}

#[test]
fn ri_late_history_cannot_rewrite_frozen_anchor() {
    let (mut c, s) = setup(day());
    let frozen = c.frozen(day()).cloned();
    let state = s.state();
    let now = epoch(day(), 7, 0);
    let mut b = bar(epoch(date("2026-10-12"), 23, 40), 102.);
    b.origin = DataOrigin::HistoryGap;
    let i = ContextInput {
        sequence: c.last_sequence() + 1,
        bar: b,
        received_at_utc: Some(now),
        durable_at_utc: Some(now),
    };
    assert_eq!(
        c.ingest(i, now).unwrap(),
        ContextRoute::Quarantined(QuarantineReason::AtOrAfterCutoff)
    );
    assert_eq!(c.frozen(day()), frozen.as_ref());
    assert_eq!(s.state(), state);
}

#[test]
fn ri_restart_preserves_pending_and_next_result_exactly() {
    let (c, s) = bo_pending();
    let now = epoch(day(), 9, 0);
    let before = s.checkpoint_bytes().unwrap();
    let mut restored =
        RiSession::restore(c.identity(), "unit-generation", day(), &before, now).unwrap();
    assert_eq!(restored.state(), s.state());
    assert!(!restored.order_emission_allowed());
    let mut uninterrupted = s.clone();
    let mut c1 = c.clone();
    let mut c2 = c;
    let b = bar(epoch(day(), 9, 0), 105.);
    let a = process(&mut c1, &mut uninterrupted, b.clone());
    let b = process(&mut c2, &mut restored, b);
    assert_eq!(a, b);
    assert_eq!(
        uninterrupted.checkpoint_bytes().unwrap(),
        restored.checkpoint_bytes().unwrap()
    );
}

#[test]
fn ri_restart_rejects_generation_day_policy_calendar_and_clock_rebind() {
    let (c, s) = bo_pending();
    let now = epoch(day(), 9, 0);
    let bytes = s.checkpoint_bytes().unwrap();
    assert!(RiSession::restore(c.identity(), "wrong", day(), &bytes, now).is_err());
    assert!(RiSession::restore(
        c.identity(),
        "unit-generation",
        day().succ_opt().unwrap(),
        &bytes,
        now
    )
    .is_err());
    let mut id = c.identity().clone();
    id.policy = ContextPolicy::RiConfirmedClosingContextV1;
    assert!(RiSession::restore(&id, "unit-generation", day(), &bytes, now).is_err());
    let mut id = c.identity().clone();
    id.excluded_dates.clear();
    assert!(RiSession::restore(&id, "unit-generation", day(), &bytes, now).is_err());
    assert!(RiSession::restore(c.identity(), "unit-generation", day(), &bytes, now - 1).is_err());
}

#[test]
fn ri_recomputed_checksum_cannot_inject_private_observation_state() {
    let (c, s) = bo_pending();
    let bytes = s.checkpoint_bytes().unwrap();
    let original = String::from_utf8(bytes).unwrap();
    let changed = original.replace("\"count\":7", "\"count\":8");
    assert_ne!(changed, original);
    let start = changed.find("\"payload\":").unwrap() + 10;
    let end = changed.rfind(",\"sha256\":").unwrap();
    let digest = format!("{:x}", Sha256::digest(&changed.as_bytes()[start..end]));
    let value: Value = serde_json::from_str(&original).unwrap();
    let changed = changed.replace(value["sha256"].as_str().unwrap(), &digest);
    let err = RiSession::restore(
        c.identity(),
        "unit-generation",
        day(),
        changed.as_bytes(),
        epoch(day(), 9, 0),
    )
    .err()
    .unwrap()
    .to_string();
    assert!(err.contains("reconstructed state differs"), "{err}");
}

#[test]
fn ri_processed_clock_is_bound_to_exact_ingress_record() {
    let (mut c, mut s) = setup(day());
    let ts = epoch(day(), 7, 0);
    let i = ContextInput {
        sequence: c.last_sequence() + 1,
        bar: bar(ts, 100.5),
        received_at_utc: Some(ts + 600),
        durable_at_utc: Some(ts + 600),
    };
    c.ingest(i.clone(), ts + 600).unwrap();
    let before = s.checkpoint_bytes().unwrap();
    assert!(s.process(&c, i, ts + 601).is_err());
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
}

#[test]
fn ri_delayed_freeze_restore_does_not_retimestamp_old_receipts() {
    let mut c = context(day(), ContextPolicy::RuntimeAcceptedOnly);
    let now = epoch(day(), 7, 2);
    freeze(&mut c, day(), now);
    let mut s = RiSession::new(&c, day(), "unit-generation".into()).unwrap();
    at(&mut c, &mut s, 7, 0, 100.5);
    let bytes = s.checkpoint_bytes().unwrap();
    let restored = RiSession::restore(
        c.identity(),
        "unit-generation",
        day(),
        &bytes,
        epoch(day(), 7, 10),
    )
    .unwrap();
    assert_eq!(restored.state(), s.state());
    assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);
}

#[test]
fn ri_nonfinite_raw_rejection_cannot_publish_json_null_checkpoint_payloads() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let (mut c, mut s) = setup(day());
        let ts = epoch(day(), 7, 0);
        let before = s.checkpoint_bytes().unwrap();
        let mut b = bar(ts, 100.5);
        b.v = value;
        let i = ContextInput {
            sequence: c.last_sequence() + 1,
            bar: b,
            received_at_utc: Some(ts + 600),
            durable_at_utc: Some(ts + 600),
        };
        assert_eq!(
            c.ingest(i.clone(), ts + 600).unwrap(),
            ContextRoute::Quarantined(QuarantineReason::InvalidOhlcv)
        );
        assert!(s.process(&c, i, ts + 600).is_err());
        assert_eq!(s.checkpoint_bytes().unwrap(), before);
        RiContextStore::restore(c.identity(), &c.checkpoint_bytes().unwrap(), ts + 600).unwrap();
    }
}

#[test]
fn ri_restore_guard_does_not_admit_new_processing_before_restart() {
    let (mut c, s) = setup(day());
    let bytes = s.checkpoint_bytes().unwrap();
    let mut restored = RiSession::restore(
        c.identity(),
        "unit-generation",
        day(),
        &bytes,
        epoch(day(), 7, 15),
    )
    .unwrap();
    let before = restored.checkpoint_bytes().unwrap();
    let ts = epoch(day(), 7, 0);
    let i = ContextInput {
        sequence: c.last_sequence() + 1,
        bar: bar(ts, 100.5),
        received_at_utc: Some(ts + 600),
        durable_at_utc: Some(ts + 600),
    };
    c.ingest(i.clone(), ts + 600).unwrap();
    assert!(restored.process(&c, i, ts + 600).is_err());
    assert_eq!(restored.checkpoint_bytes().unwrap(), before);
}

#[test]
fn ri_finite_ohlcv_feature_overflow_is_atomic_and_checkpoint_stays_restorable() {
    let id = selected_identity(ContextPolicy::RuntimeAcceptedOnly, "ri-overflow".into()).unwrap();
    let mut c = RiContextStore::new(id.clone()).unwrap();
    let price = f64::MAX * 0.9;
    for d in [date("2026-10-09"), date("2026-10-12")] {
        for ts in (id.cutoff(d)..=epoch(d, 23, 30)).step_by(600) {
            let mut b = bar(ts, price);
            b.h = f64::MAX;
            b.l = 1.;
            b.origin = DataOrigin::History;
            c.ingest(
                ContextInput {
                    sequence: c.last_sequence() + 1,
                    bar: b,
                    received_at_utc: Some(ts + 600),
                    durable_at_utc: Some(ts + 600),
                },
                ts + 600,
            )
            .unwrap();
        }
    }
    freeze(&mut c, day(), epoch(day(), 7, 0));
    let mut s = RiSession::new(&c, day(), "unit-generation".into()).unwrap();
    for m in (0..50).step_by(10) {
        at(&mut c, &mut s, 7, m, price);
    }
    let bytes = s.checkpoint_bytes().unwrap();
    let ts = epoch(day(), 7, 50);
    let input = ContextInput {
        sequence: c.last_sequence() + 1,
        bar: bar(ts, price),
        received_at_utc: Some(ts + 600),
        durable_at_utc: Some(ts + 600),
    };
    c.ingest(input.clone(), ts + 600).unwrap();
    let error = s.process(&c, input, ts + 600).err().unwrap().to_string();
    assert!(error.contains("feature overflow"), "{error}");
    assert_eq!(s.checkpoint_bytes().unwrap(), bytes);
    RiSession::restore(&id, "unit-generation", day(), &bytes, epoch(day(), 7, 50)).unwrap();
}
