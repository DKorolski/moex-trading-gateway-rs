//! Fixed create-new files; cohort manifest last, only after writer drain.
use super::*;
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
};
pub(crate) fn read(path: &Path, bound: u64) -> Result<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(meta.is_file() && meta.len() <= bound, "file type/bound");
    let mut bytes = Vec::new();
    File::open(path)?.take(bound + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= bound, "file bound");
    Ok(bytes)
}
fn options() -> OpenOptions {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o
}
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure!(bytes.len() <= 24 * 1024 * 1024, "export bound");
    let mut file = options().open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn json_bytes(v: &impl Serialize) -> Result<Vec<u8>> {
    let mut b = serde_json::to_vec_pretty(v)?;
    b.push(b'\n');
    Ok(b)
}
pub(crate) fn record_inputs(
    output: &Path,
    plan: &ModelPlan,
    manifest: &finam_shadow_models::local::contract::Manifest,
    history: &[u8],
    calendar: &[u8],
    receipt: &Value,
) -> Result<()> {
    for (name, bytes) in [
        ("models-plan.json", json_bytes(plan)?),
        ("models-manifest.json", json_bytes(manifest)?),
        ("models-history.json", history.to_vec()),
        ("models-context-calendar.json", calendar.to_vec()),
        ("models-admission.json", json_bytes(receipt)?),
    ] {
        write(&output.join(name), &bytes)?;
    }
    File::open(output)?.sync_all()?;
    Ok(())
}
pub(crate) fn finish(
    output: &Path,
    config: &WsPrimaryConfig,
    plan: &ModelPlan,
    manifest: &finam_shadow_models::local::contract::Manifest,
    report: &Value,
    ordinary_export_returned: bool,
) -> Result<Value> {
    write(
        &output.join("models-post-join-report.json"),
        &json_bytes(report)?,
    )?;
    let incomplete = |reason: &'static str| -> Result<Value> {
        let result = json!({"status":"PARTIAL","coherent":false,"cohort_manifest_published":false,
            "reason":reason,"observed_clean_session":false,"broker_execution":false,
            "all_workers_drained":report["Ok"]["all_workers_drained"]==true,
            "requires_writer_completion_witness":report["Ok"]["all_workers_drained"]!=true});
        write(
            &output.join("models-incomplete.json"),
            &json_bytes(&result)?,
        )?;
        File::open(output)?.sync_all()?;
        Ok(result)
    };
    if report["Ok"]["all_workers_drained"] != true {
        return incomplete("workers_not_drained");
    }
    if !ordinary_export_returned {
        return incomplete("control_export_failed");
    }
    let journal = read(
        &output.join("journal.jsonl"),
        config.base.max_output_bytes.min(64 * 1024 * 1024),
    )?;
    let completion = match reader::evaluate(
        config,
        plan,
        manifest,
        &journal,
        &output.join("models.sqlite"),
        report,
        crate::timing::now_ms(),
    ) {
        Ok(done) => done,
        Err(_) => return incomplete("cohort_verification_refused"),
    };
    write(
        &output.join("models-completion.json"),
        &json_bytes(&completion)?,
    )?;
    let mut hashes = BTreeMap::new();
    for name in REQUIRED {
        hashes.insert(name, digest(&read(&output.join(name), 64 * 1024 * 1024)?));
    }
    let document = json_bytes(
        &json!({"schema":"finam.imoexf.cohort-artifacts.v1","files_sha256":hashes,
        "requires_successful_export_return":true,"manifest_alone_is_not_durability_or_worker_witness":true}),
    )?;
    // Only THIS newly-created final marker is removed on failed publication.
    let path = output.join("models-artifact-manifest.json");
    let mut final_file = options().open(&path)?;
    if let Err(error) = (|| -> std::io::Result<()> {
        final_file.write_all(&document)?;
        final_file.sync_all()?;
        File::open(output)?.sync_all()
    })() {
        let _ = std::fs::remove_file(&path);
        let _ = File::open(output).and_then(|d| d.sync_all());
        return Err(error.into());
    }
    Ok(serde_json::to_value(completion)?)
}
const REQUIRED: [&str; 11] = [
    "models-plan.json",
    "models-history.json",
    "models-context-calendar.json",
    "models-manifest.json",
    "models-admission.json",
    "models-post-join-report.json",
    "models-completion.json",
    "journal.jsonl",
    "models.sqlite",
    "entry.json",
    "run-report.json",
];
pub(crate) fn verify_manifest(output: &Path) -> Result<()> {
    ensure!(
        std::fs::symlink_metadata(output)?.is_dir(),
        "output directory"
    );
    ensure!(
        !output.join("models-incomplete.json").exists()
            && !output.join("export-failure.json").exists()
            && !output.join("export-incomplete.json").exists(),
        "failed/incomplete export"
    );
    let document: Value = parse(
        &read(&output.join("models-artifact-manifest.json"), 16384)?,
        16384,
    )?;
    ensure!(
        document["schema"] == "finam.imoexf.cohort-artifacts.v1",
        "artifact schema"
    );
    let hashes = document["files_sha256"]
        .as_object()
        .context("artifact manifest")?;
    ensure!(hashes.len() == REQUIRED.len(), "artifact inventory");
    for name in REQUIRED {
        ensure!(
            hashes.get(name) == Some(&json!(digest(&read(&output.join(name), 64 * 1024 * 1024)?))),
            "artifact digest"
        );
    }
    Ok(())
}
