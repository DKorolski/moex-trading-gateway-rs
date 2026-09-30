use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use broker_finam::AccessToken;
use chrono::{DateTime, SecondsFormat, Utc};
use finam_gateway::{collect_stage8b_p1f_o2_source_v1, Stage8bP1fO2MaterializationEvidenceV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const POLICY_PATH: &str = "/etc/moex-finam-p1-paper/o2/materialization-policy.json";
const SOURCE_TEMPLATE_PATH: &str = "/etc/moex-finam-p1-paper/o2/source-template.json";
const TOKEN_PATH: &str =
    "/run/credentials/moex-finam-p1f-o2-materializer.service/finam-readonly.token";
const ACCOUNT_PATH: &str =
    "/run/credentials/moex-finam-p1f-o2-materializer.service/finam-account.id";
const STAGING_ROOT: &str = "/var/lib/moex-finam-p1-paper-o2-staging";
const MAX_POLICY_BYTES: u64 = 64 * 1024;
const MAX_TOKEN_BYTES: u64 = 16 * 1024;
const MAX_STAGED_BYTES: u64 = 32 * 1024 * 1024;
const MIN_BARS_RANGE_DAYS: i64 = 180;
const MAX_BARS_RANGE_DAYS: i64 = 400;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterializationPolicyV1 {
    schema_version: u16,
    domain: String,
    account_id_sha256: String,
    account_alias: String,
    venue_symbol: String,
    bars_start_utc: String,
    bars_end_utc: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StagedMaterializationV1 {
    schema_version: u16,
    domain: String,
    manifest_sha256: String,
    source_bundle_sha256: String,
    exact_source_json: String,
    evidence: Stage8bP1fO2MaterializationEvidenceV1,
}

#[derive(Debug, Serialize)]
struct MaterializerResultV1 {
    schema_version: u16,
    domain: &'static str,
    manifest_sha256: String,
    source_bundle_sha256: String,
    staged_package_sha256: String,
    disposition: &'static str,
}

fn main() -> ExitCode {
    match run() {
        Ok(result) => match serde_json::to_string(&result) {
            Ok(output) => {
                println!("{output}");
                ExitCode::SUCCESS
            }
            Err(_) => ExitCode::from(70),
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(70)
        }
    }
}

fn run() -> Result<MaterializerResultV1, String> {
    require_root()?;
    if std::env::args_os().len() != 1 {
        return Err("this fixed-path materializer accepts no arguments".into());
    }
    let policy_bytes = read_protected(Path::new(POLICY_PATH), MAX_POLICY_BYTES, false)?;
    let policy: MaterializationPolicyV1 =
        serde_json::from_slice(&policy_bytes).map_err(|_| "invalid materialization policy")?;
    validate_policy(&policy)?;
    let account_bytes = Zeroizing::new(read_protected(
        Path::new(ACCOUNT_PATH),
        MAX_TOKEN_BYTES,
        true,
    )?);
    let account_id = Zeroizing::new(read_single_line_secret(&account_bytes, "FINAM account id")?);
    if sha256_hex(account_id.as_bytes()) != policy.account_id_sha256
        || broker_finam::Stage8bP1fO2GetOnlyClientV1::new(account_id.as_str()).is_err()
    {
        return Err("FINAM account credential does not match policy".into());
    }
    let template_bytes = read_protected(
        Path::new(SOURCE_TEMPLATE_PATH),
        runtime_durable_service::STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES,
        false,
    )?;
    let manifest_sha256 = runtime_durable_service::stage8b_p1f_o2_active_manifest_sha256_v1()
        .map_err(|error| error.to_string())?;
    let collection_lock =
        runtime_durable_service::lock_stage8b_p1f_o2_collection_v1(&manifest_sha256)
            .map_err(|error| error.to_string())?;
    let output_path = staging_path(&manifest_sha256)?;
    if output_path.exists() {
        return validate_existing(&output_path, &manifest_sha256, &policy, &account_id);
    }
    validate_staging_root()?;
    let token_bytes = Zeroizing::new(read_protected(
        Path::new(TOKEN_PATH),
        MAX_TOKEN_BYTES,
        true,
    )?);
    let token_text = read_single_line_secret(&token_bytes, "FINAM token")?;
    let token = AccessToken::new(token_text.to_string());
    let trusted_now = Utc::now();
    let materialized = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?
        .block_on(collect_stage8b_p1f_o2_source_v1(
            &account_id,
            &token,
            &template_bytes,
            &policy.bars_start_utc,
            &policy.bars_end_utc,
            trusted_now,
        ))
        .map_err(|error| {
            error.diagnostic_json(
                &manifest_sha256,
                &sha256_hex(&policy_bytes),
                &sha256_hex(&template_bytes),
                &policy.bars_start_utc,
                &policy.bars_end_utc,
                trusted_now,
            )
        })?;
    let source_bundle_sha256 = sha256_hex(&materialized.exact_source_bytes);
    let exact_source_json = String::from_utf8(materialized.exact_source_bytes)
        .map_err(|_| "materialized source is not UTF-8 JSON")?;
    let staged = StagedMaterializationV1 {
        schema_version: 1,
        domain: "stage8b-p1f-o2-staged-materialization-v1".into(),
        manifest_sha256: manifest_sha256.clone(),
        source_bundle_sha256: source_bundle_sha256.clone(),
        exact_source_json,
        evidence: materialized.evidence,
    };
    let staged_bytes = serde_json::to_vec(&staged).map_err(|error| error.to_string())?;
    collection_lock
        .validate_before_publication()
        .map_err(|error| error.to_string())?;
    write_create_new(&output_path, &staged_bytes)?;
    Ok(MaterializerResultV1 {
        schema_version: 1,
        domain: "stage8b-p1f-o2-materializer-result-v1",
        manifest_sha256,
        source_bundle_sha256,
        staged_package_sha256: sha256_hex(&staged_bytes),
        disposition: "CREATED",
    })
}

fn validate_existing(
    path: &Path,
    manifest_sha256: &str,
    policy: &MaterializationPolicyV1,
    account_id: &str,
) -> Result<MaterializerResultV1, String> {
    let bytes = read_protected(path, MAX_STAGED_BYTES, false)?;
    validate_retained_bytes(&bytes, manifest_sha256, policy, account_id, Utc::now())
}

fn validate_retained_bytes(
    bytes: &[u8],
    manifest_sha256: &str,
    policy: &MaterializationPolicyV1,
    account_id: &str,
    trusted_now: DateTime<Utc>,
) -> Result<MaterializerResultV1, String> {
    validate_policy(policy)?;
    let staged: StagedMaterializationV1 =
        serde_json::from_slice(bytes).map_err(|_| "invalid retained staged package")?;
    if staged.schema_version != 1
        || staged.domain != "stage8b-p1f-o2-staged-materialization-v1"
        || staged.manifest_sha256 != manifest_sha256
        || staged.source_bundle_sha256 != sha256_hex(staged.exact_source_json.as_bytes())
        || staged.evidence.source_bundle_sha256 != staged.source_bundle_sha256
        || staged.evidence.account_id_sha256 != policy.account_id_sha256
        || staged.evidence.account_id_sha256 != sha256_hex(account_id.as_bytes())
    {
        return Err("retained staged package conflicts with active authority".into());
    }
    let source: serde_json::Value = serde_json::from_str(&staged.exact_source_json)
        .map_err(|_| "retained source is invalid JSON")?;
    let identity = source
        .get("operational_identity_sha256")
        .and_then(serde_json::Value::as_str)
        .ok_or("retained source identity is missing")?;
    runtime_durable_service::validate_stage8b_p1e_first_boot_source_bytes_v1(
        staged.exact_source_json.as_bytes(),
        identity,
        &policy.account_alias,
        trusted_now,
    )
    .map_err(|_| "retained source no longer satisfies fresh admission")?;
    Ok(MaterializerResultV1 {
        schema_version: 1,
        domain: "stage8b-p1f-o2-materializer-result-v1",
        manifest_sha256: manifest_sha256.to_string(),
        source_bundle_sha256: staged.source_bundle_sha256,
        staged_package_sha256: sha256_hex(bytes),
        disposition: "EXACT_RETAINED_REPLAY",
    })
}

fn validate_policy(policy: &MaterializationPolicyV1) -> Result<(), String> {
    if policy.schema_version != 1
        || policy.domain != "stage8b-p1f-o2-materialization-policy-v1"
        || policy.venue_symbol != broker_finam::STAGE8B_P1F_O2_VENUE_SYMBOL
        || policy.account_alias != finam_gateway::STAGE8B_P1F_O2_ACCOUNT_ALIAS
        || !valid_sha256(&policy.account_id_sha256)
    {
        return Err("materialization policy identity is invalid".into());
    }
    let start = canonical_time(&policy.bars_start_utc)?;
    let end = canonical_time(&policy.bars_end_utc)?;
    let range = end.signed_duration_since(start);
    if start >= end
        || range < chrono::Duration::days(MIN_BARS_RANGE_DAYS)
        || range > chrono::Duration::days(MAX_BARS_RANGE_DAYS)
    {
        return Err("materialization policy bars interval is insufficient".into());
    }
    Ok(())
}

fn read_single_line_secret(bytes: &[u8], label: &str) -> Result<String, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| format!("{label} is not UTF-8"))?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.is_empty() || text.len() > 8192 || text.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err(format!("{label} boundary is invalid"));
    }
    Ok(text.to_string())
}

fn canonical_time(value: &str) -> Result<DateTime<Utc>, String> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| "policy timestamp is invalid")?
        .with_timezone(&Utc);
    if parsed.to_rfc3339_opts(SecondsFormat::Secs, true) != value {
        return Err("policy timestamp is not canonical UTC seconds".into());
    }
    Ok(parsed)
}

fn staging_path(manifest_sha256: &str) -> Result<PathBuf, String> {
    if !valid_sha256(manifest_sha256) {
        return Err("active manifest identity is invalid".into());
    }
    Ok(Path::new(STAGING_ROOT).join(format!("{manifest_sha256}.json")))
}

fn validate_staging_root() -> Result<(), String> {
    let metadata = fs::symlink_metadata(STAGING_ROOT).map_err(|error| error.to_string())?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != 0
        || metadata.gid() != 0
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err("O2 staging root custody is invalid".into());
    }
    Ok(())
}

fn read_protected(path: &Path, maximum: u64, exact_credential: bool) -> Result<Vec<u8>, String> {
    let before = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    let mode = before.permissions().mode() & 0o777;
    if !before.is_file()
        || before.file_type().is_symlink()
        || before.nlink() != 1
        || before.uid() != 0
        || before.len() == 0
        || before.len() > maximum
        || (exact_credential && mode != 0o400)
        || (!exact_credential && mode & 0o022 != 0)
    {
        return Err("protected input custody is invalid".into());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| error.to_string())?;
    let opened = file.metadata().map_err(|error| error.to_string())?;
    if !same_file(&before, &opened) {
        return Err("protected input changed before read".into());
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    (&mut file)
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let after = file.metadata().map_err(|error| error.to_string())?;
    if bytes.len() as u64 != opened.len() || !same_file(&opened, &after) {
        return Err("protected input changed during read".into());
    }
    Ok(bytes)
}

fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.len() == right.len()
        && left.uid() == right.uid()
        && left.gid() == right.gid()
        && left.nlink() == right.nlink()
        && left.permissions().mode() & 0o777 == right.permissions().mode() & 0o777
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

fn write_create_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    validate_staging_root()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o400)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(STAGING_ROOT)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

fn require_root() -> Result<(), String> {
    if unsafe { libc::geteuid() } == 0 {
        Ok(())
    } else {
        Err("root is required".into())
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ACCOUNT: &str = "ACC_TEST_0001";

    fn fixture() -> (
        StagedMaterializationV1,
        MaterializationPolicyV1,
        DateTime<Utc>,
    ) {
        let fixture = runtime_durable_service::Stage8bP1fIeLinkedFixtureV1::materialize().unwrap();
        let mut source: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.source_path()).unwrap()).unwrap();
        source["broker_truth"]["account_id"] = json!(finam_gateway::STAGE8B_P1F_O2_ACCOUNT_ALIAS);
        let now = canonical_time(source["captured_at_utc"].as_str().unwrap()).unwrap();
        let source_json = serde_json::to_string(&source).unwrap();
        let source_hash = sha256_hex(source_json.as_bytes());
        let staged = StagedMaterializationV1 {
            schema_version: 1,
            domain: "stage8b-p1f-o2-staged-materialization-v1".into(),
            manifest_sha256: "a".repeat(64),
            source_bundle_sha256: source_hash.clone(),
            exact_source_json: source_json,
            evidence: serde_json::from_value(json!({
                "schema_version": 1,
                "domain": "stage8b-p1f-o2-materialization-evidence-v1",
                "captured_at_utc": source["captured_at_utc"],
                "account_id_sha256": sha256_hex(ACCOUNT.as_bytes()),
                "venue_symbol": broker_finam::STAGE8B_P1F_O2_VENUE_SYMBOL,
                "target_position_qty": "0", "target_active_orders_count": 0,
                "account_active_orders_count": 0, "active_orders_complete": true,
                "selected_m1_count": 10,
                "candidate_redis_id": source["candidate"]["redis_id"],
                "candidate_semantic_id_sha256": source["candidate"]["semantic_id_sha256"],
                "source_bundle_sha256": source_hash, "route_evidence": []
            }))
            .unwrap(),
        };
        let policy = MaterializationPolicyV1 {
            schema_version: 1,
            domain: "stage8b-p1f-o2-materialization-policy-v1".into(),
            account_id_sha256: sha256_hex(ACCOUNT.as_bytes()),
            account_alias: finam_gateway::STAGE8B_P1F_O2_ACCOUNT_ALIAS.into(),
            venue_symbol: broker_finam::STAGE8B_P1F_O2_VENUE_SYMBOL.into(),
            bars_start_utc: "2026-01-01T00:00:00Z".into(),
            bars_end_utc: "2026-07-15T00:00:00Z".into(),
        };
        (staged, policy, now)
    }

    #[test]
    fn retained_replay_uses_alias_and_preserves_exact_package_bytes() {
        let (staged, policy, now) = fixture();
        let bytes = serde_json::to_vec(&staged).unwrap();
        for _ in 0..2 {
            let result =
                validate_retained_bytes(&bytes, &staged.manifest_sha256, &policy, ACCOUNT, now)
                    .unwrap();
            assert_eq!(result.disposition, "EXACT_RETAINED_REPLAY");
            assert_eq!(result.staged_package_sha256, sha256_hex(&bytes));
            assert_eq!(result.source_bundle_sha256, staged.source_bundle_sha256);
        }
    }

    #[test]
    fn retained_replay_rejects_account_manifest_hash_and_alias_conflicts() {
        let (mut staged, mut policy, now) = fixture();
        let bytes = serde_json::to_vec(&staged).unwrap();
        assert!(validate_retained_bytes(
            &bytes,
            &staged.manifest_sha256,
            &policy,
            "OTHER_ACCOUNT",
            now
        )
        .is_err());
        assert!(validate_retained_bytes(&bytes, &"b".repeat(64), &policy, ACCOUNT, now).is_err());
        policy.account_alias = "foreign-alias".into();
        assert!(
            validate_retained_bytes(&bytes, &staged.manifest_sha256, &policy, ACCOUNT, now)
                .is_err()
        );
        policy.account_alias = finam_gateway::STAGE8B_P1F_O2_ACCOUNT_ALIAS.into();
        staged.source_bundle_sha256 = "f".repeat(64);
        assert!(validate_retained_bytes(
            &serde_json::to_vec(&staged).unwrap(),
            &staged.manifest_sha256,
            &policy,
            ACCOUNT,
            now
        )
        .is_err());
    }

    #[test]
    fn retained_replay_keeps_freshness_boundary() {
        let (staged, policy, _) = fixture();
        let source: serde_json::Value = serde_json::from_str(&staged.exact_source_json).unwrap();
        let now =
            canonical_time(source["broker_truth"]["checked_at_utc"].as_str().unwrap()).unwrap();
        let bytes = serde_json::to_vec(&staged).unwrap();
        assert!(validate_retained_bytes(
            &bytes,
            &staged.manifest_sha256,
            &policy,
            ACCOUNT,
            now + chrono::Duration::seconds(300)
        )
        .is_ok());
        assert!(validate_retained_bytes(
            &bytes,
            &staged.manifest_sha256,
            &policy,
            ACCOUNT,
            now + chrono::Duration::seconds(301)
        )
        .is_err());
    }

    #[test]
    fn retained_replay_rejects_raw_account_in_source_even_with_matching_hashes() {
        let (mut staged, policy, now) = fixture();
        let mut source: serde_json::Value =
            serde_json::from_str(&staged.exact_source_json).unwrap();
        source["broker_truth"]["account_id"] = json!(ACCOUNT);
        staged.exact_source_json = serde_json::to_string(&source).unwrap();
        staged.source_bundle_sha256 = sha256_hex(staged.exact_source_json.as_bytes());
        staged.evidence.source_bundle_sha256 = staged.source_bundle_sha256.clone();
        assert!(validate_retained_bytes(
            &serde_json::to_vec(&staged).unwrap(),
            &staged.manifest_sha256,
            &policy,
            ACCOUNT,
            now
        )
        .is_err());
    }
}
