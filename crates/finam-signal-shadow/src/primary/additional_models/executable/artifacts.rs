//! Three entry artifacts; final marker only after core reader checked join/SQL.
use super::*;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
};
const REQUIRED: [&str; 5] = [
    "entry.json",
    "entry-history.json",
    "managed-control.jsonl",
    "model-manifest.json",
    "models.sqlite",
];
pub(super) fn read(path: &Path, bound: u64) -> Result<Vec<u8>> {
    let m = std::fs::symlink_metadata(path)?;
    ensure!(
        m.is_file() && !m.file_type().is_symlink() && m.len() <= bound,
        "regular bounded entry file"
    );
    let mut bytes = Vec::new();
    File::open(path)?.take(bound + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= bound, "entry read bound");
    Ok(bytes)
}
pub(super) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure!(bytes.len() <= MAX_INPUT_BYTES, "entry artifact bound");
    let mut o = OpenOptions::new();
    o.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut file = o.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn json(path: &Path, bound: u64) -> Result<Value> {
    Ok(crate::primary::json::parse(std::str::from_utf8(&read(
        path, bound,
    )?)?)?)
}
fn content(output: &Path) -> Result<Value> {
    let admission = json(&output.join("entry.json"), 4 * 1024 * 1024)?;
    let history = json(&output.join("entry-history.json"), MAX_INPUT_BYTES as u64)?;
    let manifest: Manifest = serde_json::from_value(json(
        &output.join("model-manifest.json"),
        MAX_INPUT_BYTES as u64,
    )?)?;
    let control = read(&output.join("managed-control.jsonl"), 64 * 1024 * 1024)?;
    let first = control
        .split(|b| *b == b'\n')
        .next()
        .context("entry source preparation")?;
    let first: Value = serde_json::from_slice(first)?;
    let source = &first["payload"]["data"]["component"];
    ensure!(
        admission["schema"] == "finam.shadow.usd-ri.executable-admission.v1"
            && admission["source_only"] == true
            && admission["operational_authority"] == false
            && admission["orders"] == false
            && admission["source_admission"] == *source
            && admission["manifest_sha256"] == manifest.sha256()?
            && admission["account_sha256"] == source["plan"]["account_sha256"]
            && super::super::plan::sha(
                admission["input_sha256"]
                    .as_str()
                    .context("entry input hash")?
            )
            && admission["history_sha256"] == hash(&history)?
            && history["schema"] == "finam.shadow.usd-ri.seed-evidence.v1",
        "entry/source/seed binding"
    );
    let sources: Vec<HistoryInput> = serde_json::from_value(history["sources"].clone())?;
    let original_time = source["calendars"][0]["evidence"]["admitted_at_ms"]
        .as_i64()
        .context("source original admission")?;
    ensure!(
        admission["admitted_at_ms"] == original_time,
        "entry admission clock"
    );
    let refs = sources.iter().collect::<Vec<_>>();
    ensure!(
        seed::build(
            &manifest.generation,
            manifest.session_date,
            &refs,
            original_time
        )? == manifest,
        "exact original seed evidence"
    );
    super::super::managed::reader::inspect(output)
}
pub(super) fn finish(
    output: &Path,
    result: Result<super::super::managed::Finished>,
) -> Result<Value> {
    let incomplete = || -> Result<Value> {
        let r = json!({"status":"INCOMPLETE","scope":"source_executable_only","operational_authority":false,"orders":false});
        write_new(
            &output.join("entry-incomplete.json"),
            &serde_json::to_vec(&r)?,
        )?;
        File::open(output)?.sync_all()?;
        Ok(r)
    };
    let Ok(done) = result else {
        return incomplete();
    };
    if done.output != output
        || !done.evidence["reports"].as_array().is_some_and(|reports| {
            reports.len() == 2
                && reports.iter().all(|r| {
                    r["result"]["Ok"]["all_workers_drained"] == true
                        && r["result"]["Ok"]["session_finalized"] == true
                })
        })
    {
        return incomplete();
    }
    let source = match content(output) {
        Ok(v) => v,
        Err(_) => return incomplete(),
    };
    let mut hashes = std::collections::BTreeMap::new();
    for n in REQUIRED {
        hashes.insert(
            n,
            crate::seed::digest(&read(&output.join(n), 512 * 1024 * 1024)?),
        );
    }
    let result = json!({"schema":"finam.shadow.usd-ri.executable-result.v1","status":source["status"],"source_result":source,
        "files_sha256":hashes,"scope":"source_executable_only","observed_clean_session":false,
        "native_systemd_qualified":false,"operational_authority":false,"orders":false});
    let path = output.join("entry-result.json");
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // Ownership starts only after create_new succeeds. A refused repeated
    // export must never unlink the original durable completion.
    let mut file = options.open(&path)?;
    if let Err(e) = (|| -> Result<()> {
        file.write_all(&serde_json::to_vec(&result)?)?;
        file.sync_all()?;
        File::open(output)?.sync_all()?;
        Ok(())
    })() {
        let _ = std::fs::remove_file(&path);
        let _ = File::open(output).and_then(|f| f.sync_all());
        return Err(e);
    }
    Ok(result)
}
pub(super) fn inspect(output: &Path) -> Result<Value> {
    ensure!(
        !output.join("entry-incomplete.json").exists(),
        "incomplete executable export"
    );
    let result = json(&output.join("entry-result.json"), MAX_INPUT_BYTES as u64)?;
    ensure!(
        result["schema"] == "finam.shadow.usd-ri.executable-result.v1"
            && result["scope"] == "source_executable_only"
            && result["observed_clean_session"] == false
            && result["native_systemd_qualified"] == false
            && result["operational_authority"] == false
            && result["orders"] == false,
        "entry result scope"
    );
    let hashes = result["files_sha256"]
        .as_object()
        .context("entry artifact hashes")?;
    ensure!(hashes.len() == REQUIRED.len(), "entry artifact inventory");
    for n in REQUIRED {
        ensure!(
            hashes.get(n)
                == Some(&json!(crate::seed::digest(&read(
                    &output.join(n),
                    512 * 1024 * 1024
                )?))),
            "entry artifact hash"
        );
    }
    let current = content(output)?;
    ensure!(
        result["source_result"] == current && result["status"] == current["status"],
        "entry result readback"
    );
    Ok(result)
}
