use std::ffi::CString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::ExitCode;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use runtime_durable_service::{
    collect_stage8b_p1f_o2_readonly_evidence_v1, collect_stage8b_p1f_o2_unit_evidence_v1,
    run_stage8b_p1f_o2_fixed_cleanup_v1, run_stage8b_p1f_o2_fixed_systemd_runner_v1,
    sign_stage8b_p1f_activation_certificate_v1, sign_stage8b_p1f_genesis_manifest_v1,
    sign_stage8b_p1f_phase_manifest_v1, stage8b_p1f_authority_public_key_hex,
    stage8b_p1f_o2_active_manifest_sha256_v1, Stage8bP1fActivationCertificateV1,
    Stage8bP1fAuthorityStoreV1, Stage8bP1fClaimDispositionV1, Stage8bP1fGenesisManifestV1,
    Stage8bP1fPhaseManifestV1, STAGE8B_P1F_AUTHORITY_CONTROL_ROOT, STAGE8B_P1F_MAX_AUTHORITY_BYTES,
    STAGE8B_P1F_SERVICE_USER,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use zeroize::Zeroizing;

const O2_POLICY_PATH: &str = "/etc/moex-finam-p1-paper/o2/materialization-policy.json";
const O2_CONFIG_TEMPLATE_PATH: &str = "/etc/moex-finam-p1-paper/o2/supervisor.template.json";
const O2_SOURCE_TEMPLATE_PATH: &str = "/etc/moex-finam-p1-paper/o2/source-template.json";
const O2_STAGING_ROOT: &str = "/var/lib/moex-finam-p1-paper-o2-staging";
const O2_MAX_STAGED_BYTES: u64 = 32 * 1024 * 1024;
const O2_PUBLIC_KEY_PATH: &str = "/etc/moex-finam-p1-paper/o2/authority-public-key.hex";
const O2_GENESIS_PATH: &str = "/etc/moex-finam-p1-paper/o2/genesis-manifest.signed.json";
const O2_ACTIVATION_PATH: &str = "/etc/moex-finam-p1-paper/o2/activation-certificate.signed.json";
const O2_PHASE_PATH: &str = "/etc/moex-finam-p1-paper/o2/o2-phase-manifest.signed.json";
const O2_KEY_ID: &str = "stage8b-p1f-o2-authority-generation-1";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("stage8b-p1f-o2-operator: {error}");
            ExitCode::from(70)
        }
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [command, key] if command == "public-key" => {
            let signing = read_signing_key(Path::new(key))?;
            println!("{}", stage8b_p1f_authority_public_key_hex(&signing));
        }
        [command, key, input, output] if command == "sign-genesis" => {
            let signing = read_signing_key(Path::new(key))?;
            let document = read_json::<Stage8bP1fGenesisManifestV1>(Path::new(input))?;
            let signed = sign_stage8b_p1f_genesis_manifest_v1(document, &signing)
                .map_err(|error| error.to_string())?;
            write_create_new(Path::new(output), &signed, 0o600)?;
        }
        [command, key, input, output] if command == "sign-activation" => {
            let signing = read_signing_key(Path::new(key))?;
            let document = read_json::<Stage8bP1fActivationCertificateV1>(Path::new(input))?;
            let signed = sign_stage8b_p1f_activation_certificate_v1(document, &signing)
                .map_err(|error| error.to_string())?;
            write_create_new(Path::new(output), &signed, 0o600)?;
        }
        [command, key, input, output] if command == "sign-phase" => {
            let signing = read_signing_key(Path::new(key))?;
            let document = read_json::<Stage8bP1fPhaseManifestV1>(Path::new(input))?;
            let signed = sign_stage8b_p1f_phase_manifest_v1(document, &signing)
                .map_err(|error| error.to_string())?;
            write_create_new(Path::new(output), &signed, 0o600)?;
        }
        [command] if command == "prepare-control-root" => prepare_control_root()?,
        [command] if command == "guardian-init-fixed" => {
            let store = open_store()?;
            let receipt = store
                .initialize_authority(
                    &read_root_owned(
                        Path::new(O2_GENESIS_PATH),
                        STAGE8B_P1F_MAX_AUTHORITY_BYTES,
                        0o440,
                    )?,
                    &fixed_public_key()?,
                    O2_KEY_ID,
                    Utc::now(),
                )
                .map_err(|error| error.to_string())?;
            print_json(&receipt)?;
        }
        [command] if command == "guardian-activate-fixed" => {
            let store = open_store()?;
            store
                .activate_authority(
                    &read_root_owned(
                        Path::new(O2_ACTIVATION_PATH),
                        STAGE8B_P1F_MAX_AUTHORITY_BYTES,
                        0o440,
                    )?,
                    &fixed_public_key()?,
                    O2_KEY_ID,
                    Utc::now(),
                )
                .map_err(|error| error.to_string())?;
            print_json(&store.inspect().map_err(|error| error.to_string())?)?;
        }
        [command] if command == "guardian-claim-fixed" => {
            let store = open_store()?;
            let manifest_bytes = read_root_owned(
                Path::new(O2_PHASE_PATH),
                STAGE8B_P1F_MAX_AUTHORITY_BYTES,
                0o440,
            )?;
            let disposition = store
                .claim_phase(&manifest_bytes, &fixed_public_key()?, O2_KEY_ID, Utc::now())
                .map_err(|error| error.to_string())?;
            store
                .publish_o2_active_manifest_selector(&sha256_hex(&manifest_bytes), Utc::now())
                .map_err(|error| error.to_string())?;
            match disposition {
                Stage8bP1fClaimDispositionV1::Claimed(receipt)
                | Stage8bP1fClaimDispositionV1::ContinuedExisting(receipt) => print_json(&receipt)?,
            }
        }
        [command] if command == "guardian-materialize-fixed" => guardian_materialize_fixed()?,
        [command] if command == "guardian-inspect" => {
            print_json(&open_store()?.inspect().map_err(|error| error.to_string())?)?;
        }
        [command] if command == "runner-fixed" => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?;
            let result = runtime
                .block_on(run_stage8b_p1f_o2_fixed_systemd_runner_v1())
                .map_err(|error| error.to_string())?;
            print_json(&result)?;
        }
        [command] if command == "cleanup-fixed" => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?;
            let result = runtime
                .block_on(run_stage8b_p1f_o2_fixed_cleanup_v1())
                .map_err(|error| error.to_string())?;
            print_json(&result)?;
        }
        [command] if command == "collect-unit-evidence" => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?;
            let evidence = runtime
                .block_on(collect_stage8b_p1f_o2_unit_evidence_v1())
                .map_err(|error| error.to_string())?;
            print_json(&evidence)?;
        }
        [command] if command == "collect-evidence" => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?;
            let evidence = runtime
                .block_on(collect_stage8b_p1f_o2_readonly_evidence_v1())
                .map_err(|error| error.to_string())?;
            print_json(&evidence)?;
        }
        _ => return Err(usage()),
    }
    Ok(())
}

fn usage() -> String {
    "usage: stage8b-p1f-o2-operator <public-key|sign-genesis|sign-activation|sign-phase|prepare-control-root|guardian-init-fixed|guardian-activate-fixed|guardian-claim-fixed|guardian-materialize-fixed|guardian-inspect|runner-fixed|cleanup-fixed|collect-unit-evidence|collect-evidence> ...".into()
}

fn guardian_materialize_fixed() -> Result<(), String> {
    let manifest = stage8b_p1f_o2_active_manifest_sha256_v1().map_err(|error| error.to_string())?;
    let package_path = Path::new(O2_STAGING_ROOT).join(format!("{manifest}.json"));
    let package_bytes = read_root_owned(&package_path, O2_MAX_STAGED_BYTES, 0o400)?;
    let policy = read_root_owned(
        Path::new(O2_POLICY_PATH),
        STAGE8B_P1F_MAX_AUTHORITY_BYTES,
        0o440,
    )?;
    let template = read_root_owned(
        Path::new(O2_CONFIG_TEMPLATE_PATH),
        STAGE8B_P1F_MAX_AUTHORITY_BYTES,
        0o440,
    )?;
    let source_template = read_root_owned(
        Path::new(O2_SOURCE_TEMPLATE_PATH),
        runtime_durable_service::STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES,
        0o440,
    )?;
    let checked = runtime_durable_service::check_stage8b_p1f_staged_source(
        &package_bytes,
        &manifest,
        &policy,
        &template,
        &source_template,
        Utc::now(),
    )?;
    let receipt = open_store()?
        .materialize_o2(
            &manifest,
            &policy,
            &template,
            checked.source_bytes(),
            checked.broker_truth_checked_at_utc(),
            Utc::now(),
        )
        .map_err(|error| error.to_string())?;
    print_json(&receipt)
}

fn open_store() -> Result<Stage8bP1fAuthorityStoreV1, String> {
    require_root()?;
    Stage8bP1fAuthorityStoreV1::open_production(service_group_gid()?)
        .map_err(|error| error.to_string())
}

fn prepare_control_root() -> Result<(), String> {
    require_root()?;
    let path = Path::new(STAGE8B_P1F_AUTHORITY_CONTROL_ROOT);
    let gid = service_group_gid()?;
    if !path.exists() {
        fs::create_dir(path).map_err(|error| error.to_string())?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o750))
            .map_err(|error| error.to_string())?;
        let path_c =
            CString::new(path.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
        if unsafe { libc::chown(path_c.as_ptr(), 0, gid) } != 0 {
            return Err("control-root chown failed".into());
        }
    }
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != 0
        || metadata.gid() != gid
        || metadata.permissions().mode() & 0o777 != 0o750
        || fs::read_dir(path)
            .map_err(|error| error.to_string())?
            .next()
            .is_some()
    {
        return Err("control-root skeleton is not exact and empty".into());
    }
    Ok(())
}

fn read_signing_key(path: &Path) -> Result<SigningKey, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err("offline signing-key custody is invalid".into());
    }
    let bytes = Zeroizing::new(read_bounded(path, 65)?);
    let mut seed = Zeroizing::new([0_u8; 32]);
    if bytes.len() == 32 {
        seed.copy_from_slice(&bytes);
    } else {
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| "signing key must be raw 32 bytes or lowercase hex".to_string())?;
        let text = text.strip_suffix('\n').unwrap_or(text);
        if text.len() != 64 {
            return Err("signing key must be raw 32 bytes or lowercase hex".into());
        }
        for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
            seed[index] = decode_nibble(pair[0])? * 16 + decode_nibble(pair[1])?;
        }
    }
    Ok(SigningKey::from_bytes(&seed))
}

fn decode_nibble(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err("signing key hex must be lowercase".into()),
    }
}

fn fixed_public_key() -> Result<String, String> {
    let bytes = read_root_owned(Path::new(O2_PUBLIC_KEY_PATH), 65, 0o440)?;
    parse_public_key(&bytes)
}

fn parse_public_key(bytes: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "public key is not UTF-8".to_string())?;
    let value = text.strip_suffix('\n').unwrap_or(text);
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("public key is not exact lowercase Ed25519 hex".into());
    }
    Ok(value.to_string())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&read_bounded(path, STAGE8B_P1F_MAX_AUTHORITY_BYTES)?)
        .map_err(|error| error.to_string())
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > maximum {
        return Err("input file boundary is invalid".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    (&mut file)
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > maximum || bytes.len() as u64 != metadata.len() {
        return Err("input file changed or exceeded its bound".into());
    }
    Ok(bytes)
}

fn read_root_owned(path: &Path, maximum: u64, expected_mode: u32) -> Result<Vec<u8>, String> {
    let before = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !before.is_file()
        || before.file_type().is_symlink()
        || before.nlink() != 1
        || before.uid() != 0
        || before.len() == 0
        || before.len() > maximum
        || before.permissions().mode() & 0o777 != expected_mode
    {
        return Err("fixed O2 input custody is invalid".into());
    }
    let bytes = read_bounded(path, maximum)?;
    let after = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err("fixed O2 input changed during read".into());
    }
    Ok(bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn write_create_new(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string(value).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn require_root() -> Result<(), String> {
    if unsafe { libc::geteuid() } == 0 {
        Ok(())
    } else {
        Err("root is required".into())
    }
}

fn service_group_gid() -> Result<u32, String> {
    let name = CString::new(STAGE8B_P1F_SERVICE_USER).map_err(|error| error.to_string())?;
    let group = unsafe { libc::getgrnam(name.as_ptr()) };
    if group.is_null() {
        return Err("moex-p1-paper group is missing".into());
    }
    Ok(unsafe { (*group).gr_gid })
}
