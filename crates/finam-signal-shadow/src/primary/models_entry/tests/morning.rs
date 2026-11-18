// Reviewer-supplied regression cases, reviewed and executed locally for R1.
use super::*;
// Adapted from the supplied three reviewer cases; actual execution is logged
// in the R1 evidence. All clocks and bars here remain explicitly synthetic.
mod reviewer_morning_history {
    use super::*;

    fn morning_inputs(f: &Fixture) -> (ModelPlan, Vec<u8>, Vec<u8>) {
        let (mut plan, history, calendar) = inputs(f);
        let mut bundle: seed::HistoryBundle = serde_json::from_slice(&history).unwrap();
        for (ordinal, snapshot) in bundle.snapshots.iter_mut().enumerate() {
            // Truthful fresh GETs at 06:45 MSK of the decision day. Preserve
            // every raw body/range/OHLCV; only the acquisition clocks change.
            let requested_ms = f.open() - 15 * 60_000 + ordinal as i64 * 100;
            let received_ms = requested_ms + 34;
            snapshot.admitted_at_ms = requested_ms + 50;
            for part in &mut snapshot.evidence.parts {
                part.requested_at = chrono::DateTime::from_timestamp_millis(requested_ms).unwrap();
                part.received_at = chrono::DateTime::from_timestamp_millis(received_ms).unwrap();
            }
            let expected = broker_finam::sparse_m10::ClosedM1RequestPlan::new(
                snapshot.evidence.start,
                snapshot.evidence.end,
                vec![snapshot.evidence.end],
            )
            .unwrap();
            // The existing strict native source admission must accept every GET.
            broker_finam::sparse_m10::AdmittedClosedM1Snapshot::admit(
                snapshot.evidence.clone(),
                &expected,
                chrono::DateTime::from_timestamp_millis(snapshot.admitted_at_ms).unwrap(),
            )
            .unwrap();
        }
        let history = serde_json::to_vec(&bundle).unwrap();
        plan.history_sha256 = digest(&history);
        (plan, history, calendar)
    }

    #[test]
    fn reviewer_morning_history_is_ready_before_current_freeze() {
        let f = fixture();
        with_fixture_clock(f.open(), || {
            let (plan, history, calendar) = morning_inputs(&f);
            let manifest = seed::build(&f.config, &plan, &history, &calendar).unwrap();
            assert!(seed::require_ready(&manifest).is_ok(),
                "Valid current-morning closed-history bootstrap is rejected: the implementation uses per-bar next-cutoff ingestion instead of the decision-session bootstrap cutoff");
            assert!(prepare_fixture(&f, plan, &history, &calendar).is_ok());
        });
    }

    #[test]
    fn reviewer_morning_history_both_durable_model_arms_are_ready() {
        let f = fixture();
        with_fixture_clock(f.open(), || {
            let (plan, history, calendar) = morning_inputs(&f);
            let manifest = seed::build(&f.config, &plan, &history, &calendar).unwrap();
            let dir = Dir::new();
            let preview = finam_shadow_models::local::Journal::initial_states(&manifest).unwrap();
            let journal = finam_shadow_models::local::Journal::create(
                &dir.0.join("reviewer-morning-models.sqlite"),
                manifest,
            )
            .unwrap();
            let exported = journal.export().unwrap();
            assert_eq!(preview, exported["states"].as_array().unwrap().clone());
            let states: Vec<_> = exported["states"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|s| s["arm"].as_str().unwrap().starts_with("IMOEXF/"))
                .collect();
            assert_eq!(states.len(), 2);
            assert!(states.iter().all(|s| s["ready"] == true),
                "Fixing preflight alone is insufficient: actual Journal::create/Arm::bootstrap must use the same legitimate bootstrap semantics: {states:?}");
        });
    }

    #[test]
    fn reviewer_morning_history_mixed_retention_does_not_skip_required_previous_day() {
        let f = fixture();
        with_fixture_clock(f.open(), || {
            let (_, original_history, _) = inputs(&f);
            let original: seed::HistoryBundle = serde_json::from_slice(&original_history).unwrap();
            let mut retained: seed::HistorySnapshot =
                serde_json::from_value(serde_json::to_value(&original.snapshots[0]).unwrap())
                    .unwrap();
            // Add one explicitly synthetic older retained Wednesday session.
            // The original four-day fixture remains fully present in inventory.
            let day = chrono::Duration::days(1);
            retained.admitted_at_ms -= 86_400_000;
            retained.evidence.start -= day;
            retained.evidence.end -= day;
            // Retained Wednesday was acquired just after ITS close; all
            // remaining days are acquired on Monday morning. This reproduces
            // the reviewer's false previous2=Wednesday counterexample exactly.
            retained.admitted_at_ms = retained.evidence.end.timestamp_millis() + 200;
            for part in &mut retained.evidence.parts {
                part.start -= day;
                part.end -= day;
                part.requested_at -= day;
                part.received_at -= day;
                part.requested_at = retained.evidence.end + chrono::Duration::milliseconds(50);
                part.received_at = retained.evidence.end + chrono::Duration::milliseconds(100);
                let mut raw: Value = serde_json::from_str(&part.raw_body).unwrap();
                for row in raw["bars"].as_array_mut().unwrap() {
                    let stamp =
                        chrono::DateTime::parse_from_rfc3339(row["timestamp"].as_str().unwrap())
                            .unwrap()
                            - day;
                    row["timestamp"] = json!(stamp.to_rfc3339());
                }
                part.raw_body = serde_json::to_string(&raw).unwrap();
                part.declared_body_bytes = Some(part.raw_body.len() as u64);
                part.response_sha256 = digest(part.raw_body.as_bytes());
            }
            let (mut plan, morning_history, calendar) = morning_inputs(&f);
            let mut bundle: seed::HistoryBundle = serde_json::from_slice(&morning_history).unwrap();
            bundle.snapshots.insert(0, retained);
            let mut calendar: seed::ContextCalendar = serde_json::from_slice(&calendar).unwrap();
            let mut labels: Vec<_> = calendar.required_labels_ms[..101]
                .iter()
                .map(|label| *label - 86_400_000)
                .collect();
            labels.extend(calendar.required_labels_ms);
            calendar.required_labels_ms = labels;
            assert_eq!(calendar.required_labels_ms.len(), 413);
            for snapshot in &bundle.snapshots {
                let request = broker_finam::sparse_m10::ClosedM1RequestPlan::new(
                    snapshot.evidence.start,
                    snapshot.evidence.end,
                    vec![snapshot.evidence.end],
                )
                .unwrap();
                broker_finam::sparse_m10::AdmittedClosedM1Snapshot::admit(
                    snapshot.evidence.clone(),
                    &request,
                    chrono::DateTime::from_timestamp_millis(snapshot.admitted_at_ms).unwrap(),
                )
                .unwrap();
            }
            let history = serde_json::to_vec(&bundle).unwrap();
            let calendar = serde_json::to_vec(&calendar).unwrap();
            plan.history_sha256 = digest(&history);
            plan.context_calendar_sha256 = digest(&calendar);
            let manifest = seed::build(&f.config, &plan, &history, &calendar).unwrap();
            // Current code passes these checks even though it silently drops
            // complete, required Thursday from the actual numerical context.
            seed::require_ready(&manifest).unwrap();
            let _prepared = prepare_fixture(&f, plan, &history, &calendar).unwrap();
            let dir = Dir::new();
            let journal = finam_shadow_models::local::Journal::create(
                &dir.0.join("reviewer-mixed-retention-models.sqlite"),
                manifest,
            )
            .unwrap();
            let exported = journal.export().unwrap();
            let weekday = exported["states"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["arm"] == "IMOEXF/weekday_only")
                .unwrap();
            assert_eq!(weekday["ready"], true);
            assert_eq!(
                weekday["model"]["frozen"]["previous"]["session_date"],
                "2026-09-25"
            );
            assert_eq!(weekday["model"]["frozen"]["previous2"]["session_date"], "2026-09-24",
                "Full required Thursday must not be silently quarantined in a legitimate first-session bootstrap; current code incorrectly chooses Wednesday 23 September");
        });
    }

    #[test]
    fn qualified_bootstrap_refuses_missing_regular_session_and_at_cutoff_clocks() {
        let f = fixture();
        with_fixture_clock(f.open(), || {
            let (mut plan, history, calendar) = morning_inputs(&f);
            let mut bundle: seed::HistoryBundle = serde_json::from_slice(&history).unwrap();
            let mut inventory: seed::ContextCalendar = serde_json::from_slice(&calendar).unwrap();
            // A complete Thursday plus weekend must not masquerade as the
            // Thursday/Friday pair when Friday is absent from ALL inputs.
            bundle.snapshots.remove(1);
            inventory.required_labels_ms.drain(101..202);
            let history = serde_json::to_vec(&bundle).unwrap();
            let calendar = serde_json::to_vec(&inventory).unwrap();
            plan.history_sha256 = digest(&history);
            plan.context_calendar_sha256 = digest(&calendar);
            let manifest = seed::build(&f.config, &plan, &history, &calendar).unwrap();
            assert!(seed::require_ready(&manifest).is_err());
            let d = Dir::new();
            let path = d.0.join("must-not-create.sqlite");
            assert!(finam_shadow_models::local::Journal::create(&path, manifest).is_err());
            assert!(!path.exists());

            let (plan, history, calendar) = morning_inputs(&f);
            let mut manifest = seed::build(&f.config, &plan, &history, &calendar).unwrap();
            let freeze = manifest.freeze_at_ms;
            for c in &mut manifest
                .seeds
                .iter_mut()
                .find(|s| s.binding.instrument == "IMOEXF")
                .unwrap()
                .history
            {
                c.received_ms = freeze;
                c.durable_ms = freeze;
                c.source_processed_ms = freeze;
            }
            assert!(
                seed::require_ready(&manifest).is_err(),
                "no at-cutoff import or backdating"
            );
        });
    }

    #[test]
    fn qualified_bootstrap_rejects_gap_but_keeps_approved_calendar_exclusions() {
        let f = fixture();
        with_fixture_clock(f.open(), || {
            let (plan, history, calendar) = morning_inputs(&f);
            let mut manifest = seed::build(&f.config, &plan, &history, &calendar).unwrap();
            // Numerical-only counterexample: two full regular sessions with
            // a missing Thursday between Wednesday and Friday.
            let bars = &mut manifest
                .seeds
                .iter_mut()
                .find(|s| s.binding.instrument == "IMOEXF")
                .unwrap()
                .history;
            for c in bars.iter_mut().take(101) {
                c.start_ms -= 86_400_000;
                c.end_ms -= 86_400_000;
            }
            let error = seed::require_ready(&manifest).unwrap_err();
            assert!(
                error.to_string().contains("missing regular session"),
                "{error:#}"
            );

            // Numerical-only calendar case: Nov 4 is the already pinned
            // exclusion, so full Nov 3 + Nov 5 is a legal pair for Nov 6.
            let date = chrono::NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
            let freeze = (date.and_hms_opt(4, 0, 0).unwrap())
                .and_utc()
                .timestamp_millis();
            manifest.session_date = date;
            manifest.freeze_at_ms = freeze;
            for s in &mut manifest.seeds {
                s.declared_consumed_through_ms = freeze;
            }
            let bars = &mut manifest
                .seeds
                .iter_mut()
                .find(|s| s.binding.instrument == "IMOEXF")
                .unwrap()
                .history;
            bars.truncate(202);
            for (i, c) in bars.iter_mut().enumerate() {
                let d = date - chrono::Duration::days(if i < 101 { 3 } else { 1 });
                c.start_ms = d.and_hms_opt(4, 0, 0).unwrap().and_utc().timestamp_millis()
                    + (i % 101) as i64 * 600000;
                c.end_ms = c.start_ms + 600000;
                c.received_ms = freeze - 900000 + i as i64;
                c.durable_ms = c.received_ms + 1;
                c.source_processed_ms = c.durable_ms;
            }
            seed::require_ready(&manifest).unwrap();
            let states = finam_shadow_models::local::Journal::initial_states(&manifest).unwrap();
            let state = states
                .iter()
                .find(|s| s["arm"] == "IMOEXF/weekday_only")
                .unwrap();
            assert_eq!(
                state["model"]["frozen"]["previous"]["session_date"],
                "2026-11-05"
            );
            assert_eq!(
                state["model"]["frozen"]["previous2"]["session_date"],
                "2026-11-03"
            );
        });
    }
}
