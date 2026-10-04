//! Pure consumer of a root-custodied materializer output, not REST admission.
//! Raw FINAM normalization/replay belongs to the fixed materializer. Here the
//! protected supervisor template selects the source policy; staged metadata
//! cannot select a parser or authorize installation. The guardian still checks
//! the signed phase and commits the exact source/config bytes transactionally.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_STAGED: usize = 32 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Staged {
    schema_version: u16,
    domain: String,
    manifest_sha256: String,
    source_bundle_sha256: String,
    exact_source_json: String,
    evidence: Value,
    #[serde(default, deserialize_with = "present_input")]
    observed_input: Option<RetainedInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedInput {
    template_json: String,
    snapshot: Value,
}

fn present_input<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<RetainedInput>, D::Error> {
    RetainedInput::deserialize(d).map(Some)
}

/// Checked source bytes, not an authority/permit. Construction requires the
/// independently read protected templates, never templates from the package.
pub struct Stage8bP1fCheckedStagedSource {
    bytes: Vec<u8>,
    checked_at: String,
}

impl Stage8bP1fCheckedStagedSource {
    pub fn source_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn broker_truth_checked_at_utc(&self) -> &str {
        &self.checked_at
    }
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// No network, Redis, credentials or writes; existing supervisor validation
/// still checks local durable-parent metadata. The fixed operator must first
/// enforce root custody of all four byte inputs. Raw snapshot bytes
/// remain diagnostic evidence; they are NOT readmitted here as a fresh source.
pub fn check_stage8b_p1f_staged_source(
    bytes: &[u8],
    expected_manifest: &str,
    policy_bytes: &[u8],
    supervisor_template: &[u8],
    source_template: &[u8],
    now: DateTime<Utc>,
) -> Result<Stage8bP1fCheckedStagedSource, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_STAGED {
        return Err("invalid staged size");
    }
    let staged: Staged = serde_json::from_slice(bytes).map_err(|_| "invalid staged envelope")?;
    if staged.manifest_sha256 != expected_manifest
        || !valid_sha(expected_manifest)
        || staged.source_bundle_sha256 != sha(staged.exact_source_json.as_bytes())
        || staged.evidence["source_bundle_sha256"].as_str() != Some(&staged.source_bundle_sha256)
    {
        return Err("invalid staged manifest/source binding");
    }
    let policy: Value =
        serde_json::from_slice(policy_bytes).map_err(|_| "invalid protected policy")?;
    let mut template: Value =
        serde_json::from_slice(supervisor_template).map_err(|_| "invalid supervisor template")?;
    if template["first_boot_source_bundle_sha256"]
        != crate::STAGE8B_P1F_SOURCE_SHA256_TEMPLATE_SENTINEL
    {
        return Err("invalid source-hash sentinel");
    }
    template["first_boot_source_bundle_sha256"] =
        Value::String(staged.source_bundle_sha256.clone());
    let config = crate::parse_stage8b_p1e_supervisor_config_v1(
        &serde_json::to_vec(&template).map_err(|_| "invalid template")?,
    )
    .and_then(|config| crate::validate_stage8b_p1e_supervisor_config_v1(config, [0; 16]))
    .map_err(|_| "invalid supervisor binding")?;
    let observed = config.bootstrap().observed_source_policy();
    let profile = config.runtime_profile_kind();
    let policy_version = if observed {
        3
    } else if profile.no_riskgate() {
        2
    } else {
        1
    };
    if policy["schema_version"].as_u64() != Some(policy_version)
        || policy["domain"] != format!("stage8b-p1f-o2-materialization-policy-v{policy_version}")
        || policy["account_alias"].as_str() != Some(config.bootstrap().account_id().as_str())
        || policy["venue_symbol"] != "IMOEXF@RTSX"
        || !policy["account_id_sha256"].as_str().is_some_and(valid_sha)
        || staged.evidence["account_id_sha256"] != policy["account_id_sha256"]
    {
        return Err("protected policy/config mismatch");
    }
    for (field, expected) in [
        ("runtime_profile_id", profile.profile_id()),
        ("runtime_profile_sha256", profile.profile_sha256()),
    ] {
        if (policy_version == 1 && policy.get(field).is_some())
            || (policy_version != 1 && policy[field] != expected)
        {
            return Err("policy profile mismatch");
        }
    }
    if observed {
        if policy["market_data_policy_sha256"]
            != crate::STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256
            || policy["operational_identity_sha256"]
                != config.bootstrap().operational_identity_sha256()
        {
            return Err("observed policy binding mismatch");
        }
    } else if policy.get("market_data_policy_sha256").is_some()
        || policy.get("operational_identity_sha256").is_some()
    {
        return Err("legacy policy contains observed fields");
    }
    if staged.schema_version != if observed { 2 } else { 1 }
        || staged.domain
            != if observed {
                "stage8b-p1f-o2-staged-materialization-v2"
            } else {
                "stage8b-p1f-o2-staged-materialization-v1"
            }
        || staged.observed_input.is_some() != observed
    {
        return Err("staged format does not match configured policy");
    }
    if let Some(input) = &staged.observed_input {
        if input.template_json.as_bytes() != source_template || !input.snapshot.is_object() {
            return Err("retained source template mismatch");
        }
    }
    let source = crate::stage8b_p1e_first_boot_source::parse_fresh_bootstrap_bound_source(
        staged.exact_source_json.as_bytes(),
        &staged.source_bundle_sha256,
        config.bootstrap(),
        now,
    )
    .map_err(|_| "staged source admission failed")?;
    let value: Value =
        serde_json::from_str(&staged.exact_source_json).map_err(|_| "invalid source")?;
    if observed {
        let calendar: Value = serde_json::from_slice(source_template)
            .map_err(|_| "invalid protected source template")?;
        for field in [
            "operational_identity_sha256",
            "runtime_profile_sha256",
            "instrument_map_fingerprint_sha256",
            "source_bundle_generation",
        ] {
            if value.get(field) != calendar.get(field) {
                return Err("source/template identity mismatch");
            }
        }
        if value["history_coverage"]["candidate_session"]
            != calendar["history_coverage"]["candidate_session"]
        {
            return Err("source/template candidate calendar mismatch");
        }
        // Materialization appends the current-session prefix, but must not
        // silently substitute the four prior sessions selected by the template.
        let sessions = value["history_coverage"]["sessions"]
            .as_array()
            .ok_or("missing source sessions")?;
        let prior = calendar["history_coverage"]["sessions"]
            .as_array()
            .ok_or("missing template sessions")?;
        if prior.len() != 4 || sessions.get(..4) != Some(prior.as_slice()) {
            return Err("source/template history calendar mismatch");
        }
    }
    let evidence = &staged.evidence;
    if evidence["captured_at_utc"] != value["captured_at_utc"]
        || evidence["venue_symbol"] != "IMOEXF@RTSX"
        || evidence["target_position_qty"] != "0"
        || evidence["target_active_orders_count"].as_u64() != Some(0)
        || evidence["account_active_orders_count"].as_u64() != Some(0)
        || evidence["active_orders_complete"] != true
        || evidence["candidate_redis_id"] != value["candidate"]["redis_id"]
        || evidence["candidate_semantic_id_sha256"] != value["candidate"]["semantic_id_sha256"]
        || evidence["selected_m1_count"].as_u64()
            != value["candidate"]["source_m1"]
                .as_array()
                .map(|a| a.len() as u64)
        || evidence["schema_version"].as_u64() != Some(if observed { 2 } else { 1 })
        || evidence["domain"]
            != if observed {
                "stage8b-p1f-o2-observed-materialization-evidence-v2"
            } else {
                "stage8b-p1f-o2-materialization-evidence-v1"
            }
    {
        return Err("staged summary/source mismatch");
    }
    Ok(Stage8bP1fCheckedStagedSource {
        bytes: staged.exact_source_json.into_bytes(),
        checked_at: source
            .broker_truth_checked_at()
            .to_rfc3339_opts(SecondsFormat::Secs, true),
    })
}

fn valid_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn staged_consumer_preserves_both_strict_legacy_profiles_and_refuses_upgrade() {
        for profile in [
            crate::Stage8bP1RuntimeProfileKind::V1,
            crate::Stage8bP1RuntimeProfileKind::V2,
        ] {
            let parent =
                std::env::temp_dir().join(format!("legacy-staged-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&parent).unwrap();
            fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
            let parent = parent.canonicalize().unwrap();
            let mut config: Value = serde_json::from_slice(include_bytes!(
                "../../../docs/stage-8/stage8b-p1f-o2-supervisor-template.json"
            ))
            .unwrap();
            config["runtime_profile_id"] = json!(profile.profile_id());
            config["runtime_profile_sha256"] = json!(profile.profile_sha256());
            config["bootstrap"]["runtime_config_fingerprint_sha256"] =
                json!(profile.build_hybrid_runtime().unwrap().1);
            config["bootstrap"]["account_id"] = json!("ACC_TEST_0001");
            config["bootstrap"]["durable_parent"] = json!(parent);
            let config_bytes = serde_json::to_vec(&config).unwrap();
            let validated = crate::validate_stage8b_p1e_supervisor_config_v1(
                crate::parse_stage8b_p1e_supervisor_config_v1(&config_bytes).unwrap(),
                [0; 16],
            )
            .unwrap();
            let op = validated.bootstrap().operational_identity_sha256();
            let (source_bytes, now) = if profile.no_riskgate() {
                let (value, now) =
                    crate::stage8b_p1e_first_boot_source::no_riskgate_tests::fixture_for_identity(
                        10, 0, op,
                    );
                (serde_json::to_vec(&value).unwrap(), now)
            } else {
                let (bytes, now, _, _) =
                    crate::stage8b_p1e_first_boot_source::tests::fixture_for_binding(
                        op,
                        "ACC_TEST_0001",
                    );
                (bytes, now)
            };
            let source: Value = serde_json::from_slice(&source_bytes).unwrap();
            let mut policy = json!({
                "schema_version": if profile.no_riskgate() {2} else {1},
                "domain": if profile.no_riskgate() {"stage8b-p1f-o2-materialization-policy-v2"} else {"stage8b-p1f-o2-materialization-policy-v1"},
                "account_alias":"ACC_TEST_0001", "account_id_sha256":sha(b"ACC_TEST_0001"),
                "venue_symbol":"IMOEXF@RTSX"
            });
            if profile.no_riskgate() {
                policy["runtime_profile_id"] = json!(profile.profile_id());
                policy["runtime_profile_sha256"] = json!(profile.profile_sha256());
            }
            let package = json!({
                "schema_version":1, "domain":"stage8b-p1f-o2-staged-materialization-v1",
                "manifest_sha256":"aa".repeat(32), "source_bundle_sha256":sha(&source_bytes),
                "exact_source_json":std::str::from_utf8(&source_bytes).unwrap(),
                "evidence":{
                    "schema_version":1,"domain":"stage8b-p1f-o2-materialization-evidence-v1",
                    "source_bundle_sha256":sha(&source_bytes),
                    "account_id_sha256":policy["account_id_sha256"], "venue_symbol":"IMOEXF@RTSX",
                    "captured_at_utc":source["captured_at_utc"],
                    "target_position_qty":"0","target_active_orders_count":0,
                    "account_active_orders_count":0,"active_orders_complete":true,
                    "candidate_redis_id":source["candidate"]["redis_id"],
                    "candidate_semantic_id_sha256":source["candidate"]["semantic_id_sha256"],
                    "selected_m1_count":10
                }
            });
            let check = |package: &Value, policy: &Value| {
                check_stage8b_p1f_staged_source(
                    &serde_json::to_vec(package).unwrap(),
                    &"aa".repeat(32),
                    &serde_json::to_vec(policy).unwrap(),
                    &config_bytes,
                    b"{}",
                    now,
                )
            };
            for _ in 0..2 {
                let checked = check(&package, &policy).unwrap();
                assert_eq!(checked.source_bytes(), source_bytes);
                assert_eq!(
                    checked.broker_truth_checked_at_utc(),
                    source["broker_truth"]["checked_at_utc"].as_str().unwrap()
                );
            }
            for field in ["market_data_policy_sha256", "operational_identity_sha256"] {
                let mut wrong = policy.clone();
                wrong[field] = Value::Null;
                assert!(check(&package, &wrong).is_err());
            }
            for fault in 0..3 {
                let mut wrong = package.clone();
                match fault {
                    0 => wrong["observed_input"] = Value::Null,
                    1 => wrong["observed_input"] = json!({"template_json":"{}","snapshot":{}}),
                    _ => wrong["schema_version"] = json!(2),
                }
                assert!(check(&wrong, &policy).is_err());
            }
            fs::remove_dir(parent).unwrap();
        }
    }
}
