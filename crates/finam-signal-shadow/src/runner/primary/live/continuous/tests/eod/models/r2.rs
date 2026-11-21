//! Actual aggregate reader errors on matching SQL and native inputs.
use super::*;
pub(super) fn cases(
    f: &Fixture,
    plan: &ModelPlan,
    manifest: &Manifest,
    rows: &[Value],
    db: &std::path::Path,
    report: &Value,
) {
    let mut diagnostics = Vec::new();
    for (case, guard) in [
        (
            "shutdown_recovery_unresolved",
            "final recovery facts contradict completion",
        ),
        (
            "shutdown_unresolved_gap",
            "final gap facts contradict completion",
        ),
        (
            "shutdown_http_accounting_inconsistent",
            "final HTTP accounting arithmetic",
        ),
    ] {
        let mut changed = rows.to_vec();
        let mut report = report.clone();
        let shutdown = changed
            .iter_mut()
            .find(|r| r["event"]["event"] == "primary_async_shutdown")
            .unwrap();
        match case {
            "shutdown_recovery_unresolved" => {
                shutdown["event"]["recovery"][0]["canonical_unresolved"] = json!(true);
                report["Ok"]["recovery"][0]["canonical_unresolved"] = json!(true);
            }
            "shutdown_unresolved_gap" => {
                shutdown["event"]["unresolved_canonical_gap"] = json!(true)
            }
            _ => {
                shutdown["event"]["http"]["recovery"]["completed"] = json!(0);
                report["Ok"]["http"]["recovery"]["completed"] = json!(0);
            }
        }
        let encoded: Vec<_> = changed
            .iter()
            .flat_map(|r| {
                let mut b = serde_json::to_vec(r).unwrap();
                b.push(b'\n');
                b
            })
            .collect();
        let error = reader::evaluate(
            &f.config,
            plan,
            manifest,
            &encoded,
            db,
            &report,
            crate::timing::now_ms(),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), guard, "{case}: {error:#}");
        diagnostics.push(json!({"case":case,"result":"REFUSED","rust_error":format!("{error:#}"),
            "manifest_sha256":manifest.sha256().unwrap(),"source_config_sha256":plan.source_config_sha256}));
    }
    if let Some(root) = std::env::var_os("FINAM_MODELS_ENTRY_FIXTURES_DIR") {
        let path = std::path::PathBuf::from(root).join("r2-observed-rust-guards.json");
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&diagnostics).unwrap())
            .unwrap();
        file.sync_all().unwrap();
    }
}
