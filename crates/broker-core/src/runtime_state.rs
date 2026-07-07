use std::collections::{HashMap, HashSet};

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

impl RuntimeBootstrapSnapshotDto {
    pub fn validate_for_bootstrap(
        self,
    ) -> Result<ValidatedRuntimeBootstrapSnapshotDto, RuntimeStateValidationError> {
        validate_order_event_map("working_orders", &self.working_orders)?;
        validate_order_event_map("working_orders_strategy", &self.working_orders_strategy)?;
        validate_unique_ids("known_order_ids", &self.known_order_ids)?;

        Ok(ValidatedRuntimeBootstrapSnapshotDto { snapshot: self })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedRuntimeBootstrapSnapshotDto {
    pub snapshot: RuntimeBootstrapSnapshotDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStateReadinessBlocker {
    pub kind: RuntimeStateReadinessBlockerKind,
    pub broker_order_id_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStateReadinessBlockerKind {
    KnownOrderIdMissingFromOrders,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedRuntimeStateSnapshot {
    pub snapshot: RuntimeStateSnapshot,
    pub readiness_blockers: Vec<RuntimeStateReadinessBlocker>,
}

impl ValidatedRuntimeStateSnapshot {
    pub fn manual_intervention_required(&self) -> bool {
        self.snapshot.manual_intervention_required || !self.readiness_blockers.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeStateValidationError {
    #[error("{map_name} map key does not match payload order_id")]
    OrderMapKeyMismatch {
        map_name: &'static str,
        key_len: usize,
        payload_order_id_len: usize,
    },
    #[error("{field} contains duplicate broker order id")]
    DuplicateBrokerOrderId { field: &'static str, id_len: usize },
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

    pub fn validate_for_runtime_restore(
        self,
    ) -> Result<ValidatedRuntimeStateSnapshot, RuntimeStateValidationError> {
        validate_order_event_map("orders", &self.orders)?;
        validate_unique_ids("known_order_ids", &self.known_order_ids)?;

        let readiness_blockers = self
            .known_order_ids
            .iter()
            .filter(|order_id| !self.orders.contains_key(*order_id))
            .map(|order_id| RuntimeStateReadinessBlocker {
                kind: RuntimeStateReadinessBlockerKind::KnownOrderIdMissingFromOrders,
                broker_order_id_len: order_id.as_str().len(),
            })
            .collect();

        Ok(ValidatedRuntimeStateSnapshot {
            snapshot: self.migrated_to_v2(),
            readiness_blockers,
        })
    }
}

fn validate_order_event_map(
    map_name: &'static str,
    orders: &HashMap<BrokerOrderId, RuntimeOrderEvent>,
) -> Result<(), RuntimeStateValidationError> {
    for (key, payload) in orders {
        if key != &payload.order_id {
            return Err(RuntimeStateValidationError::OrderMapKeyMismatch {
                map_name,
                key_len: key.as_str().len(),
                payload_order_id_len: payload.order_id.as_str().len(),
            });
        }
    }
    Ok(())
}

fn validate_unique_ids(
    field: &'static str,
    ids: &[BrokerOrderId],
) -> Result<(), RuntimeStateValidationError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(RuntimeStateValidationError::DuplicateBrokerOrderId {
                field,
                id_len: id.as_str().len(),
            });
        }
    }
    Ok(())
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
    fn runtime_state_orders_map_key_must_match_order_event_id() {
        let state = serde_json::from_str::<RuntimeStateSnapshot>(
            r#"{
                "orders":{
                    "123":{"order_id":"456","status":"working"}
                }
            }"#,
        )
        .expect("mismatched state is syntactically readable");

        assert_eq!(
            state
                .validate_for_runtime_restore()
                .expect_err("map key mismatch is rejected"),
            RuntimeStateValidationError::OrderMapKeyMismatch {
                map_name: "orders",
                key_len: 3,
                payload_order_id_len: 3,
            }
        );
    }

    #[test]
    fn bootstrap_working_orders_key_must_match_order_event_id() {
        let snapshot = serde_json::from_str::<RuntimeBootstrapSnapshotDto>(
            r#"{
                "working_orders":{
                    "123":{"order_id":"456","status":"working"}
                }
            }"#,
        )
        .expect("mismatched bootstrap is syntactically readable");

        assert_eq!(
            snapshot
                .validate_for_bootstrap()
                .expect_err("working_orders key mismatch is rejected"),
            RuntimeStateValidationError::OrderMapKeyMismatch {
                map_name: "working_orders",
                key_len: 3,
                payload_order_id_len: 3,
            }
        );
    }

    #[test]
    fn working_orders_strategy_key_must_match_order_event_id() {
        let snapshot = serde_json::from_str::<RuntimeBootstrapSnapshotDto>(
            r#"{
                "working_orders_strategy":{
                    "123":{"order_id":"456","status":"working"}
                }
            }"#,
        )
        .expect("mismatched bootstrap is syntactically readable");

        assert_eq!(
            snapshot
                .validate_for_bootstrap()
                .expect_err("working_orders_strategy key mismatch is rejected"),
            RuntimeStateValidationError::OrderMapKeyMismatch {
                map_name: "working_orders_strategy",
                key_len: 3,
                payload_order_id_len: 3,
            }
        );
    }

    #[test]
    fn known_order_ids_cannot_contain_empty_zero_negative_null_or_duplicates() {
        for payload in [
            r#"{"known_order_ids":[""]}"#,
            r#"{"known_order_ids":[0]}"#,
            r#"{"known_order_ids":[-1]}"#,
            r#"{"known_order_ids":[null]}"#,
        ] {
            serde_json::from_str::<RuntimeStateSnapshot>(payload)
                .expect_err("invalid known order id rejected at serde boundary");
        }

        let duplicate =
            serde_json::from_str::<RuntimeStateSnapshot>(r#"{"known_order_ids":[123,"123"]}"#)
                .expect("duplicate is syntactically readable");

        assert_eq!(
            duplicate
                .validate_for_runtime_restore()
                .expect_err("duplicate known id rejected"),
            RuntimeStateValidationError::DuplicateBrokerOrderId {
                field: "known_order_ids",
                id_len: 3,
            }
        );
    }

    #[test]
    fn known_order_id_missing_from_orders_blocks_readiness_without_losing_state() {
        let state = serde_json::from_str::<RuntimeStateSnapshot>(
            r#"{
                "orders":{
                    "123":{"order_id":123,"status":"working"}
                },
                "known_order_ids":[123,999],
                "pending_exit_request_id":"00000000-0000-4000-8000-000000000011",
                "deferred_exit_state":"waiting_fill"
            }"#,
        )
        .expect("state imports");

        let validated = state
            .validate_for_runtime_restore()
            .expect("map is consistent");

        assert!(validated.manual_intervention_required());
        assert_eq!(
            validated.readiness_blockers,
            vec![RuntimeStateReadinessBlocker {
                kind: RuntimeStateReadinessBlockerKind::KnownOrderIdMissingFromOrders,
                broker_order_id_len: 3,
            }]
        );
        assert_eq!(
            validated.snapshot.pending_exit_request_id,
            Some(request_id("00000000-0000-4000-8000-000000000011"))
        );
        assert_eq!(
            validated.snapshot.deferred_exit_state.as_deref(),
            Some("waiting_fill")
        );
    }

    #[test]
    fn new_state_serializes_broker_order_id_keys_as_exact_strings() {
        let state = serde_json::from_str::<RuntimeStateSnapshot>(
            r#"{
                "orders":{
                    "FINAM-ORDER-ABC-001":{"order_id":"FINAM-ORDER-ABC-001","status":"working"}
                },
                "known_order_ids":["FINAM-ORDER-ABC-001"]
            }"#,
        )
        .expect("state imports")
        .validate_for_runtime_restore()
        .expect("state validates")
        .snapshot;

        let serialized = serde_json::to_string(&state).expect("state serializes");
        assert!(serialized.contains(r#""FINAM-ORDER-ABC-001":{"order_id":"FINAM-ORDER-ABC-001""#));
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
