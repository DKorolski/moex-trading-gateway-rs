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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinamOrderExecutionOutcome {
    Accepted {
        #[serde(skip_serializing_if = "Option::is_none")]
        broker_order_id: Option<BrokerOrderId>,
    },
    Rejected {
        reason_code: CommandAckReasonCode,
    },
    Timeout,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamOrderEndpointAcceptedDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FinamOrderEndpointFixture {
    Accepted(FinamOrderEndpointAcceptedDto),
    Rejected {
        reason_code: FinamOrderEndpointRejectedCode,
    },
    RateLimited {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_after_ms: Option<u64>,
    },
    Maintenance {
        maintenance_kind: FinamOrderEndpointMaintenanceKind,
    },
    Timeout,
    DecodeError {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinamOrderEndpointMappedResult {
    Execution(FinamOrderExecutionOutcome),
    RateLimited {
        retry_after_ms: Option<u64>,
    },
    Maintenance {
        maintenance_kind: FinamOrderEndpointMaintenanceKind,
    },
    DecodeError {
        status: Option<u16>,
        body_kind: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FinamOrderEndpointMapperError {
    #[error("FINAM order endpoint accepted response has empty broker order id")]
    EmptyBrokerOrderId,
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

#[async_trait]
pub trait FinamApprovedOrderExecutionClient: Send {
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
pub struct MockFinamApprovedOrderExecutionClient {
    script: Vec<FinamOrderExecutionOutcome>,
    next_index: usize,
    records: Vec<FinamMockExecutionRecord>,
}

impl MockFinamApprovedOrderExecutionClient {
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
impl FinamApprovedOrderExecutionClient for MockFinamApprovedOrderExecutionClient {
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
        TimeInForce::GoodTillDate => Ok("TIME_IN_FORCE_GOOD_TILL_DATE"),
        TimeInForce::FillOrKill => Ok("TIME_IN_FORCE_FILL_OR_KILL"),
        TimeInForce::ImmediateOrCancel => Ok("TIME_IN_FORCE_IMMEDIATE_OR_CANCEL"),
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
    fn approved_execution_client_contract_is_request_spec_based() {
        fn assert_client<T: FinamApprovedOrderExecutionClient>() {}
        fn assert_place_boundary<T: FinamApprovedOrderExecutionClient>(
            _client: &mut T,
            _spec: FinamPlaceOrderRequestSpec,
        ) {
        }
        fn assert_cancel_boundary<T: FinamApprovedOrderExecutionClient>(
            _client: &mut T,
            _spec: FinamCancelOrderRequestSpec,
        ) {
        }

        assert_client::<MockFinamApprovedOrderExecutionClient>();
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
        let mut client = MockFinamApprovedOrderExecutionClient::new(Vec::new());

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
