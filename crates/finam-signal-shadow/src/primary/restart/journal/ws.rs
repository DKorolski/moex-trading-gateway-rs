//! Owned, strict V8 schema and independent timing/provenance validation.
//! No producer "PASS" or mutable cache state is consumed here.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Version {
    generation: u64,
    first_received_at_ms: i64,
    last_changed_at_ms: i64,
    frame_sha256: String,
    first_frame_sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    #[serde(flatten)]
    canonical: Canonical,
    source_transport: String,
    processing_mode: String,
    timeliness: String,
    broker_execution: bool,
    episode: u8,
    episode_deadline_ms: i64,
    continuity_revision: u64,
    data_gap_at_ms: i64,
    boundary_frame_sha256: String,
    versions: Vec<Version>,
    virtual_callback_at_ms: i64,
    replay_deadline_ms: i64,
}
pub(super) fn inspect(
    value: Value,
    validated: &ValidatedConfig,
) -> Result<(Canonical, String, i64), RestartError> {
    let r: Receipt = serde_json::from_value(value).map_err(|_| RestartError::Source)?;
    let policy = validated.primary_policy().ok_or(RestartError::Source)?;
    let c = &r.canonical;
    let boundary = c.boundary_received_at_ms.ok_or(RestartError::Source)?;
    let live_deadline = c
        .close_ms
        .checked_add(2000)
        .and_then(|t| t.checked_sub(validated.config().clock_uncertainty_ms as i64))
        .ok_or(RestartError::Source)?;
    if !policy.ws_historical_recovery()
        || c.origin != "WS_RECOVERY_BACKFILL"
        || r.source_transport != "WS"
        || r.processing_mode != "BACKFILL_REPLAY"
        || r.timeliness != "LATE"
        || r.broker_execution
        || !(1..=3).contains(&r.episode)
        || r.virtual_callback_at_ms != c.close_ms
        || r.data_gap_at_ms <= live_deadline
        || r.data_gap_at_ms > c.canonical_ready_at_ms
        || c.canonical_ready_at_ms <= live_deadline
        || c.canonical_ready_at_ms >= r.replay_deadline_ms
        || r.replay_deadline_ms
            != r.episode_deadline_ms
                .min(validated.config().run_deadline_utc_ms)
        || !hash(&r.boundary_frame_sha256)
        || c.actual_m1.len() != 10
        || r.versions.len() != 10
        || c.source_frame_sha256.len() != 10
        || c.reason.is_some()
        || c.rest_snapshot_sha256.is_some()
        || c.proof_received_at_ms.is_some()
        || boundary < c.close_ms
        || boundary
            .checked_add(policy.close_grace_ms as i64)
            .map_or(true, |t| t > c.canonical_ready_at_ms)
        || r.versions
            .iter()
            .zip(&c.source_frame_sha256)
            .any(|(v, frame)| {
                v.generation != c.generation
                    || v.first_received_at_ms < c.subscribed_at_ms
                    || v.first_received_at_ms > v.last_changed_at_ms
                    || v.last_changed_at_ms > c.canonical_ready_at_ms
                    || &v.frame_sha256 != frame
                    || !hash(frame)
                    || !hash(&v.first_frame_sha256)
            })
        || r.versions.iter().map(|v| v.last_changed_at_ms).max()
            != Some(c.last_component_received_at_ms)
    {
        return Err(RestartError::Source);
    }
    let digest = typed_hash(&r)?;
    Ok((r.canonical, digest, r.replay_deadline_ms))
}

#[cfg(test)]
pub(crate) fn reseal(value: Value) -> String {
    typed_hash(&serde_json::from_value::<Receipt>(value).unwrap()).unwrap()
}
