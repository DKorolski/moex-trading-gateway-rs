use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::de;
use serde::{Deserialize, Deserializer, Serialize};

use crate::command::{CommandAck, CommandAckReason, CommandAckStatus};
use crate::ids::{
    deserialize_broker_order_id_legacy_numeric_or_string,
    deserialize_option_broker_order_id_legacy_numeric_or_string,
    deserialize_vec_broker_order_id_legacy_numeric_or_string, BrokerOrderId, BrokerTradeId,
    ClientOrderId, StrategyRequestId, BROKER_ORDER_ID_ENCODING,
    LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT, RUNTIME_STATE_SCHEMA_VERSION_V2,
};

fn runtime_state_schema_version_v2() -> u16 {
    RUNTIME_STATE_SCHEMA_VERSION_V2
}

fn broker_order_id_encoding_marker() -> String {
    BROKER_ORDER_ID_ENCODING.to_string()
}

fn legacy_alor_numeric_import_marker() -> String {
    LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeOrderEvent {
    #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
    pub order_id: BrokerOrderId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<ClientOrderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ts: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTradeEvent {
    pub trade_id: BrokerTradeId,
    #[serde(deserialize_with = "deserialize_broker_order_id_legacy_numeric_or_string")]
    pub order_id: BrokerOrderId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<ClientOrderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ts: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBootstrapSnapshotDto {
    #[serde(default)]
    pub working_orders: HashMap<BrokerOrderId, RuntimeOrderEvent>,
    #[serde(default, alias = "working_orders_strategy")]
    pub working_orders_strategy: HashMap<BrokerOrderId, RuntimeOrderEvent>,
    #[serde(
        default,
        deserialize_with = "deserialize_vec_broker_order_id_legacy_numeric_or_string"
    )]
    pub known_order_ids: Vec<BrokerOrderId>,
    #[serde(default)]
    pub account_wide_orders_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStateSnapshot {
    #[serde(default = "runtime_state_schema_version_v2")]
    pub schema_version: u16,
    #[serde(default = "broker_order_id_encoding_marker")]
    pub broker_order_id_encoding: String,
    #[serde(default = "legacy_alor_numeric_import_marker")]
    pub legacy_alor_numeric_order_id_import: String,
    #[serde(default)]
    pub orders: HashMap<BrokerOrderId, RuntimeOrderEvent>,
    #[serde(
        default,
        deserialize_with = "deserialize_vec_broker_order_id_legacy_numeric_or_string"
    )]
    pub known_order_ids: Vec<BrokerOrderId>,
    #[serde(default)]
    pub trades: Vec<RuntimeTradeEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_entry_request_id: Option<StrategyRequestId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_exit_request_id: Option<StrategyRequestId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferred_entry_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferred_exit_state: Option<String>,
    #[serde(default)]
    pub manual_intervention_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_intervention_reason: Option<String>,
}

impl RuntimeStateSnapshot {
    pub fn new_v2() -> Self {
        Self {
            schema_version: RUNTIME_STATE_SCHEMA_VERSION_V2,
            broker_order_id_encoding: BROKER_ORDER_ID_ENCODING.to_string(),
            legacy_alor_numeric_order_id_import: LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT.to_string(),
            orders: HashMap::new(),
            known_order_ids: Vec::new(),
            trades: Vec::new(),
            pending_entry_request_id: None,
            pending_exit_request_id: None,
            deferred_entry_state: None,
            deferred_exit_state: None,
            manual_intervention_required: false,
            manual_intervention_reason: None,
        }
    }

    pub fn migrated_to_v2(mut self) -> Self {
        self.schema_version = RUNTIME_STATE_SCHEMA_VERSION_V2;
        self.broker_order_id_encoding = BROKER_ORDER_ID_ENCODING.to_string();
        self.legacy_alor_numeric_order_id_import = LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT.to_string();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeCommandAckDto {
    pub request_id: StrategyRequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<ClientOrderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker_order_id: Option<BrokerOrderId>,
    pub status: CommandAckStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<CommandAckReason>,
    pub received_ts: DateTime<Utc>,
}

#[derive(Deserialize)]
struct RuntimeCommandAckDtoRaw {
    request_id: StrategyRequestId,
    #[serde(default)]
    client_order_id: Option<ClientOrderId>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_broker_order_id_legacy_numeric_or_string"
    )]
    broker_order_id: Option<BrokerOrderId>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_broker_order_id_legacy_numeric_or_string"
    )]
    broker_order_id_str: Option<BrokerOrderId>,
    status: CommandAckStatus,
    #[serde(default)]
    reason: Option<CommandAckReason>,
    received_ts: DateTime<Utc>,
}

impl<'de> Deserialize<'de> for RuntimeCommandAckDto {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RuntimeCommandAckDtoRaw::deserialize(deserializer)?;
        let broker_order_id = match (raw.broker_order_id, raw.broker_order_id_str) {
            (Some(primary), Some(legacy_string)) if primary != legacy_string => {
                return Err(de::Error::custom(
                    "conflicting broker_order_id and broker_order_id_str",
                ));
            }
            (Some(primary), _) => Some(primary),
            (None, Some(legacy_string)) => Some(legacy_string),
            (None, None) => None,
        };

        Ok(Self {
            request_id: raw.request_id,
            client_order_id: raw.client_order_id,
            broker_order_id,
            status: raw.status,
            reason: raw.reason,
            received_ts: raw.received_ts,
        })
    }
}

impl From<RuntimeCommandAckDto> for CommandAck {
    fn from(value: RuntimeCommandAckDto) -> Self {
        Self {
            request_id: value.request_id,
            client_order_id: value.client_order_id,
            broker_order_id: value.broker_order_id,
            status: value.status,
            reason: value.reason,
            received_ts: value.received_ts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_id(value: &str) -> StrategyRequestId {
        StrategyRequestId::new(uuid::Uuid::parse_str(value).expect("uuid"))
    }

    #[test]
    fn old_alor_numeric_order_id_deserializes_as_broker_order_id() {
        let order = serde_json::from_str::<RuntimeOrderEvent>(
            r#"{"order_id":2033126389943253218,"status":"working","symbol":"IMOEXF"}"#,
        )
        .expect("legacy numeric order id imports");

        assert_eq!(order.order_id.as_str(), "2033126389943253218");
        assert_eq!(order.status.as_deref(), Some("working"));
    }

    #[test]
    fn finam_string_broker_order_id_serializes_deserializes_exact() {
        let order = serde_json::from_str::<RuntimeOrderEvent>(
            r#"{"order_id":"FINAM-ORDER-ABC-001","status":"working"}"#,
        )
        .expect("native string id imports");

        assert_eq!(order.order_id.as_str(), "FINAM-ORDER-ABC-001");

        let serialized = serde_json::to_string(&order).expect("order serializes");
        assert!(serialized.contains(r#""order_id":"FINAM-ORDER-ABC-001""#));

        let roundtrip =
            serde_json::from_str::<RuntimeOrderEvent>(&serialized).expect("order roundtrips");
        assert_eq!(roundtrip.order_id, order.order_id);
    }

    #[test]
    fn empty_broker_order_id_rejected_at_runtime_state_boundary() {
        serde_json::from_str::<RuntimeOrderEvent>(r#"{"order_id":""}"#)
            .expect_err("empty order event id rejected");
        serde_json::from_str::<RuntimeStateSnapshot>(r#"{"known_order_ids":[""]}"#)
            .expect_err("empty known order id rejected");
        serde_json::from_str::<RuntimeCommandAckDto>(
            r#"{
                "request_id":"00000000-0000-4000-8000-000000000001",
                "broker_order_id_str":"",
                "status":"Submitted",
                "received_ts":"2026-07-07T09:10:00Z"
            }"#,
        )
        .expect_err("empty ack broker id rejected");
    }

    #[test]
    fn old_state_snapshot_reads_numeric_ids_and_can_write_v2_markers() {
        let state = serde_json::from_str::<RuntimeStateSnapshot>(
            r#"{
                "schema_version":1,
                "orders":{
                    "123":{"order_id":123,"status":"working","symbol":"IMOEXF"}
                },
                "known_order_ids":[123,456],
                "pending_entry_request_id":"00000000-0000-4000-8000-000000000010",
                "deferred_entry_state":"waiting_next_bar"
            }"#,
        )
        .expect("old numeric state snapshot imports");

        assert_eq!(
            state
                .orders
                .get(&BrokerOrderId::new("123"))
                .expect("order key imported")
                .order_id
                .as_str(),
            "123"
        );
        assert_eq!(
            state
                .known_order_ids
                .iter()
                .map(BrokerOrderId::as_str)
                .collect::<Vec<_>>(),
            vec!["123", "456"]
        );
        assert_eq!(
            state.pending_entry_request_id,
            Some(request_id("00000000-0000-4000-8000-000000000010"))
        );
        assert_eq!(
            state.deferred_entry_state.as_deref(),
            Some("waiting_next_bar")
        );

        let migrated = state.migrated_to_v2();
        let serialized = serde_json::to_value(&migrated).expect("v2 state serializes");
        assert_eq!(
            serialized["schema_version"],
            serde_json::json!(RUNTIME_STATE_SCHEMA_VERSION_V2)
        );
        assert_eq!(
            serialized["broker_order_id_encoding"],
            serde_json::json!(BROKER_ORDER_ID_ENCODING)
        );
        assert_eq!(
            serialized["legacy_alor_numeric_order_id_import"],
            serde_json::json!(LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT)
        );
    }

    #[test]
    fn runtime_command_ack_imports_legacy_ids_without_replacing_request_id() {
        let ack = serde_json::from_str::<RuntimeCommandAckDto>(
            r#"{
                "request_id":"00000000-0000-4000-8000-000000000020",
                "client_order_id":"CID000000000000020",
                "broker_order_id":789,
                "status":"Submitted",
                "received_ts":"2026-07-07T09:10:00Z"
            }"#,
        )
        .expect("legacy numeric ack broker id imports");

        assert_eq!(
            ack.request_id,
            request_id("00000000-0000-4000-8000-000000000020")
        );
        assert_eq!(
            ack.client_order_id.as_ref().map(ClientOrderId::as_str),
            Some("CID000000000000020")
        );
        assert_eq!(
            ack.broker_order_id.as_ref().map(BrokerOrderId::as_str),
            Some("789")
        );

        let command_ack = CommandAck::from(ack);
        assert_eq!(
            command_ack.request_id,
            request_id("00000000-0000-4000-8000-000000000020")
        );
        assert_eq!(
            command_ack
                .broker_order_id
                .as_ref()
                .map(BrokerOrderId::as_str),
            Some("789")
        );
    }

    #[test]
    fn runtime_command_ack_rejects_conflicting_primary_and_legacy_string_ids() {
        serde_json::from_str::<RuntimeCommandAckDto>(
            r#"{
                "request_id":"00000000-0000-4000-8000-000000000021",
                "broker_order_id":789,
                "broker_order_id_str":"790",
                "status":"Submitted",
                "received_ts":"2026-07-07T09:10:00Z"
            }"#,
        )
        .expect_err("conflicting broker ids rejected");
    }

    #[test]
    fn client_order_id_does_not_replace_strategy_request_id() {
        let ack = serde_json::from_str::<RuntimeCommandAckDto>(
            r#"{
                "request_id":"00000000-0000-4000-8000-000000000030",
                "client_order_id":"CID000000000000999",
                "status":"Accepted",
                "received_ts":"2026-07-07T09:10:00Z"
            }"#,
        )
        .expect("ack imports");

        assert_eq!(
            ack.request_id,
            request_id("00000000-0000-4000-8000-000000000030")
        );
        assert_ne!(
            ack.client_order_id.map(|value| value.to_string()),
            Some(ack.request_id.to_string())
        );
    }
}
