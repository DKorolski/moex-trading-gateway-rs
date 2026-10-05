//! Pure assembly of the new first-boot wire. This shares the existing account,
//! order, instrument, schedule and raw-route checks; it never fills sparse V3.

use super::super::{
    short_history_coverage, validate_flat_target, validate_params, validate_route_observations,
    validate_schedule, validate_zero_active_orders, Stage8bP1fO2MaterializationEvidenceV1,
    Stage8bP1fO2MaterializedSourceV1, Stage8bP1fO2MaterializerErrorV1, Stage8bP1fO2RouteEvidenceV1,
    STAGE8B_P1F_O2_ACCOUNT_ALIAS, STAGE8B_P1F_O2_ACCOUNT_TEMPLATE_SENTINEL,
};
use super::*;
use broker_finam::{
    AccountOrdersResponse, AccountResponse, AssetParamsResponse, AssetScheduleResponse,
    BarsResponse, Stage8bP1fO2GetObservationV1, Stage8bP1fO2GetRouteKindV1,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

// V4 must preserve receipt <= capture <= trusted_now at the original precision.
// The shared legacy formatter intentionally emits whole seconds; do not use it
// here or rewrite the exact retained receipt to compensate for truncation.
fn observation_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

fn source_validation_error(
    error: runtime_durable_service::Stage8bP1eFirstBootSourceError,
) -> Stage8bP1fO2MaterializerErrorV1 {
    use runtime_durable_service::Stage8bP1eFirstBootSourceError as E;
    // Exhaustive, static allowlist: never include source JSON, accounts, paths,
    // HTTP errors or response strings in the public diagnostic.
    let code = match error {
        E::InvalidFileBoundary => "source_invalid_file_boundary",
        E::SourceTooLarge => "source_too_large",
        E::SourceReadFailed => "source_read_failed",
        E::SourceHashMismatch => "source_hash_mismatch",
        E::InvalidJson => "source_invalid_json",
        E::DuplicateJsonKey => "source_duplicate_json_key",
        E::InvalidSchema => "source_invalid_schema",
        E::IdentityMismatch => "source_identity_mismatch",
        E::InvalidBrokerTruth => "source_invalid_broker_truth",
        E::InvalidHistory => "source_invalid_history",
        E::InvalidRiskGateHistory => "source_invalid_riskgate_history",
        E::InvalidCandidate => "source_invalid_candidate",
    };
    Stage8bP1fO2MaterializerErrorV1::detailed("canonical_validation", code)
}

impl Stage8bP1fObservedM10Materialization {
    /// The caller retains `snapshot().evidence()` exactly once alongside this
    /// result; the authenticated source bundle contains the normalized receipt
    /// and its raw snapshot link. No runtime entrypoint is switched by assembly.
    #[allow(clippy::too_many_arguments)]
    pub fn first_boot_source_v4(
        &self,
        template_bytes: &[u8],
        expected_account_id: &str,
        account: AccountResponse,
        orders: AccountOrdersResponse,
        params: AssetParamsResponse,
        schedule: AssetScheduleResponse,
        observations: Vec<Stage8bP1fO2GetObservationV1>,
        trusted_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fO2MaterializedSourceV1, Stage8bP1fO2MaterializerErrorV1> {
        use runtime_durable_service as durable;
        if sha256_hex(template_bytes) != self.template_sha256 {
            return Err(Stage8bP1fO2MaterializerErrorV1::Template);
        }
        let plan = Stage8bP1fObservedM10Plan::from_calendar_template(
            template_bytes,
            &self.operational_identity_sha256,
            trusted_now,
        )
        .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Template)?;
        if plan.calendar_sha256() != self.calendar_sha256
            || plan.request_end() != self.candidate.bar().close_ts
        {
            return Err(Stage8bP1fO2MaterializerErrorV1::BarsTruth);
        }
        self.snapshot
            .candidate_at(plan.request_end(), trusted_now)
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::BarsTruth)?;
        let mut source: Value = serde_json::from_slice(template_bytes)
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Template)?;
        if source["broker_truth"]["account_id"] != STAGE8B_P1F_O2_ACCOUNT_TEMPLATE_SENTINEL
            || account.account_id != expected_account_id
        {
            return Err(Stage8bP1fO2MaterializerErrorV1::AccountTruth);
        }
        validate_flat_target(&account)?;
        validate_zero_active_orders(expected_account_id, &orders)?;
        validate_params(expected_account_id, &params)?;
        validate_schedule(&schedule)?;
        // No detached summary hash or second response may replace the raw
        // Bars observations used by this already-admitted materialization.
        let observed_bars = observations
            .iter()
            .filter(|o| o.route == Stage8bP1fO2GetRouteKindV1::Bars)
            .collect::<Vec<_>>();
        let parts = &self.snapshot.evidence().parts;
        let expected_observations =
            broker_finam::Stage8bP1fO2GetOnlyClientV1::new(expected_account_id)
                .and_then(|client| {
                    client.observations_for_closed_m1_snapshot(self.snapshot.evidence())
                })
                .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Get)?;
        if observed_bars.len() != parts.len()
            || observed_bars
                .iter()
                .zip(&expected_observations)
                .any(|(a, b)| **a != *b)
            || observed_bars.iter().zip(parts).any(|(o, p)| {
                o.exact_response_bytes != p.raw_body.as_bytes()
                    || o.response_sha256 != p.response_sha256
            })
        {
            return Err(Stage8bP1fO2MaterializerErrorV1::Get);
        }
        let mut combined = BTreeMap::new();
        for part in parts {
            let response: BarsResponse = serde_json::from_str(&part.raw_body)
                .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Get)?;
            for bar in response.bars {
                // Conflicts were rejected by full raw admission, and are
                // checked again by the shared route validator below.
                combined.insert(bar.timestamp.clone(), bar);
            }
        }
        validate_route_observations(
            &observations,
            &account,
            &orders,
            &params,
            &schedule,
            &BarsResponse {
                symbol: "IMOEXF@RTSX".into(),
                bars: combined.into_values().collect(),
            },
        )?;

        let candidate_bytes = self
            .candidate_canonical_bytes()
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::BarsTruth)?;
        let canonical = self
            .validate_candidate_canonical(&candidate_bytes)
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::BarsTruth)?;
        let wire: Value = serde_json::from_slice(&candidate_bytes)
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::BarsTruth)?;
        let history = self
            .history
            .iter()
            .map(|b| bar_value(b.bar(), "history"))
            .collect::<Vec<_>>();
        let sessions = short_history_coverage(&source, self.candidate.bar().close_ts.timestamp())?;
        let history_hash = canonical_value_sha256(&json!(history));
        source["schema_version"] = json!(4);
        source["domain"] = json!(durable::STAGE8B_P1E_FIRST_BOOT_SOURCE_V4_DOMAIN);
        source["captured_at_utc"] = json!(observation_timestamp(trusted_now));
        source["broker_truth"] = json!({
            "checked_at_utc":observation_timestamp(trusted_now), "account_id":STAGE8B_P1F_O2_ACCOUNT_ALIAS,
            "instrument":"IMOEXF@RTSX", "target_position_qty":"0", "target_positions_complete":true,
            "target_active_orders_count":0, "account_active_orders_count":0, "active_orders_complete":true,
            "instrument_price_step":"0.5"
        });
        source["history_provenance"] = json!({
            "source_mode":OBSERVED_M1_POLICY_V1, "source_timeframe_sec":60, "target_timeframe_sec":600,
            "aggregation_complete":true, "gap_absence_proven":false
        });
        source["history_bars"] = json!(history);
        source["history_coverage"]["sessions_sha256"] = json!(canonical_value_sha256(&sessions));
        source["history_coverage"]["sessions"] = sessions;
        source["riskgate_history"] = json!({
            "source_mode":"disabled-bo-only-v1", "state_generation":"disabled-v1",
            "history_bars_sha256":history_hash, "session_observations":[],
            "session_observations_sha256":canonical_value_sha256(&json!([]))
        });
        let mut candidate = bar_value(self.candidate.bar(), "replay");
        for key in ["open_ts_utc_ms", "close_ts_utc_ms", "source_m1"] {
            candidate[key] = wire["payload"][key].clone();
        }
        candidate["redis_id"] = json!(canonical.redis_id());
        candidate["semantic_id_sha256"] = json!(canonical.semantic_id_sha256());
        candidate["payload_sha256"] = json!(canonical.payload_sha256());
        source["candidate"] = candidate;
        let receipt = self.snapshot.receipt();
        source["observed_source"] = json!({
            "policy":OBSERVED_M1_POLICY_V1, "source_plan_sha256":durable::STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256,
            "receipt_sha256":receipt.sha256(),
            "receipt":std::str::from_utf8(receipt.retained_bytes()).map_err(|_| Stage8bP1fO2MaterializerErrorV1::SourceRejected)?
        });
        let exact_source_bytes = serde_json::to_vec(&source)
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::SourceRejected)?;
        let source_bundle_sha256 = sha256_hex(&exact_source_bytes);
        // Assembly self-check only. Operational admission must bind this SHA
        // through accepted config/seals, not take it from the returned payload.
        durable::validate_stage8b_p1e_observed_first_boot_source_v4(
            &exact_source_bytes,
            &source_bundle_sha256,
            &self.operational_identity_sha256,
            STAGE8B_P1F_O2_ACCOUNT_ALIAS,
            trusted_now,
        )
        .map_err(source_validation_error)?;
        Ok(Stage8bP1fO2MaterializedSourceV1 {
            exact_source_bytes,
            evidence: Stage8bP1fO2MaterializationEvidenceV1 {
                schema_version: 2,
                domain: "stage8b-p1f-o2-observed-materialization-evidence-v2".into(),
                captured_at_utc: observation_timestamp(trusted_now),
                account_id_sha256: sha256_hex(expected_account_id.as_bytes()),
                venue_symbol: "IMOEXF@RTSX".into(),
                target_position_qty: "0".into(),
                target_active_orders_count: 0,
                account_active_orders_count: 0,
                active_orders_complete: true,
                selected_m1_count: self.candidate.actual_m1().len(),
                candidate_redis_id: canonical.redis_id().into(),
                candidate_semantic_id_sha256: canonical.semantic_id_sha256().into(),
                source_bundle_sha256,
                route_evidence: observations
                    .into_iter()
                    .map(|o| Stage8bP1fO2RouteEvidenceV1 {
                        route: format!("{:?}", o.route),
                        request_sha256: o.request_sha256,
                        response_sha256: o.response_sha256,
                    })
                    .collect(),
            },
        })
    }

    /// Fresh replay validation against the protected template and the raw
    /// snapshot, not just self-consistent hashes from staged source JSON.
    pub fn validate_retained_source(
        &self,
        template_bytes: &[u8],
        source_bytes: &[u8],
        evidence: &Stage8bP1fO2MaterializationEvidenceV1,
        expected_account_id: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<(), Stage8bP1fO2MaterializerErrorV1> {
        let invalid = || Stage8bP1fO2MaterializerErrorV1::SourceRejected;
        if sha256_hex(template_bytes) != self.template_sha256 {
            return Err(invalid());
        }
        let source: Value = serde_json::from_slice(source_bytes).map_err(|_| invalid())?;
        let template: Value = serde_json::from_slice(template_bytes).map_err(|_| invalid())?;
        runtime_durable_service::validate_stage8b_p1e_observed_first_boot_source_v4(
            source_bytes,
            &sha256_hex(source_bytes),
            &self.operational_identity_sha256,
            STAGE8B_P1F_O2_ACCOUNT_ALIAS,
            trusted_now,
        )
        .map_err(source_validation_error)?;
        for key in [
            "operational_identity_sha256",
            "runtime_profile_sha256",
            "instrument_map_fingerprint_sha256",
            "source_bundle_generation",
        ] {
            if source[key] != template[key] {
                return Err(invalid());
            }
        }
        let sessions =
            short_history_coverage(&template, self.candidate.bar().close_ts.timestamp())?;
        let history = self
            .history
            .iter()
            .map(|b| bar_value(b.bar(), "history"))
            .collect::<Vec<_>>();
        let candidate_bytes = self.candidate_canonical_bytes().map_err(|_| invalid())?;
        let canonical = self
            .validate_candidate_canonical(&candidate_bytes)
            .map_err(|_| invalid())?;
        let receipt = self.snapshot.receipt();
        if source["history_coverage"]["sessions"] != sessions
            || source["history_coverage"]["candidate_session"]
                != template["history_coverage"]["candidate_session"]
            || source["history_bars"] != json!(history)
            || source["observed_source"]["receipt_sha256"] != receipt.sha256()
            || source["observed_source"]["receipt"]
                .as_str()
                .map(str::as_bytes)
                != Some(receipt.retained_bytes())
            || source["candidate"]["payload_sha256"] != canonical.payload_sha256()
            || evidence.schema_version != 2
            || evidence.domain != "stage8b-p1f-o2-observed-materialization-evidence-v2"
            || evidence.captured_at_utc != source["captured_at_utc"].as_str().ok_or_else(invalid)?
            || evidence.account_id_sha256 != sha256_hex(expected_account_id.as_bytes())
            || evidence.source_bundle_sha256 != sha256_hex(source_bytes)
            || evidence.venue_symbol != "IMOEXF@RTSX"
            || evidence.target_position_qty != "0"
            || evidence.target_active_orders_count != 0
            || evidence.account_active_orders_count != 0
            || !evidence.active_orders_complete
            || evidence.selected_m1_count != self.candidate.actual_m1().len()
            || evidence.candidate_redis_id != canonical.redis_id()
            || evidence.candidate_semantic_id_sha256 != canonical.semantic_id_sha256()
        {
            return Err(invalid());
        }
        // Preserve all five route kinds exactly once (plus planned Bars parts).
        let bars = broker_finam::Stage8bP1fO2GetOnlyClientV1::new(expected_account_id)
            .and_then(|client| client.observations_for_closed_m1_snapshot(self.snapshot.evidence()))
            .map_err(|_| invalid())?;
        if evidence.route_evidence.len() != 4 + bars.len() {
            return Err(invalid());
        }
        for (row, kind) in evidence.route_evidence.iter().take(4).zip([
            "Account",
            "AccountOrders",
            "AssetParams",
            "AssetSchedule",
        ]) {
            if row.route != kind
                || !super::super::valid_sha256(&row.request_sha256)
                || !super::super::valid_sha256(&row.response_sha256)
            {
                return Err(invalid());
            }
        }
        for (row, bar) in evidence.route_evidence.iter().skip(4).zip(bars) {
            if row.route != "Bars"
                || row.request_sha256 != bar.request_sha256
                || row.response_sha256 != bar.response_sha256
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

fn bar_value(bar: &broker_core::event::Bar, origin: &str) -> Value {
    json!({"instrument":"IMOEXF@RTSX", "timeframe_sec":600, "close_time_utc":bar.close_ts.timestamp(),
        "open":bar.open.normalize().to_string(), "high":bar.high.normalize().to_string(), "low":bar.low.normalize().to_string(),
        "close":bar.close.normalize().to_string(), "volume":bar.volume.normalize().to_string(), "is_final":true, "origin":origin})
}
