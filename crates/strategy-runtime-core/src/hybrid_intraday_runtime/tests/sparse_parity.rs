//! Offline data-difference replay, not a transport or paper-provider acceptance.
use super::*;
use broker_finam::sparse_m10::{
    AdmittedClosedM1Snapshot, ClosedM1RequestPlan, ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1,
    OBSERVED_M1_POLICY_V1,
};
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

type ReplayRow = (FrozenParityBar, NaiveDateTime, i64, i64);

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../broker-finam/tests/fixtures/sparse-m10")
            .join(name),
    )
    .unwrap()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn snapshot() -> AdmittedClosedM1Snapshot {
    let raw = fixture("finam-long-response.json");
    let meta: Value = serde_json::from_str(&fixture("finam-long-response-meta.json")).unwrap();
    assert_eq!(digest(raw.as_bytes()), meta["response_sha256"]);
    let time = |key: &str| {
        meta[key]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap()
    };
    let received = time("body_received_at_utc");
    assert_eq!(raw.len() as u64, meta["bytes"].as_u64().unwrap());
    let plan =
        ClosedM1RequestPlan::new(time("start_time"), time("end_time"), vec![time("end_time")])
            .unwrap();
    AdmittedClosedM1Snapshot::admit(
        ClosedM1SnapshotEvidenceV1 {
            policy: OBSERVED_M1_POLICY_V1.into(),
            start: time("start_time"),
            end: time("end_time"),
            parts: vec![ClosedM1RestPartV1 {
                method: meta["method"].as_str().unwrap().into(),
                endpoint: format!(
                    "https://{}{}",
                    meta["host"].as_str().unwrap(),
                    meta["path"].as_str().unwrap()
                ),
                symbol: meta["symbol"].as_str().unwrap().into(),
                timeframe: meta["timeframe"].as_str().unwrap().into(),
                start: time("start_time"),
                end: time("end_time"),
                requested_at: time("request_started_at_utc"),
                received_at: received,
                status: meta["http_status"].as_u64().unwrap().try_into().unwrap(),
                transport_complete: true,
                declared_body_bytes: None,
                response_sha256: meta["response_sha256"].as_str().unwrap().into(),
                raw_body: raw,
            }],
        },
        &plan,
        received,
    )
    .unwrap()
}

fn row(open: DateTime<Utc>, prices: [f64; 5]) -> ReplayRow {
    let label = open.naive_utc() + ChronoDuration::hours(3);
    let end = open + ChronoDuration::minutes(10);
    (
        FrozenParityBar {
            bar_start_msk: label.format("%Y-%m-%d %H:%M:%S").to_string(),
            bar_start_utc: open.to_rfc3339(),
            available_at_utc: end.to_rfc3339(),
            open: prices[0],
            high: prices[1],
            low: prices[2],
            close: prices[3],
            volume: prices[4],
        },
        label,
        open.timestamp(),
        end.timestamp(),
    )
}

fn inputs() -> (Vec<ReplayRow>, Vec<ReplayRow>, Vec<Value>) {
    let snapshot = snapshot();
    let mut alor = Vec::new();
    let mut finam = Vec::new();
    let mut differences = Vec::new();
    for line in fixture("alor-native-m10.jsonl").lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        let p = &v["envelope"]["payload"];
        // This legacy ALOR field names the candle OPEN, not availability.
        let start = DateTime::from_timestamp(p["close_time_utc"].as_i64().unwrap(), 0).unwrap();
        let a = ["o", "h", "l", "c", "v"].map(|key| p[key].as_f64().unwrap());
        let bucket = snapshot.bucket(start).unwrap();
        let b = bucket.bar();
        let f = [b.open, b.high, b.low, b.close, b.volume].map(|n| n.to_f64().unwrap());
        if a != f {
            differences.push(json!({"open_utc":start, "alor_ohlcv":a, "finam_ohlcv":f, "actual_m1":bucket.actual_m1().len()}));
        }
        alor.push(row(start, a));
        finam.push(row(start, f));
    }
    assert_eq!((alor.len(), finam.len(), differences.len()), (404, 404, 4));
    for rows in [&alor, &finam] {
        assert!(rows.windows(2).all(|pair| pair[0].2 < pair[1].2));
        assert!(rows.iter().all(|r| frozen_no_riskgate_eligible(r.1)));
    }
    (alor, finam, differences)
}

fn warmed() -> (HybridIntradayRuntimeStrategy, usize) {
    let mut strategy =
        HybridIntradayRuntimeStrategy::new(frozen_baseline07_no_riskgate_profile_v2_config());
    // Exactly the four prior completed sessions actually present in the freeze.
    // No invented weekend bars and no stale July anchors.
    let first = NaiveDate::from_ymd_opt(2026, 9, 22).unwrap();
    let last = NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
    let rows = parse_frozen_parity_bars();
    let selected = rows
        .iter()
        .filter(|r| r.1.date() >= first && r.1.date() <= last && frozen_no_riskgate_eligible(r.1))
        .collect::<Vec<_>>();
    for date in [22, 23, 24, 25] {
        let day = selected
            .iter()
            .filter(|r| r.1.date().day() == date)
            .collect::<Vec<_>>();
        assert!(!day.is_empty());
        assert_eq!(
            day.first().unwrap().1.time(),
            NaiveTime::from_hms_opt(7, 0, 0).unwrap()
        );
        assert_eq!(
            day.last().unwrap().1.time(),
            NaiveTime::from_hms_opt(23, 40, 0).unwrap()
        );
    }
    let bars = selected
        .iter()
        .map(|r| frozen_no_riskgate_bar(&r.0, r.2))
        .collect::<Vec<_>>();
    assert_eq!(
        strategy.warmup_from_history(&frozen_no_riskgate_context(), &bars),
        bars.len()
    );
    assert!(strategy.entry_ready);
    assert_eq!(
        frozen_no_riskgate_bo_projection(&strategy)["current_day_close"],
        json!(2275.5)
    );
    assert!(strategy.prev_day_range.is_some());
    assert!(strategy.pending_entry.is_none() && strategy.pending_exit.is_none());
    assert_frozen_no_riskgate_accounting(&strategy);
    (strategy, bars.len())
}

/// Same immediate-current-close position simulation as the already accepted
/// freeze replay. It is explicitly NOT real fills, ACK timing or live parity.
fn trace(mut strategy: HybridIntradayRuntimeStrategy, rows: &[ReplayRow]) -> Value {
    let mut ctx = frozen_no_riskgate_context();
    let mut qty = 0.0_f64;
    let mut open_round: Option<(&str, String, f64)> = None;
    let mut rounds = Vec::new();
    let mut trace = Vec::new();
    for (row, _, start, end) in rows {
        ctx.position_qty = Some(qty);
        ctx.event_ts_utc = *end;
        ctx.now_ts_utc = *end;
        ctx.last_bar_ts = Some(*start);
        let intents = strategy.on_bar(&ctx, &frozen_no_riskgate_bar(row, *start));
        let before_fill = serde_json::to_value(Strategy::state(&strategy)).unwrap();
        let requests = json!({"entry":strategy.pending_entry_request_id,"exit":strategy.pending_exit_request_id});
        let mut fill = None;
        if qty.abs() <= f64::EPSILON {
            if let Some(entry) = strategy.pending_entry {
                assert_eq!(entry.owner, Owner::IntradayBreakout);
                assert!(!intents.is_empty());
                let (side, size) = match entry.side {
                    Side::Long => ("long", 1.0),
                    Side::Short => ("short", -1.0),
                };
                assert!(open_round.is_none());
                open_round = Some((side, row.bar_start_msk.clone(), row.close));
                qty = size;
                fill = Some(row.close);
            }
        } else if let Some(exit) = strategy.pending_exit {
            assert_eq!(exit.owner, Owner::IntradayBreakout);
            assert!(!intents.is_empty());
            let (side, entry_bar, entry_price) = open_round.take().unwrap();
            rounds.push(json!({"side":side,"entry_bar":entry_bar,"entry_price":entry_price,"exit_bar":row.bar_start_msk,"exit_price":row.close,"reason":parity_reason(exit.reason)}));
            qty = 0.0;
            fill = Some(0.0);
        }
        if let Some(avg_price) = fill {
            ctx.position_qty = Some(qty);
            assert!(strategy
                .on_position(
                    &ctx,
                    &PositionEvent {
                        symbol: "IMOEXF".into(),
                        qty,
                        existing: false,
                        avg_price,
                        ts_utc: *end
                    }
                )
                .is_empty());
        }
        assert_frozen_no_riskgate_accounting(&strategy);
        trace.push(json!({"model_label_msk":row.bar_start_msk,"input_ohlcv":[row.open,row.high,row.low,row.close,row.volume],"state_after_bar":before_fill,"intents_debug":format!("{intents:?}"),"requests":requests,"state_after_fill":serde_json::to_value(Strategy::state(&strategy)).unwrap()}));
    }
    assert!(open_round.is_none());
    json!({"trace":trace,"paper_rounds":rounds})
}

#[test]
fn real_four_day_observed_m10_model_replay() {
    let (alor, finam, input_differences) = inputs();
    let (a, warmup_count) = warmed();
    let (f, _) = warmed();
    let initial = serde_json::to_value(Strategy::state(&a)).unwrap();
    assert_eq!(initial, serde_json::to_value(Strategy::state(&f)).unwrap());
    let a = trace(a, &alor);
    let f = trace(f, &finam);
    // Cross-check the new evidence driver against the retained accepted
    // simulator rather than silently introducing a different fill convention.
    for (rows, output) in [(&alor, &a), (&finam, &f)] {
        let (mut strategy, _) = warmed();
        let (rounds, decisions) = replay_frozen_no_riskgate(&mut strategy, rows.iter());
        assert_eq!(
            rounds.len(),
            output["paper_rounds"].as_array().unwrap().len()
        );
        for (round, actual) in rounds
            .iter()
            .zip(output["paper_rounds"].as_array().unwrap())
        {
            assert_eq!(
                *actual,
                json!({"side":round.side,"entry_bar":round.entry_bar,"entry_price":round.entry_price,"exit_bar":round.exit_bar,"exit_price":round.exit_price,"reason":round.exit_reason})
            );
        }
        for (decision, actual) in decisions.iter().zip(output["trace"].as_array().unwrap()) {
            assert_eq!(
                format!("{:?}", decision.intents),
                actual["intents_debug"].as_str().unwrap()
            );
            for (field, value) in decision.bo_state.as_object().unwrap() {
                assert_eq!(
                    *value,
                    actual["state_after_bar"]["HybridIntradayRuntime"][field]
                );
            }
        }
    }
    let mut state_differences = Vec::new();
    let mut decision_differences = Vec::new();
    for (a, f) in a["trace"]
        .as_array()
        .unwrap()
        .iter()
        .zip(f["trace"].as_array().unwrap())
    {
        if a["state_after_bar"] != f["state_after_bar"]
            || a["state_after_fill"] != f["state_after_fill"]
        {
            state_differences
                .push(json!({"model_label_msk":a["model_label_msk"],"alor":a,"finam":f}));
        }
        if a["intents_debug"] != f["intents_debug"] || a["requests"] != f["requests"] {
            decision_differences
                .push(json!({"model_label_msk":a["model_label_msk"],"alor":a,"finam":f}));
        }
    }
    assert!(decision_differences.is_empty());
    assert_eq!(a["paper_rounds"], f["paper_rounds"]);
    assert_eq!(a["paper_rounds"].as_array().unwrap().len(), 3);
    assert_eq!(
        state_differences
            .iter()
            .map(|d| d["model_label_msk"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["2026-09-29 17:10:00", "2026-09-30 17:00:00"]
    );
    for d in &state_differences {
        let a = d["alor"]["state_after_bar"]["HybridIntradayRuntime"]
            .as_object()
            .unwrap();
        let f = &d["finam"]["state_after_bar"]["HybridIntradayRuntime"];
        assert_eq!(
            a.iter()
                .filter(|(k, v)| **v != f[k.as_str()])
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            ["current_day_close", "last_bar_close"]
        );
    }
    let report = json!({
        "policy":OBSERVED_M1_POLICY_V1,
        "execution_assumption":"same immediate current-close position simulation; not live fills/ACKs",
        "warmup":"freeze 2026-09-22..2026-09-25, four completed sessions",
        "warmup_bars":warmup_count,
        "warmup_fixture_sha256":digest(include_bytes!("../../../../../fixtures/stage8b-p1f-parity/imoexf_raw_10m_msk_utc.csv")),
        "runtime_profile_sha256":digest(include_bytes!("../../../../../docs/stage-8/stage8b-p1e-runtime-profile-v2.json")),
        "initial_state":initial,"input_differences":input_differences,
        "state_differences":state_differences,"decision_differences":decision_differences,
        "paper_rounds_equal":a["paper_rounds"]==f["paper_rounds"],
        "alor":a,"finam":f,
        "operational_activation":false,"o2_verdict":"HOLD"
    });
    println!(
        "SPARSE_PARITY_EVIDENCE={}",
        serde_json::to_string(&report).unwrap()
    );
    // Pin observed consequences after recording the real calculation; do not
    // assert full state equality merely from daily OHLC/range agreement.
    assert_eq!(report["alor"]["trace"].as_array().unwrap().len(), 404);
    assert_eq!(report["finam"]["trace"].as_array().unwrap().len(), 404);
}
