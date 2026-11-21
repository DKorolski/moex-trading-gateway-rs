//! Administrative, fresh-only DB15 provisioning. Never called by runtime attach.
//! The two consumer groups still use the accepted fixed initializer. Output
//! streams get one non-consumed marker. The schedule stream starts with a real
//! authenticated first envelope, NEVER a marker that its strict reader would
//! reject. No delete, rewind, repair, group reset or credential is exposed.
use super::*;
use strategy_runtime_core::{
    verify_stage8b_p1e_schedule_envelope_v3, Stage8bP1eScheduleEnvelopeV3,
    Stage8bP1eScheduleVerificationContextV1, STAGE8B_P1E_SCHEDULE_STREAM,
};

fn plan(supervisor: &Stage8bP1eValidatedSupervisorConfigV1) -> Stage8bP1eRedisAttachPlanV1 {
    Stage8bP1eRedisAttachPlanV1 {
        observed_source_policy: supervisor.bootstrap.observed_source_policy(),
        redis_url: supervisor.redis_url.clone(),
        redis_deployment_manifest_sha256: supervisor.redis_deployment_manifest_sha256.clone(),
        redis_config: supervisor.redis_config.clone(),
        namespace: supervisor.namespace.clone(),
        operational_identity_sha256: supervisor.bootstrap.operational_identity_sha256().into(),
        runtime_config_fingerprint_sha256: supervisor.runtime_config_fingerprint_sha256.clone(),
        deployment_generation: supervisor.bootstrap.deployment_generation(),
        consumer_generation: supervisor.bootstrap.command_consumer_generation(),
    }
}

/// Pure materialization of the existing manifest contract. No Redis access.
pub fn stage8b_p1f_o3_redis_manifest_v1(
    supervisor: &Stage8bP1eValidatedSupervisorConfigV1,
) -> Result<Vec<u8>, Stage8bP1eRedisControlError> {
    let plan = plan(supervisor);
    let manifest = stage8b_p1e_manifest_for_plan_v1(&plan);
    validate_redis_deployment_manifest(&plan, &manifest)?;
    let bytes =
        serde_json::to_vec(&manifest).map_err(|_| Stage8bP1eRedisControlError::ManifestMismatch)?;
    canonical_json_bytes(&bytes).map_err(|_| Stage8bP1eRedisControlError::ManifestMismatch)
}

/// One explicitly administrative fresh initialization. Existing/partial state
/// is rejected, including a previously successful invocation. Runtime remains
/// verify-only. The caller retains the matching Prepared publisher state and
/// replays those exact signed bytes through the normal publisher afterwards.
pub async fn provision_stage8b_p1f_o3_redis_v1(
    supervisor: &Stage8bP1eValidatedSupervisorConfigV1,
    exact_initial_schedule: &[u8],
    expected_schedule_semantic_sha256: &str,
) -> Result<(), Stage8bP1eRedisControlError> {
    let reject = || Stage8bP1eRedisControlError::ManifestMismatch;
    if !supervisor.bootstrap.observed_source_policy()
        || supervisor.runtime_profile_kind != Stage8bP1RuntimeProfileKind::V2
        || !matches!(
            supervisor.redis_url.as_str(),
            STAGE8B_P1E_REDIS_URL_IPV4 | STAGE8B_P1E_REDIS_URL_IPV6
        )
        || exact_initial_schedule.len() > 128 * 1024
    {
        return Err(reject());
    }
    let manifest = stage8b_p1f_o3_redis_manifest_v1(supervisor)?;
    if sha256_hex(&manifest) != supervisor.redis_deployment_manifest_sha256 {
        return Err(reject());
    }
    let accepted = verify_stage8b_p1e_schedule_envelope_v3(
        exact_initial_schedule,
        &Stage8bP1eScheduleVerificationContextV1 {
            expected_instrument_map_fingerprint_sha256:
                stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            expected_operational_identity_sha256: supervisor
                .bootstrap
                .operational_identity_sha256()
                .into(),
            expected_registry_identity_sha256: supervisor.schedule_registry_identity_sha256.clone(),
            expected_registry_version: supervisor.schedule_registry_version.clone(),
            expected_runtime_config_fingerprint_sha256: supervisor
                .runtime_config_fingerprint_sha256
                .clone(),
            high_water: None,
            trusted_now: Utc::now(),
        },
    )
    .map_err(|_| reject())?;
    let envelope: Stage8bP1eScheduleEnvelopeV3 =
        serde_json::from_slice(exact_initial_schedule).map_err(|_| reject())?;
    if accepted.high_water().schedule_semantic_sha256() != expected_schedule_semantic_sha256
        || envelope.source_generation != "1"
        || envelope.key_generation != 2
        || envelope.publication_sequence != "1"
        || envelope.semantic_revision != "1"
    {
        return Err(reject());
    }
    let plan = plan(supervisor);
    let client = redis::Client::open(plan.redis_url.as_str())
        .map_err(|_| Stage8bP1eRedisControlError::Redis)?;
    let mut connection = redis_operation(ConnectionManager::new(client)).await?;
    fresh_inventory(&mut connection, &plan).await?;
    // Accepted typed role/initializer; no hand-written XGROUP path.
    tokio::time::timeout(
        StdDuration::from_millis(STAGE8B_P1E_REDIS_OPERATION_TIMEOUT_MS),
        crate::provision_stage8b_p1f_fresh_namespace_v1(&plan.redis_url, plan.redis_config.clone()),
    )
    .await
    .map_err(|_| Stage8bP1eRedisControlError::OperationTimeout)?
    .map_err(|_| Stage8bP1eRedisControlError::GroupMismatch)?;
    finish_fresh_inventory(&mut connection, &plan, &manifest, exact_initial_schedule).await
}

pub(super) async fn fresh_inventory(
    connection: &mut ConnectionManager,
    plan: &Stage8bP1eRedisAttachPlanV1,
) -> Result<(), Stage8bP1eRedisControlError> {
    let manifest = stage8b_p1e_manifest_for_plan_v1(plan);
    for key in manifest.keys.iter().map(|k| k.name.as_str()).chain([
        STAGE8B_P1E_SCHEDULE_STREAM,
        STAGE8B_P1E_DEPLOYMENT_MANIFEST_KEY,
    ]) {
        let kind: String =
            redis_operation(redis::cmd("TYPE").arg(key).query_async(connection)).await?;
        if kind != "none" {
            return Err(Stage8bP1eRedisControlError::KeyTypeMismatch);
        }
    }
    Ok(())
}

pub(super) async fn finish_fresh_inventory(
    connection: &mut ConnectionManager,
    plan: &Stage8bP1eRedisAttachPlanV1,
    manifest_bytes: &[u8],
    initial_schedule: &[u8],
) -> Result<(), Stage8bP1eRedisControlError> {
    let manifest = stage8b_p1e_manifest_for_plan_v1(plan);
    for key in manifest.keys.iter().filter(|k| k.groups.is_empty()) {
        let _: String = redis_operation(
            redis::cmd("XADD")
                .arg(&key.name)
                .arg("1-0")
                .arg("provisioning")
                .arg("stage8b-p1f-administrative-boundary")
                .query_async(connection),
        )
        .await?;
    }
    let _: String = redis_operation(
        redis::cmd("XADD")
            .arg(STAGE8B_P1E_SCHEDULE_STREAM)
            .arg("1-0")
            .arg("payload")
            .arg(initial_schedule)
            .query_async(connection),
    )
    .await?;
    let stored: Option<String> = redis_operation(
        redis::cmd("SET")
            .arg(STAGE8B_P1E_DEPLOYMENT_MANIFEST_KEY)
            .arg(manifest_bytes)
            .arg("NX")
            .query_async(connection),
    )
    .await?;
    if stored.as_deref() != Some("OK") {
        return Err(Stage8bP1eRedisControlError::ManifestMismatch);
    }
    let reread: Vec<u8> = redis_operation(
        redis::cmd("GET")
            .arg(STAGE8B_P1E_DEPLOYMENT_MANIFEST_KEY)
            .query_async(connection),
    )
    .await?;
    if reread != manifest_bytes {
        return Err(Stage8bP1eRedisControlError::ManifestMismatch);
    }
    Ok(())
}
