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

#[path = "stage8b-p1f-o2-materializer/observed.rs"]
mod observed;
#[cfg(test)]
#[path = "stage8b-p1f-o2-materializer/observed_tests.rs"]
mod observed_tests;

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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterializationPolicyV1 {
    schema_version: u16,
    domain: String,
    account_id_sha256: String,
    account_alias: String,
    venue_symbol: String,
    bars_start_utc: String,
    bars_end_utc: String,
    // Absent for the immutable V1 policy. V2 binds the reviewed short profile
    // explicitly; a present null is not an absent legacy field.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_profile_field"
    )]
    runtime_profile_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_profile_field"
    )]
    runtime_profile_sha256: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_profile_field"
    )]
    market_data_policy_sha256: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_profile_field"
    )]
    operational_identity_sha256: Option<String>,
}

fn present_profile_field<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "observed::present_input"
    )]
    observed_input: Option<observed::RetainedObservedInput>,
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
    validate_template_policy(&template_bytes, &policy)?;
    let manifest_sha256 = runtime_durable_service::stage8b_p1f_o2_active_manifest_sha256_v1()
        .map_err(|error| error.to_string())?;
    let collection_lock =
        runtime_durable_service::lock_stage8b_p1f_o2_collection_v1(&manifest_sha256)
            .map_err(|error| error.to_string())?;
    let output_path = staging_path(&manifest_sha256)?;
    if output_path.exists() {
        return validate_existing(
            &output_path,
            &manifest_sha256,
            &policy,
            &account_id,
            &template_bytes,
        );
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
    let observed_plan = if policy.schema_version == 3 {
        Some(observed::plan(&policy, &template_bytes, trusted_now)?)
    } else {
        None
    };
    let (materialized, observed_input) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?
        .block_on(async {
            if let Some(plan) = observed_plan {
                let collected = plan
                    .collect_first_boot_source(&account_id, &token, &template_bytes)
                    .await?;
                Ok((
                    collected.source,
                    Some(observed::RetainedObservedInput {
                        template_json: String::from_utf8(template_bytes.clone()).map_err(|_| {
                            finam_gateway::Stage8bP1fO2MaterializerErrorV1::Template
                        })?,
                        snapshot: collected.snapshot,
                    }),
                ))
            } else {
                collect_stage8b_p1f_o2_source_v1(
                    &account_id,
                    &token,
                    &template_bytes,
                    &policy.bars_start_utc,
                    &policy.bars_end_utc,
                    trusted_now,
                )
                .await
                .map(|source| (source, None))
            }
        })
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
        schema_version: if observed_input.is_some() { 2 } else { 1 },
        domain: if observed_input.is_some() {
            "stage8b-p1f-o2-staged-materialization-v2"
        } else {
            "stage8b-p1f-o2-staged-materialization-v1"
        }
        .into(),
        manifest_sha256: manifest_sha256.clone(),
        source_bundle_sha256: source_bundle_sha256.clone(),
        exact_source_json,
        evidence: materialized.evidence,
        observed_input,
    };
    let staged_bytes = serde_json::to_vec(&staged).map_err(|error| error.to_string())?;
    if staged_bytes.len() as u64 > MAX_STAGED_BYTES {
        return Err("staged package exceeds retained input limit".into());
    }
    // Validate the same replay contract before publishing the first package.
    validate_retained_bytes(
        &staged_bytes,
        &manifest_sha256,
        &policy,
        &account_id,
        Utc::now(),
    )?;
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
    template_bytes: &[u8],
) -> Result<MaterializerResultV1, String> {
    let bytes = read_protected(path, MAX_STAGED_BYTES, false)?;
    if policy.schema_version == 3 {
        let staged: StagedMaterializationV1 =
            serde_json::from_slice(&bytes).map_err(|_| "invalid retained staged package")?;
        if staged
            .observed_input
            .as_ref()
            .map(|input| input.template_json.as_bytes())
            != Some(template_bytes)
        {
            return Err("retained calendar template differs from protected input".into());
        }
    }
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
    if bytes.is_empty() || bytes.len() as u64 > MAX_STAGED_BYTES {
        return Err("invalid retained staged package size".into());
    }
    let staged: StagedMaterializationV1 =
        serde_json::from_slice(bytes).map_err(|_| "invalid retained staged package")?;
    let observed = policy.schema_version == 3;
    if staged.schema_version != if observed { 2 } else { 1 }
        || staged.domain
            != if observed {
                "stage8b-p1f-o2-staged-materialization-v2"
            } else {
                "stage8b-p1f-o2-staged-materialization-v1"
            }
        || staged.observed_input.is_some() != observed
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
    validate_source_profile_pairing(&source, policy)?;
    let identity = source
        .get("operational_identity_sha256")
        .and_then(serde_json::Value::as_str)
        .ok_or("retained source identity is missing")?;
    if observed {
        observed::validate_retained(&staged, policy, account_id, trusted_now)?;
    } else {
        runtime_durable_service::validate_stage8b_p1e_first_boot_source_bytes_v1(
            staged.exact_source_json.as_bytes(),
            identity,
            &policy.account_alias,
            trusted_now,
        )
        .map_err(|_| "retained source no longer satisfies fresh admission")?;
    }
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
    let profile = policy_profile(policy)?;
    observed::validate_policy_binding(policy)?;
    if policy.venue_symbol != broker_finam::STAGE8B_P1F_O2_VENUE_SYMBOL
        || policy.account_alias != finam_gateway::STAGE8B_P1F_O2_ACCOUNT_ALIAS
        || !valid_sha256(&policy.account_id_sha256)
    {
        return Err("materialization policy identity is invalid".into());
    }
    let start = canonical_time(&policy.bars_start_utc)?;
    let end = canonical_time(&policy.bars_end_utc)?;
    let range = end.signed_duration_since(start);
    let valid_range = if policy.schema_version == 3 {
        range > chrono::Duration::zero()
            && range <= chrono::Duration::days(7)
            && start.timestamp() % 60 == 0
            && end.timestamp() % 60 == 0
    } else if profile.no_riskgate() {
        finam_gateway::Stage8bP1fO2MaterializedSourceV1::validate_bars_interval(profile, start, end)
            .is_ok()
    } else {
        range >= chrono::Duration::days(MIN_BARS_RANGE_DAYS)
            && range <= chrono::Duration::days(MAX_BARS_RANGE_DAYS)
    };
    if !valid_range {
        return Err("materialization policy bars interval is insufficient".into());
    }
    Ok(())
}

fn policy_profile(
    policy: &MaterializationPolicyV1,
) -> Result<runtime_durable_service::Stage8bP1RuntimeProfileKind, String> {
    use runtime_durable_service::Stage8bP1RuntimeProfileKind;
    match (policy.schema_version, policy.domain.as_str()) {
        (1, "stage8b-p1f-o2-materialization-policy-v1")
            if policy.runtime_profile_id.is_none() && policy.runtime_profile_sha256.is_none() =>
        {
            Stage8bP1RuntimeProfileKind::from_sha256(
                runtime_durable_service::STAGE8B_P1E_RUNTIME_PROFILE_SHA256,
            )
            .map_err(|_| "legacy policy profile is invalid".into())
        }
        (2, "stage8b-p1f-o2-materialization-policy-v2")
        | (3, "stage8b-p1f-o2-materialization-policy-v3") => {
            let profile = Stage8bP1RuntimeProfileKind::from_identity(
                policy
                    .runtime_profile_id
                    .as_deref()
                    .ok_or("policy profile id is missing")?,
                policy
                    .runtime_profile_sha256
                    .as_deref()
                    .ok_or("policy profile hash is missing")?,
            )
            .map_err(|_| "policy profile identity is invalid")?;
            if !profile.no_riskgate() {
                return Err("short policy requires the no-riskgate profile".into());
            }
            Ok(profile)
        }
        _ => Err("materialization policy identity is invalid".into()),
    }
}

fn validate_source_profile_pairing(
    source: &serde_json::Value,
    policy: &MaterializationPolicyV1,
) -> Result<(), String> {
    let profile = policy_profile(policy)?;
    let (schema, domain) = if policy.schema_version == 3 {
        (
            4,
            runtime_durable_service::STAGE8B_P1E_FIRST_BOOT_SOURCE_V4_DOMAIN,
        )
    } else if profile.no_riskgate() {
        (3, "moex.stage8b.p1e.first-boot-source-bundle.v3")
    } else {
        (2, "moex.stage8b.p1e.first-boot-source-bundle.v2")
    };
    if source["schema_version"].as_u64() != Some(schema)
        || source["domain"].as_str() != Some(domain)
        || source["runtime_profile_sha256"].as_str() != Some(profile.profile_sha256())
    {
        return Err("source schema/profile does not match materialization policy".into());
    }
    Ok(())
}

fn validate_template_policy(
    template_bytes: &[u8],
    policy: &MaterializationPolicyV1,
) -> Result<(), String> {
    validate_policy(policy)?;
    finam_gateway::Stage8bP1fO2MaterializedSourceV1::validate_template_profile(template_bytes)
        .map_err(|_| "source template preflight rejected")?;
    let source = serde_json::from_slice(template_bytes).map_err(|_| "invalid source template")?;
    if policy.schema_version == 3 {
        observed::validate_template(&source, policy)
    } else {
        validate_source_profile_pairing(&source, policy)
    }
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

    pub(super) const ACCOUNT: &str = "ACC_TEST_0001";

    #[test]
    fn documented_no_riskgate_examples_pass_read_only_preflight_at_frozen_time() {
        let template_bytes = include_bytes!(
            "../../../../docs/stage-8/stage8b-p1f-o2-no-riskgate-source-template.example.json"
        );
        let policy_bytes = include_bytes!(
            "../../../../docs/stage-8/stage8b-p1f-o2-no-riskgate-policy.example.json"
        );
        let policy: MaterializationPolicyV1 = serde_json::from_slice(policy_bytes).unwrap();
        let source: serde_json::Value = serde_json::from_slice(template_bytes).unwrap();
        let frozen_now = canonical_time("2026-09-28T11:11:00Z").unwrap();

        // Validate the exact documented bytes, not a repaired/rebound fixture.
        // This is format/profile preflight only: no credentials, GETs, source
        // materialization, active authority lookup or deployment admission.
        validate_template_policy(template_bytes, &policy).unwrap();
        assert!(policy_profile(&policy).unwrap().no_riskgate());
        assert!(canonical_time(&policy.bars_start_utc).unwrap() <= frozen_now);
        assert!(frozen_now < canonical_time(&policy.bars_end_utc).unwrap());
        assert_eq!(
            source["history_coverage"]["candidate_session"]["session_date"],
            (frozen_now + chrono::Duration::hours(3))
                .date_naive()
                .format("%Y-%m-%d")
                .to_string()
        );
        assert_eq!(
            source["history_coverage"]["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|session| session["session_date"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["2026-09-22", "2026-09-23", "2026-09-24", "2026-09-25"]
        );
        // Examples deliberately retain non-operational identity placeholders.
        assert_eq!(source["operational_identity_sha256"], "1".repeat(64));
        assert_eq!(policy.account_id_sha256, "2".repeat(64));
        assert_eq!(source["captured_at_utc"], "1970-01-01T00:00:00Z");
        assert_eq!(
            source["broker_truth"]["account_id"],
            finam_gateway::STAGE8B_P1F_O2_ACCOUNT_TEMPLATE_SENTINEL
        );
    }

    pub(super) fn short_policy() -> MaterializationPolicyV1 {
        let profile = runtime_durable_service::Stage8bP1RuntimeProfileKind::V2;
        MaterializationPolicyV1 {
            schema_version: 2,
            domain: "stage8b-p1f-o2-materialization-policy-v2".into(),
            account_id_sha256: sha256_hex(ACCOUNT.as_bytes()),
            account_alias: finam_gateway::STAGE8B_P1F_O2_ACCOUNT_ALIAS.into(),
            venue_symbol: broker_finam::STAGE8B_P1F_O2_VENUE_SYMBOL.into(),
            bars_start_utc: "2026-09-22T00:00:00Z".into(),
            bars_end_utc: "2026-09-28T07:00:00Z".into(),
            runtime_profile_id: Some(profile.profile_id().into()),
            runtime_profile_sha256: Some(profile.profile_sha256().into()),
            market_data_policy_sha256: None,
            operational_identity_sha256: None,
        }
    }

    pub(super) fn short_template() -> serde_json::Value {
        let mut template: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../docs/stage-8/stage8b-p1f-o2-source-template.json"
        ))
        .unwrap();
        template["schema_version"] = json!(3);
        template["domain"] = json!("moex.stage8b.p1e.first-boot-source-bundle.v3");
        template["runtime_profile_sha256"] =
            json!(runtime_durable_service::Stage8bP1RuntimeProfileKind::V2.profile_sha256());
        let session = |day: u32| {
            let date = chrono::NaiveDate::from_ymd_opt(2026, 9, day).unwrap();
            json!({
                "session_date": format!("2026-09-{day:02}"),
                "windows": [{
                    "first_close_time_utc": date.and_hms_opt(4, 10, 0).unwrap().and_utc().timestamp(),
                    "last_close_time_utc": date.and_hms_opt(20, 50, 0).unwrap().and_utc().timestamp()
                }]
            })
        };
        template["history_coverage"]["sessions"] =
            json!([session(22), session(23), session(24), session(25)]);
        template["history_coverage"]["candidate_session"] = session(28);
        template
    }

    #[test]
    fn short_policy_accepts_age_14_session_with_full_intraday_tail() {
        let mut template = short_template();
        let earliest = &mut template["history_coverage"]["sessions"][0];
        earliest["session_date"] = json!("2026-09-14");
        for window in earliest["windows"].as_array_mut().unwrap() {
            for field in ["first_close_time_utc", "last_close_time_utc"] {
                window[field] = json!(window[field].as_i64().unwrap() - 8 * 86400);
            }
        }
        let bytes = serde_json::to_vec(&template).unwrap();
        let mut policy = short_policy();
        // Include the first M1 candle of the 07:10 MSK M10 close and the full
        // candidate day. Age 14 calendar days is not a 14*24-hour fetch span.
        policy.bars_start_utc = "2026-09-14T04:00:00Z".into();
        policy.bars_end_utc = "2026-09-28T20:50:00Z".into();
        assert!(
            finam_gateway::Stage8bP1fO2MaterializedSourceV1::validate_template_profile(&bytes)
                .is_ok()
        );
        validate_template_policy(&bytes, &policy).unwrap();
    }

    #[test]
    fn policy_ranges_are_versioned_without_relaxing_legacy_limits() {
        let mut legacy: MaterializationPolicyV1 = serde_json::from_slice(include_bytes!(
            "../../../../docs/stage-8/stage8b-p1f-o2-materialization-policy.json"
        ))
        .unwrap();
        let start = canonical_time(&legacy.bars_start_utc).unwrap();
        for (days, valid) in [
            (0, false),
            (14, false),
            (179, false),
            (180, true),
            (400, true),
            (401, false),
        ] {
            legacy.bars_end_utc =
                (start + chrono::Duration::days(days)).to_rfc3339_opts(SecondsFormat::Secs, true);
            assert_eq!(validate_policy(&legacy).is_ok(), valid, "legacy {days}");
        }
        let mut policy = short_policy();
        let start = canonical_time(&policy.bars_start_utc).unwrap();
        for (seconds, valid) in [
            (-1, false),
            (0, false),
            (1, true),
            (14 * 86400, true),
            (14 * 86400 + 1, true),
            (14 * 86400 + 16 * 3600 + 50 * 60, true),
            (15 * 86400, true),
            (15 * 86400 + 1, false),
            (180 * 86400, false),
        ] {
            policy.bars_end_utc = (start + chrono::Duration::seconds(seconds))
                .to_rfc3339_opts(SecondsFormat::Secs, true);
            assert_eq!(validate_policy(&policy).is_ok(), valid, "short {seconds}");
        }
    }

    #[test]
    fn policy_and_template_require_exact_schema_profile_pairing() {
        let legacy: MaterializationPolicyV1 = serde_json::from_slice(include_bytes!(
            "../../../../docs/stage-8/stage8b-p1f-o2-materialization-policy.json"
        ))
        .unwrap();
        let legacy_template =
            include_bytes!("../../../../docs/stage-8/stage8b-p1f-o2-source-template.json");
        let short_template = serde_json::to_vec(&short_template()).unwrap();
        let mut short = short_policy();
        assert!(validate_template_policy(legacy_template, &legacy).is_ok());
        assert!(validate_template_policy(&short_template, &short).is_ok());
        assert!(validate_template_policy(&short_template, &legacy).is_err());
        assert!(validate_template_policy(legacy_template, &short).is_err());
        short.runtime_profile_id = Some("foreign-profile".into());
        assert!(validate_policy(&short).is_err());
        short = short_policy();
        short.runtime_profile_sha256 = Some(
            runtime_durable_service::Stage8bP1RuntimeProfileKind::V1
                .profile_sha256()
                .into(),
        );
        assert!(validate_policy(&short).is_err());
        short.runtime_profile_id = Some(
            runtime_durable_service::Stage8bP1RuntimeProfileKind::V1
                .profile_id()
                .into(),
        );
        assert!(validate_policy(&short).is_err());
        short = short_policy();
        short.runtime_profile_sha256 = Some("ff".repeat(32));
        assert!(validate_policy(&short).is_err());
        short = short_policy();
        short.runtime_profile_id = None;
        assert!(validate_policy(&short).is_err());
        short = short_policy();
        short.schema_version = 1;
        assert!(validate_policy(&short).is_err());
    }

    #[test]
    fn legacy_policy_does_not_gain_optional_profile_fields() {
        let bytes =
            include_bytes!("../../../../docs/stage-8/stage8b-p1f-o2-materialization-policy.json");
        let mut value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        value["runtime_profile_id"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<MaterializationPolicyV1>(value.clone()).is_err());
        value["runtime_profile_id"] =
            json!(runtime_durable_service::Stage8bP1RuntimeProfileKind::V1.profile_id());
        assert!(validate_policy(&serde_json::from_value(value).unwrap()).is_err());
    }

    #[test]
    fn short_policy_cannot_retain_a_legacy_source() {
        let (staged, _, now) = fixture();
        let policy = short_policy();
        let error = validate_retained_bytes(
            &serde_json::to_vec(&staged).unwrap(),
            &staged.manifest_sha256,
            &policy,
            ACCOUNT,
            now,
        )
        .err()
        .unwrap();
        assert_eq!(
            error,
            "source schema/profile does not match materialization policy"
        );
    }

    pub(super) fn fixture() -> (
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
            observed_input: None,
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
            runtime_profile_id: None,
            runtime_profile_sha256: None,
            market_data_policy_sha256: None,
            operational_identity_sha256: None,
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
