use super::*;
use std::{
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

use broker_core::{BrokerTruthSnapshot, InstrumentMapEntry};
use broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strategy_runtime_core::stage8b_p1e_canonical_json;

const MAX_INPUT: u64 = 32 * 1024 * 1024;
const PHASE_KEY: &str = "/etc/moex-finam-p1-paper/o2/authority-public-key.hex";
const KEY_ID: &str = "stage8b-p1f-o2-authority-generation-1";

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// Exact O3 fixture authority, pinned by materialization_policy_sha256 in the
/// signed phase. This is synthetic input, NOT a FINAM observation. Absolute
/// timestamps are data chosen before issuance; no runtime clock is replaceable.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct O3CompositionPlan {
    pub schema_version: u16,
    pub domain: String,
    pub phase_id: String,
    pub operational_identity_sha256: String,
    pub runtime_config_fingerprint_sha256: String,
    pub instrument_map_fingerprint_sha256: String,
    pub schedule_fixture_sha256: String,
    pub schedule_semantic_sha256: String,
    pub schedule: O3ScheduleTemplate,
    pub decision: O3ObservedInput,
    pub successor: O3ObservedInput,
    pub catchup_coverage: strategy_runtime_core::stage8b_o3_readmission::Stage8bO3Coverage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct O3ScheduleTemplate {
    pub synthetic_truth: BrokerTruthSnapshot,
    pub exact_synthetic_schedule_json: String,
    pub registry_entry: InstrumentMapEntry,
    pub registry_version: String,
    pub registry_identity_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct O3ObservedInput {
    pub evidence: ClosedM1SnapshotEvidenceV1,
    pub snapshot_sha256: String,
    pub receipt_sha256: String,
    pub canonical_m10_sha256: String,
}

#[derive(Clone)]
pub(crate) struct AdmittedInputs {
    pub plan: O3CompositionPlan,
    pub durable_parent: std::path::PathBuf,
    pub supervisor_bytes: Vec<u8>,
    pub manifest_sha256: String,
    pub not_before: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
    pub shutdown_grace_ms: u64,
    pub paper_source: strategy_runtime_core::stage8b_o3_readmission::Stage8bO3PaperSource,
}

impl AdmittedInputs {
    pub fn load_fixed(expected_manifest: &str) -> Result<Self, O3Error> {
        let phase_bytes = read_root_file(Path::new(O3_PHASE_PATH), 64 * 1024)?;
        let key = read_root_file(Path::new(PHASE_KEY), 65)?;
        let key = std::str::from_utf8(&key).map_err(|_| O3Error::Input)?;
        let plan = read_root_file(
            &Path::new(O3_CONFIG_ROOT).join("composition.json"),
            MAX_INPUT,
        )?;
        let supervisor = read_root_file(
            Path::new(durable::STAGE8B_P1E_SUPERVISOR_CONFIG_PATH),
            128 * 1024,
        )?;
        let boot =
            fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(|_| O3Error::Io)?;
        let boot = uuid::Uuid::parse_str(boot.trim()).map_err(|_| O3Error::Input)?;
        Self::admit(
            &phase_bytes,
            expected_manifest,
            key.strip_suffix('\n').unwrap_or(key),
            &plan,
            &supervisor,
            Utc::now(),
            *boot.as_bytes(),
        )
    }

    fn admit(
        phase: &[u8],
        expected: &str,
        public: &str,
        plan_bytes: &[u8],
        supervisor: &[u8],
        now: DateTime<Utc>,
        boot_id: [u8; 16],
    ) -> Result<Self, O3Error> {
        if !is_hash(expected) || hash(phase) != expected {
            return Err(O3Error::Input);
        }
        // Guardian V1 signs struct-order serde bytes, unlike schedule/plan
        // canonical JSON. Preserve that accepted domain byte-for-byte.
        let document: durable::Stage8bP1fPhaseManifestV1 =
            serde_json::from_slice(phase).map_err(|_| O3Error::Input)?;
        if serde_json::to_vec(&document).map_err(|_| O3Error::Input)? != phase {
            return Err(O3Error::Input);
        }
        let phase = document;
        let mut unsigned = phase.clone();
        unsigned.signature_ed25519_hex.clear();
        let mut signed = b"moex.stage8b.p1f.signed-phase-manifest.v1\0".to_vec();
        signed.extend(serde_json::to_vec(&unsigned).map_err(|_| O3Error::Input)?);
        VerifyingKey::from_bytes(&decode::<32>(public)?)
            .map_err(|_| O3Error::Input)?
            .verify_strict(
                &signed,
                &Signature::from_bytes(&decode::<64>(&phase.signature_ed25519_hex)?),
            )
            .map_err(|_| O3Error::Input)?;
        let not_before = parse_time(&phase.not_before_utc)?;
        let deadline = parse_time(&phase.deadline_utc)?;
        if phase.schema_version != 1
            || phase.domain != "stage8b-p1f-phase-manifest-v1"
            || phase.issuer_key_id != KEY_ID
            || phase.phase != durable::Stage8bP1fPhaseV1::O3SyntheticPaper
            || phase.authority_generation == 0
            || phase.authority_sequence == 0
            || phase.target_host_id != durable::STAGE8B_P1F_TARGET_HOST_ID
            || phase.target_host_ssh_ed25519_sha256
                != durable::STAGE8B_P1F_TARGET_HOST_SSH_ED25519_SHA256
            || deadline - not_before != chrono::Duration::seconds(1800)
            || now < not_before
            || now >= deadline
            || phase.materialization_policy_sha256 != hash(plan_bytes)
            || phase.config_template_sha256 != hash(supervisor)
        {
            return Err(O3Error::Input);
        }
        let plan: O3CompositionPlan = canonical_parse(plan_bytes)?;
        let config = durable::parse_stage8b_p1e_supervisor_config_v1(supervisor)
            .map_err(|_| O3Error::Input)?;
        if config.runtime_profile_id != durable::Stage8bP1RuntimeProfileKind::V2.profile_id()
            || config.bootstrap.market_data_policy_sha256.as_deref()
                != Some(durable::STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256)
            || config.schedule_registry_version != plan.schedule.registry_version
            || config.schedule_registry_identity_sha256 != plan.schedule.registry_identity_sha256
        {
            return Err(O3Error::Input);
        }
        // Ordinary config validator with the actual Linux boot identity.
        // No bootstrap, secret read or mutable runtime operation occurs here.
        let durable_parent = config.bootstrap.durable_parent.clone();
        let validated = durable::validate_stage8b_p1e_supervisor_config_v1(config, boot_id)
            .map_err(|_| O3Error::Input)?;
        if plan.schema_version != 1
            || plan.domain != "moex.stage8b.p1f.o3-composition.v1"
            || plan.phase_id.is_empty()
            || plan.phase_id.len() > 128
            || !plan
                .phase_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || plan.operational_identity_sha256
                != validated.bootstrap().operational_identity_sha256()
            || plan.instrument_map_fingerprint_sha256
                != durable::stage8b_p1_imoexf_instrument_map_fingerprint_sha256()
            || plan.runtime_config_fingerprint_sha256
                != durable::Stage8bP1RuntimeProfileKind::V2
                    .build_hybrid_runtime()
                    .map_err(|_| O3Error::Input)?
                    .1
            || plan.decision.evidence.end < not_before
            || plan.successor.evidence.end >= deadline
            || plan.successor.evidence.end - plan.decision.evidence.end
                != chrono::Duration::minutes(10)
            || ![
                &plan.schedule_fixture_sha256,
                &plan.schedule_semantic_sha256,
                &plan.schedule.registry_identity_sha256,
            ]
            .into_iter()
            .all(|s| is_hash(s))
        {
            return Err(O3Error::Input);
        }
        for source in [&plan.decision, &plan.successor] {
            if source.evidence.end.timestamp() % 600 != 0
                || source.evidence.end.timestamp_subsec_nanos() != 0
                || ![
                    &source.snapshot_sha256,
                    &source.receipt_sha256,
                    &source.canonical_m10_sha256,
                ]
                .into_iter()
                .all(|s| is_hash(s))
            {
                return Err(O3Error::Input);
            }
        }
        let paper_source =
            strategy_runtime_core::stage8b_o3_readmission::Stage8bO3PaperSource::new(
                strategy_runtime_core::stage8b_o3_readmission::Stage8bO3PaperScope {
                    phase_id: plan.phase_id.clone(),
                    phase_sha256: expected.into(),
                    config_sha256: phase.config_template_sha256.clone(),
                    plan_sha256: phase.materialization_policy_sha256.clone(),
                    fixture_sha256: hash(
                        &serde_json::to_vec(&plan.schedule.synthetic_truth)
                            .map_err(|_| O3Error::Input)?,
                    ),
                    operational_identity_sha256: plan.operational_identity_sha256.clone(),
                    runtime_config_fingerprint_sha256: plan
                        .runtime_config_fingerprint_sha256
                        .clone(),
                    strategy_id: durable::STAGE8B_P1_STRATEGY_ID.into(),
                    not_before,
                    deadline,
                    catchup_coverage: plan.catchup_coverage.clone(),
                },
                plan.schedule.synthetic_truth.clone(),
            )
            .map_err(|_| O3Error::Input)?;
        Ok(Self {
            paper_source,
            durable_parent,
            plan,
            supervisor_bytes: supervisor.to_vec(),
            manifest_sha256: expected.into(),
            not_before,
            deadline,
            shutdown_grace_ms: validated.shutdown_grace_ms(),
        })
    }
}

pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn is_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn parse_time(s: &str) -> Result<DateTime<Utc>, O3Error> {
    let parsed = DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| O3Error::Input)?;
    // Guardian V1 requires whole-second canonical UTC, not schedule micros.
    if parsed.to_rfc3339_opts(chrono::SecondsFormat::Secs, true) != s {
        return Err(O3Error::Input);
    }
    Ok(parsed)
}
fn canonical_parse<T: serde::de::DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, O3Error> {
    let value: T = serde_json::from_slice(bytes).map_err(|_| O3Error::Input)?;
    if stage8b_p1e_canonical_json(&value).map_err(|_| O3Error::Input)? != bytes {
        return Err(O3Error::Input);
    }
    Ok(value)
}
pub(crate) fn decode<const N: usize>(s: &str) -> Result<[u8; N], O3Error> {
    if s.len() != N * 2
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(O3Error::Input);
    }
    let mut out = [0; N];
    for (i, pair) in s.as_bytes().chunks_exact(2).enumerate() {
        let nibble = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
        out[i] = nibble(pair[0]) * 16 + nibble(pair[1]);
    }
    Ok(out)
}

// Fixed configuration is root-owned, non-writable by the paper UID, and read
// once from a descriptor. Never follow links or block on FIFO/device input.
pub(crate) fn read_root_file(path: &Path, max: u64) -> Result<Vec<u8>, O3Error> {
    let parent = path.parent().ok_or(O3Error::Input)?;
    if parent.canonicalize().map_err(|_| O3Error::Io)? != parent {
        return Err(O3Error::Input);
    }
    for directory in parent.ancestors() {
        let m = fs::symlink_metadata(directory).map_err(|_| O3Error::Io)?;
        if !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
            return Err(O3Error::Input);
        }
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| O3Error::Io)?;
    let before = file.metadata().map_err(|_| O3Error::Io)?;
    if !before.is_file()
        || before.uid() != 0
        || before.nlink() != 1
        || before.mode() & 0o022 != 0
        || before.len() > max
    {
        return Err(O3Error::Input);
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| O3Error::Io)?;
    let after = file.metadata().map_err(|_| O3Error::Io)?;
    if bytes.len() as u64 != before.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
        || before.len() != after.len()
    {
        return Err(O3Error::Input);
    }
    Ok(bytes)
}
