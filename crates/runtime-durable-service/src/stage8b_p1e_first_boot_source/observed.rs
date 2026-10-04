//! Explicit wire-V4 admission. Only schema-2 bootstrap identity selects V4;
//! legacy identities and context-free V2/V3 parsers remain strict.
//! The whole document hash must be authenticated by the source/config boundary;
//! the receipt is retained once in that document, not repeated in each M10.

use super::*;
use broker_core::observed_m1::{ObservedM1Receipt, OBSERVED_M1_POLICY_V1};

pub const STAGE8B_P1E_FIRST_BOOT_SOURCE_V4_DOMAIN: &str =
    "moex.stage8b.p1e.first-boot-source-bundle.v4";
pub const STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4: &str =
    "moex.stage8b.p1e.first-boot-source-plan.v4|closed-rest-observed-m1-to-m10-v1|full-source-receipt|canonical-m10-v2|bo-only-no-riskgate|exact-calendar-history-and-candidate";
pub const STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256: &str =
    strategy_runtime_core::STAGE8B_P1_OBSERVED_SOURCE_POLICY_SHA256;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceAggregationPolicy {
    StrictLegacy,
    ObservedV4,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FirstBootObservedSourceV4 {
    policy: String,
    source_plan_sha256: String,
    receipt_sha256: String,
    /// Exact canonical receipt bytes as UTF-8. The enclosing authenticated
    /// bundle, not the candidate or this self-declared hash, binds these bytes.
    receipt: String,
}

/// Pure source admission only; no fixed-path override, runtime start, Redis or
/// deployment authority. Callers must obtain the expected bundle hash from an
/// independent accepted source/config binding, not from candidate contents.
pub fn validate_stage8b_p1e_observed_first_boot_source_v4(
    bytes: &[u8],
    expected_source_bundle_sha256: &str,
    expected_operational_identity_sha256: &str,
    expected_account_id: &str,
    trusted_now: DateTime<Utc>,
) -> Result<Stage8bP1eValidatedFirstBootSourceV1, Stage8bP1eFirstBootSourceError> {
    parse_first_boot_source(
        bytes,
        expected_source_bundle_sha256,
        expected_operational_identity_sha256,
        expected_account_id,
        trusted_now,
        FirstBootTruthPolicy::FreshAdmission,
        SourceAggregationPolicy::ObservedV4,
    )
}

impl crate::Stage7bRestartOutcome {
    /// Reconstitute the INITIAL source context from a verified current seal.
    /// The receipt hash is not taken from Redis or trusted on its own: the
    /// retained bundle must match both configured source bytes and immutable
    /// first-boot provenance in the authenticated Stage 6 package. This grants
    /// no fresh broker-truth or execution authority and selects no successor
    /// snapshot. Legacy roots cannot acquire a V4 context through this method.
    pub fn restore_stage8b_p1_observed_source_binding(
        &self,
        config: &Stage8bP1ValidatedBootstrapConfig,
        source_bytes: &[u8],
        configured_source_bundle_sha256: &str,
        commitment_key: &strategy_runtime_core::Stage5gLifecycleCommitmentKey,
        trusted_now: DateTime<Utc>,
    ) -> Result<crate::Stage8bP1ObservedM10Binding, Stage8bP1eFirstBootSourceError> {
        let invalid = Stage8bP1eFirstBootSourceError::IdentityMismatch;
        if !config.observed_source_policy() {
            return Err(invalid);
        }
        let audit = self
            .stage8b_p1e_current_restart_package_audit(commitment_key)
            .map_err(|_| invalid)?;
        let provenance = audit.first_boot_provenance;
        if audit.operational_identity_sha256 != config.operational_identity_sha256()
            || provenance.operational_identity_sha256() != config.operational_identity_sha256()
            || provenance.runtime_config_fingerprint_sha256()
                != config.runtime_config_fingerprint_sha256()
            || config.first_boot_source_plan_sha256() != Some(provenance.source_plan_sha256())
            || provenance.source_bundle_sha256() != configured_source_bundle_sha256
        {
            return Err(invalid);
        }
        let mut source = parse_bootstrap_bound_source(
            source_bytes,
            configured_source_bundle_sha256,
            config,
            trusted_now,
            FirstBootTruthPolicy::HistoricalRecovery,
        )?;
        if provenance.runtime_profile_sha256() != source.runtime_profile.profile_sha256()
            || provenance.source_bundle_generation() != source.source_bundle_generation
            || provenance.history_bars_sha256() != source.history_bars_sha256
            || provenance.riskgate_session_observations_sha256()
                != source.riskgate_session_observations_sha256
            || provenance.candidate_semantic_id_sha256() != source.candidate_semantic_id_sha256
        {
            return Err(invalid);
        }
        let receipt = source.observed_receipt.take().ok_or(invalid)?;
        // The enclosing bytes were just matched against sealed/configured SHA.
        let hash = receipt.sha256().to_string();
        crate::Stage8bP1ObservedM10Binding::new(
            config.operational_identity_sha256(),
            receipt,
            &hash,
        )
        .map_err(|_| invalid)
    }
}

/// Same fixed source-file custody used by first boot; used only after the
/// current durable restart has been authenticated. No discovery or writes.
pub(crate) fn restore_fixed_observed_context(
    restart: &crate::Stage7bRestartOutcome,
    config: &Stage8bP1ValidatedBootstrapConfig,
    configured_source_sha256: &str,
    key: &strategy_runtime_core::Stage5gLifecycleCommitmentKey,
    now: DateTime<Utc>,
) -> Result<
    crate::stage8b_p1_semantic::observed::Stage8bP1ObservedRecoveryContext,
    Stage8bP1eFirstBootBuildError,
> {
    let bytes = read_protected_first_boot_source(
        Path::new(STAGE8B_P1E_FIRST_BOOT_SOURCE_PATH),
        0,
        service_group_gid()?,
        || {},
    )?;
    let initial = restart.restore_stage8b_p1_observed_source_binding(
        config,
        &bytes,
        configured_source_sha256,
        key,
        now,
    )?;
    crate::stage8b_p1_semantic::observed::Stage8bP1ObservedRecoveryContext::new(initial, config)
        .map_err(|_| Stage8bP1eFirstBootBuildError::Source)
}

pub(super) fn validate_receipt(
    document: &FirstBootSourceDocumentV1,
    policy: SourceAggregationPolicy,
    captured_at: DateTime<Utc>,
) -> Result<Option<ObservedM1Receipt>, Stage8bP1eFirstBootSourceError> {
    if policy == SourceAggregationPolicy::StrictLegacy {
        return Ok(None);
    }
    let invalid = Stage8bP1eFirstBootSourceError::InvalidHistory;
    let observed = document.observed_source.as_ref().ok_or(invalid)?;
    if observed.policy != OBSERVED_M1_POLICY_V1
        || observed.source_plan_sha256 != STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256
    {
        return Err(invalid);
    }
    let receipt = ObservedM1Receipt::restore(observed.receipt.as_bytes(), &observed.receipt_sha256)
        .map_err(|_| invalid)?;
    if receipt.received_at() > captured_at
        || captured_at - receipt.received_at() > Duration::seconds(900)
        || receipt.instrument() != &crate::stage8b_p1_semantic::p1_instrument()
        || receipt.bars().iter().any(|bar| {
            [bar.open, bar.high, bar.low, bar.close]
                .iter()
                .any(|price| *price % Decimal::new(5, 1) != Decimal::ZERO)
        })
    {
        return Err(invalid);
    }
    Ok(Some(receipt))
}

pub(super) fn validate_derived_bar(
    receipt: &ObservedM1Receipt,
    bar: &Stage8bP1eFirstBootBarV1,
) -> Result<(), Stage8bP1eFirstBootSourceError> {
    let invalid = Stage8bP1eFirstBootSourceError::InvalidCandidate;
    let open = bar
        .close_time_utc
        .checked_sub(600)
        .and_then(|t| DateTime::from_timestamp(t, 0))
        .ok_or(invalid)?;
    let derived = receipt.bucket(open).map_err(|_| invalid)?.bar;
    if [
        &bar.open_text,
        &bar.high_text,
        &bar.low_text,
        &bar.close_text,
        &bar.volume_text,
    ]
    .into_iter()
    .zip([
        derived.open,
        derived.high,
        derived.low,
        derived.close,
        derived.volume,
    ])
    .any(|(actual, expected)| actual != &expected.normalize().to_string())
    {
        return Err(invalid);
    }
    Ok(())
}

pub(super) fn validate_candidate(
    receipt: &ObservedM1Receipt,
    document: &FirstBootCandidateBarV1,
    candidate: &Stage8bP1eFirstBootBarV1,
    history: &[Stage8bP1eFirstBootBarV1],
    operational_identity: &str,
) -> Result<crate::Stage8bP1ValidatedCanonicalM10, Stage8bP1eFirstBootSourceError> {
    let invalid = Stage8bP1eFirstBootSourceError::InvalidCandidate;
    validate_derived_bar(receipt, candidate)?;
    if receipt.end().timestamp() != candidate.close_time_utc
        || history
            .first()
            .and_then(|b| b.close_time_utc.checked_sub(600))
            != Some(receipt.start().timestamp())
    {
        return Err(invalid);
    }
    let bytes = crate::build_stage8b_p1_observed_canonical_m10(
        operational_identity,
        document.open_ts_utc_ms,
        receipt,
    )
    .map_err(|_| invalid)?;
    let expected: Value = serde_json::from_slice(&bytes).map_err(|_| invalid)?;
    if serde_json::to_value(&document.source_m1).map_err(|_| invalid)?
        != expected["payload"]["source_m1"]
    {
        return Err(invalid);
    }
    crate::parse_stage8b_p1_observed_canonical_m10(
        &bytes,
        operational_identity,
        receipt,
        receipt.sha256(),
    )
    .map_err(|_| invalid)
}

#[cfg(test)]
pub(crate) mod tests;
