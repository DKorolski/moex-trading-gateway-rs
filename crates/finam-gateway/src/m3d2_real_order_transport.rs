//! M3d-2c real order endpoint transport, disabled by default.
//!
//! This module owns the only reviewed `reqwest` order POST/DELETE surface for
//! the protected M3d-2 path. It is not live-capable by itself:
//! `EndpointGateApproved` remains unconstructible in production config, command
//! consumer/live runtime features remain disabled, and tests exercise this code
//! only against a loopback local mock endpoint.

use broker_finam::{
    AccessToken, FinamCancelOrderRequestSpec, FinamOrderEndpointClassifiedResponse,
    FinamOrderEndpointContext, FinamOrderEndpointLocalHttpResponse, FinamOrderEndpointMappedResult,
    FinamOrderEndpointResponseKind, FinamOrderExecutionOutcome, FinamPlaceOrderRequestSpec,
};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::EndpointGateApproved;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinamAuthorizationHeaderMode {
    BearerJwt,
}

impl FinamAuthorizationHeaderMode {
    pub fn redacted_diagnostic(self) -> FinamAuthorizationHeaderPolicyDiagnostic {
        FinamAuthorizationHeaderPolicyDiagnostic {
            mode: self,
            header_name: "Authorization".to_string(),
            scheme: Some("Bearer".to_string()),
            raw_token_exported: false,
            evidence_source: "https://api.finam.ru/docs/rest/llms.txt".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamAuthorizationHeaderPolicyDiagnostic {
    pub mode: FinamAuthorizationHeaderMode,
    pub header_name: String,
    pub scheme: Option<String>,
    pub raw_token_exported: bool,
    pub evidence_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct M3d2RealOrderEndpointTransportConfig {
    pub rest_base_url: String,
    pub request_timeout_ms: u64,
    pub authorization_header_mode: FinamAuthorizationHeaderMode,
    pub external_endpoint_mode: M3d2ExternalOrderEndpointMode,
}

impl Default for M3d2RealOrderEndpointTransportConfig {
    fn default() -> Self {
        Self {
            rest_base_url: "https://api.finam.ru".to_string(),
            request_timeout_ms: 10_000,
            authorization_header_mode: FinamAuthorizationHeaderMode::BearerJwt,
            external_endpoint_mode: M3d2ExternalOrderEndpointMode::LocalMockOnly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum M3d2ExternalOrderEndpointMode {
    LocalMockOnly,
    ExternalFinamDisabled,
    FutureExternalFinamRequiresLiveGate,
    #[cfg(feature = "m3j16-actual-one-shot")]
    M3j16ActualOneShotExternalFinam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum M3d2OrderEndpointBaseUrlKind {
    Loopback,
    ExternalFinam,
    OtherExternal,
}

#[derive(Clone)]
pub struct M3d2RealOrderEndpointTransport {
    http: reqwest::Client,
    base_url: reqwest::Url,
    authorization_header_mode: FinamAuthorizationHeaderMode,
}

impl std::fmt::Debug for M3d2RealOrderEndpointTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("M3d2RealOrderEndpointTransport")
            .field("base_url_scheme", &self.base_url.scheme())
            .field(
                "authorization_header_policy",
                &self.authorization_header_mode.redacted_diagnostic(),
            )
            .field("raw_token_exported", &false)
            .field("external_finam_order_calls_allowed_by_default", &false)
            .finish()
    }
}

impl M3d2RealOrderEndpointTransport {
    pub fn try_new(
        config: M3d2RealOrderEndpointTransportConfig,
    ) -> Result<Self, M3d2RealOrderEndpointTransportError> {
        let base_url =
            reqwest::Url::parse(&format!("{}/", config.rest_base_url.trim_end_matches('/')))
                .map_err(|_| M3d2RealOrderEndpointTransportError::InvalidBaseUrl)?;
        let base_url_kind = classify_order_endpoint_base_url(&base_url);
        if !external_endpoint_firewall_allows(config.external_endpoint_mode, base_url_kind) {
            return Err(
                M3d2RealOrderEndpointTransportError::ExternalOrderEndpointBlocked {
                    mode: config.external_endpoint_mode,
                    base_url_kind,
                },
            );
        }
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(
                config.request_timeout_ms.max(1),
            ))
            .build()
            .map_err(|error| M3d2RealOrderEndpointTransportError::ClientBuild {
                error_kind: error_kind(&error),
            })?;
        Ok(Self {
            http,
            base_url,
            authorization_header_mode: config.authorization_header_mode,
        })
    }

    pub async fn place_order(
        &self,
        _gate: &EndpointGateApproved,
        access_token: &AccessToken,
        spec: &FinamPlaceOrderRequestSpec,
    ) -> M3d2RealOrderEndpointTransportOutcome {
        self.place_order_execution(_gate, access_token, spec)
            .await
            .redacted_outcome()
    }

    pub async fn place_order_execution(
        &self,
        _gate: &EndpointGateApproved,
        access_token: &AccessToken,
        spec: &FinamPlaceOrderRequestSpec,
    ) -> M3d2RealOrderEndpointTransportExecution {
        let request = match self.place_request(access_token, spec) {
            Ok(request) => request,
            Err(error) => return M3d2RealOrderEndpointTransportExecution::not_sent(error),
        };
        self.execute_after_gate(FinamOrderEndpointContext::Place, request)
            .await
    }

    pub async fn cancel_order(
        &self,
        _gate: &EndpointGateApproved,
        access_token: &AccessToken,
        spec: &FinamCancelOrderRequestSpec,
    ) -> M3d2RealOrderEndpointTransportOutcome {
        self.cancel_order_execution(_gate, access_token, spec)
            .await
            .redacted_outcome()
    }

    pub async fn cancel_order_execution(
        &self,
        _gate: &EndpointGateApproved,
        access_token: &AccessToken,
        spec: &FinamCancelOrderRequestSpec,
    ) -> M3d2RealOrderEndpointTransportExecution {
        let request = match self.cancel_request(access_token, spec) {
            Ok(request) => request,
            Err(error) => return M3d2RealOrderEndpointTransportExecution::not_sent(error),
        };
        self.execute_after_gate(FinamOrderEndpointContext::Cancel, request)
            .await
    }

    fn place_request(
        &self,
        access_token: &AccessToken,
        spec: &FinamPlaceOrderRequestSpec,
    ) -> Result<reqwest::RequestBuilder, M3d2RealOrderEndpointTransportError> {
        if access_token.is_empty() {
            return Err(M3d2RealOrderEndpointTransportError::MissingToken);
        }
        let url = self.url_from_segments(spec.rest_path_segments())?;
        let request = self.http.post(url).json(&spec.body);
        Ok(self.authorize(request, access_token))
    }

    fn cancel_request(
        &self,
        access_token: &AccessToken,
        spec: &FinamCancelOrderRequestSpec,
    ) -> Result<reqwest::RequestBuilder, M3d2RealOrderEndpointTransportError> {
        if access_token.is_empty() {
            return Err(M3d2RealOrderEndpointTransportError::MissingToken);
        }
        let url = self.url_from_segments(spec.rest_path_segments())?;
        let request = self
            .http
            .delete(url)
            .header(CONTENT_TYPE, "application/json");
        Ok(self.authorize(request, access_token))
    }

    fn authorize(
        &self,
        request: reqwest::RequestBuilder,
        access_token: &AccessToken,
    ) -> reqwest::RequestBuilder {
        match self.authorization_header_mode {
            FinamAuthorizationHeaderMode::BearerJwt => {
                request.header(AUTHORIZATION, format!("Bearer {}", access_token.as_str()))
            }
        }
    }

    fn url_from_segments(
        &self,
        segments: Vec<String>,
    ) -> Result<reqwest::Url, M3d2RealOrderEndpointTransportError> {
        let mut url = self.base_url.clone();
        {
            let mut path_segments = url
                .path_segments_mut()
                .map_err(|_| M3d2RealOrderEndpointTransportError::InvalidBaseUrl)?;
            path_segments.clear();
            path_segments.extend(segments.iter().map(String::as_str));
        }
        Ok(url)
    }

    async fn execute_after_gate(
        &self,
        context: FinamOrderEndpointContext,
        request: reqwest::RequestBuilder,
    ) -> M3d2RealOrderEndpointTransportExecution {
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) if error.is_timeout() => {
                let classified =
                    broker_finam::classify_order_endpoint_local_http_response_for_context(
                        context,
                        &FinamOrderEndpointLocalHttpResponse::Timeout,
                    );
                return M3d2RealOrderEndpointTransportExecution::sent(
                    classified,
                    M3d2PostSendOrderOutcomeSemantics::TimeoutUnknownPending,
                );
            }
            Err(error) => {
                return M3d2RealOrderEndpointTransportExecution::sent_error(
                    M3d2RealOrderEndpointTransportError::HttpSend {
                        error_kind: error_kind(&error),
                    },
                    M3d2PostSendOrderOutcomeSemantics::ReconciliationRequired,
                );
            }
        };
        let status = response.status().as_u16();
        let retry_after_ms = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|seconds| seconds.saturating_mul(1_000));
        let local_response = match response.text().await {
            Ok(body) => FinamOrderEndpointLocalHttpResponse::Response {
                status,
                body,
                retry_after_ms,
            },
            Err(_) => FinamOrderEndpointLocalHttpResponse::BodyReadFailed {
                status: Some(status),
            },
        };
        let classified = broker_finam::classify_order_endpoint_local_http_response_for_context(
            context,
            &local_response,
        );
        let semantics = post_send_semantics(context, &classified);
        M3d2RealOrderEndpointTransportExecution::sent(classified, semantics)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct M3d2RealOrderEndpointTransportExecution {
    pub request_sent: bool,
    pub classified_response: Option<FinamOrderEndpointClassifiedResponse>,
    pub post_send_semantics: M3d2PostSendOrderOutcomeSemantics,
    pub error: Option<M3d2RealOrderEndpointTransportError>,
}

impl std::fmt::Debug for M3d2RealOrderEndpointTransportExecution {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("M3d2RealOrderEndpointTransportExecution")
            .field("request_sent", &self.request_sent)
            .field(
                "classified_response_present",
                &self.classified_response.is_some(),
            )
            .field("post_send_semantics", &self.post_send_semantics)
            .field("error", &self.error)
            .field("raw_token_exported", &false)
            .field("raw_path_exported", &false)
            .field("raw_body_exported", &false)
            .finish()
    }
}

impl M3d2RealOrderEndpointTransportExecution {
    fn not_sent(error: M3d2RealOrderEndpointTransportError) -> Self {
        Self {
            request_sent: false,
            classified_response: None,
            post_send_semantics: M3d2PostSendOrderOutcomeSemantics::NotSent,
            error: Some(error),
        }
    }

    fn sent(
        classified: FinamOrderEndpointClassifiedResponse,
        post_send_semantics: M3d2PostSendOrderOutcomeSemantics,
    ) -> Self {
        Self {
            request_sent: true,
            classified_response: Some(classified),
            post_send_semantics,
            error: None,
        }
    }

    fn sent_error(
        error: M3d2RealOrderEndpointTransportError,
        post_send_semantics: M3d2PostSendOrderOutcomeSemantics,
    ) -> Self {
        Self {
            request_sent: true,
            classified_response: None,
            post_send_semantics,
            error: Some(error),
        }
    }

    pub fn redacted_outcome(&self) -> M3d2RealOrderEndpointTransportOutcome {
        M3d2RealOrderEndpointTransportOutcome {
            request_sent: self.request_sent,
            classified_response_present: self.classified_response.is_some(),
            response_kind: self
                .classified_response
                .as_ref()
                .map(|classified| classified.diagnostic.kind),
            post_send_semantics: self.post_send_semantics,
            error: self.error.clone(),
            raw_token_exported: false,
            raw_path_exported: false,
            raw_body_exported: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct M3d2RealOrderEndpointTransportOutcome {
    pub request_sent: bool,
    pub classified_response_present: bool,
    pub response_kind: Option<FinamOrderEndpointResponseKind>,
    pub post_send_semantics: M3d2PostSendOrderOutcomeSemantics,
    pub error: Option<M3d2RealOrderEndpointTransportError>,
    pub raw_token_exported: bool,
    pub raw_path_exported: bool,
    pub raw_body_exported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum M3d2PostSendOrderOutcomeSemantics {
    NotSent,
    Submitted,
    SubmittedPendingBrokerOrderIdReconciliation,
    CancelAcceptedPendingReconciliation,
    ReconciliationRequired,
    TimeoutUnknownPending,
    RateLimitedDisarm,
    UnauthorizedDisarm,
    MaintenanceDisarm,
    BrokerRejectedTerminal,
}

pub fn post_send_semantics(
    context: FinamOrderEndpointContext,
    classified: &FinamOrderEndpointClassifiedResponse,
) -> M3d2PostSendOrderOutcomeSemantics {
    match &classified.result {
        FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Accepted {
            broker_order_id,
        }) => match context {
            FinamOrderEndpointContext::Place if broker_order_id.is_some() => {
                M3d2PostSendOrderOutcomeSemantics::Submitted
            }
            FinamOrderEndpointContext::Place => {
                M3d2PostSendOrderOutcomeSemantics::SubmittedPendingBrokerOrderIdReconciliation
            }
            FinamOrderEndpointContext::Cancel => {
                M3d2PostSendOrderOutcomeSemantics::CancelAcceptedPendingReconciliation
            }
        },
        FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Rejected {
            ..
        }) => M3d2PostSendOrderOutcomeSemantics::BrokerRejectedTerminal,
        FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Timeout) => {
            M3d2PostSendOrderOutcomeSemantics::TimeoutUnknownPending
        }
        FinamOrderEndpointMappedResult::RateLimited { .. } => {
            M3d2PostSendOrderOutcomeSemantics::RateLimitedDisarm
        }
        FinamOrderEndpointMappedResult::Maintenance { .. } => {
            M3d2PostSendOrderOutcomeSemantics::MaintenanceDisarm
        }
        FinamOrderEndpointMappedResult::Unauthorized { .. } => {
            M3d2PostSendOrderOutcomeSemantics::UnauthorizedDisarm
        }
        FinamOrderEndpointMappedResult::ReconciliationRequired { .. }
        | FinamOrderEndpointMappedResult::DecodeError { .. } => {
            M3d2PostSendOrderOutcomeSemantics::ReconciliationRequired
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum M3d2RealOrderEndpointTransportError {
    MissingToken,
    InvalidBaseUrl,
    ExternalOrderEndpointBlocked {
        mode: M3d2ExternalOrderEndpointMode,
        base_url_kind: M3d2OrderEndpointBaseUrlKind,
    },
    ClientBuild {
        error_kind: String,
    },
    HttpSend {
        error_kind: String,
    },
}

pub fn classify_order_endpoint_base_url(url: &reqwest::Url) -> M3d2OrderEndpointBaseUrlKind {
    let Some(host) = url.host_str() else {
        return M3d2OrderEndpointBaseUrlKind::OtherExternal;
    };
    if matches!(host, "127.0.0.1" | "::1" | "localhost") {
        return M3d2OrderEndpointBaseUrlKind::Loopback;
    }
    if host.eq_ignore_ascii_case("api.finam.ru") {
        return M3d2OrderEndpointBaseUrlKind::ExternalFinam;
    }
    M3d2OrderEndpointBaseUrlKind::OtherExternal
}

pub fn external_endpoint_firewall_allows(
    mode: M3d2ExternalOrderEndpointMode,
    base_url_kind: M3d2OrderEndpointBaseUrlKind,
) -> bool {
    match mode {
        M3d2ExternalOrderEndpointMode::LocalMockOnly => {
            base_url_kind == M3d2OrderEndpointBaseUrlKind::Loopback
        }
        M3d2ExternalOrderEndpointMode::ExternalFinamDisabled => {
            base_url_kind == M3d2OrderEndpointBaseUrlKind::Loopback
        }
        M3d2ExternalOrderEndpointMode::FutureExternalFinamRequiresLiveGate => false,
        #[cfg(feature = "m3j16-actual-one-shot")]
        M3d2ExternalOrderEndpointMode::M3j16ActualOneShotExternalFinam => {
            base_url_kind == M3d2OrderEndpointBaseUrlKind::ExternalFinam
        }
    }
}

fn error_kind(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "timeout".to_string()
    } else if error.is_connect() {
        "connect".to_string()
    } else if error.is_body() {
        "body".to_string()
    } else if error.is_decode() {
        "decode".to_string()
    } else if error.is_request() {
        "request".to_string()
    } else {
        "other".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        AccountId, BrokerOrderId, CancelOrder, CancelPreflightApproval, ClientOrderId, Exchange,
        InstrumentId, Market, OperatorArm, OrderPathEvent, OrderPathRecord, OrderPathState,
        OrderPreflightContext, OrderPreflightPolicy, OrderReferencePrice, OrderSide, OrderType,
        PlaceOrder, StrategyRequestId, TimeInForce,
    };
    use broker_finam::{build_cancel_order_request, build_place_order_request};
    use chrono::{DateTime, TimeZone, Utc};
    use rust_decimal::Decimal;
    use serde_json::Value;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use uuid::Uuid;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CapturedRequest {
        method: String,
        path: String,
        authorization_present: bool,
        authorization_is_bearer: bool,
        authorization_len: Option<usize>,
        body_shape_ok: bool,
        raw_secret_exported: bool,
    }

    fn request_id(n: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128(n))
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "TESTFUT".to_string(),
            venue_symbol: Some("TESTFUT@TEST".to_string()),
            exchange: Exchange::Other("TEST".to_string()),
            market: Market::Futures,
        }
    }

    fn place_order() -> PlaceOrder {
        PlaceOrder {
            request_id: request_id(20),
            created_ts: Utc
                .with_ymd_and_hms(2026, 7, 3, 9, 20, 0)
                .single()
                .expect("timestamp"),
            ttl_ms: Some(1_000),
            account_id: AccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::new("CID000000000000020").expect("client id"),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            qty: Decimal::new(1, 0),
            limit_price: Some(Decimal::new(10_050, 2)),
            time_in_force: TimeInForce::Day,
            comment: None,
        }
    }

    fn sample_arm(now: DateTime<Utc>) -> OperatorArm {
        OperatorArm {
            session_id: "ARM_TEST_M3D2C".to_string(),
            armed_until: now + chrono::Duration::minutes(5),
            endpoint_calls_enabled: true,
            one_shot: false,
            endpoint_attempted: false,
            preflight_digest: "digest-test".to_string(),
        }
    }

    fn preflight_policy(now: DateTime<Utc>) -> OrderPreflightPolicy {
        OrderPreflightPolicy {
            allowed_accounts: vec![AccountId::new("ACC_TEST_0001")],
            allowed_venue_symbols: vec!["TESTFUT@TEST".to_string()],
            allowed_order_types: vec![OrderType::Market, OrderType::Limit],
            allowed_time_in_force: vec![TimeInForce::Day],
            min_qty: Decimal::new(1, 0),
            qty_step: Decimal::new(1, 0),
            max_qty: Decimal::new(3, 0),
            price_step: Some(Decimal::new(1, 2)),
            max_market_qty: Decimal::new(1, 0),
            max_notional_per_order: None,
            max_notional_per_run: None,
            max_limit_deviation_bps: None,
            max_reference_age_ms: 1_000,
            allow_cancel_by_broker_order_id_without_mapping: false,
            operator_arm: sample_arm(now),
        }
    }

    fn approve_place(order: &PlaceOrder) -> broker_core::PreflightApprovedPlaceOrder {
        let now = order.created_ts + chrono::Duration::milliseconds(1);
        let context = OrderPreflightContext {
            reference_price: Some(OrderReferencePrice {
                price: order.limit_price.unwrap_or(Decimal::new(10_050, 2)),
                received_ts: now,
            }),
            current_run_notional: Decimal::ZERO,
        };
        preflight_policy(now)
            .approve_place_order_with_context(order, now, &context)
            .expect("place preflight")
    }

    fn approve_cancel(cancel: &CancelOrder) -> broker_core::PreflightApprovedCancelOrder {
        let now = cancel.created_ts + chrono::Duration::milliseconds(1);
        let mut existing =
            OrderPathRecord::from_place_order(&place_order(), cancel.created_ts, None);
        existing.broker_order_id = Some(cancel.order_id.clone());
        existing
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit");
        existing
            .transition(OrderPathEvent::SubmitAccepted, now)
            .expect("submitted");
        assert_eq!(existing.state, OrderPathState::Submitted);
        match preflight_policy(now)
            .approve_cancel_order(cancel, now, Some(&existing))
            .expect("cancel preflight")
        {
            CancelPreflightApproval::Submit(approved) => approved,
            CancelPreflightApproval::AlreadyTerminal => panic!("expected submit approval"),
        }
    }

    fn run_mock_server_once(
        response: &'static str,
    ) -> (String, Arc<Mutex<Option<CapturedRequest>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let captured = Arc::new(Mutex::new(None));
        let captured_server = Arc::clone(&captured);
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("timeout");
            let raw = read_http_request(&mut stream);
            let captured_request = capture_request(&raw);
            *captured_server.lock().expect("lock") = Some(captured_request);
            stream.write_all(response.as_bytes()).expect("response");
        });
        (format!("http://{address}"), captured)
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = stream.read(&mut chunk).expect("read");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&buffer);
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let content_len = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .or_else(|| {
                        head.lines()
                            .find_map(|line| line.strip_prefix("Content-Length:"))
                    })
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if body.len() >= content_len {
                    break;
                }
            }
        }
        String::from_utf8(buffer).expect("utf8 request")
    }

    fn capture_request(raw: &str) -> CapturedRequest {
        let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
        let mut lines = head.lines();
        let request_line = lines.next().unwrap_or_default();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let path = parts.next().unwrap_or_default().to_string();
        let authorization = head
            .lines()
            .find_map(|line| line.strip_prefix("authorization: "))
            .or_else(|| {
                head.lines()
                    .find_map(|line| line.strip_prefix("Authorization: "))
            });
        let parsed_body = serde_json::from_str::<Value>(body).ok();
        CapturedRequest {
            method,
            path,
            authorization_present: authorization.is_some(),
            authorization_is_bearer: authorization
                .is_some_and(|value| value.starts_with("Bearer ") && value.len() > "Bearer ".len()),
            authorization_len: authorization.map(str::len),
            body_shape_ok: parsed_body.is_some_and(|value| {
                value.get("type").and_then(Value::as_str) == Some("ORDER_TYPE_LIMIT")
                    && value.get("side").and_then(Value::as_str) == Some("SIDE_BUY")
                    && value.get("client_order_id").is_some()
            }),
            raw_secret_exported: false,
        }
    }

    #[tokio::test]
    async fn m3d2c_place_transport_sends_exact_post_to_local_mock_with_bearer_auth() {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 39\r\n\r\n{\"broker_order_id\":\"BROKER_TEST_ORDER\"}";
        let (base_url, captured) = run_mock_server_once(response);
        let transport =
            M3d2RealOrderEndpointTransport::try_new(M3d2RealOrderEndpointTransportConfig {
                rest_base_url: base_url,
                ..M3d2RealOrderEndpointTransportConfig::default()
            })
            .expect("transport");
        let order = place_order();
        let spec = build_place_order_request(&approve_place(&order), None).expect("spec");
        let gate = EndpointGateApproved::m3d2c_test_only_for_loopback_transport();
        let token = AccessToken::new("JWT_TEST_TOKEN");

        let outcome = transport.place_order(&gate, &token, &spec).await;

        assert!(outcome.request_sent);
        assert_eq!(
            outcome.post_send_semantics,
            M3d2PostSendOrderOutcomeSemantics::Submitted
        );
        let captured = captured
            .lock()
            .expect("lock captured")
            .clone()
            .expect("captured");
        assert_eq!(captured.method, "POST");
        assert_eq!(captured.path, "/v1/accounts/ACC_TEST_0001/orders");
        assert!(captured.authorization_present);
        assert!(captured.authorization_is_bearer);
        assert_eq!(
            captured.authorization_len,
            Some("Bearer JWT_TEST_TOKEN".len())
        );
        assert!(captured.body_shape_ok);
        assert!(!captured.raw_secret_exported);
        assert!(!format!("{outcome:?}").contains("JWT_TEST_TOKEN"));
    }

    #[tokio::test]
    async fn m3d2c_cancel_transport_sends_exact_delete_to_local_mock_with_bearer_auth() {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 39\r\n\r\n{\"broker_order_id\":\"BROKER_TEST_ORDER\"}";
        let (base_url, captured) = run_mock_server_once(response);
        let transport =
            M3d2RealOrderEndpointTransport::try_new(M3d2RealOrderEndpointTransportConfig {
                rest_base_url: base_url,
                ..M3d2RealOrderEndpointTransportConfig::default()
            })
            .expect("transport");
        let cancel = CancelOrder {
            request_id: request_id(21),
            created_ts: Utc
                .with_ymd_and_hms(2026, 7, 3, 9, 21, 0)
                .single()
                .expect("timestamp"),
            ttl_ms: Some(1_000),
            account_id: AccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new("BROKER_TEST_ORDER"),
            client_order_id: Some(ClientOrderId::new("CID000000000000020").expect("client id")),
        };
        let spec = build_cancel_order_request(&approve_cancel(&cancel)).expect("spec");
        let gate = EndpointGateApproved::m3d2c_test_only_for_loopback_transport();
        let token = AccessToken::new("JWT_TEST_TOKEN");

        let outcome = transport.cancel_order(&gate, &token, &spec).await;

        assert!(outcome.request_sent);
        assert_eq!(
            outcome.post_send_semantics,
            M3d2PostSendOrderOutcomeSemantics::CancelAcceptedPendingReconciliation
        );
        let captured = captured
            .lock()
            .expect("lock captured")
            .clone()
            .expect("captured");
        assert_eq!(captured.method, "DELETE");
        assert_eq!(
            captured.path,
            "/v1/accounts/ACC_TEST_0001/orders/BROKER_TEST_ORDER"
        );
        assert!(captured.authorization_is_bearer);
    }

    #[test]
    fn m3d2c_post_send_semantics_preserve_reconciliation_and_no_blind_retry() {
        let classify = |context, response| {
            broker_finam::classify_order_endpoint_local_http_response_for_context(
                context, &response,
            )
        };

        let accepted_without_id = classify(
            FinamOrderEndpointContext::Place,
            FinamOrderEndpointLocalHttpResponse::Response {
                status: 202,
                body: "{}".to_string(),
                retry_after_ms: None,
            },
        );
        assert_eq!(
            post_send_semantics(FinamOrderEndpointContext::Place, &accepted_without_id),
            M3d2PostSendOrderOutcomeSemantics::SubmittedPendingBrokerOrderIdReconciliation
        );

        let malformed = classify(
            FinamOrderEndpointContext::Place,
            FinamOrderEndpointLocalHttpResponse::Response {
                status: 200,
                body: "not-json".to_string(),
                retry_after_ms: None,
            },
        );
        assert_eq!(
            post_send_semantics(FinamOrderEndpointContext::Place, &malformed),
            M3d2PostSendOrderOutcomeSemantics::ReconciliationRequired
        );

        let body_read_failed = classify(
            FinamOrderEndpointContext::Cancel,
            FinamOrderEndpointLocalHttpResponse::BodyReadFailed { status: Some(200) },
        );
        assert_eq!(
            post_send_semantics(FinamOrderEndpointContext::Cancel, &body_read_failed),
            M3d2PostSendOrderOutcomeSemantics::ReconciliationRequired
        );

        let timeout = classify(
            FinamOrderEndpointContext::Place,
            FinamOrderEndpointLocalHttpResponse::Timeout,
        );
        assert_eq!(
            post_send_semantics(FinamOrderEndpointContext::Place, &timeout),
            M3d2PostSendOrderOutcomeSemantics::TimeoutUnknownPending
        );
    }
}
