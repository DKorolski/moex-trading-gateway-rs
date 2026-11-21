//! Per-decision synthetic paper permission, separate from historical O2 receipts.
//! No bootstrap notification, runtime reset, IO, signing or broker transport.
use broker_core::*;
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Stage5cPaperHostAdmission, Stage5cPaperHostAdmissionInput};

const DOMAIN: &str = "moex.stage8b.o3.paper-decision-proof.v1";
const FRESHNESS_MS: u64 = 60_000;
const MAX_PROOF: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bO3PaperScope {
    pub phase_id: String,
    pub phase_sha256: String,
    pub config_sha256: String,
    pub plan_sha256: String,
    pub fixture_sha256: String,
    pub operational_identity_sha256: String,
    pub runtime_config_fingerprint_sha256: String,
    pub strategy_id: String,
    pub not_before: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
    /// Same explicit session-window representation as accepted V4 History.
    /// The enclosing exact plan hash binds it; missing calendar days are not inferred.
    pub catchup_coverage: Stage8bO3Coverage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bO3Coverage {
    pub source_mode: String,
    pub sessions: Vec<Stage8bO3CoverageSession>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bO3CoverageSession {
    pub session_date: String,
    pub windows: Vec<Stage8bO3CoverageWindow>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bO3CoverageWindow {
    // Legacy History field names; these are model candle labels (opening UTC).
    pub first_close_time_utc: i64,
    pub last_close_time_utc: i64,
}
impl Stage8bO3Coverage {
    pub(crate) fn validate(&self) -> bool {
        let mut last_date = None;
        self.source_mode == "config-bound-explicit-session-windows-v1"
            && !self.sessions.is_empty()
            && self.sessions.len() <= 5
            && self.sessions.iter().all(|s| {
                let Ok(date) = chrono::NaiveDate::parse_from_str(&s.session_date, "%Y-%m-%d")
                else {
                    return false;
                };
                if date.to_string() != s.session_date || last_date.is_some_and(|d| date <= d) {
                    return false;
                }
                last_date = Some(date);
                let mut end = 0;
                s.windows.iter().all(|w| {
                    let a = w.first_close_time_utc;
                    let b = w.last_close_time_utc;
                    let valid = a > end
                        && a % 600 == 0
                        && b >= a
                        && b % 600 == 0
                        && Self::date(a).as_deref() == Some(&s.session_date)
                        && Self::date(b).as_deref() == Some(&s.session_date);
                    end = b;
                    valid
                })
            })
    }
    fn date(ts: i64) -> Option<String> {
        DateTime::from_timestamp(ts, 0)
            .map(|t| (t + chrono::Duration::hours(3)).date_naive().to_string())
    }
    /// None means unknown date; an explicitly listed empty day is closed.
    pub(crate) fn contains(&self, label: i64) -> Option<bool> {
        let date = Self::date(label)?;
        let s = self.sessions.iter().find(|s| s.session_date == date)?;
        Some(
            s.windows
                .iter()
                .any(|w| label >= w.first_close_time_utc && label <= w.last_close_time_utc),
        )
    }
}

/// Data to be checked against the authenticated owner, never authority by itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bO3DecisionBinding {
    pub source_seal_generation: u64,
    pub source_seal_commitment_sha256: String,
    pub source_checkpoint_sha256: String,
    pub source_frontier_sha256: String,
    pub m10_redis_id: String,
    pub m10_semantic_id_sha256: String,
    pub m10_payload_sha256: String,
    pub observed_source_sha256: String,
    pub prior_model_label: i64,
    pub candidate_model_label: i64,
}

/// Created from an already phase/fixture-bound O3 plan by the composition.
/// Does not authorize any state transition until joined to an authenticated owner.
#[derive(Clone)]
pub struct Stage8bO3PaperSource {
    scope: Stage8bO3PaperScope,
    fixture: BrokerTruthSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Proof {
    domain: String,
    pub(crate) scope: Stage8bO3PaperScope,
    fixture: BrokerTruthSnapshot,
    pub(crate) binding: Stage8bO3DecisionBinding,
    observed_at: DateTime<Utc>,
    issued_at: DateTime<Utc>,
    pub(crate) decision_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

/// Opaque, one-shot. Neither a PASS report nor a schedule signature can make it.
pub struct Stage8bO3PaperPermit {
    proof: Proof,
    admission: Stage5cPaperHostAdmission,
    bytes: Vec<u8>,
    replay: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("O3 fresh paper admission or exact replay evidence rejected")]
pub struct Stage8bO3ReadmissionError;
type E = Stage8bO3ReadmissionError;

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, E> {
    serde_json::to_vec(value).map_err(|_| Stage8bO3ReadmissionError)
}

impl Stage8bO3PaperSource {
    pub fn new(scope: Stage8bO3PaperScope, fixture: BrokerTruthSnapshot) -> Result<Self, E> {
        if ![
            &scope.phase_sha256,
            &scope.config_sha256,
            &scope.plan_sha256,
            &scope.fixture_sha256,
            &scope.operational_identity_sha256,
            &scope.runtime_config_fingerprint_sha256,
        ]
        .into_iter()
        .all(|s| hash(s))
            || scope.phase_id.is_empty()
            || scope.strategy_id.is_empty()
            || scope.not_before >= scope.deadline
            || !scope.catchup_coverage.validate()
            || scope.deadline - scope.not_before > chrono::Duration::minutes(30)
            || digest(&encode(&fixture)?) != scope.fixture_sha256
            || !fixture.orders.is_empty()
            || !fixture.positions.is_empty()
            || !fixture.trades.is_empty()
            || fixture.cash.is_some()
            || fixture.account_id.as_str().is_empty()
            || fixture.instruments.len() != 1
        {
            return Err(Stage8bO3ReadmissionError);
        }
        let spec = &fixture.instruments[0];
        let id = spec.instrument_id();
        if spec.broker_asset_id.as_deref() != Some("SYNTHETIC-O3")
            || id.symbol != "IMOEXF"
            || id.venue_symbol.as_deref() != Some("IMOEXF@RTSX")
            || id.exchange != Exchange::Moex
            || id.market != Market::Futures
            || spec.instrument.price_step.to_f64() != Some(0.5)
        {
            return Err(Stage8bO3ReadmissionError);
        }
        Ok(Self { scope, fixture })
    }

    pub fn scope(&self) -> &Stage8bO3PaperScope {
        &self.scope
    }

    /// Production wall clock; the original fixture is not retimestamped in place.
    pub fn issue(&self, binding: Stage8bO3DecisionBinding) -> Result<Stage8bO3PaperPermit, E> {
        let observed = Utc::now();
        self.issue_at(binding, observed, Utc::now(), true)
    }

    fn issue_at(
        &self,
        binding: Stage8bO3DecisionBinding,
        observed_at: DateTime<Utc>,
        decision_at: DateTime<Utc>,
        wall_clock: bool,
    ) -> Result<Stage8bO3PaperPermit, E> {
        let admission = self.admit(observed_at, decision_at, wall_clock)?;
        let expires_at = admission.expires_at().min(self.scope.deadline);
        let issued_at = admission.issued_ts();
        let decision_at = decision_at.max(issued_at);
        let proof = Proof {
            domain: DOMAIN.into(),
            scope: self.scope.clone(),
            fixture: self.fixture.clone(),
            binding,
            observed_at,
            issued_at,
            decision_at,
            expires_at,
        };
        proof.validate_shape()?;
        let bytes = encode(&proof)?;
        Ok(Stage8bO3PaperPermit {
            proof,
            admission,
            bytes,
            replay: false,
        })
    }

    fn admit(
        &self,
        observed_at: DateTime<Utc>,
        decision_at: DateTime<Utc>,
        wall_clock: bool,
    ) -> Result<Stage5cPaperHostAdmission, E> {
        if observed_at < self.scope.not_before
            || observed_at > decision_at
            || decision_at > self.scope.deadline
        {
            return Err(Stage8bO3ReadmissionError);
        }
        // A NEW explicitly synthetic observation. Never retimestamp real O2 truth.
        let mut truth = self.fixture.clone();
        truth.received_ts = observed_at;
        let spec = &truth.instruments[0];
        let instrument = spec.instrument_id();
        let mut freshness = Stage4BrokerTruthFreshnessInput::from_broker_truth_received_ts(
            observed_at,
            FRESHNESS_MS,
        );
        freshness.schedule =
            Stage4BrokerTruthFreshnessProbe::fresh(observed_at, FRESHNESS_MS, true);
        let validated = validate_stage4_broker_truth_bootstrap(Stage4BrokerTruthBootstrapInput {
            broker_truth: &truth,
            broker_truth_source_status: Stage4BrokerTruthSourceStatus::Present,
            target_instrument: instrument.clone(),
            restored_runtime_state: None,
            freshness,
            schedule_state: BrokerMarketSessionState::Open,
            adoption: Stage4AdoptionDisposition::default(),
            external_issues: Vec::new(),
            safety_boundary: Stage4BrokerTruthSafetyBoundary::closed(),
            checked_ts: observed_at,
        });
        let sections = validated
            .freshness
            .sections
            .iter()
            .map(|section| Stage4BootstrapEvidenceSourceStatusSection {
                section: section.section,
                source_status: Stage4BrokerTruthSourceStatus::Present,
                required_for_bootstrap: section.required_for_bootstrap,
            })
            .collect::<Vec<_>>();
        let stage4_evidence = build_stage4_accepted_paper_host_evidence(&validated, &sections)
            .map_err(|_| Stage8bO3ReadmissionError)?;
        let input = Stage5cPaperHostAdmissionInput {
            stage4_evidence,
            strategy_id: self.scope.strategy_id.clone(),
            instrument_spec: spec,
            configured_account_id: &truth.account_id,
            configured_target_instrument: &instrument,
            configured_tick_size: 0.5,
            allow_live_orders: false,
        };
        if wall_clock {
            crate::admit_stage5c_paper_host(input)
        } else {
            crate::stage5c_paper_host::admit_stage5c_paper_host_at(input, decision_at)
        }
        .map_err(|_| Stage8bO3ReadmissionError)
    }
}

impl Proof {
    fn validate_shape(&self) -> Result<(), E> {
        let b = &self.binding;
        if self.domain != DOMAIN
            || b.source_seal_generation == 0
            || ![
                &b.source_seal_commitment_sha256,
                &b.source_checkpoint_sha256,
                &b.source_frontier_sha256,
                &b.m10_semantic_id_sha256,
                &b.m10_payload_sha256,
                &b.observed_source_sha256,
            ]
            .into_iter()
            .all(|s| hash(s))
            || b.prior_model_label <= 0
            || b.prior_model_label.rem_euclid(600) != 0
            || b.candidate_model_label <= b.prior_model_label
            || b.candidate_model_label.rem_euclid(600) != 0
            || self
                .scope
                .catchup_coverage
                .contains(b.candidate_model_label)
                != Some(true)
            || b.candidate_model_label
                .checked_add(600)
                .and_then(|t| t.checked_mul(1000))
                .map(|t| format!("{t}-0"))
                .as_deref()
                != Some(&b.m10_redis_id)
            || self.decision_at.timestamp() < b.candidate_model_label + 600
            || self.observed_at > self.issued_at
            || self.issued_at > self.decision_at
            || self.decision_at > self.expires_at
            || self.expires_at > self.scope.deadline
        {
            return Err(Stage8bO3ReadmissionError);
        }
        Ok(())
    }
}

impl Stage8bO3PaperPermit {
    pub fn retained_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn sha256(&self) -> String {
        digest(&self.bytes)
    }
    pub fn binding(&self) -> &Stage8bO3DecisionBinding {
        &self.proof.binding
    }
    pub fn scope(&self) -> &Stage8bO3PaperScope {
        &self.proof.scope
    }
    pub(crate) fn admission(&self) -> &Stage5cPaperHostAdmission {
        &self.admission
    }
    pub(crate) fn decision_at(&self) -> DateTime<Utc> {
        self.proof.decision_at
    }
    pub(crate) fn proof(&self) -> &Proof {
        &self.proof
    }

    pub(crate) fn observe_callback_attempt(&self) {
        observe_attempt(if self.replay { 2 } else { 1 });
    }

    pub(crate) fn authorize_recorded_replay(&mut self) -> Result<(), E> {
        self.replay = true;
        self.execution_time().map(|_| ())
    }

    /// Real time always bounds phase authority. Only a protected journal match
    /// can select replay; pre-append expiry is never inferred from candle time.
    pub(crate) fn execution_time(&self) -> Result<DateTime<Utc>, E> {
        self.execution_time_at(Utc::now())
    }

    fn execution_time_at(&self, now: DateTime<Utc>) -> Result<DateTime<Utc>, E> {
        if now < self.proof.decision_at
            || now > self.proof.scope.deadline
            || (!self.replay && now > self.proof.expires_at)
        {
            return Err(Stage8bO3ReadmissionError);
        }
        Ok(self.proof.decision_at)
    }

    /// Retained pre-append attempt: expiry is checked against the real clock.
    pub fn restore_pending(bytes: &[u8], expected: &Stage8bO3PaperScope) -> Result<Self, E> {
        let permit = Self::reconstruct(bytes, expected)?;
        permit.execution_time()?;
        Ok(permit)
    }

    // Only a core journal/source-evidence match may authorize expired replay.
    pub(crate) fn reconstruct(bytes: &[u8], expected: &Stage8bO3PaperScope) -> Result<Self, E> {
        if bytes.len() > MAX_PROOF {
            return Err(Stage8bO3ReadmissionError);
        }
        let proof: Proof = serde_json::from_slice(bytes).map_err(|_| Stage8bO3ReadmissionError)?;
        if encode(&proof)? != bytes || &proof.scope != expected {
            return Err(Stage8bO3ReadmissionError);
        }
        proof.validate_shape()?;
        let source = Stage8bO3PaperSource::new(proof.scope.clone(), proof.fixture.clone())?;
        let admission = source.admit(proof.observed_at, proof.issued_at, false)?;
        if admission.expires_at().min(expected.deadline) != proof.expires_at {
            return Err(Stage8bO3ReadmissionError);
        }
        Ok(Self {
            proof,
            admission,
            bytes: bytes.to_vec(),
            replay: false,
        })
    }
}

impl Proof {
    pub(crate) fn account_id(&self) -> &BrokerAccountId {
        &self.fixture.account_id
    }
    pub(crate) fn sha256(&self) -> String {
        digest(&encode(self).expect("serializable O3 proof"))
    }

    pub(crate) fn validates_commit(
        &self,
        op: &str,
        redis: &str,
        semantic: &str,
        payload: &str,
    ) -> bool {
        self.scope.operational_identity_sha256 == op
            && self.binding.m10_redis_id == redis
            && self.binding.m10_semantic_id_sha256 == semantic
            && self.binding.m10_payload_sha256 == payload
            && encode(self)
                .ok()
                .is_some_and(|bytes| Stage8bO3PaperPermit::reconstruct(&bytes, &self.scope).is_ok())
    }
}

#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
std::thread_local! {
    static ATTEMPTS: std::cell::Cell<[usize;3]> = const { std::cell::Cell::new([0;3]) };
}
/// Test-only actual observation/fresh/replay callback attempts, not logical commits.
#[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
#[doc(hidden)]
pub fn take_o3_callback_attempts() -> [usize; 3] {
    ATTEMPTS.with(|a| a.replace([0; 3]))
}
pub(crate) fn observe_attempt(kind: usize) {
    #[cfg(any(test, feature = "stage5g-artifact-fixtures"))]
    ATTEMPTS.with(|a| {
        let mut v = a.get();
        v[kind] += 1;
        a.set(v);
    });
    #[cfg(not(any(test, feature = "stage5g-artifact-fixtures")))]
    let _ = kind;
}
