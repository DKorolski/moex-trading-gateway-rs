use serde::{Deserialize, Serialize};

use crate::account::AccountId;
use crate::ids::{
    deserialize_broker_order_id_legacy_numeric_or_string, BrokerOrderId, ClientOrderId,
    StrategyRequestId,
};
use crate::instrument::{InstrumentId, Price, Quantity};
use crate::order::{OrderSide, OrderType, TimeInForce};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BrokerCommand {
    PlaceOrder(PlaceOrder),
    CancelOrder(CancelOrder),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceOrder {
    pub request_id: StrategyRequestId,
    pub created_ts: chrono::DateTime<chrono::Utc>,
    pub ttl_ms: Option<u64>,
    pub account_id: AccountId,
    pub client_order_id: ClientOrderId,
    pub instrument: InstrumentId,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub qty: Quantity,
    pub limit_price: Option<Price>,
    pub time_in_force: TimeInForce,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelOrder {
    pub request_id: StrategyRequestId,
    pub created_ts: chrono::DateTime<chrono::Utc>,
    pub ttl_ms: Option<u64>,
    pub account_id: AccountId,
    #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
    pub order_id: BrokerOrderId,
    pub client_order_id: Option<ClientOrderId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplaceOrder {
    pub request_id: StrategyRequestId,
    pub created_ts: chrono::DateTime<chrono::Utc>,
    pub ttl_ms: Option<u64>,
    pub account_id: AccountId,
    #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
    pub order_id: BrokerOrderId,
    pub client_order_id: Option<ClientOrderId>,
    pub new_qty: Option<Quantity>,
    pub new_limit_price: Option<Price>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaceOrderFeatureDisabled {
    pub request_id: StrategyRequestId,
    #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
    pub order_id: BrokerOrderId,
    pub reason: CommandAckReasonCode,
}

impl ReplaceOrder {
    pub fn feature_disabled(&self) -> ReplaceOrderFeatureDisabled {
        ReplaceOrderFeatureDisabled {
            request_id: self.request_id,
            order_id: self.order_id.clone(),
            reason: CommandAckReasonCode::FeatureDisabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelOrderBuilderInput {
    pub request_id: StrategyRequestId,
    pub created_ts: chrono::DateTime<chrono::Utc>,
    pub ttl_ms: Option<u64>,
    pub account_id: AccountId,
    pub order_id: BrokerOrderId,
    pub client_order_id: Option<ClientOrderId>,
}

pub fn build_cancel_command(input: CancelOrderBuilderInput) -> BrokerCommand {
    BrokerCommand::CancelOrder(CancelOrder {
        request_id: input.request_id,
        created_ts: input.created_ts,
        ttl_ms: input.ttl_ms,
        account_id: input.account_id,
        order_id: input.order_id,
        client_order_id: input.client_order_id,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandAck {
    pub request_id: StrategyRequestId,
    pub client_order_id: Option<ClientOrderId>,
    pub broker_order_id: Option<BrokerOrderId>,
    pub status: CommandAckStatus,
    pub reason: Option<CommandAckReason>,
    pub received_ts: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandAckStatus {
    Accepted,
    Submitted,
    Duplicate,
    Expired,
    Recovered,
    Rejected,
    Error,
    Timeout,
    UnknownPending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandAckReason {
    pub code: CommandAckReasonCode,
}

impl CommandAckReason {
    pub fn new(code: CommandAckReasonCode) -> Self {
        Self { code }
    }

    pub fn feature_disabled() -> Self {
        Self::new(CommandAckReasonCode::FeatureDisabled)
    }

    pub fn synthetic_submitted() -> Self {
        Self::new(CommandAckReasonCode::SyntheticSubmitted)
    }

    pub fn cancel_timeout_unknown_pending() -> Self {
        Self::new(CommandAckReasonCode::CancelTimeoutUnknownPending)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandAckReasonCode {
    FeatureDisabled,
    SyntheticSubmitted,
    LocalValidationRejected,
    BrokerRejected,
    TransportTimeout,
    TimeoutUnknownPending,
    CancelTimeoutUnknownPending,
    RecoveredByBrokerTruth,
    ReconciliationRequired,
    DuplicateCommand,
    ExpiredCommand,
    ManualInterventionRequired,
    DryRunOnly,
    RateLimited,
    BrokerMaintenance,
    TradingWindowClosed,
    ResponseDecodeError,
    Unauthorized,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn request_id() -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128(0x2b08))
    }

    fn created_ts() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 8, 9, 0, 0)
            .single()
            .expect("valid ts")
    }

    fn account() -> AccountId {
        AccountId::new("ACC_TEST_0001")
    }

    #[test]
    fn cancel_order_id_uses_broker_order_id() {
        let cancel = CancelOrder {
            request_id: request_id(),
            created_ts: created_ts(),
            ttl_ms: Some(1_000),
            account_id: account(),
            order_id: BrokerOrderId::new("FINAM/CANCEL:EXACT_Ё"),
            client_order_id: None,
        };

        assert_eq!(cancel.order_id.as_str(), "FINAM/CANCEL:EXACT_Ё");
    }

    #[test]
    fn legacy_numeric_cancel_id_imports_as_decimal_string() {
        let json = serde_json::json!({
            "request_id": request_id(),
            "created_ts": created_ts(),
            "ttl_ms": 1000,
            "account_id": "ACC_TEST_0001",
            "order_id": 42,
            "client_order_id": null
        });

        let cancel: CancelOrder = serde_json::from_value(json).expect("legacy cancel");

        assert_eq!(cancel.order_id.as_str(), "42");
    }

    #[test]
    fn broker_native_cancel_id_string_preserved_exact() {
        let json = serde_json::json!({
            "request_id": request_id(),
            "created_ts": created_ts(),
            "ttl_ms": null,
            "account_id": "ACC_TEST_0001",
            "order_id": "FINAM/CANCEL:EXACT_Ё",
            "client_order_id": null
        });

        let cancel: CancelOrder = serde_json::from_value(json).expect("cancel");

        assert_eq!(cancel.order_id.as_str(), "FINAM/CANCEL:EXACT_Ё");
    }

    #[test]
    fn empty_cancel_order_id_rejected() {
        let json = serde_json::json!({
            "request_id": request_id(),
            "created_ts": created_ts(),
            "ttl_ms": null,
            "account_id": "ACC_TEST_0001",
            "order_id": "",
            "client_order_id": null
        });

        let error = serde_json::from_value::<CancelOrder>(json)
            .expect_err("empty broker order id rejected");

        assert!(error
            .to_string()
            .contains("broker-native order id cannot be empty"));
    }

    #[test]
    fn build_cancel_command_accepts_broker_order_id_without_numeric_logic() {
        let request_id = request_id();
        let command = build_cancel_command(CancelOrderBuilderInput {
            request_id,
            created_ts: created_ts(),
            ttl_ms: Some(1_000),
            account_id: account(),
            order_id: BrokerOrderId::new("FINAM/NON_NUMERIC_CANCEL"),
            client_order_id: None,
        });

        let BrokerCommand::CancelOrder(cancel) = command else {
            panic!("expected cancel command");
        };

        assert_eq!(cancel.request_id, request_id);
        assert_eq!(cancel.order_id.as_str(), "FINAM/NON_NUMERIC_CANCEL");
    }

    #[test]
    fn replace_order_id_uses_broker_order_id_but_replace_remains_disabled() {
        let replace: ReplaceOrder = serde_json::from_value(serde_json::json!({
            "request_id": request_id(),
            "created_ts": created_ts(),
            "ttl_ms": null,
            "account_id": "ACC_TEST_0001",
            "order_id": "FINAM/REPLACE:EXACT_Ё",
            "client_order_id": null,
            "new_qty": "1",
            "new_limit_price": "2210.5"
        }))
        .expect("replace shape");

        let disabled = replace.feature_disabled();

        assert_eq!(replace.order_id.as_str(), "FINAM/REPLACE:EXACT_Ё");
        assert_eq!(disabled.order_id.as_str(), "FINAM/REPLACE:EXACT_Ё");
        assert_eq!(disabled.reason, CommandAckReasonCode::FeatureDisabled);
    }

    #[test]
    fn legacy_numeric_replace_id_imports_as_decimal_string() {
        let replace: ReplaceOrder = serde_json::from_value(serde_json::json!({
            "request_id": request_id(),
            "created_ts": created_ts(),
            "ttl_ms": null,
            "account_id": "ACC_TEST_0001",
            "order_id": 314,
            "client_order_id": null,
            "new_qty": null,
            "new_limit_price": null
        }))
        .expect("replace shape");

        assert_eq!(replace.order_id.as_str(), "314");
        assert_eq!(
            replace.feature_disabled().reason,
            CommandAckReasonCode::FeatureDisabled
        );
    }
}
