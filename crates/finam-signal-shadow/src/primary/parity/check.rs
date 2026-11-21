//! Test-only consumer of persisted numeric/model/feedback evidence. Expectations
//! are frozen Python + CSV, never results computed by another current model.
use super::*;
use broker_core::event::Bar;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use std::{collections::BTreeMap, str::FromStr};

pub(super) fn compare(expected: &Value, actual: &Value, path: &str) -> Result<(), String> {
    match expected {
        Value::Object(fields) => {
            for (k, v) in fields {
                compare(v, &actual[k], &format!("{path}.{k}"))?;
            }
        }
        Value::Number(n) => {
            let e = n.as_f64().ok_or("expected numeric")?;
            let a = actual
                .as_f64()
                .ok_or_else(|| format!("{path}: missing number"))?;
            if !a.is_finite() || (a - e).abs() > 1e-10 {
                return Err(format!("{path}: {a} != {e}"));
            }
        }
        _ if expected != actual => {
            return Err(format!("{path}: actual {actual} != expected {expected}"))
        }
        _ => (),
    }
    Ok(())
}
fn projection(schema: &Value, actual: &Value) -> Value {
    match schema {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), projection(v, &actual[k])))
                .collect(),
        ),
        _ => actual.clone(),
    }
}
pub(super) fn last_state(rows: &[Value]) -> &Value {
    let row = rows
        .iter()
        .rfind(|r| r["kind"] == "model_step" || r["kind"] == "hypothetical_candle_feedback")
        .unwrap();
    if row["kind"] == "model_step" {
        &row["event"]["after"]
    } else {
        &row["event"]["state_after"]
    }
}
fn number(v: &Value, key: &str) -> Result<i64, String> {
    v[key]
        .as_i64()
        .ok_or_else(|| format!("invalid integer: {key}"))
}
fn require(ok: bool, why: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(why.into())
    }
}
fn rows_of<'a>(rows: &'a [Value], kind: &str) -> Vec<(usize, &'a Value)> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| r["kind"] == kind)
        .collect()
}

pub(super) fn verify(f: &Fixture, rows: &[Value]) -> Result<Value, String> {
    require(
        digest(fixture::CSV) == fixture::CSV_SHA && digest(fixture::ORACLE) == fixture::ORACLE_SHA,
        "fixture pin",
    )?;
    let seed = rows.first().ok_or("no seed")?;
    require(seed["kind"] == "seed", "seed kind")?;
    compare(
        &f.oracle["seed_after"],
        &seed["observation"]["state"],
        "seed",
    )?;
    let fingerprint = f.config.fingerprint().map_err(|_| "config")?;
    require(
        seed["observation"]["config_sha256"] == fingerprint,
        "seed identity",
    )?;
    for (i, row) in rows.iter().enumerate() {
        require(row["sequence"].as_u64() == Some(i as u64), "sequence")?;
        if i > 0 {
            require(
                row["config_sha256"] == fingerprint
                    && row["seed_sha256"] == digest(&f.material[3])
                    && row["reference_sha256"] == fixture::ORACLE_SHA,
                "row identity",
            )?;
        }
    }
    let sources = rows_of(rows, "canonical_source");
    let steps = rows_of(rows, "model_step");
    let commits = rows_of(rows, "canonical_commit");
    let fills = rows_of(rows, "hypothetical_candle_feedback");
    require(
        sources.len() == 505 && steps.len() == 505 && commits.len() == 505 && fills.len() == 10,
        "source/step/commit/fill cardinality",
    )?;
    require(
        rows_of(rows, "hypothetical_feedback").is_empty(),
        "quotes forbidden",
    )?;
    let mut previous = seed["observation"]["state"].clone();
    let mut fill_index = 0;
    let mut reason_counts = BTreeMap::<String, u64>::new();
    let mut semantic = vec![];
    let mut rounds = vec![];
    let mut pending_entry: Option<Value> = None;
    let mut ws = 0;
    let mut rest = 0;
    let mut sparse = 0;
    let mut entry_count = 0;
    let mut exits = 0;
    for (index, (bar, gold)) in f
        .selected
        .iter()
        .zip(f.oracle["steps"].as_array().ok_or("oracle rows")?)
        .enumerate()
    {
        let (source_at, source) = sources[index];
        let (step_at, step) = steps[index];
        let (commit_at, commit) = commits[index];
        let receipt = &source["event"]["receipt"];
        let s = &step["event"];
        let c = &commit["event"];
        let label = bar.label();
        let close = label + 600000;
        let expected_digest = crate::input::canonical_digest(&bar.bar).map_err(|_| "digest")?;
        require(
            source_at < step_at && step_at < commit_at,
            "source/model/commit order",
        )?;
        if index > 0 {
            require(
                commits[index - 1].0 < source_at,
                "overlapping canonical operations",
            )?;
        }
        require(
            receipt["label_ms"].as_i64() == Some(label)
                && receipt["close_ms"].as_i64() == Some(close)
                && s["model_label_utc_ms"].as_i64() == Some(label)
                && c["model_label_utc_ms"].as_i64() == Some(label),
            "labels",
        )?;
        require(
            receipt["canonical_digest"] == expected_digest
                && s["canonical_input_sha256"] == expected_digest,
            "canonical digest",
        )?;
        let actual: Vec<Bar> =
            serde_json::from_value(receipt["actual_m1"].clone()).map_err(|_| "M1 decode")?;
        let numeric =
            broker_core::canonical_m1::aggregate_m1_window(&instrument(), at(label), &actual)
                .map_err(|_| "M1 numeric")?;
        let b = numeric.provisional_bar;
        require(
            [b.open, b.high, b.low, b.close, b.volume] == bar.values,
            "independent CSV OHLCV",
        )?;
        require(
            receipt["minute_bitmap"].as_u64() == Some(u64::from(numeric.minute_bitmap)),
            "bitmap",
        )?;
        require(
            receipt["generation"].as_u64() == Some((index / 101 + 1) as u64),
            "daily admission owner",
        )?;
        match receipt["origin"].as_str() {
            Some("WS_LIVE") => {
                ws += 1;
                require(
                    receipt["reason"].is_null() && numeric.minute_bitmap == 1023,
                    "WS provenance",
                )?;
            }
            Some("REST_EXCEPTION") => {
                rest += 1;
                require(
                    source["event"]["rest_evidence"].is_object(),
                    "retained REST proof",
                )?;
                match receipt["reason"].as_str() {
                    Some("BoundaryUnproven") => {
                        require(numeric.minute_bitmap == 1023, "dense exception")?
                    }
                    Some("SparseUnproven") => {
                        sparse += 1;
                        require(numeric.minute_bitmap == 1007, "sparse inventory")?;
                    }
                    _ => return Err("exception reason".into()),
                }
            }
            _ => return Err("source origin".into()),
        }
        require(
            s["mode"] == "LIVE_OBSERVED" && s["before"] == previous,
            "one continuous measured model",
        )?;
        require(
            s["raw_provenance_sha256"] == source["event"]["source_sha256"]
                && c["source_sha256"] == source["event"]["source_sha256"]
                && c["feedback_complete"] == true,
            "logical commit binding",
        )?;
        compare(
            &gold["position_before"],
            &s["before"]["last_position_qty"],
            "position before",
        )?;
        require(
            // Python records these after the bar's day rollover, before fill.
            // The persisted `before` is before on_bar (still yesterday on the
            // first bar); `after` is the matching pre-feedback observation.
            s["after"]["was_long_today"] == gold["used_before"][0]
                && s["after"]["was_short_today"] == gold["used_before"][1],
            "side-used before feedback",
        )?;
        let action = match s["decision"]["action"].as_str() {
            Some("EnterLong") => "enter_long",
            Some("EnterShort") => "enter_short",
            Some("Exit") => "exit",
            None if s["decision"]["action"].is_null() => "none",
            _ => return Err("decision type".into()),
        };
        require(gold["action"] == action, "Python action")?;
        let reason = if action == "none" {
            &gold["no_intent_trace"]["reason"]
        } else {
            &gold["reason"]
        };
        require(s["decision"]["reason"] == *reason, "Python branch reason")?;
        *reason_counts
            .entry(reason.as_str().ok_or("reason type")?.into())
            .or_default() += 1;
        let mut actual_fill = Value::Null;
        let after = if action == "none" {
            require(s["decision"]["quantity_lots"] == 0, "no-intent quantity")?;
            compare(
                &gold["no_intent_trace"],
                &s["after"]["no_intent_trace"],
                "no-intent trace",
            )?;
            require(
                s["after"]["pending_entry_reason"].is_null()
                    && s["after"]["pending_exit_reason"].is_null(),
                "no-intent pending reason",
            )?;
            require(
                s["after"]["no_intent_trace"]["label_utc_ms"].as_i64() == Some(label),
                "trace label",
            )?;
            &s["after"]
        } else {
            let (fill_at, row) = fills.get(fill_index).copied().ok_or("missing fill")?;
            fill_index += 1;
            let e = &row["event"];
            let fill = &e["fill"];
            require(
                step_at < fill_at
                    && fill_at < commit_at
                    && e["mode"] == "LIVE_OBSERVED"
                    && e["broker_execution"] == false
                    && e["quotes_observed"] == false
                    && fill["policy"] == crate::feedback::CANDLE_POLICY
                    && e["price_assumption"] == "accepted_closed_m10_close",
                "feedback order/mode/policy",
            )?;
            let key = if action == "exit" {
                "pending_exit_reason"
            } else {
                "pending_entry_reason"
            };
            require(
                s["after"][key] == *reason,
                "actual retained pre-fill reason",
            )?;
            let price = (bar.values[3] * Decimal::from(2))
                .to_i64()
                .ok_or("price ticks")?;
            require(
                fill["request"]["sequence"].as_u64() == Some(fill_index as u64)
                    && fill["request"]["action"] == s["decision"]["action"]
                    && fill["request"]["quantity_lots"] == 1
                    && s["decision"]["quantity_lots"] == 1,
                "one-lot sequential feedback",
            )?;
            require(
                number(fill, "price_ticks")? == price
                    && number(fill, "model_label_utc_ms")? == label
                    && fill["canonical_input_sha256"] == expected_digest
                    && fill["raw_provenance_sha256"] == source["event"]["source_sha256"],
                "close-fill source/price",
            )?;
            compare(
                &gold["after_feedback"]["last_position_qty"],
                &fill["position_lots"],
                "fill position",
            )?;
            if action == "exit" {
                exits += 1;
                require(fill["average_price_ticks"].is_null(), "flat average")?;
                let mut round = pending_entry.take().ok_or("exit without entry")?;
                round["exit_bar"] = json!(bar.local_label().replace('T', " "));
                round["exit_price_ticks"] = json!(price);
                round["exit_reason"] = json!(match reason.as_str() {
                    Some("breakout_eod_exit") => "eod_2330",
                    Some("breakout_stop1_short") => "bo_stop1",
                    _ => return Err("exit reason".into()),
                });
                rounds.push(round);
            } else {
                entry_count += 1;
                require(
                    pending_entry.is_none() && number(fill, "average_price_ticks")? == price,
                    "entry book",
                )?;
                pending_entry = Some(
                    json!({"side":if action=="enter_long"{"long"}else{"short"},"entry_bar":bar.local_label().replace('T'," "),"entry_price_ticks":price}),
                );
            }
            actual_fill = json!({"sequence":fill["request"]["sequence"],"action":fill["request"]["action"],"quantity_lots":fill["request"]["quantity_lots"],"price_ticks":fill["price_ticks"],"position_lots":fill["position_lots"],"average_price_ticks":fill["average_price_ticks"]});
            &e["state_after"]
        };
        compare(&gold["after_feedback"], after, &bar.local_label())?;
        for key in [
            "pending_entry_request_id",
            "pending_exit_request_id",
            "pending_entry_reason",
            "pending_exit_reason",
        ] {
            require(after[key].is_null(), "feedback not settled")?;
        }
        let publication = number(c, "publication_checked_at_ms")?;
        require(
            publication >= number(s, "result_at_ms")? && publication - close + 100 <= 10000,
            "measured publication fence",
        )?;
        semantic.push(json!({"label_ms":label,"canonical_digest":expected_digest,"action":s["decision"]["action"],"reason":s["decision"]["reason"],"after_feedback":projection(&gold["after_feedback"],after),"fill":actual_fill}));
        previous = after.clone();
    }
    require(
        pending_entry.is_none() && entry_count == 5 && exits == 5 && fill_index == 10,
        "complete 5-round book",
    )?;
    let expected_rounds = f.oracle["rounds"].as_array().ok_or("rounds")?;
    require(rounds.len() == expected_rounds.len(), "round cardinality")?;
    for (a, e) in rounds.iter().zip(expected_rounds) {
        for key in ["side", "entry_bar", "exit_bar", "exit_reason"] {
            require(a[key] == e[key], "Python round field")?;
        }
        for key in ["entry_price", "exit_price"] {
            let price = Decimal::from_str(e[key].as_str().ok_or("round price")?)
                .map_err(|_| "round decimal")?;
            require(
                a[format!("{key}_ticks")].as_i64() == (price * Decimal::from(2)).to_i64(),
                "Python round price",
            )?;
        }
    }
    require(
        rows.last().ok_or("terminal")?["event"]["reason"]
            == "T37_HISTORICAL_ADMISSION_COMPONENT_NOT_EOD_QUALIFICATION",
        "component terminal",
    )?;
    Ok(
        json!({"schema_version":1,"scenario":"T37","scope":"HISTORICAL_ADMISSION_COMPONENT_NOT_NETWORK_CALENDAR_OR_EOD_QUALIFICATION",
        "fixture_csv_sha256":fixture::CSV_SHA,"python_oracle_sha256":fixture::ORACLE_SHA,"seed_bars":404,"steps":505,"commits":505,"fills":10,"rounds":rounds.len(),"entries":entry_count,"exits":exits,"no_intent":495,
        "ws_labels":ws,"rest_labels":rest,"sparse_labels":sparse,"reason_counts":reason_counts,"actual_rounds":rounds,"model_resets":0,"one_model_across_all_days":true,
        "synthetic_m1_not_provider_data":true,"semantic_sha256":digest(&serde_json::to_vec(&semantic).map_err(|_|"semantic encoding")?),
        "checked_fixture_model_parity":true,"network_qualified":false,"calendar_entry_qualified":false,"eod_qualified":false,"operational_parity_accepted":false}),
    )
}
