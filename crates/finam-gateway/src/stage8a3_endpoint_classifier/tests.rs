use super::*;
use broker_core::{Exchange, Market};
use uuid::Uuid;

const ACCOUNT: &str = "ACC_TEST_0001";
const SYMBOL: &str = "IMOEXF@RTSX";
const CLIENT_ID: &str = "ABCDEF234567ABCDEF23";
const ORDER_ID: &str = "broker-order-9007199254740993";

fn request_id() -> StrategyRequestId {
    StrategyRequestId::new(Uuid::parse_str("11111111-2222-4333-8444-555555555555").unwrap())
}

fn client_order_id() -> ClientOrderId {
    ClientOrderId::new(CLIENT_ID).unwrap()
}

fn instrument() -> InstrumentId {
    InstrumentId {
        symbol: "IMOEXF".to_owned(),
        venue_symbol: Some(SYMBOL.to_owned()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    }
}

fn place_context() -> Stage8a3EndpointContext {
    Stage8a3EndpointContext::for_place(
        request_id(),
        client_order_id(),
        AccountId::new(ACCOUNT),
        instrument(),
    )
    .unwrap()
}

fn cancel_context() -> Stage8a3EndpointContext {
    Stage8a3EndpointContext::for_cancel(
        request_id(),
        AccountId::new(ACCOUNT),
        BrokerOrderId::new(ORDER_ID),
        Some(client_order_id()),
    )
    .unwrap()
}

fn place_body(order_id: &str, account: &str, symbol: &str, client_id: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "order_id": order_id,
        "exec_id": "exec-redacted",
        "status": "ORDER_STATUS_NEW",
        "order": {
            "account_id": account,
            "symbol": symbol,
            "client_order_id": client_id
        }
    }))
    .unwrap()
}

fn cancel_body(order_id: &str, account: &str, client_id: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "order_id": order_id,
        "status": "ORDER_STATUS_CANCELED",
        "order": {
            "account_id": account,
            "client_order_id": client_id
        }
    }))
    .unwrap()
}

fn classify_place(status: u16, body: Vec<u8>) -> Stage8a3ClassifiedObservation {
    place_context().classify(Stage8a3LocalHttpObservation::response(status, body))
}

fn classify_cancel(status: u16, body: Vec<u8>) -> Stage8a3ClassifiedObservation {
    cancel_context().classify(Stage8a3LocalHttpObservation::response(status, body))
}

#[test]
fn place_exact_200_is_candidate_and_preserves_opaque_string_order_id() {
    let classified = classify_place(200, place_body(ORDER_ID, ACCOUNT, SYMBOL, CLIENT_ID));
    assert_eq!(
        classified.diagnostic.category,
        Stage8a3SemanticCategory::PlaceAcceptedCandidate
    );
    assert_eq!(
        classified.diagnostic.correlation,
        Stage8a3CorrelationState::Matched
    );
    assert!(classified.diagnostic.accepted_candidate);
    assert_eq!(classified.diagnostic.broker_order_id_len, ORDER_ID.len());
    match &classified.disposition {
        Stage8a3Disposition::PlaceAcceptedCandidate {
            broker_order_id,
            request_id: actual_request_id,
        } => {
            assert_eq!(broker_order_id.as_str(), ORDER_ID);
            assert_eq!(*actual_request_id, request_id());
        }
        _ => panic!("expected private place candidate"),
    }
}

#[test]
fn place_missing_or_empty_order_id_requires_reconciliation() {
    let missing = serde_json::to_vec(&serde_json::json!({
        "order": {"account_id": ACCOUNT, "symbol": SYMBOL, "client_order_id": CLIENT_ID}
    }))
    .unwrap();
    for body in [
        missing,
        place_body("", ACCOUNT, SYMBOL, CLIENT_ID),
        place_body("   ", ACCOUNT, SYMBOL, CLIENT_ID),
    ] {
        let diagnostic = classify_place(200, body).into_diagnostic();
        assert!(diagnostic.reconciliation_required);
        assert_eq!(
            diagnostic.reconciliation_reason,
            Some(Stage8a3ReconciliationReason::MissingBrokerOrderId)
        );
    }
}

#[test]
fn place_every_correlation_mismatch_requires_reconciliation() {
    for body in [
        place_body(ORDER_ID, "ACC_OTHER", SYMBOL, CLIENT_ID),
        place_body(ORDER_ID, ACCOUNT, "OTHER@RTSX", CLIENT_ID),
        place_body(ORDER_ID, ACCOUNT, SYMBOL, "OTHERCLIENT000000001"),
    ] {
        let diagnostic = classify_place(200, body).into_diagnostic();
        assert_eq!(diagnostic.correlation, Stage8a3CorrelationState::Mismatched);
        assert_eq!(
            diagnostic.reconciliation_reason,
            Some(Stage8a3ReconciliationReason::CorrelationMismatch)
        );
    }
}

#[test]
fn place_malformed_truncated_empty_non_object_and_oversized_200_reconcile() {
    for body in [
        Vec::new(),
        b"{".to_vec(),
        b"[]".to_vec(),
        vec![b'x'; MAX_CLASSIFIER_BODY_BYTES + 1],
    ] {
        let diagnostic = classify_place(200, body).into_diagnostic();
        assert_eq!(
            diagnostic.reconciliation_reason,
            Some(Stage8a3ReconciliationReason::MalformedSuccessBody)
        );
    }
}

#[test]
fn place_undocumented_2xx_never_accepts() {
    for status in [201, 202, 204, 206, 299] {
        let diagnostic = classify_place(status, place_body(ORDER_ID, ACCOUNT, SYMBOL, CLIENT_ID))
            .into_diagnostic();
        assert_eq!(
            diagnostic.reconciliation_reason,
            Some(Stage8a3ReconciliationReason::UndocumentedSuccessStatus)
        );
    }
}

#[test]
fn place_400_is_never_status_or_text_promoted_to_rejection() {
    for body in [
        Vec::new(),
        br#"{"message":"invalid trading parameters"}"#.to_vec(),
        br#"{"code":3,"details":[]}"#.to_vec(),
        b"invalid trading parameters".to_vec(),
    ] {
        let diagnostic = classify_place(400, body).into_diagnostic();
        assert_eq!(
            diagnostic.category,
            Stage8a3SemanticCategory::ReconciliationRequired
        );
        assert_eq!(
            diagnostic.reconciliation_reason,
            Some(Stage8a3ReconciliationReason::UnsafeOrUnknownClientError)
        );
    }
}

#[test]
fn place_auth_and_configuration_statuses_are_endpoint_specific_blocks() {
    let auth = classify_place(401, Vec::new()).into_diagnostic();
    assert_eq!(
        auth.category,
        Stage8a3SemanticCategory::PlaceAuthenticationBlocked
    );
    assert!(auth.authentication_blocked);
    assert!(!auth.accepted_candidate);

    let config = classify_place(404, Vec::new()).into_diagnostic();
    assert_eq!(
        config.category,
        Stage8a3SemanticCategory::PlaceConfigurationOrInstrumentBlocked
    );
    assert!(config.configuration_blocked);
    assert!(!config.accepted_candidate);
}

#[test]
fn place_transient_and_undocumented_statuses_reconcile() {
    for status in [429, 500, 503, 504] {
        let diagnostic = classify_place(status, Vec::new()).into_diagnostic();
        assert_eq!(
            diagnostic.reconciliation_reason,
            Some(Stage8a3ReconciliationReason::TransientOrServerStatus)
        );
    }
    for status in [0, 100, 399, 403, 408, 409, 410, 418, 502, 505, 599] {
        let diagnostic = classify_place(status, Vec::new()).into_diagnostic();
        assert_eq!(
            diagnostic.reconciliation_reason,
            Some(Stage8a3ReconciliationReason::UndocumentedStatus)
        );
    }
}

#[test]
fn cancel_exact_200_is_candidate_without_flatness_semantics() {
    let classified = classify_cancel(200, cancel_body(ORDER_ID, ACCOUNT, CLIENT_ID));
    assert_eq!(
        classified.diagnostic.category,
        Stage8a3SemanticCategory::CancelAcceptedCandidate
    );
    assert!(classified.diagnostic.accepted_candidate);
    assert!(!classified.diagnostic.reconciliation_required);
    match &classified.disposition {
        Stage8a3Disposition::CancelAcceptedCandidate {
            broker_order_id,
            source_request_id,
        } => {
            assert_eq!(broker_order_id.as_str(), ORDER_ID);
            assert_eq!(*source_request_id, request_id());
        }
        _ => panic!("expected private cancel candidate"),
    }
}

#[test]
fn cancel_empty_malformed_or_contradictory_200_reconciles() {
    let cases = [
        (Vec::new(), Stage8a3CorrelationState::Unavailable),
        (b"{".to_vec(), Stage8a3CorrelationState::Unavailable),
        (
            cancel_body("other-order", ACCOUNT, CLIENT_ID),
            Stage8a3CorrelationState::Mismatched,
        ),
        (
            cancel_body(ORDER_ID, "ACC_OTHER", CLIENT_ID),
            Stage8a3CorrelationState::Mismatched,
        ),
        (
            cancel_body(ORDER_ID, ACCOUNT, "OTHERCLIENT000000001"),
            Stage8a3CorrelationState::Mismatched,
        ),
    ];
    for (body, correlation) in cases {
        let diagnostic = classify_cancel(200, body).into_diagnostic();
        assert!(diagnostic.reconciliation_required);
        assert_eq!(diagnostic.correlation, correlation);
    }
}

#[test]
fn cancel_204_and_other_undocumented_2xx_reconcile() {
    for status in [201, 202, 204, 299] {
        let diagnostic = classify_cancel(status, Vec::new()).into_diagnostic();
        assert_eq!(
            diagnostic.reconciliation_reason,
            Some(Stage8a3ReconciliationReason::UndocumentedSuccessStatus)
        );
    }
}

#[test]
fn cancel_status_table_is_fail_closed_and_endpoint_specific() {
    let executed = classify_cancel(400, Vec::new()).into_diagnostic();
    assert_eq!(
        executed.reconciliation_reason,
        Some(Stage8a3ReconciliationReason::CancelAlreadyExecuted)
    );
    let auth = classify_cancel(401, Vec::new()).into_diagnostic();
    assert_eq!(
        auth.category,
        Stage8a3SemanticCategory::CancelAuthenticationBlockedReconciliationRequired
    );
    assert!(auth.authentication_blocked && auth.reconciliation_required);
    let missing = classify_cancel(404, Vec::new()).into_diagnostic();
    assert_eq!(
        missing.reconciliation_reason,
        Some(Stage8a3ReconciliationReason::CancelTargetNotFound)
    );
    for status in [409, 410, 403, 408, 502] {
        assert_eq!(
            classify_cancel(status, Vec::new())
                .into_diagnostic()
                .reconciliation_reason,
            Some(Stage8a3ReconciliationReason::UndocumentedStatus)
        );
    }
    for status in [429, 500, 503, 504] {
        assert_eq!(
            classify_cancel(status, Vec::new())
                .into_diagnostic()
                .reconciliation_reason,
            Some(Stage8a3ReconciliationReason::TransientOrServerStatus)
        );
    }
}

#[test]
fn local_failure_observations_always_require_reconciliation() {
    let observations = [
        Stage8a3LocalHttpObservation::body_read_failed(Some(200)),
        Stage8a3LocalHttpObservation::timeout(),
        Stage8a3LocalHttpObservation::disconnected(),
        Stage8a3LocalHttpObservation::response_lost(),
    ];
    for observation in observations {
        let diagnostic = place_context().classify(observation).into_diagnostic();
        assert!(diagnostic.reconciliation_required);
        assert!(!diagnostic.accepted_candidate);
        assert_eq!(diagnostic.body_category, Stage8a3BodyCategory::Unavailable);
    }
}

#[test]
fn same_status_has_endpoint_specific_semantics() {
    let place = classify_place(401, Vec::new()).into_diagnostic();
    let cancel = classify_cancel(401, Vec::new()).into_diagnostic();
    assert_ne!(place.category, cancel.category);
    assert!(!place.reconciliation_required);
    assert!(cancel.reconciliation_required);
}

#[test]
fn public_diagnostic_is_redacted_and_deterministic() {
    let first =
        classify_place(200, place_body(ORDER_ID, ACCOUNT, SYMBOL, CLIENT_ID)).into_diagnostic();
    let second =
        classify_place(200, place_body(ORDER_ID, ACCOUNT, SYMBOL, CLIENT_ID)).into_diagnostic();
    let json = serde_json::to_string(&first).unwrap();
    assert_eq!(first, second);
    assert!(!json.contains(ACCOUNT));
    assert!(!json.contains(SYMBOL));
    assert!(!json.contains(CLIENT_ID));
    assert!(!json.contains(ORDER_ID));
    assert!(!json.contains("Authorization"));
    assert_eq!(first.classification_binding_sha256.len(), 64);
    assert_eq!(first.body_sha256.as_deref().map(str::len), Some(64));
}

#[test]
fn invalid_context_is_rejected_before_classification() {
    assert!(matches!(
        Stage8a3EndpointContext::for_place(
            request_id(),
            client_order_id(),
            AccountId::new(""),
            instrument(),
        ),
        Err(Stage8a3ContextError::EmptyAccountIdentity)
    ));
    let empty_instrument = InstrumentId {
        symbol: String::new(),
        venue_symbol: None,
        exchange: Exchange::Moex,
        market: Market::Futures,
    };
    assert!(matches!(
        Stage8a3EndpointContext::for_place(
            request_id(),
            client_order_id(),
            AccountId::new(ACCOUNT),
            empty_instrument,
        ),
        Err(Stage8a3ContextError::EmptyInstrumentIdentity)
    ));
}

#[test]
fn private_reconciliation_disposition_carries_only_reason() {
    let classified = place_context().classify(Stage8a3LocalHttpObservation::timeout());
    match classified.disposition {
        Stage8a3Disposition::ReconciliationRequired(reason) => {
            assert_eq!(reason, Stage8a3ReconciliationReason::Timeout)
        }
        _ => panic!("expected private reconciliation marker"),
    }
}
