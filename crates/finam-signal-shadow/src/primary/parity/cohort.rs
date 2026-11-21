//! T38 actual non-flat feedback under shared admission. Same historical numeric
//! fixture as T37, not a multi-day calendar/network qualification.
use super::*;
use crate::runner::primary::{Coordinator, CoordinatorError, Tick};

#[test]
fn shared_cohort_runs_frozen_entry_exit_and_stops_on_consumer_feedback_failure() {
    for fail_feedback in [false, true] {
        let f = Fixture::new(Mode::Dense);
        with_fixture_clock(f.config.base.observation_start_utc_ms, || {
            let dir = Dir::new();
            let root = StopHandle::default();
            let paths: Vec<_> = (0..3)
                .map(|i| dir.0.join(format!("runtime-{i}.jsonl")))
                .collect();
            let mut sessions: Vec<_> = paths
                .iter()
                .map(|path| ObservedSession::create(f.segment(), path, root.clone()).unwrap())
                .collect();
            if fail_feedback {
                sessions[1].fail_primary_evidence("hypothetical_candle_feedback");
            }
            let bars = &f.selected[..48]; // first frozen entry 12:30, exit 14:50
            let p = PrimaryProcessor::from_config(
                &f.config,
                instrument(),
                bars.iter().map(GoldBar::label).collect(),
                1,
                now_ms(),
            )
            .unwrap();
            let mut owner = Coordinator::new_group(p, sessions).unwrap();
            let sub = crate::socket::primary::fixture_subscription(1, now_ms());
            owner.bind_subscription(&sub).unwrap();
            for (index, bar) in bars.iter().enumerate() {
                for i in 0..=10 {
                    if i == 0 && index > 0 {
                        continue;
                    }
                    let received = bar.label() + i * 60000 + 100;
                    go(received);
                    let row = if i == 10 {
                        f.selected[index + 1].minute(0)
                    } else {
                        bar.minute(i)
                    };
                    let raw=json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"cohort-fills",
                        "payload":{"symbol":"IMOEXF@RTSX","timeframe":"TIME_FRAME_M1","bars":[row]}}).to_string();
                    let mut frame =
                        WireFrame::normalize(&raw, 1, "cohort-fills", at(received)).unwrap();
                    frame.owner = Some(sub.owner.clone());
                    owner.on_frame(frame).unwrap();
                }
                go(bar.label() + 601100);
                let result = owner.tick();
                if fail_feedback && f.oracle["steps"][index]["action"] != "none" {
                    assert_eq!(index, 33);
                    assert!(result.is_err());
                    assert!(root.is_stopped());
                    assert_eq!(owner.processor().counters().committed, 33);
                    assert_eq!(owner.processor().next_label_ms(), Some(bar.label()));
                    assert_eq!(owner.tick().unwrap_err(), CoordinatorError::Held);
                    break;
                }
                assert!(matches!(
                    result.unwrap(),
                    Tick::Published {
                        origin: "WS_LIVE",
                        ..
                    }
                ));
            }
            if !fail_feedback {
                assert_eq!(owner.processor().counters().committed, 48);
            }
            root.stop();
            drop(owner);
            for (member, path) in paths.iter().enumerate() {
                let rows: Vec<Value> = std::fs::read_to_string(path)
                    .unwrap()
                    .lines()
                    .map(|s| serde_json::from_str(s).unwrap())
                    .collect();
                let steps: Vec<_> = rows.iter().filter(|r| r["kind"] == "model_step").collect();
                let fills: Vec<_> = rows
                    .iter()
                    .filter(|r| r["kind"] == "hypothetical_candle_feedback")
                    .collect();
                let commits = rows
                    .iter()
                    .filter(|r| r["kind"] == "canonical_commit")
                    .count();
                if fail_feedback {
                    assert_eq!(steps.len(), if member < 2 { 34 } else { 33 });
                    assert_eq!(fills.len(), usize::from(member == 0));
                    assert_eq!(commits, if member == 0 { 34 } else { 33 });
                } else {
                    assert_eq!(steps.len(), 48);
                    assert_eq!(fills.len(), 2);
                    assert_eq!(commits, 48);
                    assert_eq!(fills[0]["event"]["fill"]["price_ticks"], 4504);
                    assert_eq!(fills[1]["event"]["fill"]["price_ticks"], 4524);
                    assert_eq!(fills[0]["event"]["fill"]["position_lots"], -1);
                    assert_eq!(fills[1]["event"]["fill"]["position_lots"], 0);
                    for (i, step) in steps.iter().enumerate() {
                        let label = step["event"]["model_label_utc_ms"].as_i64().unwrap();
                        let state = fills
                            .iter()
                            .find(|r| r["event"]["fill"]["model_label_utc_ms"] == label)
                            .map_or(&step["event"]["after"], |r| &r["event"]["state_after"]);
                        check::compare(
                            &f.oracle["steps"][i]["after_feedback"],
                            state,
                            "cohort frozen state",
                        )
                        .unwrap();
                    }
                }
            }
        });
    }
}
