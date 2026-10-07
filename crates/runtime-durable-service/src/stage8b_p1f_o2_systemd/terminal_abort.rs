//! Fixed, root-only staged operator. Reads the OLD installation; never installs
//! itself, stops/starts a unit, signs, contacts Redis or fetches market data.
use super::*;
use crate::stage8b_p1f_guardian::materialization_abort::incident_pins;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

const PRESERVATION: &str =
    include_str!("../../../../docs/stage-8/stage8b-p1f-o2-abort-preservation.json");
const STAGING: &str = "/var/lib/moex-finam-p1-paper-o2-staging";
type Result<T> = std::result::Result<T, Stage8bP1fO2RunnerErrorV1>;

fn conflict() -> Stage8bP1fO2RunnerErrorV1 {
    Stage8bP1fAuthorityErrorV1::HistoryConflict.into()
}
fn io(e: std::io::Error) -> Stage8bP1fO2RunnerErrorV1 {
    Stage8bP1fAuthorityErrorV1::Io(e.kind()).into()
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// Read only compiled-in baseline paths, never paths supplied by an external
// JSON file. Report actual bytes' hashes and metadata, not retained PASS flags.
fn file(path: &Path, uid: u32, gid: u32, mode: u32, size: u64) -> Result<Vec<u8>> {
    if fs::canonicalize(path).map_err(io)? != path || size > 32 * 1024 * 1024 {
        return Err(conflict());
    }
    let before = fs::symlink_metadata(path).map_err(io)?;
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(io)?;
    let opened = f.metadata().map_err(io)?;
    for m in [&before, &opened] {
        if !m.is_file()
            || m.nlink() != 1
            || m.uid() != uid
            || m.gid() != gid
            || m.mode() & 0o7777 != mode
            || m.len() != size
        {
            return Err(conflict());
        }
    }
    if before.dev() != opened.dev() || before.ino() != opened.ino() {
        return Err(conflict());
    }
    let mut bytes = Vec::new();
    (&mut f)
        .take(size + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    let after = fs::symlink_metadata(path).map_err(io)?;
    if bytes.len() as u64 != size
        || before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err(conflict());
    }
    Ok(bytes)
}

fn inventory_entry(path: &Path, e: &Value) -> Result<Value> {
    let m = fs::symlink_metadata(path).map_err(io)?;
    let mode = e["mode"].as_str().ok_or_else(conflict)?;
    let mode = u32::from_str_radix(mode.trim_start_matches("0o"), 8).map_err(|_| conflict())?;
    if fs::canonicalize(path).map_err(io)? != path
        || Some(m.uid() as u64) != e["uid"].as_u64()
        || Some(m.gid() as u64) != e["gid"].as_u64()
        || m.mode() & 0o7777 != mode
        || Some(m.nlink()) != e["nlink"].as_u64()
    {
        return Err(conflict());
    }
    let mut actual = json!({"uid":m.uid(),"gid":m.gid(),"mode":format!("0o{:o}",m.mode() & 0o7777),"nlink":m.nlink(),"size":m.len(),"type":m.mode() & 0o170000});
    if let Some(hash) = e["sha256"].as_str() {
        let bytes = file(
            path,
            m.uid(),
            m.gid(),
            mode,
            e["size"].as_u64().ok_or_else(conflict)?,
        )?;
        actual["sha256"] = json!(sha(&bytes));
        if actual["sha256"] != hash {
            return Err(conflict());
        }
    } else if !m.is_dir() {
        return Err(conflict());
    }
    Ok(actual)
}

fn tree(root: &Path, expected: &Value) -> Result<Value> {
    let expected = expected.as_object().ok_or_else(conflict)?;
    let mut actual = serde_json::Map::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(path) = todo.pop() {
        let name = if path == root {
            ".".into()
        } else {
            path.strip_prefix(root)
                .map_err(|_| conflict())?
                .to_string_lossy()
                .into_owned()
        };
        let e = expected.get(&name).ok_or_else(conflict)?;
        actual.insert(name, inventory_entry(&path, e)?);
        if fs::symlink_metadata(&path).map_err(io)?.is_dir() {
            for entry in fs::read_dir(path).map_err(io)? {
                todo.push(entry.map_err(io)?.path());
            }
        }
    }
    if actual.len() != expected.len() {
        return Err(conflict());
    }
    Ok(Value::Object(actual))
}

fn inventory(expected: &Value) -> Result<Value> {
    let mut slots = serde_json::Map::new();
    for (name, e) in expected["payload"].as_object().ok_or_else(conflict)? {
        slots.insert(name.clone(), inventory_entry(Path::new(name), e)?);
    }
    if slots.len() != 16 {
        return Err(conflict());
    }
    let mut trees = serde_json::Map::new();
    for (name, e) in expected["trees"].as_object().ok_or_else(conflict)? {
        trees.insert(name.clone(), tree(Path::new(name), e)?);
    }
    let mut retained = serde_json::Map::new();
    for (name, e) in expected["retained_authority"]
        .as_object()
        .ok_or_else(conflict)?
    {
        retained.insert(
            name.clone(),
            inventory_entry(
                &Path::new(crate::STAGE8B_P1F_AUTHORITY_CONTROL_ROOT).join(name),
                e,
            )?,
        );
    }
    for path in [
        "/etc/moex-finam-p1-paper/credentials/stage8b-p1-lifecycle.key",
        "/run/moex-finam-p1f-o2-input",
    ] {
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            _ => return Err(conflict()),
        }
    }
    Ok(json!({"installed_slots":slots, "retained_trees":trees, "retained_authority":retained}))
}

async fn units(expected: &Value) -> Result<Value> {
    let mut p1 = Vec::new();
    for name in [
        BOOTSTRAP_UNIT,
        MATERIALIZER_UNIT,
        STAGE8B_P1F_O2_RUNNER_UNIT,
        "moex-finam-p1-paper.service",
    ] {
        let evidence = collect_unit_evidence(name).await?;
        require_stopped(&evidence, name)?;
        p1.push(evidence);
    }
    let mut p0 = serde_json::Map::new();
    for (name, e) in expected["p0_units"].as_object().ok_or_else(conflict)? {
        let props = e
            .as_object()
            .ok_or_else(conflict)?
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        let output = bounded_systemctl(&["show", name, &format!("--property={props}")]).await?;
        if !output.status.success() {
            return Err(conflict());
        }
        let text = std::str::from_utf8(&output.stdout).map_err(|_| conflict())?;
        let mut actual = serde_json::Map::new();
        for line in text.lines() {
            let (key, value) = line.split_once('=').ok_or_else(conflict)?;
            if actual.insert(key.into(), json!(value)).is_some() {
                return Err(conflict());
            }
        }
        if Value::Object(actual.clone()) != *e {
            return Err(conflict());
        }
        let path = PathBuf::from("/etc/systemd/system").join(name);
        let size = fs::symlink_metadata(&path).map_err(io)?.len();
        let hash = sha(&file(&path, 0, 0, 0o644, size)?);
        if expected["p0_fragments"][name] != hash {
            return Err(conflict());
        }
        p0.insert(
            name.clone(),
            json!({"properties":actual,"fragment_sha256":hash}),
        );
    }
    Ok(json!({"p0":p0,"p1":p1}))
}

/// Exactly the reviewed expired October-7 phase. No caller-selected paths,
/// generation, timestamps, command, pins or alternative installation.
pub async fn recover_stage8b_p1f_o2_expired_materialization_fixed_v1() -> Result<Value> {
    require_root()?;
    let gid = service_group_gid()?;
    if gid != 987 {
        return Err(conflict());
    }
    let pins = incident_pins();
    if sha(PRESERVATION.as_bytes())
        != "3fa3808a09223799f9a04b89a48c2e5d78329ebbc8a2a50b4ad33623a5f48c8c"
    {
        return Err(conflict());
    }
    let store = Stage8bP1fAuthorityStoreV1::open_production(gid)?;
    let _execution = store.lock_o2_effects()?;
    if read_fixed_manifest_selector()? != pins.phase {
        return Err(conflict());
    }
    let expected: Value = serde_json::from_str(PRESERVATION).map_err(|_| conflict())?;
    let before = inventory(&expected)?;
    let stopped = units(&expected).await?;
    let installation = read_installation()?;
    let staged_path = Path::new(STAGING).join(format!("{}.json", pins.phase));
    let m = fs::symlink_metadata(STAGING).map_err(io)?;
    if !m.is_dir() || m.uid() != 0 || m.gid() != 0 || m.mode() & 0o7777 != 0o700 {
        return Err(conflict());
    }
    let staged_size = fs::symlink_metadata(&staged_path).map_err(io)?.len();
    let staged = file(&staged_path, 0, 0, 0o400, staged_size)?;
    let phase_path = Path::new(CONFIG_ROOT).join("o2/o2-phase-manifest.signed.json");
    let phase_size = fs::symlink_metadata(&phase_path).map_err(io)?.len();
    if sha(&file(&phase_path, 0, gid, 0o440, phase_size)?) != pins.phase {
        return Err(conflict());
    }
    // Recheck selector and preservation immediately before the first guardian
    // mutation. Both locks guard the transaction; no stop or start is issued.
    if inventory(&expected)? != before || read_fixed_manifest_selector()? != pins.phase {
        return Err(conflict());
    }
    let receipt = store.abort_expired_materialization(
        &pins,
        Path::new(CONFIG_ROOT),
        &installation,
        &staged,
        Utc::now(),
    )?;
    let after = inventory(&expected)?;
    let stopped_after = units(&expected).await?;
    if after != before || stopped_after != stopped {
        return Err(conflict());
    }
    Ok(
        json!({"schema_version":1,"domain":"stage8b-p1f-oct7-terminal-abort-result-v1",
        "terminal_receipt":receipt,"before_inventory":before,"after_inventory":after,
        "before_units":stopped,"after_units":stopped_after,
        "closed_surfaces":["FINAM","Redis","Hybrid","bootstrap","ReadyForBootstrap"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abort_preflight_requires_actual_stopped_units_not_asserted_flags() {
        let base = Stage8bP1fO2UnitEvidenceV1 {
            schema_version: 1,
            domain: "stage8b-p1f-o2-unit-evidence-v1".into(),
            unit: BOOTSTRAP_UNIT.into(),
            active_state: "inactive".into(),
            sub_state: "dead".into(),
            result: "success".into(),
            exec_main_status: 0,
            main_pid: 0,
            control_pid: 0,
            job: String::new(),
            control_group: String::new(),
            cgroup_procs_empty: true,
            stopped_proven: true,
        };
        assert!(require_stopped(&base, BOOTSTRAP_UNIT).is_ok());
        for n in 0..6 {
            let mut e = base.clone();
            match n {
                0 => e.main_pid = 100,
                1 => e.control_pid = 100,
                2 => e.job = "42".into(),
                3 => e.cgroup_procs_empty = false,
                4 => e.active_state = "active".into(),
                _ => e.unit = MATERIALIZER_UNIT.into(),
            }
            assert!(require_stopped(&e, BOOTSTRAP_UNIT).is_err());
        }
    }

    #[test]
    fn abort_preflight_real_file_inventory_rejects_drift_and_links() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let scratch = std::env::current_dir().unwrap().join("target");
        fs::create_dir_all(&scratch).unwrap();
        let root = scratch.join(format!(
            "abort-preflight-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("slot");
        fs::write(&path, b"retained").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let uid = unsafe { libc::geteuid() };
        let gid = unsafe { libc::getegid() };
        let expected = json!({"uid":uid,"gid":gid,"mode":"0o600","size":8,"nlink":1,"sha256":sha(b"retained")});
        assert!(inventory_entry(&path, &expected).is_ok());
        fs::write(&path, b"changed!").unwrap();
        assert!(inventory_entry(&path, &expected).is_err());
        fs::write(&path, b"retained").unwrap();
        fs::hard_link(&path, root.join("alias")).unwrap();
        assert!(inventory_entry(&path, &expected).is_err());
        fs::remove_file(root.join("alias")).unwrap();
        fs::rename(&path, root.join("original")).unwrap();
        std::os::unix::fs::symlink(root.join("original"), &path).unwrap();
        assert!(inventory_entry(&path, &expected).is_err());
        fs::remove_dir_all(root).unwrap();
        let pins = incident_pins();
        let baseline: Value = serde_json::from_str(PRESERVATION).unwrap();
        assert_eq!(baseline["payload"].as_object().unwrap().len(), 16);
        assert_eq!(
            baseline["payload"][INSTALLATION_PATH]["sha256"],
            pins.installation
        );
    }
}
