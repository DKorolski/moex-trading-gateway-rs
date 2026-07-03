use async_trait::async_trait;
use broker_core::{
    BrokerOrderId, CommandAckReasonCode, OrderSide, OrderType, OutgoingOrderComment,
    PreflightApprovedCancelOrder, PreflightApprovedPlaceOrder, TimeInForce,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::dto::DecimalValue;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct FinamPlaceOrderRequest {
    pub symbol: String,
    pub quantity: DecimalValue,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<DecimalValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl std::fmt::Debug for FinamPlaceOrderRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FinamPlaceOrderRequest")
            .field("symbol_present", &!self.symbol.is_empty())
            .field("symbol_len", &self.symbol.len())
            .field("quantity_present", &!self.quantity.value.is_empty())
            .field("quantity_len", &self.quantity.value.len())
            .field("side_present", &!self.side.is_empty())
            .field("order_type_present", &!self.order_type.is_empty())
            .field("time_in_force", &self.time_in_force)
            .field("limit_price_present", &self.limit_price.is_some())
            .field(
                "limit_price_len",
                &self.limit_price.as_ref().map(|price| price.value.len()),
            )
            .field(
                "client_order_id_present",
                &self
                    .client_order_id
                    .as_ref()
                    .is_some_and(|value| !value.is_empty()),
            )
            .field(
                "client_order_id_len",
                &self.client_order_id.as_ref().map(|value| value.len()),
            )
            .field("comment_present", &self.comment.is_some())
            .field(
                "comment_len",
                &self.comment.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct FinamPlaceOrderRequestSpec {
    pub account_id: String,
    pub body: FinamPlaceOrderRequest,
}

impl std::fmt::Debug for FinamPlaceOrderRequestSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FinamPlaceOrderRequestSpec")
            .field("account_id_present", &!self.account_id.is_empty())
            .field("account_id_len", &self.account_id.len())
            .field("body", &self.body)
            .finish()
    }
}

impl FinamPlaceOrderRequestSpec {
    pub fn rest_path_segments(&self) -> Vec<String> {
        vec![
            "v1".to_string(),
            "accounts".to_string(),
            self.account_id.clone(),
            "orders".to_string(),
        ]
    }

    pub fn redacted_path_shape(&self) -> FinamOrderPathDiagnostic {
        FinamOrderPathDiagnostic {
            method: "POST".to_string(),
            path_template: "/v1/accounts/{account_id}/orders".to_string(),
            account_id_present: !self.account_id.is_empty(),
            account_id_len: self.account_id.len(),
            order_id_present: false,
            order_id_len: None,
        }
    }

    pub fn redacted_body_shape(&self) -> FinamPlaceOrderBodyDiagnostic {
        FinamPlaceOrderBodyDiagnostic {
            symbol_present: !self.body.symbol.is_empty(),
            symbol_len: self.body.symbol.len(),
            quantity_present: !self.body.quantity.value.is_empty(),
            quantity_len: self.body.quantity.value.len(),
            side_present: !self.body.side.is_empty(),
            order_type_present: !self.body.order_type.is_empty(),
            time_in_force_present: self.body.time_in_force.is_some(),
            limit_price_present: self.body.limit_price.is_some(),
            limit_price_len: self
                .body
                .limit_price
                .as_ref()
                .map(|price| price.value.len()),
            client_order_id_present: self.body.client_order_id.is_some(),
            client_order_id_len: self.body.client_order_id.as_ref().map(|value| value.len()),
            comment_present: self.body.comment.is_some(),
            comment_len: self.body.comment.as_ref().map(|value| value.len()),
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct FinamCancelOrderRequestSpec {
    pub account_id: String,
    pub order_id: String,
}

impl std::fmt::Debug for FinamCancelOrderRequestSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FinamCancelOrderRequestSpec")
            .field("account_id_present", &!self.account_id.is_empty())
            .field("account_id_len", &self.account_id.len())
            .field("order_id_present", &!self.order_id.is_empty())
            .field("order_id_len", &self.order_id.len())
            .finish()
    }
}

impl FinamCancelOrderRequestSpec {
    pub fn rest_path_segments(&self) -> Vec<String> {
        vec![
            "v1".to_string(),
            "accounts".to_string(),
            self.account_id.clone(),
            "orders".to_string(),
            self.order_id.clone(),
        ]
    }

    pub fn redacted_path_shape(&self) -> FinamOrderPathDiagnostic {
        FinamOrderPathDiagnostic {
            method: "DELETE".to_string(),
            path_template: "/v1/accounts/{account_id}/orders/{order_id}".to_string(),
            account_id_present: !self.account_id.is_empty(),
            account_id_len: self.account_id.len(),
            order_id_present: !self.order_id.is_empty(),
            order_id_len: Some(self.order_id.len()),
        }
    }
}

pub fn build_place_order_request(
    approved: &PreflightApprovedPlaceOrder,
    outgoing_comment: Option<&OutgoingOrderComment>,
) -> Result<FinamPlaceOrderRequestSpec, FinamOrderRequestBuildError> {
    let order = approved.order();
    if order.comment.is_some() {
        return Err(FinamOrderRequestBuildError::RawCommandCommentNotAllowed);
    }
    let symbol = order
        .instrument
        .venue_symbol
        .clone()
        .ok_or(FinamOrderRequestBuildError::MissingVenueSymbol)?;
    let (order_type, limit_price) = match order.order_type {
        OrderType::Market => {
            if order.limit_price.is_some() {
                return Err(FinamOrderRequestBuildError::MarketOrderHasLimitPrice);
            }
            ("ORDER_TYPE_MARKET".to_string(), None)
        }
        OrderType::Limit => (
            "ORDER_TYPE_LIMIT".to_string(),
            Some(DecimalValue {
                value: decimal_to_finam_string(
                    order
                        .limit_price
                        .ok_or(FinamOrderRequestBuildError::LimitPriceMissing)?,
                ),
            }),
        ),
        _ => return Err(FinamOrderRequestBuildError::UnsupportedOrderType),
    };

    Ok(FinamPlaceOrderRequestSpec {
        account_id: order.account_id.as_str().to_string(),
        body: FinamPlaceOrderRequest {
            symbol,
            quantity: DecimalValue {
                value: decimal_to_finam_string(order.qty),
            },
            side: finam_side(order.side).to_string(),
            order_type,
            time_in_force: Some(finam_time_in_force(order.time_in_force)?.to_string()),
            limit_price,
            client_order_id: Some(order.client_order_id.as_str().to_string()),
            comment: outgoing_comment.map(|comment| comment.value().to_string()),
        },
    })
}

pub fn build_cancel_order_request(
    approved: &PreflightApprovedCancelOrder,
) -> Result<FinamCancelOrderRequestSpec, FinamOrderRequestBuildError> {
    let cancel = approved.cancel();
    if cancel.order_id.as_str().is_empty() {
        return Err(FinamOrderRequestBuildError::MissingBrokerOrderId);
    }
    Ok(FinamCancelOrderRequestSpec {
        account_id: cancel.account_id.as_str().to_string(),
        order_id: cancel.order_id.as_str().to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamOrderPathDiagnostic {
    pub method: String,
    pub path_template: String,
    pub account_id_present: bool,
    pub account_id_len: usize,
    pub order_id_present: bool,
    pub order_id_len: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinamDryOrderRequestKind {
    Place,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamDryOrderRequestDiagnostic {
    pub kind: FinamDryOrderRequestKind,
    pub path: FinamOrderPathDiagnostic,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<FinamPlaceOrderBodyDiagnostic>,
}

pub trait FinamDryOrderClient {
    fn record_place_order_request(
        &mut self,
        spec: &FinamPlaceOrderRequestSpec,
    ) -> FinamDryOrderRequestDiagnostic;

    fn record_cancel_order_request(
        &mut self,
        spec: &FinamCancelOrderRequestSpec,
    ) -> FinamDryOrderRequestDiagnostic;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MockFinamDryOrderClient {
    requests: Vec<FinamDryOrderRequestDiagnostic>,
}

impl MockFinamDryOrderClient {
    pub fn requests(&self) -> &[FinamDryOrderRequestDiagnostic] {
        &self.requests
    }
}

impl FinamDryOrderClient for MockFinamDryOrderClient {
    fn record_place_order_request(
        &mut self,
        spec: &FinamPlaceOrderRequestSpec,
    ) -> FinamDryOrderRequestDiagnostic {
        let diagnostic = FinamDryOrderRequestDiagnostic {
            kind: FinamDryOrderRequestKind::Place,
            path: spec.redacted_path_shape(),
            body: Some(spec.redacted_body_shape()),
        };
        self.requests.push(diagnostic.clone());
        diagnostic
    }

    fn record_cancel_order_request(
        &mut self,
        spec: &FinamCancelOrderRequestSpec,
    ) -> FinamDryOrderRequestDiagnostic {
        let diagnostic = FinamDryOrderRequestDiagnostic {
            kind: FinamDryOrderRequestKind::Cancel,
            path: spec.redacted_path_shape(),
            body: None,
        };
        self.requests.push(diagnostic.clone());
        diagnostic
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum FinamOrderExecutionOutcome {
    Accepted {
        broker_order_id: Option<BrokerOrderId>,
    },
    Rejected {
        reason_code: CommandAckReasonCode,
    },
    Timeout,
}

impl std::fmt::Debug for FinamOrderExecutionOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accepted { broker_order_id } => formatter
                .debug_struct("FinamOrderExecutionOutcome::Accepted")
                .field("broker_order_id_present", &broker_order_id.is_some())
                .field(
                    "broker_order_id_len",
                    &broker_order_id.as_ref().map(|value| value.as_str().len()),
                )
                .finish(),
            Self::Rejected { reason_code } => formatter
                .debug_struct("FinamOrderExecutionOutcome::Rejected")
                .field("reason_code", reason_code)
                .finish(),
            Self::Timeout => formatter
                .debug_struct("FinamOrderExecutionOutcome::Timeout")
                .finish(),
        }
    }
}

/// FINAM accepted-order response DTO.
///
/// This type is intentionally deserialize-only. It may carry a broker-native
/// order id while parsing an endpoint response, so it must not become a
/// report/log/handoff export boundary. Export `FinamOrderEndpointResponseDiagnostic`
/// instead.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct FinamOrderEndpointAcceptedDto {
    #[serde(default)]
    #[serde(alias = "brokerOrderId", alias = "order_id", alias = "orderId")]
    pub broker_order_id: Option<String>,
}

impl std::fmt::Debug for FinamOrderEndpointAcceptedDto {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FinamOrderEndpointAcceptedDto")
            .field("broker_order_id_present", &self.broker_order_id.is_some())
            .field(
                "broker_order_id_len",
                &self.broker_order_id.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinamOrderEndpointRejectedCode {
    BrokerRejected,
    LocalBrokerValidation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinamOrderEndpointMaintenanceKind {
    ServiceInterval,
    MarketClosed,
    Unknown,
}

/// Synthetic endpoint fixture used by dry tests and local simulators.
///
/// It is intentionally not serde-exportable because an accepted fixture may
/// contain a raw broker order id. Use `redacted_diagnostic()` for every
/// diagnostic, review, or handoff path.
#[derive(Clone, PartialEq, Eq)]
pub enum FinamOrderEndpointFixture {
    Accepted(FinamOrderEndpointAcceptedDto),
    Rejected {
        reason_code: FinamOrderEndpointRejectedCode,
    },
    RateLimited {
        retry_after_ms: Option<u64>,
    },
    Maintenance {
        maintenance_kind: FinamOrderEndpointMaintenanceKind,
    },
    Timeout,
    DecodeError {
        status: Option<u16>,
        body_kind: Option<String>,
    },
}

impl std::fmt::Debug for FinamOrderEndpointFixture {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.redacted_diagnostic().fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamOrderEndpointResponseDiagnostic {
    pub kind: FinamOrderEndpointResponseKind,
    pub broker_order_id_present: bool,
    pub broker_order_id_len: Option<usize>,
    pub reason_code: Option<FinamOrderEndpointRejectedCode>,
    pub retry_after_ms_present: bool,
    pub maintenance_kind: Option<FinamOrderEndpointMaintenanceKind>,
    pub status: Option<u16>,
    pub body_kind: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinamOrderEndpointResponseKind {
    Accepted,
    Rejected,
    RateLimited,
    Maintenance,
    Unauthorized,
    ReconciliationRequired,
    Timeout,
    DecodeError,
}

impl FinamOrderEndpointFixture {
    pub fn redacted_diagnostic(&self) -> FinamOrderEndpointResponseDiagnostic {
        match self {
            Self::Accepted(dto) => FinamOrderEndpointResponseDiagnostic {
                kind: FinamOrderEndpointResponseKind::Accepted,
                broker_order_id_present: dto.broker_order_id.is_some(),
                broker_order_id_len: dto.broker_order_id.as_ref().map(|value| value.len()),
                reason_code: None,
                retry_after_ms_present: false,
                maintenance_kind: None,
                status: None,
                body_kind: None,
            },
            Self::Rejected { reason_code } => FinamOrderEndpointResponseDiagnostic {
                kind: FinamOrderEndpointResponseKind::Rejected,
                broker_order_id_present: false,
                broker_order_id_len: None,
                reason_code: Some(*reason_code),
                retry_after_ms_present: false,
                maintenance_kind: None,
                status: None,
                body_kind: None,
            },
            Self::RateLimited { retry_after_ms } => FinamOrderEndpointResponseDiagnostic {
                kind: FinamOrderEndpointResponseKind::RateLimited,
                broker_order_id_present: false,
                broker_order_id_len: None,
                reason_code: None,
                retry_after_ms_present: retry_after_ms.is_some(),
                maintenance_kind: None,
                status: None,
                body_kind: None,
            },
            Self::Maintenance { maintenance_kind } => FinamOrderEndpointResponseDiagnostic {
                kind: FinamOrderEndpointResponseKind::Maintenance,
                broker_order_id_present: false,
                broker_order_id_len: None,
                reason_code: None,
                retry_after_ms_present: false,
                maintenance_kind: Some(*maintenance_kind),
                status: None,
                body_kind: None,
            },
            Self::Timeout => FinamOrderEndpointResponseDiagnostic {
                kind: FinamOrderEndpointResponseKind::Timeout,
                broker_order_id_present: false,
                broker_order_id_len: None,
                reason_code: None,
                retry_after_ms_present: false,
                maintenance_kind: None,
                status: None,
                body_kind: None,
            },
            Self::DecodeError { status, body_kind } => FinamOrderEndpointResponseDiagnostic {
                kind: FinamOrderEndpointResponseKind::DecodeError,
                broker_order_id_present: false,
                broker_order_id_len: None,
                reason_code: None,
                retry_after_ms_present: false,
                maintenance_kind: None,
                status: *status,
                body_kind: body_kind.clone(),
            },
        }
    }

    pub fn map_fixture(
        &self,
    ) -> Result<FinamOrderEndpointMappedResult, FinamOrderEndpointMapperError> {
        match self {
            Self::Accepted(dto) => Ok(FinamOrderEndpointMappedResult::Execution(
                FinamOrderExecutionOutcome::Accepted {
                    broker_order_id: dto
                        .broker_order_id
                        .as_ref()
                        .map(|value| {
                            if value.is_empty() {
                                Err(FinamOrderEndpointMapperError::EmptyBrokerOrderId)
                            } else {
                                Ok(BrokerOrderId::new(value))
                            }
                        })
                        .transpose()?,
                },
            )),
            Self::Rejected { .. } => Ok(FinamOrderEndpointMappedResult::Execution(
                FinamOrderExecutionOutcome::Rejected {
                    reason_code: CommandAckReasonCode::BrokerRejected,
                },
            )),
            Self::RateLimited { retry_after_ms } => {
                Ok(FinamOrderEndpointMappedResult::RateLimited {
                    retry_after_ms: *retry_after_ms,
                })
            }
            Self::Maintenance { maintenance_kind } => {
                Ok(FinamOrderEndpointMappedResult::Maintenance {
                    maintenance_kind: *maintenance_kind,
                })
            }
            Self::Timeout => Ok(FinamOrderEndpointMappedResult::Execution(
                FinamOrderExecutionOutcome::Timeout,
            )),
            Self::DecodeError { status, body_kind } => {
                Ok(FinamOrderEndpointMappedResult::DecodeError {
                    status: *status,
                    body_kind: body_kind.clone(),
                })
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum FinamOrderEndpointMappedResult {
    Execution(FinamOrderExecutionOutcome),
    RateLimited {
        retry_after_ms: Option<u64>,
    },
    Maintenance {
        maintenance_kind: FinamOrderEndpointMaintenanceKind,
    },
    Unauthorized {
        status: u16,
    },
    ReconciliationRequired {
        status: u16,
    },
    DecodeError {
        status: Option<u16>,
        body_kind: Option<String>,
    },
}

impl std::fmt::Debug for FinamOrderEndpointMappedResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Execution(outcome) => formatter
                .debug_struct("FinamOrderEndpointMappedResult::Execution")
                .field("outcome", outcome)
                .finish(),
            Self::RateLimited { retry_after_ms } => formatter
                .debug_struct("FinamOrderEndpointMappedResult::RateLimited")
                .field("retry_after_ms_present", &retry_after_ms.is_some())
                .finish(),
            Self::Maintenance { maintenance_kind } => formatter
                .debug_struct("FinamOrderEndpointMappedResult::Maintenance")
                .field("maintenance_kind", maintenance_kind)
                .finish(),
            Self::Unauthorized { status } => formatter
                .debug_struct("FinamOrderEndpointMappedResult::Unauthorized")
                .field("status", status)
                .finish(),
            Self::ReconciliationRequired { status } => formatter
                .debug_struct("FinamOrderEndpointMappedResult::ReconciliationRequired")
                .field("status", status)
                .finish(),
            Self::DecodeError { status, body_kind } => formatter
                .debug_struct("FinamOrderEndpointMappedResult::DecodeError")
                .field("status", status)
                .field("body_kind", body_kind)
                .finish(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FinamOrderEndpointMapperError {
    #[error("FINAM order endpoint accepted response has empty broker order id")]
    EmptyBrokerOrderId,
}

#[derive(Clone, PartialEq, Eq)]
pub enum FinamOrderEndpointLocalHttpResponse {
    Response {
        status: u16,
        body: String,
        retry_after_ms: Option<u64>,
    },
    BodyReadFailed {
        status: Option<u16>,
    },
    Timeout,
}

impl std::fmt::Debug for FinamOrderEndpointLocalHttpResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Response {
                status,
                body,
                retry_after_ms,
            } => formatter
                .debug_struct("FinamOrderEndpointLocalHttpResponse::Response")
                .field("status", status)
                .field("body_len", &body.len())
                .field("body_kind", &json_body_kind(body))
                .field("retry_after_ms_present", &retry_after_ms.is_some())
                .finish(),
            Self::BodyReadFailed { status } => formatter
                .debug_struct("FinamOrderEndpointLocalHttpResponse::BodyReadFailed")
                .field("status", status)
                .finish(),
            Self::Timeout => formatter
                .debug_struct("FinamOrderEndpointLocalHttpResponse::Timeout")
                .finish(),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct FinamOrderEndpointClassifiedResponse {
    pub result: FinamOrderEndpointMappedResult,
    pub diagnostic: FinamOrderEndpointResponseDiagnostic,
}

impl std::fmt::Debug for FinamOrderEndpointClassifiedResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FinamOrderEndpointClassifiedResponse")
            .field("result", &self.result)
            .field("diagnostic", &self.diagnostic)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinamOrderEndpointContext {
    Place,
    Cancel,
}

pub fn classify_order_endpoint_local_http_response(
    response: &FinamOrderEndpointLocalHttpResponse,
) -> FinamOrderEndpointClassifiedResponse {
    classify_order_endpoint_local_http_response_for_context(
        FinamOrderEndpointContext::Place,
        response,
    )
}

pub fn classify_order_endpoint_local_http_response_for_context(
    context: FinamOrderEndpointContext,
    response: &FinamOrderEndpointLocalHttpResponse,
) -> FinamOrderEndpointClassifiedResponse {
    match response {
        FinamOrderEndpointLocalHttpResponse::Timeout => {
            let fixture = FinamOrderEndpointFixture::Timeout;
            FinamOrderEndpointClassifiedResponse {
                result: FinamOrderEndpointMappedResult::Execution(
                    FinamOrderExecutionOutcome::Timeout,
                ),
                diagnostic: fixture.redacted_diagnostic(),
            }
        }
        FinamOrderEndpointLocalHttpResponse::BodyReadFailed { status } => {
            decode_error_response(*status, Some("body_read_failed".to_string()))
        }
        FinamOrderEndpointLocalHttpResponse::Response {
            status,
            body,
            retry_after_ms,
        } if *status == 401 || *status == 403 => FinamOrderEndpointClassifiedResponse {
            result: FinamOrderEndpointMappedResult::Unauthorized { status: *status },
            diagnostic: FinamOrderEndpointResponseDiagnostic {
                kind: FinamOrderEndpointResponseKind::Unauthorized,
                broker_order_id_present: false,
                broker_order_id_len: None,
                reason_code: None,
                retry_after_ms_present: false,
                maintenance_kind: None,
                status: Some(*status),
                body_kind: Some(json_body_kind(body)),
            },
        },
        FinamOrderEndpointLocalHttpResponse::Response {
            status,
            body,
            retry_after_ms,
        } if *status == 429 => {
            let fixture = FinamOrderEndpointFixture::RateLimited {
                retry_after_ms: *retry_after_ms,
            };
            let mut diagnostic = fixture.redacted_diagnostic();
            diagnostic.status = Some(*status);
            diagnostic.body_kind = Some(json_body_kind(body));
            FinamOrderEndpointClassifiedResponse {
                result: FinamOrderEndpointMappedResult::RateLimited {
                    retry_after_ms: *retry_after_ms,
                },
                diagnostic,
            }
        }
        FinamOrderEndpointLocalHttpResponse::Response { status, body, .. }
            if *status == 408 || *status == 504 =>
        {
            let fixture = FinamOrderEndpointFixture::Timeout;
            let mut diagnostic = fixture.redacted_diagnostic();
            diagnostic.status = Some(*status);
            diagnostic.body_kind = Some(json_body_kind(body));
            FinamOrderEndpointClassifiedResponse {
                result: FinamOrderEndpointMappedResult::Execution(
                    FinamOrderExecutionOutcome::Timeout,
                ),
                diagnostic,
            }
        }
        FinamOrderEndpointLocalHttpResponse::Response { status, body, .. }
            if *status == 500 || *status == 502 || *status == 503 =>
        {
            let maintenance_kind = if *status == 503 {
                FinamOrderEndpointMaintenanceKind::ServiceInterval
            } else {
                FinamOrderEndpointMaintenanceKind::Unknown
            };
            let fixture = FinamOrderEndpointFixture::Maintenance { maintenance_kind };
            let mut diagnostic = fixture.redacted_diagnostic();
            diagnostic.status = Some(*status);
            diagnostic.body_kind = Some(json_body_kind(body));
            FinamOrderEndpointClassifiedResponse {
                result: FinamOrderEndpointMappedResult::Maintenance { maintenance_kind },
                diagnostic,
            }
        }
        FinamOrderEndpointLocalHttpResponse::Response { status, body, .. }
            if (200..300).contains(status) =>
        {
            classify_success_order_endpoint_response(*status, body)
        }
        FinamOrderEndpointLocalHttpResponse::Response { status, body, .. }
            if context == FinamOrderEndpointContext::Cancel
                && matches!(*status, 404 | 409 | 410) =>
        {
            FinamOrderEndpointClassifiedResponse {
                result: FinamOrderEndpointMappedResult::ReconciliationRequired { status: *status },
                diagnostic: FinamOrderEndpointResponseDiagnostic {
                    kind: FinamOrderEndpointResponseKind::ReconciliationRequired,
                    broker_order_id_present: false,
                    broker_order_id_len: None,
                    reason_code: None,
                    retry_after_ms_present: false,
                    maintenance_kind: None,
                    status: Some(*status),
                    body_kind: Some(json_body_kind(body)),
                },
            }
        }
        FinamOrderEndpointLocalHttpResponse::Response { status, body, .. }
            if (400..500).contains(status) =>
        {
            let fixture = FinamOrderEndpointFixture::Rejected {
                reason_code: FinamOrderEndpointRejectedCode::BrokerRejected,
            };
            let mut diagnostic = fixture.redacted_diagnostic();
            diagnostic.status = Some(*status);
            diagnostic.body_kind = Some(json_body_kind(body));
            FinamOrderEndpointClassifiedResponse {
                result: FinamOrderEndpointMappedResult::Execution(
                    FinamOrderExecutionOutcome::Rejected {
                        reason_code: CommandAckReasonCode::BrokerRejected,
                    },
                ),
                diagnostic,
            }
        }
        FinamOrderEndpointLocalHttpResponse::Response { status, body, .. } => {
            decode_error_response(Some(*status), Some(json_body_kind(body)))
        }
    }
}

fn classify_success_order_endpoint_response(
    status: u16,
    body: &str,
) -> FinamOrderEndpointClassifiedResponse {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return decode_error_response(Some(status), Some("malformed_json".to_string()));
    };
    let body_kind = json_value_kind(&value).to_string();
    let Ok(dto) = serde_json::from_value::<FinamOrderEndpointAcceptedDto>(value) else {
        return decode_error_response(Some(status), Some(body_kind));
    };
    if dto.broker_order_id.as_deref().is_some_and(str::is_empty) {
        return decode_error_response(Some(status), Some(body_kind));
    }

    let diagnostic = FinamOrderEndpointFixture::Accepted(dto.clone()).redacted_diagnostic();
    FinamOrderEndpointClassifiedResponse {
        result: FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Accepted {
            broker_order_id: dto.broker_order_id.map(|value| BrokerOrderId::new(&value)),
        }),
        diagnostic: FinamOrderEndpointResponseDiagnostic {
            status: Some(status),
            body_kind: Some(body_kind),
            ..diagnostic
        },
    }
}

fn decode_error_response(
    status: Option<u16>,
    body_kind: Option<String>,
) -> FinamOrderEndpointClassifiedResponse {
    let fixture = FinamOrderEndpointFixture::DecodeError {
        status,
        body_kind: body_kind.clone(),
    };
    FinamOrderEndpointClassifiedResponse {
        result: FinamOrderEndpointMappedResult::DecodeError { status, body_kind },
        diagnostic: fixture.redacted_diagnostic(),
    }
}

fn json_body_kind(body: &str) -> String {
    if body.is_empty() {
        return "empty".to_string();
    }
    serde_json::from_str::<serde_json::Value>(body)
        .map(|value| json_value_kind(&value).to_string())
        .unwrap_or_else(|_| "malformed_json".to_string())
}

fn json_value_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Object(_) => "object",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Null => "null",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinamMockExecutionDiagnosticOutcome {
    Accepted {
        broker_order_id_present: bool,
        broker_order_id_len: Option<usize>,
    },
    Rejected {
        reason_code: CommandAckReasonCode,
    },
    Timeout,
}

impl From<&FinamOrderExecutionOutcome> for FinamMockExecutionDiagnosticOutcome {
    fn from(outcome: &FinamOrderExecutionOutcome) -> Self {
        match outcome {
            FinamOrderExecutionOutcome::Accepted { broker_order_id } => {
                FinamMockExecutionDiagnosticOutcome::Accepted {
                    broker_order_id_present: broker_order_id.is_some(),
                    broker_order_id_len: broker_order_id.as_ref().map(|value| value.as_str().len()),
                }
            }
            FinamOrderExecutionOutcome::Rejected { reason_code } => {
                FinamMockExecutionDiagnosticOutcome::Rejected {
                    reason_code: *reason_code,
                }
            }
            FinamOrderExecutionOutcome::Timeout => FinamMockExecutionDiagnosticOutcome::Timeout,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamMockExecutionRecord {
    pub kind: FinamDryOrderRequestKind,
    pub request: FinamDryOrderRequestDiagnostic,
    pub outcome: FinamMockExecutionDiagnosticOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FinamOrderExecutionError {
    #[error("mock FINAM order execution script exhausted")]
    MockScriptExhausted,
}

/// Dry-only approved order execution client used by M3a/M3b simulators.
///
/// Production FINAM order transport must not implement or reuse this trait.
/// The real endpoint boundary is `EndpointGateApproved + request spec ->
/// FinamOrderEndpointClassifiedResponse`.
#[async_trait]
pub trait FinamDryApprovedOrderExecutionClient: Send {
    async fn place_approved(
        &mut self,
        spec: FinamPlaceOrderRequestSpec,
    ) -> Result<FinamOrderExecutionOutcome, FinamOrderExecutionError>;

    async fn cancel_approved(
        &mut self,
        spec: FinamCancelOrderRequestSpec,
    ) -> Result<FinamOrderExecutionOutcome, FinamOrderExecutionError>;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MockFinamDryApprovedOrderExecutionClient {
    script: Vec<FinamOrderExecutionOutcome>,
    next_index: usize,
    records: Vec<FinamMockExecutionRecord>,
}

impl MockFinamDryApprovedOrderExecutionClient {
    pub fn new(script: Vec<FinamOrderExecutionOutcome>) -> Self {
        Self {
            script,
            next_index: 0,
            records: Vec::new(),
        }
    }

    pub fn records(&self) -> &[FinamMockExecutionRecord] {
        &self.records
    }

    fn next_outcome(&mut self) -> Result<FinamOrderExecutionOutcome, FinamOrderExecutionError> {
        let outcome = self
            .script
            .get(self.next_index)
            .cloned()
            .ok_or(FinamOrderExecutionError::MockScriptExhausted)?;
        self.next_index += 1;
        Ok(outcome)
    }
}

#[async_trait]
impl FinamDryApprovedOrderExecutionClient for MockFinamDryApprovedOrderExecutionClient {
    async fn place_approved(
        &mut self,
        spec: FinamPlaceOrderRequestSpec,
    ) -> Result<FinamOrderExecutionOutcome, FinamOrderExecutionError> {
        let outcome = self.next_outcome()?;
        self.records.push(FinamMockExecutionRecord {
            kind: FinamDryOrderRequestKind::Place,
            request: FinamDryOrderRequestDiagnostic {
                kind: FinamDryOrderRequestKind::Place,
                path: spec.redacted_path_shape(),
                body: Some(spec.redacted_body_shape()),
            },
            outcome: FinamMockExecutionDiagnosticOutcome::from(&outcome),
        });
        Ok(outcome)
    }

    async fn cancel_approved(
        &mut self,
        spec: FinamCancelOrderRequestSpec,
    ) -> Result<FinamOrderExecutionOutcome, FinamOrderExecutionError> {
        let outcome = self.next_outcome()?;
        self.records.push(FinamMockExecutionRecord {
            kind: FinamDryOrderRequestKind::Cancel,
            request: FinamDryOrderRequestDiagnostic {
                kind: FinamDryOrderRequestKind::Cancel,
                path: spec.redacted_path_shape(),
                body: None,
            },
            outcome: FinamMockExecutionDiagnosticOutcome::from(&outcome),
        });
        Ok(outcome)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamPlaceOrderBodyDiagnostic {
    pub symbol_present: bool,
    pub symbol_len: usize,
    pub quantity_present: bool,
    pub quantity_len: usize,
    pub side_present: bool,
    pub order_type_present: bool,
    pub time_in_force_present: bool,
    pub limit_price_present: bool,
    pub limit_price_len: Option<usize>,
    pub client_order_id_present: bool,
    pub client_order_id_len: Option<usize>,
    pub comment_present: bool,
    pub comment_len: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FinamOrderRequestBuildError {
    #[error("venue symbol is missing")]
    MissingVenueSymbol,
    #[error("order type is unsupported")]
    UnsupportedOrderType,
    #[error("limit price is required for limit order")]
    LimitPriceMissing,
    #[error("market order must not carry limit price")]
    MarketOrderHasLimitPrice,
    #[error("raw broker command comment is not allowed")]
    RawCommandCommentNotAllowed,
    #[error("broker order id is missing")]
    MissingBrokerOrderId,
    #[error("time in force is unsupported")]
    UnsupportedTimeInForce,
}

fn decimal_to_finam_string(value: Decimal) -> String {
    value.normalize().to_string()
}

fn finam_side(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "SIDE_BUY",
        OrderSide::Sell => "SIDE_SELL",
    }
}

fn finam_time_in_force(
    time_in_force: TimeInForce,
) -> Result<&'static str, FinamOrderRequestBuildError> {
    match time_in_force {
        TimeInForce::Day => Ok("TIME_IN_FORCE_DAY"),
        TimeInForce::GoodTillCancel => Ok("TIME_IN_FORCE_GOOD_TILL_CANCEL"),
        TimeInForce::GoodTillDate => Err(FinamOrderRequestBuildError::UnsupportedTimeInForce),
        TimeInForce::FillOrKill => Ok("TIME_IN_FORCE_FOK"),
        TimeInForce::ImmediateOrCancel => Ok("TIME_IN_FORCE_IOC"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        AccountId, BrokerOrderId, CancelOrder, CancelPreflightApproval, ClientOrderId, Exchange,
        InstrumentId, Market, OperatorArm, OrderPathEvent, OrderPathRecord, OrderPathState,
        OrderPreflightContext, OrderPreflightPolicy, OrderReferencePrice, OutgoingCommentIntent,
        OutgoingOrderCommentPolicy, PlaceOrder, PreflightApprovedCancelOrder,
        PreflightApprovedPlaceOrder, StrategyRequestId,
    };
    use chrono::{DateTime, TimeZone, Utc};
    use rust_decimal::Decimal;
    use serde_json::json;
    use uuid::Uuid;

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
            request_id: request_id(1),
            created_ts: Utc
                .with_ymd_and_hms(2026, 6, 30, 9, 10, 0)
                .single()
                .expect("timestamp"),
            ttl_ms: Some(1_000),
            account_id: AccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::new("CID000000000000001").expect("client id"),
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
            session_id: "ARM_TEST_1".to_string(),
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

    fn preflight_policy_with_tif(
        now: DateTime<Utc>,
        allowed_time_in_force: Vec<TimeInForce>,
    ) -> OrderPreflightPolicy {
        OrderPreflightPolicy {
            allowed_time_in_force,
            ..preflight_policy(now)
        }
    }

    fn approve_place(order: &PlaceOrder) -> PreflightApprovedPlaceOrder {
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
            .expect("place preflight approval")
    }

    fn approve_cancel(cancel: &CancelOrder) -> PreflightApprovedCancelOrder {
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
            .expect("cancel preflight approval")
        {
            CancelPreflightApproval::Submit(approved) => approved,
            CancelPreflightApproval::AlreadyTerminal => panic!("expected submit approval"),
        }
    }

    #[test]
    fn builds_limit_place_order_body_without_sending_http() {
        let order = place_order();
        let approved = approve_place(&order);
        let spec = build_place_order_request(&approved, None).expect("request spec");

        assert_eq!(
            spec.rest_path_segments(),
            vec!["v1", "accounts", "ACC_TEST_0001", "orders"]
        );
        assert_eq!(
            spec.redacted_path_shape().path_template,
            "/v1/accounts/{account_id}/orders"
        );
        let diagnostic = spec.redacted_body_shape();
        assert!(diagnostic.symbol_present);
        assert_eq!(diagnostic.symbol_len, "TESTFUT@TEST".len());
        assert!(diagnostic.limit_price_present);
        let body = serde_json::to_value(&spec.body).expect("body");
        assert_eq!(
            body,
            json!({
                "symbol": "TESTFUT@TEST",
                "quantity": { "value": "1" },
                "side": "SIDE_BUY",
                "type": "ORDER_TYPE_LIMIT",
                "time_in_force": "TIME_IN_FORCE_DAY",
                "limit_price": { "value": "100.5" },
                "client_order_id": "CID000000000000001"
            })
        );
    }

    #[test]
    fn builds_market_place_order_body_and_rejects_raw_command_comment() {
        let mut order = place_order();
        order.order_type = OrderType::Market;
        order.limit_price = None;
        order.side = OrderSide::Sell;
        let approved = approve_place(&order);

        let body = serde_json::to_value(
            &build_place_order_request(&approved, None)
                .expect("market request")
                .body,
        )
        .expect("body");
        assert_eq!(body["type"], "ORDER_TYPE_MARKET");
        assert_eq!(body["side"], "SIDE_SELL");
        assert!(body.get("limit_price").is_none());

        order.comment = Some("raw broker comment must not leak".to_string());
        assert_eq!(
            preflight_policy(order.created_ts)
                .approve_place_order(&order, order.created_ts)
                .expect_err("raw comment"),
            broker_core::OrderPreflightError::RawCommandCommentNotAllowed
        );
    }

    #[test]
    fn maps_supported_finam_time_in_force_to_pinned_wire_enums() {
        assert_eq!(
            finam_time_in_force(TimeInForce::Day).expect("day"),
            "TIME_IN_FORCE_DAY"
        );
        assert_eq!(
            finam_time_in_force(TimeInForce::GoodTillCancel).expect("gtc"),
            "TIME_IN_FORCE_GOOD_TILL_CANCEL"
        );
        assert_eq!(
            finam_time_in_force(TimeInForce::ImmediateOrCancel).expect("ioc"),
            "TIME_IN_FORCE_IOC"
        );
        assert_eq!(
            finam_time_in_force(TimeInForce::FillOrKill).expect("fok"),
            "TIME_IN_FORCE_FOK"
        );
    }

    #[test]
    fn blocks_good_till_date_for_plain_place_order_until_valid_before_support() {
        let mut order = place_order();
        order.time_in_force = TimeInForce::GoodTillDate;
        let now = order.created_ts + chrono::Duration::milliseconds(1);
        let context = OrderPreflightContext {
            reference_price: Some(OrderReferencePrice {
                price: order.limit_price.expect("limit price"),
                received_ts: now,
            }),
            current_run_notional: Decimal::ZERO,
        };
        let approved = preflight_policy_with_tif(now, vec![TimeInForce::GoodTillDate])
            .approve_place_order_with_context(&order, now, &context)
            .expect("preflight can be configured to allow GoodTillDate");

        assert_eq!(
            build_place_order_request(&approved, None).expect_err("blocked by FINAM mapper"),
            FinamOrderRequestBuildError::UnsupportedTimeInForce
        );
    }

    #[test]
    fn pinned_finam_spec_fixture_contains_supported_time_in_force_enums() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/finam_spec/order_contract_enums_v2026_07_03.json"
        ))
        .expect("fixture json");
        let values = fixture["time_in_force"]
            .as_array()
            .expect("time_in_force array");

        for expected in [
            "TIME_IN_FORCE_DAY",
            "TIME_IN_FORCE_GOOD_TILL_CANCEL",
            "TIME_IN_FORCE_IOC",
            "TIME_IN_FORCE_FOK",
        ] {
            assert!(
                values.iter().any(|value| value == expected),
                "missing pinned FINAM TIF enum {expected}"
            );
        }
        assert!(
            !values
                .iter()
                .any(|value| value == "TIME_IN_FORCE_GOOD_TILL_DATE"),
            "plain order GoodTillDate must not be introduced silently"
        );
    }

    #[test]
    fn every_pinned_finam_time_in_force_has_plain_order_policy() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/finam_spec/order_contract_enums_v2026_07_03.json"
        ))
        .expect("fixture json");
        let values = fixture["time_in_force"]
            .as_array()
            .expect("time_in_force array");
        let policy = fixture["time_in_force_plain_order_policy"]
            .as_object()
            .expect("time_in_force_plain_order_policy object");

        assert_eq!(values.len(), policy.len());
        for value in values {
            let value = value.as_str().expect("tif string");
            let policy_value = policy
                .get(value)
                .and_then(|value| value.as_str())
                .unwrap_or_else(|| panic!("missing TIF plain-order policy for {value}"));
            match policy_value {
                "supported" => assert!(matches!(
                    value,
                    "TIME_IN_FORCE_DAY"
                        | "TIME_IN_FORCE_GOOD_TILL_CANCEL"
                        | "TIME_IN_FORCE_IOC"
                        | "TIME_IN_FORCE_FOK"
                )),
                "unsupported" => assert!(!matches!(
                    value,
                    "TIME_IN_FORCE_DAY"
                        | "TIME_IN_FORCE_GOOD_TILL_CANCEL"
                        | "TIME_IN_FORCE_IOC"
                        | "TIME_IN_FORCE_FOK"
                )),
                other => panic!("unknown TIF plain-order policy {other}"),
            }
        }
    }

    #[test]
    fn every_pinned_finam_valid_before_has_plain_order_policy() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/finam_spec/order_contract_enums_v2026_07_03.json"
        ))
        .expect("fixture json");
        let values = fixture["valid_before"]
            .as_array()
            .expect("valid_before array");
        let policy = fixture["valid_before_plain_order_policy"]
            .as_object()
            .expect("valid_before_plain_order_policy object");

        assert_eq!(values.len(), policy.len());
        for value in values {
            let value = value.as_str().expect("valid_before string");
            let policy_value = policy
                .get(value)
                .and_then(|value| value.as_str())
                .unwrap_or_else(|| panic!("missing valid_before plain-order policy for {value}"));
            match (value, policy_value) {
                ("VALID_BEFORE_GOOD_TILL_DATE", "sltp_only") => {}
                (_, "unsupported") => {}
                (_, other) => {
                    panic!("unexpected valid_before plain-order policy {other} for {value}")
                }
            }
        }
    }

    #[test]
    fn uses_only_policy_generated_outgoing_comment() {
        let order = place_order();
        let policy = OutgoingOrderCommentPolicy {
            mode: broker_core::CommentPolicyMode::SanitizedDeterministic,
            max_len: 64,
        };
        let comment = policy
            .build(OutgoingCommentIntent {
                strategy_id: "micro",
                intent_class: "entry",
                client_order_id: &order.client_order_id,
            })
            .expect("comment policy")
            .expect("comment");

        let approved = approve_place(&order);
        let spec = build_place_order_request(&approved, Some(&comment)).expect("request");
        let body = serde_json::to_value(&spec.body).expect("body");
        assert_eq!(
            body["comment"],
            "strategy=micro;intent=entry;cid=CID000000000000001"
        );
        assert!(!format!("{spec:?}").contains("CID000000000000001"));
        assert!(!format!("{spec:?}").contains("strategy=micro"));
    }

    #[test]
    fn builds_cancel_order_path_without_body_or_http_send() {
        let cancel = CancelOrder {
            request_id: request_id(2),
            created_ts: Utc
                .with_ymd_and_hms(2026, 6, 30, 9, 11, 0)
                .single()
                .expect("timestamp"),
            ttl_ms: Some(1_000),
            account_id: AccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new("BROKER_TEST_1"),
            client_order_id: None,
        };

        let approved = approve_cancel(&cancel);
        let spec = build_cancel_order_request(&approved).expect("cancel spec");
        assert_eq!(
            spec.rest_path_segments(),
            vec!["v1", "accounts", "ACC_TEST_0001", "orders", "BROKER_TEST_1"]
        );
        assert_eq!(
            spec.redacted_path_shape().path_template,
            "/v1/accounts/{account_id}/orders/{order_id}"
        );
        assert!(!format!("{spec:?}").contains("BROKER_TEST_1"));
        assert!(!format!("{spec:?}").contains("ACC_TEST_0001"));
    }

    #[test]
    fn mock_dry_order_client_records_only_redacted_diagnostics() {
        let order = place_order();
        let approved = approve_place(&order);
        let place_spec = build_place_order_request(&approved, None).expect("place spec");
        let cancel = CancelOrder {
            request_id: request_id(3),
            created_ts: Utc
                .with_ymd_and_hms(2026, 6, 30, 9, 12, 0)
                .single()
                .expect("timestamp"),
            ttl_ms: Some(1_000),
            account_id: AccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new("BROKER_TEST_3"),
            client_order_id: None,
        };
        let approved_cancel = approve_cancel(&cancel);
        let cancel_spec = build_cancel_order_request(&approved_cancel).expect("cancel spec");
        let mut client = MockFinamDryOrderClient::default();

        client.record_place_order_request(&place_spec);
        client.record_cancel_order_request(&cancel_spec);

        assert_eq!(client.requests().len(), 2);
        assert_eq!(client.requests()[0].kind, FinamDryOrderRequestKind::Place);
        assert_eq!(client.requests()[1].kind, FinamDryOrderRequestKind::Cancel);
        let rendered = serde_json::to_string(client.requests()).expect("diagnostics serialize");
        assert!(rendered.contains("/v1/accounts/{account_id}/orders"));
        assert!(!rendered.contains("ACC_TEST_0001"));
        assert!(!rendered.contains("BROKER_TEST_3"));
        assert!(!rendered.contains("TESTFUT@TEST"));
        assert!(!rendered.contains("CID000000000000001"));
    }

    #[test]
    fn dry_approved_execution_client_contract_is_request_spec_based() {
        fn assert_client<T: FinamDryApprovedOrderExecutionClient>() {}
        fn assert_place_boundary<T: FinamDryApprovedOrderExecutionClient>(
            _client: &mut T,
            _spec: FinamPlaceOrderRequestSpec,
        ) {
        }
        fn assert_cancel_boundary<T: FinamDryApprovedOrderExecutionClient>(
            _client: &mut T,
            _spec: FinamCancelOrderRequestSpec,
        ) {
        }

        assert_client::<MockFinamDryApprovedOrderExecutionClient>();
        let source = include_str!("order_request.rs");
        let production_source = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        assert!(production_source.contains("pub trait FinamDryApprovedOrderExecutionClient"));
        assert!(!production_source.contains("pub trait FinamApprovedOrderExecutionClient"));
        let order = place_order();
        let approved = approve_place(&order);
        let place_spec = build_place_order_request(&approved, None).expect("place spec");
        let cancel = CancelOrder {
            request_id: request_id(4),
            created_ts: Utc
                .with_ymd_and_hms(2026, 6, 30, 9, 13, 0)
                .single()
                .expect("timestamp"),
            ttl_ms: Some(1_000),
            account_id: AccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new("BROKER_TEST_4"),
            client_order_id: None,
        };
        let approved_cancel = approve_cancel(&cancel);
        let cancel_spec = build_cancel_order_request(&approved_cancel).expect("cancel spec");
        let mut client = MockFinamDryApprovedOrderExecutionClient::new(Vec::new());

        assert_place_boundary(&mut client, place_spec);
        assert_cancel_boundary(&mut client, cancel_spec);
    }

    #[test]
    fn endpoint_response_fixtures_map_without_raw_debug_leaks() {
        let accepted = FinamOrderEndpointFixture::Accepted(FinamOrderEndpointAcceptedDto {
            broker_order_id: Some("BROKER_TEST_ACCEPTED_1".to_string()),
        });
        assert_eq!(
            accepted.map_fixture().expect("accepted maps"),
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Accepted {
                broker_order_id: Some(BrokerOrderId::new("BROKER_TEST_ACCEPTED_1")),
            })
        );
        let accepted_debug = format!("{accepted:?}");
        assert!(accepted_debug.contains("broker_order_id_len"));
        assert!(!accepted_debug.contains("BROKER_TEST_ACCEPTED_1"));
        let diagnostic_json =
            serde_json::to_string(&accepted.redacted_diagnostic()).expect("diagnostic serializes");
        assert!(!diagnostic_json.contains("BROKER_TEST_ACCEPTED_1"));

        let accepted_without_id =
            FinamOrderEndpointFixture::Accepted(FinamOrderEndpointAcceptedDto {
                broker_order_id: None,
            });
        assert_eq!(
            accepted_without_id
                .map_fixture()
                .expect("accepted without id"),
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Accepted {
                broker_order_id: None,
            })
        );

        let rejected = FinamOrderEndpointFixture::Rejected {
            reason_code: FinamOrderEndpointRejectedCode::BrokerRejected,
        };
        assert_eq!(
            rejected.map_fixture().expect("rejected maps"),
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Rejected {
                reason_code: CommandAckReasonCode::BrokerRejected,
            })
        );

        let rate_limited = FinamOrderEndpointFixture::RateLimited {
            retry_after_ms: Some(1_000),
        };
        assert_eq!(
            rate_limited.map_fixture().expect("rate limited maps"),
            FinamOrderEndpointMappedResult::RateLimited {
                retry_after_ms: Some(1_000),
            }
        );

        let maintenance = FinamOrderEndpointFixture::Maintenance {
            maintenance_kind: FinamOrderEndpointMaintenanceKind::ServiceInterval,
        };
        assert_eq!(
            maintenance.map_fixture().expect("maintenance maps"),
            FinamOrderEndpointMappedResult::Maintenance {
                maintenance_kind: FinamOrderEndpointMaintenanceKind::ServiceInterval,
            }
        );

        let timeout = FinamOrderEndpointFixture::Timeout;
        assert_eq!(
            timeout.map_fixture().expect("timeout maps"),
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Timeout)
        );

        let decode_error = FinamOrderEndpointFixture::DecodeError {
            status: Some(502),
            body_kind: Some("object".to_string()),
        };
        assert_eq!(
            decode_error.map_fixture().expect("decode error maps"),
            FinamOrderEndpointMappedResult::DecodeError {
                status: Some(502),
                body_kind: Some("object".to_string()),
            }
        );
    }

    #[test]
    fn endpoint_response_raw_dto_and_fixture_are_not_serde_export_boundaries() {
        let source = include_str!("order_request.rs");
        assert!(source.contains(
            "#[derive(Clone, PartialEq, Eq, Deserialize)]\npub struct FinamOrderEndpointAcceptedDto"
        ));
        assert!(!source.contains(
            "#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]\npub struct FinamOrderEndpointAcceptedDto"
        ));
        assert!(!source.contains(
            "#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum FinamOrderEndpointFixture"
        ));

        let dto = serde_json::from_value::<FinamOrderEndpointAcceptedDto>(
            json!({"broker_order_id": "BROKER_TEST_DTO_BOUNDARY"}),
        )
        .expect("accepted dto still deserializes");
        assert_eq!(
            dto.broker_order_id.as_deref(),
            Some("BROKER_TEST_DTO_BOUNDARY")
        );
        let dto_debug = format!("{dto:?}");
        assert!(dto_debug.contains("broker_order_id_len"));
        assert!(!dto_debug.contains("BROKER_TEST_DTO_BOUNDARY"));
    }

    #[test]
    fn endpoint_response_diagnostics_are_redacted_for_all_result_kinds() {
        let responses = vec![
            classify_order_endpoint_local_http_response(
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status: 200,
                    body: json!({"broker_order_id": "BROKER_TEST_DIAG_ACCEPTED"}).to_string(),
                    retry_after_ms: None,
                },
            ),
            classify_order_endpoint_local_http_response(
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status: 422,
                    body: json!({"message": "raw broker rejection body must not leak"}).to_string(),
                    retry_after_ms: None,
                },
            ),
            classify_order_endpoint_local_http_response(
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status: 429,
                    body: json!({"message": "raw rate body must not leak"}).to_string(),
                    retry_after_ms: Some(1_000),
                },
            ),
            classify_order_endpoint_local_http_response(
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status: 503,
                    body: json!({"message": "raw maintenance body must not leak"}).to_string(),
                    retry_after_ms: None,
                },
            ),
            classify_order_endpoint_local_http_response(
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status: 401,
                    body: json!({"message": "raw auth body must not leak"}).to_string(),
                    retry_after_ms: None,
                },
            ),
            classify_order_endpoint_local_http_response_for_context(
                FinamOrderEndpointContext::Cancel,
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status: 404,
                    body: json!({"message": "raw cancel uncertainty body must not leak"})
                        .to_string(),
                    retry_after_ms: None,
                },
            ),
            classify_order_endpoint_local_http_response(
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status: 504,
                    body: json!({"message": "raw timeout body must not leak"}).to_string(),
                    retry_after_ms: None,
                },
            ),
            classify_order_endpoint_local_http_response(
                &FinamOrderEndpointLocalHttpResponse::BodyReadFailed { status: Some(200) },
            ),
            classify_order_endpoint_local_http_response(
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status: 200,
                    body: "{raw malformed body must not leak".to_string(),
                    retry_after_ms: None,
                },
            ),
        ];

        for classified in responses {
            let rendered = format!("{classified:?}");
            let diagnostic_json =
                serde_json::to_string(&classified.diagnostic).expect("diagnostic export");
            for forbidden in [
                "BROKER_TEST_DIAG_ACCEPTED",
                "raw broker rejection body must not leak",
                "raw rate body must not leak",
                "raw maintenance body must not leak",
                "raw auth body must not leak",
                "raw cancel uncertainty body must not leak",
                "raw timeout body must not leak",
                "raw malformed body must not leak",
            ] {
                assert!(!rendered.contains(forbidden));
                assert!(!diagnostic_json.contains(forbidden));
            }
        }
    }

    #[test]
    fn endpoint_response_fixture_rejects_empty_broker_order_id() {
        let accepted = FinamOrderEndpointFixture::Accepted(FinamOrderEndpointAcceptedDto {
            broker_order_id: Some(String::new()),
        });

        assert_eq!(
            accepted
                .map_fixture()
                .expect_err("empty broker id must fail"),
            FinamOrderEndpointMapperError::EmptyBrokerOrderId
        );
    }

    #[test]
    fn local_http_endpoint_classifier_maps_statuses_and_redacts_body() {
        let accepted = FinamOrderEndpointLocalHttpResponse::Response {
            status: 200,
            body: json!({"broker_order_id": "BROKER_TEST_HTTP_1"}).to_string(),
            retry_after_ms: None,
        };
        let accepted_debug = format!("{accepted:?}");
        assert!(!accepted_debug.contains("BROKER_TEST_HTTP_1"));
        let classified = classify_order_endpoint_local_http_response(&accepted);
        assert_eq!(
            classified.result,
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Accepted {
                broker_order_id: Some(BrokerOrderId::new("BROKER_TEST_HTTP_1")),
            })
        );
        let classified_debug = format!("{classified:?}");
        let result_debug = format!("{:?}", classified.result);
        assert!(classified_debug.contains("broker_order_id_len"));
        assert!(!classified_debug.contains("BROKER_TEST_HTTP_1"));
        assert!(!result_debug.contains("BROKER_TEST_HTTP_1"));
        assert_eq!(
            classified.diagnostic.kind,
            FinamOrderEndpointResponseKind::Accepted
        );
        assert_eq!(classified.diagnostic.status, Some(200));
        assert_eq!(classified.diagnostic.body_kind.as_deref(), Some("object"));
        let diagnostic_json =
            serde_json::to_string(&classified.diagnostic).expect("diagnostic serializes");
        assert!(!diagnostic_json.contains("BROKER_TEST_HTTP_1"));

        let empty_id = FinamOrderEndpointLocalHttpResponse::Response {
            status: 200,
            body: json!({"broker_order_id": ""}).to_string(),
            retry_after_ms: None,
        };
        let classified = classify_order_endpoint_local_http_response(&empty_id);
        assert_eq!(
            classified.result,
            FinamOrderEndpointMappedResult::DecodeError {
                status: Some(200),
                body_kind: Some("object".to_string()),
            }
        );
        assert_eq!(
            classified.diagnostic.kind,
            FinamOrderEndpointResponseKind::DecodeError
        );

        let malformed = FinamOrderEndpointLocalHttpResponse::Response {
            status: 200,
            body: "{not-json".to_string(),
            retry_after_ms: None,
        };
        let classified = classify_order_endpoint_local_http_response(&malformed);
        assert_eq!(
            classified.result,
            FinamOrderEndpointMappedResult::DecodeError {
                status: Some(200),
                body_kind: Some("malformed_json".to_string()),
            }
        );

        for status in [401, 403] {
            let classified = classify_order_endpoint_local_http_response(
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status,
                    body: json!({"message": "secret-ish broker text"}).to_string(),
                    retry_after_ms: None,
                },
            );
            assert_eq!(
                classified.result,
                FinamOrderEndpointMappedResult::Unauthorized { status }
            );
            assert_eq!(
                classified.diagnostic.kind,
                FinamOrderEndpointResponseKind::Unauthorized
            );
            assert_eq!(classified.diagnostic.status, Some(status));
        }

        let rate_limited = classify_order_endpoint_local_http_response(
            &FinamOrderEndpointLocalHttpResponse::Response {
                status: 429,
                body: json!({"error": "too many requests"}).to_string(),
                retry_after_ms: Some(2_000),
            },
        );
        assert_eq!(
            rate_limited.result,
            FinamOrderEndpointMappedResult::RateLimited {
                retry_after_ms: Some(2_000),
            }
        );
        assert_eq!(
            rate_limited.diagnostic.kind,
            FinamOrderEndpointResponseKind::RateLimited
        );

        for (status, maintenance_kind) in [
            (500, FinamOrderEndpointMaintenanceKind::Unknown),
            (502, FinamOrderEndpointMaintenanceKind::Unknown),
            (503, FinamOrderEndpointMaintenanceKind::ServiceInterval),
        ] {
            let classified = classify_order_endpoint_local_http_response(
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status,
                    body: json!({"error": "maintenance window"}).to_string(),
                    retry_after_ms: None,
                },
            );
            assert_eq!(
                classified.result,
                FinamOrderEndpointMappedResult::Maintenance { maintenance_kind }
            );
            assert_eq!(
                classified.diagnostic.kind,
                FinamOrderEndpointResponseKind::Maintenance
            );
        }

        for status in [408, 504] {
            let classified = classify_order_endpoint_local_http_response(
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status,
                    body: json!({"error": "ambiguous timeout"}).to_string(),
                    retry_after_ms: None,
                },
            );
            assert_eq!(
                classified.result,
                FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Timeout)
            );
            assert_eq!(
                classified.diagnostic.kind,
                FinamOrderEndpointResponseKind::Timeout
            );
            assert_eq!(classified.diagnostic.status, Some(status));
        }

        let broker_rejected = classify_order_endpoint_local_http_response(
            &FinamOrderEndpointLocalHttpResponse::Response {
                status: 400,
                body: json!({"error": "broker rejected"}).to_string(),
                retry_after_ms: None,
            },
        );
        assert_eq!(
            broker_rejected.result,
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Rejected {
                reason_code: CommandAckReasonCode::BrokerRejected,
            })
        );

        let timeout = classify_order_endpoint_local_http_response(
            &FinamOrderEndpointLocalHttpResponse::Timeout,
        );
        assert_eq!(
            timeout.result,
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Timeout)
        );
        assert_eq!(
            timeout.diagnostic.kind,
            FinamOrderEndpointResponseKind::Timeout
        );

        let body_read_failed = classify_order_endpoint_local_http_response(
            &FinamOrderEndpointLocalHttpResponse::BodyReadFailed { status: Some(200) },
        );
        assert_eq!(
            body_read_failed.result,
            FinamOrderEndpointMappedResult::DecodeError {
                status: Some(200),
                body_kind: Some("body_read_failed".to_string()),
            }
        );
        assert_eq!(
            body_read_failed.diagnostic.kind,
            FinamOrderEndpointResponseKind::DecodeError
        );
    }

    #[test]
    fn local_http_endpoint_classifier_is_context_aware_for_cancel_conflicts() {
        for status in [404, 409, 410] {
            let place = classify_order_endpoint_local_http_response_for_context(
                FinamOrderEndpointContext::Place,
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status,
                    body: json!({"error": "place rejected or unavailable"}).to_string(),
                    retry_after_ms: None,
                },
            );
            assert_eq!(
                place.result,
                FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Rejected {
                    reason_code: CommandAckReasonCode::BrokerRejected,
                })
            );

            let cancel = classify_order_endpoint_local_http_response_for_context(
                FinamOrderEndpointContext::Cancel,
                &FinamOrderEndpointLocalHttpResponse::Response {
                    status,
                    body: json!({"error": "cancel state uncertain"}).to_string(),
                    retry_after_ms: None,
                },
            );
            assert_eq!(
                cancel.result,
                FinamOrderEndpointMappedResult::ReconciliationRequired { status }
            );
            assert_eq!(
                cancel.diagnostic.kind,
                FinamOrderEndpointResponseKind::ReconciliationRequired
            );
            let cancel_debug = format!("{cancel:?}");
            assert!(!cancel_debug.contains("cancel state uncertain"));
        }
    }

    #[test]
    fn source_surface_keeps_network_boundary_approved_only() {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace root");
        let checked_files = rust_source_files(&workspace_root.join("crates"));
        let forbidden_patterns = vec![
            ["place(order:", " PlaceOrder"].concat(),
            ["cancel(cancel:", " CancelOrder"].concat(),
            ["async fn ", "place("].concat(),
            ["async fn ", "cancel("].concat(),
            [".", "delete("].concat(),
        ];

        assert!(!checked_files.is_empty());
        for path in checked_files {
            if path.ends_with("crates/finam-gateway/src/m3d2_real_order_transport.rs") {
                let source = std::fs::read_to_string(&path).expect("source file readable");
                assert_eq!(source.matches(&[".", "post("].concat()).count(), 1);
                assert_eq!(source.matches(&[".", "delete("].concat()).count(), 1);
                assert_eq!(source.matches(&[".", "send("].concat()).count(), 1);
                assert!(source.contains("EndpointGateApproved"));
                assert!(source.contains("FinamPlaceOrderRequestSpec"));
                assert!(source.contains("FinamCancelOrderRequestSpec"));
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("source file readable");
            for pattern in &forbidden_patterns {
                assert!(
                    !source.contains(pattern),
                    "forbidden raw order boundary pattern {pattern:?} found in {}",
                    path.display()
                );
            }
        }
    }

    fn rust_source_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        collect_rust_source_files(root, &mut files);
        files.sort();
        files
    }

    fn collect_rust_source_files(root: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(root).expect("read source directory") {
            let entry = entry.expect("directory entry");
            let path = entry.path();
            if path.is_dir() {
                collect_rust_source_files(&path, files);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
}
