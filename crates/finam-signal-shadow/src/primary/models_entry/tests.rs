use super::*;
use crate::primary::calendar::tests::{Dir, Fixture};
use crate::timing::with_fixture_clock;
use chrono::{Datelike, Duration};

pub(crate) fn fixture() -> Fixture {
    let mut f = Fixture::historical();
    f.config.primary.delivery = DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8;
    f.config.base.previous_segment_id = None;
    f.config.base.max_input_bytes = 8 * 1024 * 1024;
    f
}
pub(crate) fn inputs(f: &Fixture) -> (ModelPlan, Vec<u8>, Vec<u8>) {
    let day = (chrono::DateTime::from_timestamp_millis(f.open()).unwrap() + Duration::hours(3))
        .date_naive();
    let mut snapshots = Vec::new();
    let mut required = Vec::new();
    for offset in (1..=4).rev() {
        let date = day - Duration::days(offset);
        let weekend = date.weekday().num_days_from_monday() >= 5;
        let start = (date
            .and_hms_opt(if weekend { 9 } else { 7 }, if weekend { 50 } else { 0 }, 0)
            .unwrap()
            - Duration::hours(3))
        .and_utc()
        .timestamp_millis();
        let labels: Vec<_> = (0..if weekend { 55 } else { 101 })
            .map(|i| start + i * 600000)
            .collect();
        let end = labels.last().unwrap() + 600000;
        let source = crate::primary::tests::reference(
            labels.iter().map(|&t| f.row(t)).collect(),
            start,
            end,
            end + 100,
        );
        snapshots.push(seed::HistorySnapshot {
            admitted_at_ms: end + 200,
            evidence: source.evidence().clone(),
        });
        required.extend(labels);
    }
    let history = serde_json::to_vec(&seed::HistoryBundle {
        schema_version: 1,
        snapshots,
    })
    .unwrap();
    let calendar = serde_json::to_vec(&seed::ContextCalendar {
        schema_version: 1,
        required_labels_ms: required,
    })
    .unwrap();
    let plan = ModelPlan {
        schema_version: 1,
        source_config_sha256: f.config.fingerprint().unwrap(),
        generation: "synthetic-production-seed-cohort".into(),
        arms: ARMS.map(String::from),
        history_sha256: digest(&history),
        context_calendar_sha256: digest(&calendar),
        stop: StopMode::Bounded {
            stop_at_ms: f.open() + 4 * 600000 + 5000,
        },
    };
    (plan, history, calendar)
}
pub(crate) fn prepare_fixture(
    f: &Fixture,
    p: ModelPlan,
    h: &[u8],
    c: &[u8],
) -> Result<PreparedModels> {
    let model = serde_json::to_vec(&f.model).unwrap();
    let raw = serde_json::to_vec(&f.raw).unwrap();
    prepare(
        f.config.clone(),
        entry::tests::plan(f, entry::Startup::FreshSession),
        entry::Inputs {
            reference: &f.materials[0],
            mapping: &f.materials[1],
            seed_calendar: &f.materials[2],
            seed: &f.materials[3],
            model_day: &model,
            raw_calendar: &raw,
            prefix: None,
        },
        p,
        h,
        c,
    )
}

#[test]
fn model_entry_source_history_is_deterministic_sparse_exact_and_readonly() {
    let f = fixture();
    with_fixture_clock(f.open(), || {
        let (p, h, c) = inputs(&f);
        let a = seed::build(&f.config, &p, &h, &c).unwrap();
        let b = seed::build(&f.config, &p, &h, &c).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.seeds.iter().filter(|s| !s.history.is_empty()).count(), 1);
        seed::require_ready(&a).unwrap();
        let prepared = prepare_fixture(&f, p, &h, &c).unwrap();
        let d = Dir::new();
        assert!(!d.0.join("out").exists());
        let receipt = prepared.receipt().unwrap();
        assert_eq!(receipt["arms"], json!(ARMS));
        assert_eq!(receipt["broker_execution"], false);
        let state = prepared
            .open(&d.0.join("out"), StopHandle::default())
            .unwrap();
        assert_eq!(state.opened.owner.processor().counters().committed, 0);
        assert!(state.output.join("models.sqlite").is_file());
        assert!(!state.output.join("models-artifact-manifest.json").exists());
        let before = std::fs::read(state.output.join("models.sqlite")).unwrap();
        let inspected =
            finam_shadow_models::local::Journal::inspect(&state.output.join("models.sqlite"), a)
                .unwrap();
        assert_eq!(inspected["revision"], 0);
        assert_eq!(
            before,
            std::fs::read(state.output.join("models.sqlite")).unwrap()
        );
    });
}

#[test]
fn model_entry_rejects_wrong_plan_history_future_source_and_missing_context_before_output() {
    let f = fixture();
    with_fixture_clock(f.open(), || {
        let (p, h, c) = inputs(&f);
        for case in 0..6 {
            let mut p = p.clone();
            let mut h = h.clone();
            let mut c = c.clone();
            match case {
                0 => p.arms.swap(0, 1),
                1 => p.source_config_sha256 = "0".repeat(64),
                2 => h.push(b' '),
                3 => {
                    let mut v: Value = serde_json::from_slice(&h).unwrap();
                    v["snapshots"][0]["admitted_at_ms"] = json!(f.open() + 1);
                    h = serde_json::to_vec(&v).unwrap();
                    p.history_sha256 = digest(&h);
                }
                4 => {
                    let mut v: Value = serde_json::from_slice(&h).unwrap();
                    v["snapshots"][0]["evidence"]["parts"][0]["status"] = json!(500);
                    h = serde_json::to_vec(&v).unwrap();
                    p.history_sha256 = digest(&h);
                }
                5 => {
                    let mut v: Value = serde_json::from_slice(&c).unwrap();
                    v["required_labels_ms"] = json!([f.open() - 2 * 600000]);
                    c = serde_json::to_vec(&v).unwrap();
                    p.context_calendar_sha256 = digest(&c);
                }
                _ => unreachable!(),
            }
            assert!(prepare_fixture(&f, p, &h, &c).is_err(), "case {case}");
        }
        let mut bad = serde_json::to_string(&p).unwrap();
        bad = bad.replacen('{', "{\"schema_version\":1,", 1);
        assert!(ModelPlan::parse(bad.as_bytes()).is_err());
    });
}

#[test]
fn model_entry_stopped_late_or_existing_output_cannot_fallback_to_baseline() {
    let f = fixture();
    with_fixture_clock(f.open(), || {
        let (p, h, c) = inputs(&f);
        let d = Dir::new();
        let stop = StopHandle::default();
        stop.stop();
        assert!(prepare_fixture(&f, p.clone(), &h, &c)
            .unwrap()
            .open(&d.0.join("stopped"), stop)
            .is_err());
        assert!(!d.0.join("stopped").exists());
        std::fs::create_dir(d.0.join("exists")).unwrap();
        assert!(prepare_fixture(&f, p.clone(), &h, &c)
            .unwrap()
            .open(&d.0.join("exists"), StopHandle::default())
            .is_err());
        let prepared = prepare_fixture(&f, p, &h, &c).unwrap();
        crate::timing::advance_fixture_clock(600000);
        assert!(prepared
            .open(&d.0.join("late"), StopHandle::default())
            .is_err());
        assert!(!d.0.join("late").exists());
    });
}
