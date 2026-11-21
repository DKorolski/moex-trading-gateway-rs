use super::PrimaryError;
use crate::ShadowConfig;
use serde::{Deserialize, Serialize};

pub const WS_PRIMARY_POLICY: &str = "moex.signal-shadow.ws-primary-m10.v1";

/// Explicit opt-in. Old inputs retain the accepted boundary/exact-volume rules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryPolicy {
    #[default]
    StrictBoundaryV1,
    /// BO-only as-observed candles, not a claim of immutable provider finality.
    ObservedBo1sVolumeAuditV1,
    /// Explicit source choice before the first callback: timely STARTUP_PARTIAL
    /// REST may replace provisional prices. Eligible WS never waits for REST.
    ObservedBoRestPrecommitWsFirstV2,
    /// Shadow-only soft REST comparisons. Valid mandatory startup data may
    /// still be used before a callback; optional REST never halts eligible WS.
    ObservedBoNonBlockingRestV3,
    /// Explicit whole admitted REST authority for an unpublished exception slot.
    /// Eligible WS remains first; historical bootstrap revisions are discarded,
    /// never imported as live data or used to rewrite committed model inputs.
    ObservedBoAdmittedRestV4,
    /// Versioned observed sources; no rewrite of original canonical inputs.
    ObservedBoConsistentSourcesV5,
    /// V5 sources plus bounded, non-destructive recent-tail recovery requests.
    ObservedBoRestTailRecoveryV6,
    /// V6 admission plus data-only repair and resync independent of first DATA.
    /// Explicit opt-in; V6 and earlier retain their historical behavior.
    ObservedBoDataWaitRecoveryV7,
    /// V7 plus same-connection, late historical WS repair. Never live authority.
    ObservedBoWsHistoricalRecoveryV8,
}

fn strict_delivery(value: &DeliveryPolicy) -> bool {
    *value == DeliveryPolicy::StrictBoundaryV1
}

/// No implicit legacy fallback. An old executable rejects this v2 envelope.
/// Parsing this type alone does not activate the unfinished new runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WsPrimaryConfig {
    pub schema_version: u32,
    pub market_data_mode: String,
    pub source_policy: String,
    pub base: ShadowConfig,
    pub primary: PrimaryPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimaryPolicy {
    #[serde(default, skip_serializing_if = "strict_delivery")]
    pub delivery: DeliveryPolicy,
    pub close_grace_ms: u64,
    pub exception_wait_ms: u64,
    pub proof_timeout_ms: u64,
    pub ingress_capacity: usize,
    pub max_ingress_age_ms: u64,
    pub max_cache_m1: usize,
    pub bootstrap_max_rows: usize,
    pub bootstrap_max_bytes: usize,
    /// None disables audit; Some(6) audits each sixth committed M10.
    pub audit_every_commits: Option<u16>,
    pub recovery_overlap_m10: usize,
    pub recovery_max_gap_ms: u64,
}

impl Default for PrimaryPolicy {
    fn default() -> Self {
        Self {
            delivery: DeliveryPolicy::StrictBoundaryV1,
            close_grace_ms: 1000,
            exception_wait_ms: 3000,
            proof_timeout_ms: 5000,
            ingress_capacity: 128,
            max_ingress_age_ms: 2000,
            max_cache_m1: 4096,
            bootstrap_max_rows: 512,
            bootstrap_max_bytes: 256 * 1024,
            audit_every_commits: Some(6),
            recovery_overlap_m10: 2,
            recovery_max_gap_ms: 6 * 60 * 60 * 1000,
        }
    }
}

impl PrimaryPolicy {
    pub(crate) fn observed_bo(&self) -> bool {
        matches!(
            self.delivery,
            DeliveryPolicy::ObservedBo1sVolumeAuditV1
                | DeliveryPolicy::ObservedBoRestPrecommitWsFirstV2
                | DeliveryPolicy::ObservedBoNonBlockingRestV3
                | DeliveryPolicy::ObservedBoAdmittedRestV4
                | DeliveryPolicy::ObservedBoConsistentSourcesV5
                | DeliveryPolicy::ObservedBoRestTailRecoveryV6
                | DeliveryPolicy::ObservedBoDataWaitRecoveryV7
                | DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8
        )
    }

    pub(crate) fn rest_precommit_ws_first(&self) -> bool {
        matches!(
            self.delivery,
            DeliveryPolicy::ObservedBoRestPrecommitWsFirstV2
                | DeliveryPolicy::ObservedBoNonBlockingRestV3
                | DeliveryPolicy::ObservedBoAdmittedRestV4
                | DeliveryPolicy::ObservedBoConsistentSourcesV5
                | DeliveryPolicy::ObservedBoRestTailRecoveryV6
                | DeliveryPolicy::ObservedBoDataWaitRecoveryV7
                | DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8
        )
    }

    pub(crate) fn rest_nonblocking(&self) -> bool {
        matches!(
            self.delivery,
            DeliveryPolicy::ObservedBoNonBlockingRestV3
                | DeliveryPolicy::ObservedBoAdmittedRestV4
                | DeliveryPolicy::ObservedBoConsistentSourcesV5
                | DeliveryPolicy::ObservedBoRestTailRecoveryV6
                | DeliveryPolicy::ObservedBoDataWaitRecoveryV7
                | DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8
        )
    }

    pub(crate) fn admitted_rest_source(&self) -> bool {
        matches!(
            self.delivery,
            DeliveryPolicy::ObservedBoAdmittedRestV4
                | DeliveryPolicy::ObservedBoConsistentSourcesV5
                | DeliveryPolicy::ObservedBoRestTailRecoveryV6
                | DeliveryPolicy::ObservedBoDataWaitRecoveryV7
                | DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8
        )
    }

    pub(crate) fn consistent_sources(&self) -> bool {
        matches!(
            self.delivery,
            DeliveryPolicy::ObservedBoConsistentSourcesV5
                | DeliveryPolicy::ObservedBoRestTailRecoveryV6
                | DeliveryPolicy::ObservedBoDataWaitRecoveryV7
                | DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8
        )
    }

    pub(crate) fn rest_tail_recovery(&self) -> bool {
        matches!(
            self.delivery,
            DeliveryPolicy::ObservedBoRestTailRecoveryV6
                | DeliveryPolicy::ObservedBoDataWaitRecoveryV7
                | DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8
        )
    }

    pub(crate) fn data_wait_recovery(&self) -> bool {
        matches!(
            self.delivery,
            DeliveryPolicy::ObservedBoDataWaitRecoveryV7
                | DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8
        )
    }

    pub(crate) fn ws_historical_recovery(&self) -> bool {
        self.delivery == DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8
    }

    pub(crate) fn proof_wait_ms(&self) -> i64 {
        if self.observed_bo() {
            1000
        } else {
            3000
        }
    }

    pub fn validate(&self) -> Result<(), PrimaryError> {
        // R0 is an exact client policy, not user-adjustable provider guarantees.
        let expected = Self::default();
        if self.audit_every_commits.is_some_and(|v| v != 6)
            || (Self {
                delivery: expected.delivery,
                audit_every_commits: expected.audit_every_commits,
                ..self.clone()
            }) != expected
        {
            return Err(PrimaryError::Configuration);
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> Result<String, PrimaryError> {
        self.validate()?;
        let mut bytes = WS_PRIMARY_POLICY.as_bytes().to_vec();
        bytes.push(0);
        bytes.extend(serde_json::to_vec(self).map_err(|_| PrimaryError::Configuration)?);
        Ok(crate::seed::digest(&bytes))
    }
}

impl WsPrimaryConfig {
    pub fn parse(bytes: &[u8]) -> Result<Self, PrimaryError> {
        if bytes.len() > 16384 {
            return Err(PrimaryError::Configuration);
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| PrimaryError::Configuration)?;
        value.primary.validate()?;
        let base = crate::ValidatedConfig::validate(value.base.clone())
            .map_err(|_| PrimaryError::Configuration)?;
        if value.schema_version != 2
            || value.market_data_mode != "ws_primary_v1"
            || value.source_policy != WS_PRIMARY_POLICY
            || !base.candle_only()
            || value.base.instrument != "IMOEXF@RTSX"
            || value.base.lmax_ms != 10000
            || value.base.clock_uncertainty_ms > 100
            || value.base.max_cache_bars != 4096
        {
            return Err(PrimaryError::Configuration);
        }
        Ok(value)
    }

    pub fn fingerprint(&self) -> Result<String, PrimaryError> {
        // Recheck even if a caller changed one of the public DTO fields.
        let canonical = serde_json::to_vec(self).map_err(|_| PrimaryError::Configuration)?;
        Self::parse(&canonical)?;
        let mut bytes = b"moex.signal-shadow.config.v2\0".to_vec();
        bytes.extend(canonical);
        Ok(crate::seed::digest(&bytes))
    }

    pub fn semantic_fingerprint(&self) -> Result<String, PrimaryError> {
        self.fingerprint()?;
        let mut value = serde_json::to_value(self).map_err(|_| PrimaryError::Configuration)?;
        let base = value
            .get_mut("base")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or(PrimaryError::Configuration)?;
        for key in ["run_id", "segment_id", "previous_segment_id"] {
            base.remove(key);
        }
        let mut bytes = b"moex.signal-shadow.semantic-config.v2\0".to_vec();
        bytes.extend(serde_json::to_vec(&value).map_err(|_| PrimaryError::Configuration)?);
        Ok(crate::seed::digest(&bytes))
    }
}

#[cfg(test)]
mod data_wait_tests {
    use super::*;
    #[test]
    fn v7_is_explicitly_versioned_and_preserves_all_source_and_latency_budgets() {
        let strict = PrimaryPolicy::default();
        let v6 = PrimaryPolicy {
            delivery: DeliveryPolicy::ObservedBoRestTailRecoveryV6,
            ..strict.clone()
        };
        let v7 = PrimaryPolicy {
            delivery: DeliveryPolicy::ObservedBoDataWaitRecoveryV7,
            ..v6.clone()
        };
        for policy in [&strict, &v6, &v7] {
            policy.validate().unwrap();
        }
        assert!(!strict.data_wait_recovery());
        assert!(!v6.data_wait_recovery());
        assert!(v7.data_wait_recovery());
        assert!(v7.rest_tail_recovery() && v7.consistent_sources());
        assert_ne!(v6.fingerprint().unwrap(), v7.fingerprint().unwrap());
        let mut v7_fields = serde_json::to_value(&v7).unwrap();
        v7_fields["delivery"] = serde_json::json!(v6.delivery);
        assert_eq!(v7_fields, serde_json::to_value(&v6).unwrap());
        assert_eq!(
            serde_json::to_value(&v6).unwrap()["delivery"],
            "ObservedBoRestTailRecoveryV6"
        );
        assert!(serde_json::to_value(&strict)
            .unwrap()
            .get("delivery")
            .is_none());
    }
}
