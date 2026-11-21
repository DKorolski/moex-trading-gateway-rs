//! One bounded failure slot, independent of the event queue and JoinSet ordering.
//! Typed codes only: no token, account, raw frame, URL, Debug or Display payload.
use crate::{
    socket::{SocketCode, SocketStage, TimestampBoundsDetail},
    transport::TransportError,
};
use std::sync::{Arc, Mutex};

#[derive(serde::Serialize)]
enum Code {
    Bounds,
    Stopped,
    Deadline,
    Held,
    Fetch,
    Admission,
    Clock,
    Socket(SocketCode),
}

#[derive(serde::Serialize)]
pub(super) struct Receipt {
    schema: u8,
    channel: &'static str,
    operation: &'static str,
    at_ms: i64,
    auth_generation: u64,
    connection_generation: u64,
    stage: SocketStage,
    code: Code,
    timestamp_bounds: Option<TimestampBoundsDetail>,
    liveness: Option<crate::socket::LivenessSnapshot>,
}

#[derive(Clone, Default)]
pub(super) struct Slot(Arc<Mutex<Option<Receipt>>>);

impl Slot {
    #[cfg(test)]
    pub(super) fn record(&self, error: TransportError, auth: u64, connection: u64) {
        self.record_with_liveness(error, auth, connection, None);
    }

    pub(super) fn record_with_liveness(
        &self,
        error: TransportError,
        auth: u64,
        connection: u64,
        liveness: Option<crate::socket::LivenessSnapshot>,
    ) {
        use TransportError as E;
        // Deadline is a bounded-wait deadline, NOT proof of market inactivity.
        let (stage, code, timestamp_bounds) = match error {
            E::Socket(e) => (e.stage, Code::Socket(e.code), e.timestamp_bounds),
            e => (
                SocketStage::Receive,
                match e {
                    E::Bounds => Code::Bounds,
                    E::Stopped => Code::Stopped,
                    E::Deadline => Code::Deadline,
                    E::Held => Code::Held,
                    E::Fetch => Code::Fetch,
                    E::Admission => Code::Admission,
                    E::Clock => Code::Clock,
                    E::Socket(_) => unreachable!(),
                },
                None,
            ),
        };
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(Receipt {
                schema: 2,
                channel: "BARS",
                operation: "observe_until",
                at_ms: chrono::Utc::now().timestamp_millis(),
                auth_generation: auth,
                connection_generation: connection,
                stage,
                code,
                timestamp_bounds,
                liveness,
            });
        }
    }

    pub(super) fn take(&self) -> Option<Receipt> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{socket::SocketFailure, ws::WsError};

    #[test]
    fn closed_codes_preserve_cause_and_generation_without_dynamic_payload() {
        for (error, expected) in [
            (TransportError::Deadline, serde_json::json!("Deadline")),
            (TransportError::Stopped, serde_json::json!("Stopped")),
            (TransportError::Clock, serde_json::json!("Clock")),
            (
                TransportError::Socket(SocketFailure {
                    stage: SocketStage::Silence,
                    code: SocketCode::Protocol(WsError::Silence),
                    timestamp_bounds: None,
                    quote_time: None,
                    quote_conflict: None,
                }),
                serde_json::json!({"Socket":{"Protocol":"Silence"}}),
            ),
            (
                TransportError::Socket(SocketFailure {
                    stage: SocketStage::Receive,
                    code: SocketCode::Close,
                    timestamp_bounds: None,
                    quote_time: None,
                    quote_conflict: None,
                }),
                serde_json::json!({"Socket":"Close"}),
            ),
        ] {
            let slot = Slot::default();
            slot.record(error, 6, 7);
            let v = serde_json::to_value(slot.take()).unwrap();
            assert_eq!(v["code"], expected);
            assert_eq!(v["auth_generation"], 6);
            assert_eq!(v["connection_generation"], 7);
            assert_eq!(v["operation"], "observe_until");
            assert!(v.get("token").is_none());
            assert!(slot.take().is_none());
        }
    }

    #[tokio::test]
    async fn receipt_survives_worker_join_before_event_queue_and_keeps_first_cause() {
        let slot = Slot::default();
        let worker = slot.clone();
        let mut jobs = tokio::task::JoinSet::new();
        jobs.spawn(async move {
            worker.record(TransportError::Deadline, 6, 7);
            Err::<(), _>("runner_bars_observe")
        });
        assert_eq!(
            jobs.join_next().await.unwrap().unwrap(),
            Err("runner_bars_observe")
        );
        slot.record(TransportError::Stopped, 99, 99);
        let terminal = serde_json::json!({"bars_observe_failure":slot.take()});
        assert_eq!(terminal["bars_observe_failure"]["code"], "Deadline");
        assert_eq!(terminal["bars_observe_failure"]["auth_generation"], 6);
        assert!(jobs.is_empty());
    }

    #[test]
    fn normal_stop_does_not_invent_failure() {
        assert!(Slot::default().take().is_none());
    }

    #[test]
    fn terminal_slot_keeps_typed_liveness_even_if_event_queue_is_not_drained() {
        let diagnostic = crate::socket::LivenessSnapshot {
            observed_at_ms: 120000,
            connected_at_ms: 0,
            last_rx_at_ms: Some(100000),
            last_live_bars_at_ms: Some(50000),
            last_ping_at_ms: Some(105000),
            last_matched_pong_at_ms: Some(90000),
            rx_age_ms: 20000,
            bars_data_age_ms: 70000,
            pending_pong_age_ms: Some(15000),
            timeout: Some(crate::socket::LivenessTimeout::Pong),
        };
        let slot = Slot::default();
        slot.record_with_liveness(
            TransportError::Socket(SocketFailure {
                stage: SocketStage::Heartbeat,
                code: SocketCode::Liveness(crate::socket::LivenessTimeout::Pong),
                timestamp_bounds: None,
                quote_time: None,
                quote_conflict: None,
            }),
            1,
            3,
            Some(diagnostic),
        );
        slot.record(TransportError::Stopped, 99, 99);
        let v = serde_json::to_value(slot.take()).unwrap();
        assert_eq!(v["schema"], 2);
        assert_eq!(v["code"], serde_json::json!({"Socket":{"Liveness":"Pong"}}));
        assert_eq!(v["liveness"]["last_live_bars_at_ms"], 50000);
        assert_eq!(v["liveness"]["pending_pong_age_ms"], 15000);
        assert_eq!(v["connection_generation"], 3);
    }
}
