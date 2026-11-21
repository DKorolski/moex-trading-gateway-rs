use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Historical reference only; it must never be labelled current ALOR live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reference {
    #[serde(rename = "alor_freeze_2026_09_27")]
    AlorFreeze20260927,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestartMode {
    #[serde(rename = "new_segment")]
    NewSegment,
}

/// All bounds are explicit. Values below are configuration, not evidence.
/// No flatten/reset, broker credentials, live switch or operational namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowConfig {
    pub schema_version: u32,
    pub reference: Reference,
    pub restart_mode: RestartMode,
    pub model_profile: String,
    pub model_sha256: String,
    pub reference_sha256: String,
    pub feedback_policy_sha256: String,
    pub instrument_mapping_sha256: String,
    pub calendar_sha256: String,
    pub seed_sha256: String,
    pub instrument: String,
    pub run_id: String,
    pub segment_id: String,
    pub previous_segment_id: Option<String>,
    /// Exact four-session inventory; calendar eligibility is checked by warmup.
    pub warmup_session_dates: [String; 4],
    pub warmup_range_start_utc_ms: i64,
    pub warmup_range_end_utc_ms: i64,
    pub observation_start_utc_ms: i64,
    pub run_deadline_utc_ms: i64,
    pub lmax_ms: u64,
    pub max_quote_age_ms: u64,
    pub clock_uncertainty_ms: u64,
    pub ws_silence_budget_ms: u64,
    pub reconnect_budget_ms: u64,
    pub stop_timeout_ms: u64,
    pub max_input_bytes: u64,
    pub max_cache_bars: u64,
    pub max_output_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    Json,
    Profile,
    Identity,
    Digest,
    Warmup,
    Bounds,
}

#[derive(Debug)]
pub struct ValidatedConfig {
    config: ShadowConfig,
    fingerprint: String,
    semantic_fingerprint: String,
    primary: Option<crate::primary::PrimaryPolicy>,
}

impl ValidatedConfig {
    /// Explicit construction only. The legacy JSON parser remains v1-only so
    /// unfinished WS-primary cannot accidentally execute the REST-driven runner.
    pub fn from_ws_primary(value: &crate::primary::WsPrimaryConfig) -> Result<Self, ConfigError> {
        let fingerprint = value.fingerprint().map_err(|_| ConfigError::Profile)?;
        let semantic_fingerprint = value
            .semantic_fingerprint()
            .map_err(|_| ConfigError::Profile)?;
        let mut config = Self::validate(value.base.clone())?;
        config.fingerprint = fingerprint;
        config.semantic_fingerprint = semantic_fingerprint;
        config.primary = Some(value.primary.clone());
        Ok(config)
    }

    pub fn primary_policy(&self) -> Option<&crate::primary::PrimaryPolicy> {
        self.primary.as_ref()
    }

    /// Existing policy digest is the explicit selector; no implicit mode switch.
    pub fn candle_only(&self) -> bool {
        self.config.feedback_policy_sha256 == crate::feedback::candle_policy_fingerprint()
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ConfigError> {
        // Bound parsing even before a configured input budget exists.
        if bytes.len() > 16_384 {
            return Err(ConfigError::Bounds);
        }
        let config: ShadowConfig = serde_json::from_slice(bytes).map_err(|_| ConfigError::Json)?;
        Self::validate(config)
    }

    pub fn validate(config: ShadowConfig) -> Result<Self, ConfigError> {
        if config.schema_version != 1
            || config.model_profile != "imoexf-baseline07-bo-only-no-riskgate"
        {
            return Err(ConfigError::Profile);
        }
        fn identity(value: &str) -> bool {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.@".contains(&b))
        }
        if [&config.instrument, &config.run_id, &config.segment_id]
            .iter()
            .any(|v| !identity(v))
            || config
                .previous_segment_id
                .as_ref()
                .is_some_and(|p| !identity(p) || p == &config.segment_id)
        {
            return Err(ConfigError::Identity);
        }
        for value in [
            &config.model_sha256,
            &config.reference_sha256,
            &config.feedback_policy_sha256,
            &config.instrument_mapping_sha256,
            &config.calendar_sha256,
            &config.seed_sha256,
        ] {
            if value.len() != 64
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(ConfigError::Digest);
            }
        }
        // This is only syntax/order validation, NOT proof that these are sessions.
        fn date_shape(value: &str) -> bool {
            value.len() == 10
                && value.bytes().enumerate().all(|(i, b)| {
                    if i == 4 || i == 7 {
                        b == b'-'
                    } else {
                        b.is_ascii_digit()
                    }
                })
        }
        if config.warmup_session_dates.iter().any(|d| !date_shape(d))
            || config.warmup_session_dates.windows(2).any(|p| p[0] >= p[1])
            || config.warmup_range_start_utc_ms < 0
            || config.warmup_range_start_utc_ms >= config.warmup_range_end_utc_ms
            || config.warmup_range_end_utc_ms > config.observation_start_utc_ms
        {
            return Err(ConfigError::Warmup);
        }
        if config.observation_start_utc_ms >= config.run_deadline_utc_ms
            || config.lmax_ms == 0
            || config.clock_uncertainty_ms >= config.lmax_ms
            || [
                config.ws_silence_budget_ms,
                config.reconnect_budget_ms,
                config.stop_timeout_ms,
                config.max_quote_age_ms,
                config.max_input_bytes,
                config.max_cache_bars,
                config.max_output_bytes,
            ]
            .contains(&0)
        {
            return Err(ConfigError::Bounds);
        }
        // Re-serialization removes JSON whitespace/key-order differences. All
        // required config fields, including feedback assumptions, are bound.
        let canonical = serde_json::to_vec(&config).map_err(|_| ConfigError::Json)?;
        let input_policy =
            if config.feedback_policy_sha256 == crate::feedback::candle_policy_fingerprint() {
                crate::feedback::CANDLE_POLICY
            } else {
                crate::quote::QUOTE_DEDUP_POLICY
            };
        let mut hash = Sha256::new();
        hash.update(b"moex.signal-shadow.config.v1\0");
        hash.update(input_policy.as_bytes());
        hash.update([0]);
        hash.update(canonical);
        // Derive from the complete serialized config, removing ONLY attribution.
        // Newly added configuration fields therefore remain bound by default.
        let mut semantic = serde_json::to_value(&config).map_err(|_| ConfigError::Json)?;
        let object = semantic.as_object_mut().ok_or(ConfigError::Json)?;
        for key in ["run_id", "segment_id", "previous_segment_id"] {
            object.remove(key);
        }
        let mut semantic_hash = Sha256::new();
        semantic_hash.update(b"moex.signal-shadow.semantic-config.v1\0");
        semantic_hash.update(input_policy.as_bytes());
        semantic_hash.update([0]);
        semantic_hash.update(serde_json::to_vec(&semantic).map_err(|_| ConfigError::Json)?);
        Ok(Self {
            config,
            fingerprint: format!("{:x}", hash.finalize()),
            semantic_fingerprint: format!("{:x}", semantic_hash.finalize()),
            primary: None,
        })
    }

    pub fn config(&self) -> &ShadowConfig {
        &self.config
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    /// All configuration except run/segment attribution; not a provenance hash.
    pub fn semantic_fingerprint(&self) -> &str {
        &self.semantic_fingerprint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ShadowConfig {
        ShadowConfig {
            schema_version: 1,
            reference: Reference::AlorFreeze20260927,
            restart_mode: RestartMode::NewSegment,
            model_profile: "imoexf-baseline07-bo-only-no-riskgate".into(),
            model_sha256: "a".repeat(64),
            reference_sha256: "b".repeat(64),
            feedback_policy_sha256: "c".repeat(64),
            instrument_mapping_sha256: "d".repeat(64),
            calendar_sha256: "e".repeat(64),
            seed_sha256: "f".repeat(64),
            instrument: "IMOEXF@RTSX".into(),
            run_id: "test".into(),
            segment_id: "s1".into(),
            previous_segment_id: None,
            warmup_session_dates: ["2026-09-22", "2026-09-23", "2026-09-24", "2026-09-25"]
                .map(str::to_owned),
            warmup_range_start_utc_ms: 1,
            warmup_range_end_utc_ms: 2,
            observation_start_utc_ms: 3,
            run_deadline_utc_ms: 4,
            lmax_ms: 1000,
            max_quote_age_ms: 1000,
            clock_uncertainty_ms: 10,
            ws_silence_budget_ms: 1000,
            reconnect_budget_ms: 1000,
            stop_timeout_ms: 1000,
            max_input_bytes: 1000,
            max_cache_bars: 100,
            max_output_bytes: 1000,
        }
    }

    #[test]
    fn semantic_binding_excludes_only_attribution_and_preserves_full_provenance() {
        let original = fixture();
        let validated = ValidatedConfig::validate(original.clone()).unwrap();
        let mut old_hash = Sha256::new();
        old_hash.update(b"moex.signal-shadow.config.v1\0");
        old_hash.update(crate::quote::QUOTE_DEDUP_POLICY.as_bytes());
        old_hash.update([0]);
        old_hash.update(serde_json::to_vec(&original).unwrap());
        assert_eq!(
            validated.fingerprint(),
            format!("{:x}", old_hash.finalize())
        );
        let old_policy =
            b"moex.shadow.quote-dedup.evict-oldest-4096.monotonic-frontier.old-hold.v1";
        let mut h = Sha256::new();
        h.update(b"moex.signal-shadow.config.v1\0");
        h.update(old_policy);
        h.update([0]);
        h.update(serde_json::to_vec(&original).unwrap());
        assert_ne!(validated.fingerprint(), format!("{:x}", h.finalize()));
        let mut semantic = serde_json::to_value(&original).unwrap();
        for k in ["run_id", "segment_id", "previous_segment_id"] {
            semantic.as_object_mut().unwrap().remove(k);
        }
        let mut h = Sha256::new();
        h.update(b"moex.signal-shadow.semantic-config.v1\0");
        h.update(old_policy);
        h.update([0]);
        h.update(serde_json::to_vec(&semantic).unwrap());
        assert_ne!(
            validated.semantic_fingerprint(),
            format!("{:x}", h.finalize())
        );
        assert_eq!(
            validated.semantic_fingerprint(),
            ValidatedConfig::parse(&serde_json::to_vec_pretty(&original).unwrap())
                .unwrap()
                .semantic_fingerprint()
        );
        let value = serde_json::to_value(&original).unwrap();
        for (key, old) in value.as_object().unwrap() {
            let mut changed = value.clone();
            changed[key] = match key.as_str() {
                "previous_segment_id" => "previous".into(),
                "warmup_session_dates" => {
                    serde_json::json!(["2026-09-21", "2026-09-23", "2026-09-24", "2026-09-25"])
                }
                "warmup_range_start_utc_ms" => 0.into(),
                "warmup_range_end_utc_ms" => 3.into(),
                "observation_start_utc_ms" => 2.into(),
                _ if key.ends_with("_sha256") => "0".repeat(64).into(),
                _ if old.is_string() => format!("{}-changed", old.as_str().unwrap()).into(),
                _ => (old.as_u64().unwrap() + 1).into(),
            };
            let result = ValidatedConfig::parse(&serde_json::to_vec(&changed).unwrap());
            // These currently have exactly one admissible value; mutations
            // cannot enter a report. Every other field must remain bound.
            if [
                "schema_version",
                "reference",
                "restart_mode",
                "model_profile",
            ]
            .contains(&key.as_str())
            {
                assert!(result.is_err(), "{key}");
                continue;
            }
            let changed = result.unwrap();
            assert_ne!(validated.fingerprint(), changed.fingerprint(), "{key}");
            assert_eq!(
                validated.semantic_fingerprint() == changed.semantic_fingerprint(),
                ["run_id", "segment_id", "previous_segment_id"].contains(&key.as_str()),
                "{key}"
            );
        }
    }

    #[test]
    fn explicit_config_roundtrip_binds_policy_and_ignores_json_formatting() {
        let config = fixture();
        let a = ValidatedConfig::validate(config.clone()).unwrap();
        let b = ValidatedConfig::parse(&serde_json::to_vec_pretty(&config).unwrap()).unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
        let mut changed = config;
        changed.feedback_policy_sha256 = "f".repeat(64);
        assert_ne!(
            a.fingerprint(),
            ValidatedConfig::validate(changed).unwrap().fingerprint()
        );
    }

    #[test]
    fn unknown_live_switch_and_missing_bound_are_rejected() {
        let mut value = serde_json::to_value(fixture()).unwrap();
        value["allow_live_orders"] = true.into();
        assert_eq!(
            ValidatedConfig::parse(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
            ConfigError::Json
        );
        value.as_object_mut().unwrap().remove("allow_live_orders");
        value.as_object_mut().unwrap().remove("lmax_ms");
        assert_eq!(
            ValidatedConfig::parse(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
            ConfigError::Json
        );
    }

    #[test]
    fn duplicate_keys_are_not_last_wins() {
        let text = serde_json::to_string(&fixture()).unwrap();
        let text = text.replacen('{', "{\"schema_version\":1,", 1);
        assert_eq!(
            ValidatedConfig::parse(text.as_bytes()).unwrap_err(),
            ConfigError::Json
        );
    }

    #[test]
    fn rejects_profile_future_warmup_zero_bounds_and_same_segment() {
        let mut c = fixture();
        c.model_profile = "candidate09".into();
        assert_eq!(
            ValidatedConfig::validate(c).unwrap_err(),
            ConfigError::Profile
        );
        let mut c = fixture();
        c.warmup_range_end_utc_ms = 4;
        assert_eq!(
            ValidatedConfig::validate(c).unwrap_err(),
            ConfigError::Warmup
        );
        let mut c = fixture();
        c.max_cache_bars = 0;
        assert_eq!(
            ValidatedConfig::validate(c).unwrap_err(),
            ConfigError::Bounds
        );
        let mut c = fixture();
        c.previous_segment_id = Some(c.segment_id.clone());
        assert_eq!(
            ValidatedConfig::validate(c).unwrap_err(),
            ConfigError::Identity
        );
    }

    #[test]
    fn rejects_oversize_input_and_malformed_digest() {
        assert_eq!(
            ValidatedConfig::parse(&vec![b' '; 16_385]).unwrap_err(),
            ConfigError::Bounds
        );
        let mut c = fixture();
        c.reference_sha256 = "not-a-digest".into();
        assert_eq!(
            ValidatedConfig::validate(c).unwrap_err(),
            ConfigError::Digest
        );
    }
}
