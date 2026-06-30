use broker_core::{
    CancelOrder, OrderSide, OrderType, OutgoingOrderComment, PlaceOrder, TimeInForce,
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
            .field("symbol", &self.symbol)
            .field("quantity", &self.quantity)
            .field("side", &self.side)
            .field("order_type", &self.order_type)
            .field("time_in_force", &self.time_in_force)
            .field("limit_price_present", &self.limit_price.is_some())
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
}

pub fn build_place_order_request(
    order: &PlaceOrder,
    outgoing_comment: Option<&OutgoingOrderComment>,
) -> Result<FinamPlaceOrderRequestSpec, FinamOrderRequestBuildError> {
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
    cancel: &CancelOrder,
) -> Result<FinamCancelOrderRequestSpec, FinamOrderRequestBuildError> {
    if cancel.order_id.as_str().is_empty() {
        return Err(FinamOrderRequestBuildError::MissingBrokerOrderId);
    }
    Ok(FinamCancelOrderRequestSpec {
        account_id: cancel.account_id.as_str().to_string(),
        order_id: cancel.order_id.as_str().to_string(),
    })
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
        AccountId, BrokerOrderId, ClientOrderId, Exchange, InstrumentId, Market,
        OutgoingCommentIntent, OutgoingOrderCommentPolicy, StrategyRequestId,
    };
    use chrono::{TimeZone, Utc};
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

    #[test]
    fn builds_limit_place_order_body_without_sending_http() {
        let order = place_order();
        let spec = build_place_order_request(&order, None).expect("request spec");

        assert_eq!(
            spec.rest_path_segments(),
            vec!["v1", "accounts", "ACC_TEST_0001", "orders"]
        );
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

        let body = serde_json::to_value(
            &build_place_order_request(&order, None)
                .expect("market request")
                .body,
        )
        .expect("body");
        assert_eq!(body["type"], "ORDER_TYPE_MARKET");
        assert_eq!(body["side"], "SIDE_SELL");
        assert!(body.get("limit_price").is_none());

        order.comment = Some("raw broker comment must not leak".to_string());
        assert_eq!(
            build_place_order_request(&order, None).expect_err("raw comment"),
            FinamOrderRequestBuildError::RawCommandCommentNotAllowed
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

        let spec = build_place_order_request(&order, Some(&comment)).expect("request");
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

        let spec = build_cancel_order_request(&cancel).expect("cancel spec");
        assert_eq!(
            spec.rest_path_segments(),
            vec!["v1", "accounts", "ACC_TEST_0001", "orders", "BROKER_TEST_1"]
        );
        assert!(!format!("{spec:?}").contains("BROKER_TEST_1"));
        assert!(!format!("{spec:?}").contains("ACC_TEST_0001"));
    }
}
