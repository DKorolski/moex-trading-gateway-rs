//! Explicit policy V3 selects source V4. No deployed policy is rewritten.
use super::*;
use broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1;
use finam_gateway::Stage8bP1fObservedM10Plan;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetainedObservedInput {
    pub template_json: String,
    pub snapshot: ClosedM1SnapshotEvidenceV1,
}

pub(super) fn present_input<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<RetainedObservedInput>, D::Error> {
    RetainedObservedInput::deserialize(d).map(Some)
}

pub(super) fn validate_policy_binding(policy: &MaterializationPolicyV1) -> Result<(), String> {
    if policy.schema_version == 3 {
        if policy.market_data_policy_sha256.as_deref()
            != Some(runtime_durable_service::STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256)
            || !policy
                .operational_identity_sha256
                .as_deref()
                .is_some_and(valid_sha256)
        {
            return Err(
                "observed policy requires exact source-policy and operational identity".into(),
            );
        }
    } else if policy.market_data_policy_sha256.is_some()
        || policy.operational_identity_sha256.is_some()
    {
        return Err("legacy policy cannot acquire observed identity fields".into());
    }
    Ok(())
}

pub(super) fn validate_template(
    source: &serde_json::Value,
    policy: &MaterializationPolicyV1,
) -> Result<(), String> {
    if source["schema_version"] != 3
        || source["domain"] != "moex.stage8b.p1e.first-boot-source-bundle.v3"
        || source["runtime_profile_sha256"].as_str() != policy.runtime_profile_sha256.as_deref()
        || source["operational_identity_sha256"].as_str()
            != policy.operational_identity_sha256.as_deref()
    {
        return Err("observed calendar template/policy binding mismatch".into());
    }
    Ok(())
}

pub(super) fn plan(
    policy: &MaterializationPolicyV1,
    template: &[u8],
    now: DateTime<Utc>,
) -> Result<Stage8bP1fObservedM10Plan, String> {
    validate_template_policy(template, policy)?;
    if policy.schema_version != 3 {
        return Err("observed policy required".into());
    }
    let plan = Stage8bP1fObservedM10Plan::from_calendar_template(
        template,
        policy
            .operational_identity_sha256
            .as_deref()
            .ok_or("missing observed identity")?,
        now,
    )
    .map_err(|_| "observed calendar request preflight rejected")?;
    // Bounds authorize a range, they do not choose a convenient received bar.
    // Never shrink history to fit a narrow policy or request the future tail.
    if canonical_time(&policy.bars_start_utc)? > plan.request_start()
        || canonical_time(&policy.bars_end_utc)? < plan.request_end()
    {
        return Err("observed request is outside protected policy bounds".into());
    }
    Ok(plan)
}

pub(super) fn validate_retained(
    staged: &StagedMaterializationV1,
    policy: &MaterializationPolicyV1,
    account: &str,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let input = staged
        .observed_input
        .as_ref()
        .ok_or("missing retained observed input")?;
    let plan = plan(policy, input.template_json.as_bytes(), now)?;
    let materialized = plan
        .materialize(input.snapshot.clone(), now)
        .map_err(|_| "retained raw snapshot failed fresh admission")?;
    materialized
        .validate_retained_source(
            input.template_json.as_bytes(),
            staged.exact_source_json.as_bytes(),
            &staged.evidence,
            account,
            now,
        )
        .map_err(|_| "retained source/raw/template cross-validation failed".into())
}
