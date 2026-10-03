//! Additive calendar-bound observed-M1 materialization.
//!
//! This is the shared History/candidate input stage, NOT a legacy wire-V3
//! source bundle. It cannot be serialized as one and does not start a runtime
//! or publication. The fixed materializer requires explicit policy V3 opt-in;
//! no installed config is switched. The strict legacy materializer is unchanged.

use broker_finam::sparse_m10::{
    AdmittedClosedM1Snapshot, ClosedM1RequestPlan, ClosedM1SnapshotError,
    ClosedM1SnapshotEvidenceV1, ObservedM10Bucket, OBSERVED_M1_POLICY_V1,
};
use chrono::{DateTime, Duration, Utc};

mod collection;
mod source;
pub use collection::Stage8bP1fObservedCollectedSource;

use super::{
    canonical_value_sha256, sha256_hex, template_profile, CoverageSessionV1, SourceTemplate,
    Stage8bP1RuntimeProfileKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1fObservedM10Error {
    #[error("observed-M1 requires an exact no-riskgate calendar template")]
    CalendarTemplate,
    #[error("calendar has no fresh closed candidate; data fallback is forbidden")]
    CalendarCandidate,
    #[error("observed-M1 plan does not match the expected operational identity")]
    OperationalIdentity,
    #[error("observed-M1 guarded GET did not complete")]
    Transport,
    #[error("observed-M1 snapshot rejected: {0}")]
    Snapshot(#[from] ClosedM1SnapshotError),
}

/// Preflight uses only the reviewed calendar and trusted clock. In particular
/// the candidate and request endpoint never depend on which M1 were received.
pub struct Stage8bP1fObservedM10Plan {
    template_sha256: String,
    operational_identity_sha256: String,
    calendar_sha256: String,
    request_plan: ClosedM1RequestPlan,
    start: DateTime<Utc>,
    candidate_close: DateTime<Utc>,
    history_closes: Vec<DateTime<Utc>>,
}

impl Stage8bP1fObservedM10Plan {
    /// V3 is read ONLY as the existing calendar/config template. No V3 sparse
    /// source bytes are emitted: the eventual source wire must be additive.
    pub fn from_calendar_template(
        template_bytes: &[u8],
        expected_operational_identity_sha256: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<Self, Stage8bP1fObservedM10Error> {
        let invalid = || Stage8bP1fObservedM10Error::CalendarTemplate;
        if template_profile(template_bytes).map_err(|_| invalid())?
            != Stage8bP1RuntimeProfileKind::V2
        {
            return Err(invalid());
        }
        let template: SourceTemplate =
            serde_json::from_slice(template_bytes).map_err(|_| invalid())?;
        if template.operational_identity_sha256 != expected_operational_identity_sha256 {
            return Err(Stage8bP1fObservedM10Error::OperationalIdentity);
        }
        let current = template
            .history_coverage
            .candidate_session
            .ok_or_else(invalid)?;
        let current_closes = session_closes(&current)?;
        let candidate_close = current_closes
            .iter()
            .copied()
            .filter(|close| *close <= trusted_now)
            .max()
            .ok_or(Stage8bP1fObservedM10Error::CalendarCandidate)?;
        if trusted_now - candidate_close > Duration::seconds(super::MAX_CANDIDATE_AGE_SECONDS) {
            return Err(Stage8bP1fObservedM10Error::CalendarCandidate);
        }
        let mut history_closes = Vec::new();
        for session in &template.history_coverage.sessions {
            history_closes.extend(session_closes(session)?);
        }
        history_closes.extend(
            current_closes
                .into_iter()
                .filter(|close| *close < candidate_close),
        );
        if history_closes.is_empty()
            || history_closes.windows(2).any(|pair| pair[0] >= pair[1])
            || history_closes
                .last()
                .is_some_and(|close| *close >= candidate_close)
        {
            return Err(invalid());
        }
        let start = history_closes[0]
            .checked_sub_signed(Duration::seconds(600))
            .ok_or_else(invalid)?;
        // Existing calendar authority allows a holiday-spanning history, but
        // a fresh M1 request must still fit the provider's documented depth.
        // Never hide older anchors by shrinking the requested history.
        if trusted_now - start > Duration::days(7) {
            return Err(ClosedM1SnapshotError::Freshness.into());
        }
        let request_plan = ClosedM1RequestPlan::new(start, candidate_close, vec![candidate_close])?;
        let calendar_sha256 = canonical_value_sha256(&serde_json::json!({
            "policy": OBSERVED_M1_POLICY_V1,
            "sessions": template.history_coverage.sessions,
            "candidate_session": current,
            "selected_candidate_close": candidate_close,
        }));
        Ok(Self {
            template_sha256: sha256_hex(template_bytes),
            operational_identity_sha256: template.operational_identity_sha256,
            calendar_sha256,
            request_plan,
            start,
            candidate_close,
            history_closes,
        })
    }

    pub fn request_start(&self) -> DateTime<Utc> {
        self.start
    }
    pub fn request_end(&self) -> DateTime<Utc> {
        self.candidate_close
    }
    pub fn request_plan(&self) -> &ClosedM1RequestPlan {
        &self.request_plan
    }
    pub fn calendar_sha256(&self) -> &str {
        &self.calendar_sha256
    }
    pub fn template_sha256(&self) -> &str {
        &self.template_sha256
    }
    pub fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    /// Explicit opt-in GET path. Legacy policy callers remain unchanged.
    /// The provider client retains actual completion timestamps and raw bytes;
    /// the plan remains the independently established calendar authority.
    pub async fn collect(
        &self,
        client: &broker_finam::Stage8bP1fO2GetOnlyClientV1,
        token: &broker_finam::AccessToken,
    ) -> Result<Stage8bP1fObservedM10Materialization, Stage8bP1fObservedM10Error> {
        let evidence = client
            .fetch_closed_m1_snapshot(token, &self.request_plan)
            .await
            .map_err(|_| Stage8bP1fObservedM10Error::Transport)?;
        self.materialize(evidence, Utc::now())
    }

    /// All buckets are derived from ONE admitted snapshot. No old dense
    /// aggregator or data-driven window search participates in this path.
    pub fn materialize(
        &self,
        evidence: ClosedM1SnapshotEvidenceV1,
        trusted_now: DateTime<Utc>,
    ) -> Result<Stage8bP1fObservedM10Materialization, Stage8bP1fObservedM10Error> {
        let snapshot = AdmittedClosedM1Snapshot::admit(evidence, &self.request_plan, trusted_now)?;
        let candidate = snapshot.candidate_at(self.candidate_close, trusted_now)?;
        let history = self
            .history_closes
            .iter()
            .map(|close| {
                let open = close
                    .checked_sub_signed(Duration::seconds(600))
                    .ok_or(ClosedM1SnapshotError::Range)?;
                snapshot.bucket(open)
            })
            .collect::<Result<Vec<_>, ClosedM1SnapshotError>>()?;
        Ok(Stage8bP1fObservedM10Materialization {
            snapshot,
            history,
            candidate,
            calendar_sha256: self.calendar_sha256.clone(),
            template_sha256: self.template_sha256.clone(),
            operational_identity_sha256: self.operational_identity_sha256.clone(),
        })
    }
}

fn session_closes(
    session: &CoverageSessionV1,
) -> Result<Vec<DateTime<Utc>>, Stage8bP1fObservedM10Error> {
    session
        .windows
        .iter()
        .flat_map(|w| (w.first_close_time_utc..=w.last_close_time_utc).step_by(600))
        .map(|close| {
            DateTime::from_timestamp(close, 0).ok_or(Stage8bP1fObservedM10Error::CalendarTemplate)
        })
        .collect()
}

/// Retains raw snapshot once; each bucket carries only its binding plus actual
/// rows. Private fields/no Deserialize prevent replacement of the selected set.
pub struct Stage8bP1fObservedM10Materialization {
    snapshot: AdmittedClosedM1Snapshot,
    history: Vec<ObservedM10Bucket>,
    candidate: ObservedM10Bucket,
    calendar_sha256: String,
    template_sha256: String,
    operational_identity_sha256: String,
}

impl Stage8bP1fObservedM10Materialization {
    /// Builds the candidate only from the full raw-admitted source. No custom
    /// selected-M1 vector, aggregate or alternate candidate enters this seam.
    pub fn candidate_canonical_bytes(
        &self,
    ) -> Result<Vec<u8>, runtime_durable_service::Stage8bP1CanonicalM10Error> {
        runtime_durable_service::build_stage8b_p1_observed_canonical_m10(
            &self.operational_identity_sha256,
            self.candidate.bar().open_ts.timestamp_millis(),
            self.snapshot.receipt(),
        )
    }

    /// Context is supplied by this materialization, NOT by the candidate.
    /// Exact nominal selection prevents substitution of another source bucket.
    pub fn validate_candidate_canonical(
        &self,
        bytes: &[u8],
    ) -> Result<
        runtime_durable_service::Stage8bP1ValidatedCanonicalM10,
        runtime_durable_service::Stage8bP1CanonicalM10Error,
    > {
        let validated = runtime_durable_service::parse_stage8b_p1_observed_canonical_m10(
            bytes,
            &self.operational_identity_sha256,
            self.snapshot.receipt(),
            self.snapshot.receipt().sha256(),
        )?;
        if validated.open_ts_utc_ms() != self.candidate.bar().open_ts.timestamp_millis()
            || validated.close_ts_utc_ms() != self.candidate.bar().close_ts.timestamp_millis()
        {
            return Err(runtime_durable_service::Stage8bP1CanonicalM10Error::InvalidChronology);
        }
        Ok(validated)
    }

    pub fn snapshot(&self) -> &AdmittedClosedM1Snapshot {
        &self.snapshot
    }
    pub fn history(&self) -> &[ObservedM10Bucket] {
        &self.history
    }
    pub fn candidate(&self) -> &ObservedM10Bucket {
        &self.candidate
    }
    pub fn calendar_sha256(&self) -> &str {
        &self.calendar_sha256
    }
    pub fn template_sha256(&self) -> &str {
        &self.template_sha256
    }
    pub fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }
}

#[cfg(test)]
mod tests;
