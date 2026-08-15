//! Stage 8A-3 endpoint-specific FINAM semantic classifier.
//!
//! Only explicit local PLACE/CANCEL context and synthetic HTTP observations
//! enter this module. It has no client, URL, token or transport.
//!
//! ```compile_fail
//! use finam_gateway::{Stage8a3EndpointContext, Stage8a3LocalHttpObservation};
//! fn raw_body(context: Stage8a3EndpointContext) {
//!     let value = context.classify(Stage8a3LocalHttpObservation::response(200, b"{}".to_vec()));
//!     let _ = value.raw_body();
//! }
//! ```
//!
//! ```compile_fail
//! use finam_gateway::{Stage8a3EndpointContext, Stage8a3LocalHttpObservation};
//! fn raw_order_id(context: Stage8a3EndpointContext) {
//!     let value = context.classify(Stage8a3LocalHttpObservation::timeout());
//!     let _ = value.broker_order_id();
//! }
//! ```
//!
//! ```compile_fail
//! use finam_gateway::{Stage8a3EndpointContext, Stage8a3LocalHttpObservation};
//! fn retry(context: Stage8a3EndpointContext) {
//!     let value = context.classify(Stage8a3LocalHttpObservation::timeout());
//!     let _ = value.retry_authority();
//! }
//! ```
//!
//! ```compile_fail
//! use finam_gateway::{Stage8a3EndpointContext, Stage8a3LocalHttpObservation};
//! fn no_match(context: Stage8a3EndpointContext) {
//!     let value = context.classify(Stage8a3LocalHttpObservation::response(404, vec![]));
//!     let _ = value.proven_no_match();
//! }
//! ```

use broker_core::{AccountId, BrokerOrderId, ClientOrderId, InstrumentId, StrategyRequestId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_CLASSIFIER_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8a3EndpointKind {
    Place,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8a3SemanticCategory {
    PlaceAcceptedCandidate,
    CancelAcceptedCandidate,
    PlaceAuthenticationBlocked,
    PlaceConfigurationOrInstrumentBlocked,
    CancelAuthenticationBlockedReconciliationRequired,
    ReconciliationRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8a3CorrelationState {
    Matched,
    Mismatched,
    Unavailable,
    NotEvaluated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8a3BodyCategory {
    Empty,
    JsonObject,
    JsonNonObject,
    MalformedJson,
    Unavailable,
    Oversized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage8a3ReconciliationReason {
    MissingBrokerOrderId,
    CorrelationMismatch,
    MalformedSuccessBody,
    UndocumentedSuccessStatus,
    UnsafeOrUnknownClientError,
    TransientOrServerStatus,
    UndocumentedStatus,
    BodyReadFailure,
    Timeout,
    Disconnect,
    ResponseLost,
    CancelAlreadyExecuted,
    CancelTargetNotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stage8a3ClassificationDiagnostic {
    pub endpoint: Stage8a3EndpointKind,
    pub category: Stage8a3SemanticCategory,
    pub reconciliation_reason: Option<Stage8a3ReconciliationReason>,
    pub status: Option<u16>,
    pub body_present: bool,
    pub body_len: usize,
    pub body_category: Stage8a3BodyCategory,
    pub body_sha256: Option<String>,
    pub broker_order_id_present: bool,
    pub broker_order_id_len: usize,
    pub correlation: Stage8a3CorrelationState,
    pub accepted_candidate: bool,
    pub reconciliation_required: bool,
    pub authentication_blocked: bool,
    pub configuration_blocked: bool,
    pub classification_binding_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Stage8a3ContextError {
    #[error("Stage 8A-3 account identity must be non-empty")]
    EmptyAccountIdentity,
    #[error("Stage 8A-3 instrument identity must contain a non-empty venue symbol")]
    EmptyInstrumentIdentity,
}

/// Opaque expected endpoint context; it exposes no raw identity getter and has
/// no Debug, Clone or Serialize implementation.
pub struct Stage8a3EndpointContext {
    expected: Stage8a3ExpectedContext,
}

enum Stage8a3ExpectedContext {
    Place {
        request_id: StrategyRequestId,
        client_order_id: ClientOrderId,
        account_id: AccountId,
        venue_symbol: String,
    },
    Cancel {
        source_request_id: StrategyRequestId,
        account_id: AccountId,
        broker_order_id: BrokerOrderId,
        target_client_order_id: Option<ClientOrderId>,
    },
}

impl Stage8a3EndpointContext {
    pub fn for_place(
        request_id: StrategyRequestId,
        client_order_id: ClientOrderId,
        account_id: AccountId,
        instrument: InstrumentId,
    ) -> Result<Self, Stage8a3ContextError> {
        validate_account(&account_id)?;
        let venue_symbol = instrument
            .venue_symbol
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(instrument.symbol);
        if venue_symbol.trim().is_empty() {
            return Err(Stage8a3ContextError::EmptyInstrumentIdentity);
        }
        Ok(Self {
            expected: Stage8a3ExpectedContext::Place {
                request_id,
                client_order_id,
                account_id,
                venue_symbol,
            },
        })
    }

    pub fn for_cancel(
        source_request_id: StrategyRequestId,
        account_id: AccountId,
        broker_order_id: BrokerOrderId,
        target_client_order_id: Option<ClientOrderId>,
    ) -> Result<Self, Stage8a3ContextError> {
        validate_account(&account_id)?;
        Ok(Self {
            expected: Stage8a3ExpectedContext::Cancel {
                source_request_id,
                account_id,
                broker_order_id,
                target_client_order_id,
            },
        })
    }

    pub fn classify(
        self,
        observation: Stage8a3LocalHttpObservation,
    ) -> Stage8a3ClassifiedObservation {
        classify_explicit_context(self.expected, observation.kind)
    }
}

fn validate_account(account_id: &AccountId) -> Result<(), Stage8a3ContextError> {
    if account_id.as_str().trim().is_empty() {
        Err(Stage8a3ContextError::EmptyAccountIdentity)
    } else {
        Ok(())
    }
}

/// Opaque local observation. It cannot hold a request client, URL, token,
/// socket, transport adapter or callback.
pub struct Stage8a3LocalHttpObservation {
    kind: Stage8a3LocalObservationKind,
}

enum Stage8a3LocalObservationKind {
    Response { status: u16, body: Vec<u8> },
    BodyReadFailed { status: Option<u16> },
    Timeout,
    Disconnected,
    ResponseLost,
}

impl Stage8a3LocalHttpObservation {
    pub fn response(status: u16, body: Vec<u8>) -> Self {
        Self {
            kind: Stage8a3LocalObservationKind::Response { status, body },
        }
    }

    pub fn body_read_failed(status: Option<u16>) -> Self {
        Self {
            kind: Stage8a3LocalObservationKind::BodyReadFailed { status },
        }
    }

    pub fn timeout() -> Self {
        Self {
            kind: Stage8a3LocalObservationKind::Timeout,
        }
    }

    pub fn disconnected() -> Self {
        Self {
            kind: Stage8a3LocalObservationKind::Disconnected,
        }
    }

    pub fn response_lost() -> Self {
        Self {
            kind: Stage8a3LocalObservationKind::ResponseLost,
        }
    }
}

/// Opaque observation; only its bounded redacted diagnostic is public.
pub struct Stage8a3ClassifiedObservation {
    disposition: Stage8a3Disposition,
    diagnostic: Stage8a3ClassificationDiagnostic,
}

enum Stage8a3Disposition {
    PlaceAcceptedCandidate {
        broker_order_id: BrokerOrderId,
        request_id: StrategyRequestId,
    },
    CancelAcceptedCandidate {
        broker_order_id: BrokerOrderId,
        source_request_id: StrategyRequestId,
    },
    PlaceAuthenticationBlocked,
    PlaceConfigurationOrInstrumentBlocked,
    CancelAuthenticationBlockedReconciliationRequired,
    ReconciliationRequired(Stage8a3ReconciliationReason),
}

impl Stage8a3ClassifiedObservation {
    pub fn diagnostic(&self) -> &Stage8a3ClassificationDiagnostic {
        // Holding the private disposition is the reason this type is opaque;
        // the reference is intentionally not exposed to the caller.
        let _private_disposition = &self.disposition;
        &self.diagnostic
    }

    pub fn into_diagnostic(self) -> Stage8a3ClassificationDiagnostic {
        let Self {
            disposition: _private_disposition,
            diagnostic,
        } = self;
        diagnostic
    }
}

#[derive(Deserialize)]
struct Stage8a3PlaceSuccessDto {
    order_id: Option<String>,
    order: Stage8a3PlaceOrderCorrelationDto,
}

#[derive(Deserialize)]
struct Stage8a3PlaceOrderCorrelationDto {
    account_id: String,
    symbol: String,
    client_order_id: String,
}

#[derive(Deserialize)]
struct Stage8a3CancelSuccessDto {
    order_id: String,
    order: Stage8a3CancelOrderCorrelationDto,
}

#[derive(Deserialize)]
struct Stage8a3CancelOrderCorrelationDto {
    account_id: String,
    client_order_id: String,
}

struct Stage8a3BodyFacts {
    present: bool,
    len: usize,
    category: Stage8a3BodyCategory,
    sha256: Option<String>,
}

struct Stage8a3Decision {
    disposition: Stage8a3Disposition,
    category: Stage8a3SemanticCategory,
    reason: Option<Stage8a3ReconciliationReason>,
    correlation: Stage8a3CorrelationState,
    broker_order_id_len: usize,
    accepted_candidate: bool,
    reconciliation_required: bool,
    authentication_blocked: bool,
    configuration_blocked: bool,
}

fn classify_explicit_context(
    expected: Stage8a3ExpectedContext,
    observation: Stage8a3LocalObservationKind,
) -> Stage8a3ClassifiedObservation {
    let endpoint = match &expected {
        Stage8a3ExpectedContext::Place { .. } => Stage8a3EndpointKind::Place,
        Stage8a3ExpectedContext::Cancel { .. } => Stage8a3EndpointKind::Cancel,
    };
    let context_binding = expected_context_binding(&expected);
    let (status, body_facts, decision) = match observation {
        Stage8a3LocalObservationKind::Response { status, body } => {
            let facts = body_facts(&body);
            let decision = match expected {
                Stage8a3ExpectedContext::Place {
                    request_id,
                    client_order_id,
                    account_id,
                    venue_symbol,
                } => classify_place_response(
                    status,
                    &body,
                    request_id,
                    &client_order_id,
                    &account_id,
                    &venue_symbol,
                ),
                Stage8a3ExpectedContext::Cancel {
                    source_request_id,
                    account_id,
                    broker_order_id,
                    target_client_order_id,
                } => classify_cancel_response(
                    status,
                    &body,
                    source_request_id,
                    &account_id,
                    &broker_order_id,
                    target_client_order_id.as_ref(),
                ),
            };
            (Some(status), facts, decision)
        }
        Stage8a3LocalObservationKind::BodyReadFailed { status } => (
            status,
            unavailable_body_facts(),
            reconciliation_decision(Stage8a3ReconciliationReason::BodyReadFailure),
        ),
        Stage8a3LocalObservationKind::Timeout => (
            None,
            unavailable_body_facts(),
            reconciliation_decision(Stage8a3ReconciliationReason::Timeout),
        ),
        Stage8a3LocalObservationKind::Disconnected => (
            None,
            unavailable_body_facts(),
            reconciliation_decision(Stage8a3ReconciliationReason::Disconnect),
        ),
        Stage8a3LocalObservationKind::ResponseLost => (
            None,
            unavailable_body_facts(),
            reconciliation_decision(Stage8a3ReconciliationReason::ResponseLost),
        ),
    };
    let disposition_binding = disposition_binding(&decision.disposition);
    let classification_binding_sha256 = digest_parts(
        b"stage8a3-classified-local-observation-v1",
        &[
            context_binding.as_bytes(),
            format!("{endpoint:?}").as_bytes(),
            status
                .map(|value| value.to_string())
                .unwrap_or_default()
                .as_bytes(),
            body_facts.sha256.as_deref().unwrap_or("").as_bytes(),
            format!("{:?}", decision.category).as_bytes(),
            format!("{:?}", decision.reason).as_bytes(),
            disposition_binding.as_bytes(),
        ],
    );
    let diagnostic = Stage8a3ClassificationDiagnostic {
        endpoint,
        category: decision.category,
        reconciliation_reason: decision.reason,
        status,
        body_present: body_facts.present,
        body_len: body_facts.len,
        body_category: body_facts.category,
        body_sha256: body_facts.sha256,
        broker_order_id_present: decision.broker_order_id_len > 0,
        broker_order_id_len: decision.broker_order_id_len,
        correlation: decision.correlation,
        accepted_candidate: decision.accepted_candidate,
        reconciliation_required: decision.reconciliation_required,
        authentication_blocked: decision.authentication_blocked,
        configuration_blocked: decision.configuration_blocked,
        classification_binding_sha256,
    };
    Stage8a3ClassifiedObservation {
        disposition: decision.disposition,
        diagnostic,
    }
}

fn classify_place_response(
    status: u16,
    body: &[u8],
    request_id: StrategyRequestId,
    expected_client_order_id: &ClientOrderId,
    expected_account_id: &AccountId,
    expected_venue_symbol: &str,
) -> Stage8a3Decision {
    match status {
        200 => classify_place_200(
            body,
            request_id,
            expected_client_order_id,
            expected_account_id,
            expected_venue_symbol,
        ),
        201..=299 => {
            reconciliation_decision(Stage8a3ReconciliationReason::UndocumentedSuccessStatus)
        }
        401 => blocked_decision(Stage8a3SemanticCategory::PlaceAuthenticationBlocked),
        404 => blocked_decision(Stage8a3SemanticCategory::PlaceConfigurationOrInstrumentBlocked),
        400 => reconciliation_decision(Stage8a3ReconciliationReason::UnsafeOrUnknownClientError),
        429 | 500 | 503 | 504 => {
            reconciliation_decision(Stage8a3ReconciliationReason::TransientOrServerStatus)
        }
        _ => reconciliation_decision(Stage8a3ReconciliationReason::UndocumentedStatus),
    }
}

fn classify_place_200(
    body: &[u8],
    request_id: StrategyRequestId,
    expected_client_order_id: &ClientOrderId,
    expected_account_id: &AccountId,
    expected_venue_symbol: &str,
) -> Stage8a3Decision {
    if body.is_empty() || body.len() > MAX_CLASSIFIER_BODY_BYTES {
        return reconciliation_decision(Stage8a3ReconciliationReason::MalformedSuccessBody);
    }
    let decoded: Stage8a3PlaceSuccessDto = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => {
            return reconciliation_decision(Stage8a3ReconciliationReason::MalformedSuccessBody)
        }
    };
    let order_id = match decoded.order_id {
        Some(value) if !value.trim().is_empty() => value,
        _ => return reconciliation_decision(Stage8a3ReconciliationReason::MissingBrokerOrderId),
    };
    if decoded.order.account_id != expected_account_id.as_str()
        || decoded.order.symbol != expected_venue_symbol
        || decoded.order.client_order_id != expected_client_order_id.as_str()
    {
        return reconciliation_mismatch();
    }
    let broker_order_id = match BrokerOrderId::from_broker_native_exact(order_id) {
        Ok(value) => value,
        Err(_) => {
            return reconciliation_decision(Stage8a3ReconciliationReason::MissingBrokerOrderId)
        }
    };
    let broker_order_id_len = broker_order_id.as_str().len();
    Stage8a3Decision {
        disposition: Stage8a3Disposition::PlaceAcceptedCandidate {
            broker_order_id,
            request_id,
        },
        category: Stage8a3SemanticCategory::PlaceAcceptedCandidate,
        reason: None,
        correlation: Stage8a3CorrelationState::Matched,
        broker_order_id_len,
        accepted_candidate: true,
        reconciliation_required: false,
        authentication_blocked: false,
        configuration_blocked: false,
    }
}

fn classify_cancel_response(
    status: u16,
    body: &[u8],
    source_request_id: StrategyRequestId,
    expected_account_id: &AccountId,
    expected_broker_order_id: &BrokerOrderId,
    expected_client_order_id: Option<&ClientOrderId>,
) -> Stage8a3Decision {
    match status {
        200 => classify_cancel_200(
            body,
            source_request_id,
            expected_account_id,
            expected_broker_order_id,
            expected_client_order_id,
        ),
        201..=299 => {
            reconciliation_decision(Stage8a3ReconciliationReason::UndocumentedSuccessStatus)
        }
        400 => reconciliation_decision(Stage8a3ReconciliationReason::CancelAlreadyExecuted),
        401 => blocked_decision(
            Stage8a3SemanticCategory::CancelAuthenticationBlockedReconciliationRequired,
        ),
        404 => reconciliation_decision(Stage8a3ReconciliationReason::CancelTargetNotFound),
        429 | 500 | 503 | 504 => {
            reconciliation_decision(Stage8a3ReconciliationReason::TransientOrServerStatus)
        }
        _ => reconciliation_decision(Stage8a3ReconciliationReason::UndocumentedStatus),
    }
}

fn classify_cancel_200(
    body: &[u8],
    source_request_id: StrategyRequestId,
    expected_account_id: &AccountId,
    expected_broker_order_id: &BrokerOrderId,
    expected_client_order_id: Option<&ClientOrderId>,
) -> Stage8a3Decision {
    if body.is_empty() || body.len() > MAX_CLASSIFIER_BODY_BYTES {
        return reconciliation_decision(Stage8a3ReconciliationReason::MalformedSuccessBody);
    }
    let decoded: Stage8a3CancelSuccessDto = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => {
            return reconciliation_decision(Stage8a3ReconciliationReason::MalformedSuccessBody)
        }
    };
    let client_matches = expected_client_order_id
        .map(|expected| decoded.order.client_order_id == expected.as_str())
        .unwrap_or(true);
    if decoded.order_id != expected_broker_order_id.as_str()
        || decoded.order.account_id != expected_account_id.as_str()
        || !client_matches
    {
        return reconciliation_mismatch();
    }
    Stage8a3Decision {
        disposition: Stage8a3Disposition::CancelAcceptedCandidate {
            broker_order_id: expected_broker_order_id.clone(),
            source_request_id,
        },
        category: Stage8a3SemanticCategory::CancelAcceptedCandidate,
        reason: None,
        correlation: Stage8a3CorrelationState::Matched,
        broker_order_id_len: expected_broker_order_id.as_str().len(),
        accepted_candidate: true,
        reconciliation_required: false,
        authentication_blocked: false,
        configuration_blocked: false,
    }
}

fn blocked_decision(category: Stage8a3SemanticCategory) -> Stage8a3Decision {
    let (disposition, authentication_blocked, configuration_blocked, reconciliation_required) =
        match category {
            Stage8a3SemanticCategory::PlaceAuthenticationBlocked => (
                Stage8a3Disposition::PlaceAuthenticationBlocked,
                true,
                false,
                false,
            ),
            Stage8a3SemanticCategory::PlaceConfigurationOrInstrumentBlocked => (
                Stage8a3Disposition::PlaceConfigurationOrInstrumentBlocked,
                false,
                true,
                false,
            ),
            Stage8a3SemanticCategory::CancelAuthenticationBlockedReconciliationRequired => (
                Stage8a3Disposition::CancelAuthenticationBlockedReconciliationRequired,
                true,
                false,
                true,
            ),
            _ => unreachable!("only endpoint-specific blocked categories are accepted"),
        };
    Stage8a3Decision {
        disposition,
        category,
        reason: None,
        correlation: Stage8a3CorrelationState::NotEvaluated,
        broker_order_id_len: 0,
        accepted_candidate: false,
        reconciliation_required,
        authentication_blocked,
        configuration_blocked,
    }
}

fn reconciliation_mismatch() -> Stage8a3Decision {
    let mut value = reconciliation_decision(Stage8a3ReconciliationReason::CorrelationMismatch);
    value.correlation = Stage8a3CorrelationState::Mismatched;
    value
}

fn reconciliation_decision(reason: Stage8a3ReconciliationReason) -> Stage8a3Decision {
    Stage8a3Decision {
        disposition: Stage8a3Disposition::ReconciliationRequired(reason),
        category: Stage8a3SemanticCategory::ReconciliationRequired,
        reason: Some(reason),
        correlation: Stage8a3CorrelationState::Unavailable,
        broker_order_id_len: 0,
        accepted_candidate: false,
        reconciliation_required: true,
        authentication_blocked: false,
        configuration_blocked: false,
    }
}

fn body_facts(body: &[u8]) -> Stage8a3BodyFacts {
    let category = if body.is_empty() {
        Stage8a3BodyCategory::Empty
    } else if body.len() > MAX_CLASSIFIER_BODY_BYTES {
        Stage8a3BodyCategory::Oversized
    } else {
        match serde_json::from_slice::<serde_json::Value>(body) {
            Ok(serde_json::Value::Object(_)) => Stage8a3BodyCategory::JsonObject,
            Ok(_) => Stage8a3BodyCategory::JsonNonObject,
            Err(_) => Stage8a3BodyCategory::MalformedJson,
        }
    };
    Stage8a3BodyFacts {
        present: !body.is_empty(),
        len: body.len(),
        category,
        sha256: (!body.is_empty()).then(|| sha256(body)),
    }
}

fn unavailable_body_facts() -> Stage8a3BodyFacts {
    Stage8a3BodyFacts {
        present: false,
        len: 0,
        category: Stage8a3BodyCategory::Unavailable,
        sha256: None,
    }
}

fn expected_context_binding(expected: &Stage8a3ExpectedContext) -> String {
    match expected {
        Stage8a3ExpectedContext::Place {
            request_id,
            client_order_id,
            account_id,
            venue_symbol,
        } => digest_parts(
            b"stage8a3-place-expected-context-v1",
            &[
                request_id.to_string().as_bytes(),
                client_order_id.as_str().as_bytes(),
                account_id.as_str().as_bytes(),
                venue_symbol.as_bytes(),
            ],
        ),
        Stage8a3ExpectedContext::Cancel {
            source_request_id,
            account_id,
            broker_order_id,
            target_client_order_id,
        } => digest_parts(
            b"stage8a3-cancel-expected-context-v1",
            &[
                source_request_id.to_string().as_bytes(),
                account_id.as_str().as_bytes(),
                broker_order_id.as_str().as_bytes(),
                target_client_order_id
                    .as_ref()
                    .map(ClientOrderId::as_str)
                    .unwrap_or("")
                    .as_bytes(),
            ],
        ),
    }
}

fn disposition_binding(disposition: &Stage8a3Disposition) -> String {
    match disposition {
        Stage8a3Disposition::PlaceAcceptedCandidate {
            broker_order_id,
            request_id,
        } => digest_parts(
            b"stage8a3-private-place-candidate-v1",
            &[
                broker_order_id.as_str().as_bytes(),
                request_id.to_string().as_bytes(),
            ],
        ),
        Stage8a3Disposition::CancelAcceptedCandidate {
            broker_order_id,
            source_request_id,
        } => digest_parts(
            b"stage8a3-private-cancel-candidate-v1",
            &[
                broker_order_id.as_str().as_bytes(),
                source_request_id.to_string().as_bytes(),
            ],
        ),
        Stage8a3Disposition::PlaceAuthenticationBlocked => {
            sha256(b"stage8a3-place-authentication-blocked-v1")
        }
        Stage8a3Disposition::PlaceConfigurationOrInstrumentBlocked => {
            sha256(b"stage8a3-place-configuration-blocked-v1")
        }
        Stage8a3Disposition::CancelAuthenticationBlockedReconciliationRequired => {
            sha256(b"stage8a3-cancel-authentication-blocked-reconciliation-v1")
        }
        Stage8a3Disposition::ReconciliationRequired(reason) => digest_parts(
            b"stage8a3-private-reconciliation-required-v1",
            &[format!("{reason:?}").as_bytes()],
        ),
    }
}

fn sha256(value: &[u8]) -> String {
    hex(&Sha256::digest(value))
}

fn digest_parts(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("hex formatting cannot fail");
    }
    output
}

#[cfg(test)]
mod tests;
