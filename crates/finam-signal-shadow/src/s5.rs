//! Allowlisted diagnostic witness only. Never subscription authority or model input.
use crate::ws::Observation;
use serde::Serialize;

#[derive(Clone, Copy, Serialize, Debug, PartialEq, Eq)]
pub(crate) enum Channel {
    #[serde(rename = "BARS")]
    Bars,
    #[serde(rename = "QUOTES")]
    Quotes,
}

#[derive(Serialize, Debug)]
pub(crate) struct Receipt {
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_discard: Option<crate::quote::InitialDiscard>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auxiliary_total: Option<u64>,
    schema: &'static str,
    channel: Channel,
    connection_generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_generation: Option<u64>,
    at_ms: i64,
    event: &'static str,
    instrument: &'static str,
    data_items: usize,
    provider_subscription_ack: bool,
    finality_proven: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_history: Option<crate::ws::SnapshotReceipt>,
}

pub(crate) struct Witness {
    auxiliary_reported: bool,
    channel: Channel,
    generation: u64,
    sent: bool,
    data_seen: bool,
    auth_generation: Option<u64>,
}

#[cfg(test)]
impl Receipt {
    pub(crate) fn test_variant(
        mut self,
        event: &'static str,
        channel: Channel,
        data_items: usize,
        total: Option<u64>,
    ) -> Self {
        self.event = event;
        self.channel = channel;
        self.data_items = data_items;
        self.auxiliary_total = total;
        self
    }
}

impl Witness {
    pub(crate) fn receipts(&mut self, observation: &Observation, at_ms: i64) -> Vec<Receipt> {
        let mut receipts = Vec::new();
        if let Observation::Quotes(frame) = observation {
            if self.channel == Channel::Quotes && self.sent {
                if let Some(discard) = &frame.initial_discard {
                    receipts.push(Receipt {
                        initial_discard: Some(discard.clone()),
                        auxiliary_total: None,
                        schema: "signal-shadow-auth-ws-v1",
                        channel: self.channel,
                        connection_generation: self.generation,
                        auth_generation: self.auth_generation,
                        at_ms,
                        event: "discarded_initial_stale_candidate",
                        instrument: "IMOEXF@RTSX",
                        data_items: 0,
                        provider_subscription_ack: false,
                        finality_proven: false,
                        initial_history: None,
                    });
                }
            }
            if self.channel == Channel::Quotes
                && self.sent
                && !self.auxiliary_reported
                && frame.auxiliary_total > 0
            {
                self.auxiliary_reported = true;
                receipts.push(Receipt {
                    initial_discard: None,
                    schema: if self.auth_generation.is_some() {
                        "signal-shadow-auth-ws-v1"
                    } else {
                        "signal-shadow-s5-v1"
                    },
                    channel: self.channel,
                    connection_generation: self.generation,
                    auth_generation: self.auth_generation,
                    at_ms,
                    event: "auxiliary_updates_observed",
                    instrument: "IMOEXF@RTSX",
                    data_items: 0,
                    provider_subscription_ack: false,
                    finality_proven: false,
                    initial_history: None,
                    auxiliary_total: Some(frame.auxiliary_total),
                });
            }
        }
        if let Observation::InitialHistory { history, .. } = observation {
            if self.channel == Channel::Bars && self.sent {
                receipts.push(Receipt {
                    initial_discard: None,
                    auxiliary_total: None,
                    schema: if self.auth_generation.is_some() {
                        "signal-shadow-auth-ws-v1"
                    } else {
                        "signal-shadow-s5-v1"
                    },
                    channel: self.channel,
                    connection_generation: self.generation,
                    auth_generation: self.auth_generation,
                    at_ms,
                    event: "snapshot_discarded",
                    instrument: "IMOEXF@RTSX",
                    data_items: 0,
                    provider_subscription_ack: false,
                    finality_proven: false,
                    initial_history: Some(history.clone()),
                });
            }
        }
        if let Some(receipt) = self.observe(observation, at_ms) {
            receipts.push(receipt);
        }
        receipts
    }

    pub(crate) fn new(channel: Channel, generation: u64) -> Self {
        Self {
            auxiliary_reported: false,
            channel,
            generation,
            sent: false,
            data_seen: false,
            auth_generation: None,
        }
    }

    pub(crate) fn new_auth(channel: Channel, connection: u64, auth_generation: u64) -> Self {
        let mut witness = Self::new(channel, connection);
        witness.auth_generation = Some(auth_generation);
        witness
    }

    pub(crate) fn observe(&mut self, observation: &Observation, at_ms: i64) -> Option<Receipt> {
        // Socket returns Handshake only AFTER subscription send and state transition succeed.
        let (event, count) = match observation {
            Observation::Handshake if !self.sent => {
                self.sent = true;
                ("handshake_and_subscription_sent", 0)
            }
            Observation::ProvisionalBars(rows) | Observation::InitialHistory { live: rows, .. }
                if self.channel == Channel::Bars
                    && self.sent
                    && !self.data_seen
                    && !rows.is_empty() =>
            {
                self.data_seen = true;
                ("first_validated_data", rows.len())
            }
            Observation::Quotes(frame)
                if self.channel == Channel::Quotes
                    && self.sent
                    && !self.data_seen
                    && !frame.quotes.is_empty() =>
            {
                self.data_seen = true;
                ("first_validated_data", frame.quotes.len())
            }
            _ => return None,
        };
        Some(Receipt {
            initial_discard: None,
            auxiliary_total: None,
            schema: if self.auth_generation.is_some() {
                "signal-shadow-auth-ws-v1"
            } else {
                "signal-shadow-s5-v1"
            },
            channel: self.channel,
            connection_generation: self.generation,
            auth_generation: self.auth_generation,
            at_ms,
            event,
            instrument: "IMOEXF@RTSX",
            data_items: count,
            provider_subscription_ack: false,
            finality_proven: false,
            initial_history: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_witness_is_not_a_controlled_s5_witness() {
        let mut witness = Witness::new_auth(Channel::Bars, 3, 2);
        let receipt = witness.observe(&Observation::Handshake, 1).unwrap();
        let value = serde_json::to_value(receipt).unwrap();
        assert_eq!(value["schema"], "signal-shadow-auth-ws-v1");
        assert_eq!(value["auth_generation"], 2);
        assert_eq!(value["connection_generation"], 3);
        assert_eq!(value["provider_subscription_ack"], false);
        assert!(witness.observe(&Observation::Handshake, 2).is_none());
    }
    #[test]
    fn data_is_once_per_generation_and_raw_secrets_are_not_serialized() {
        let data = Observation::Quotes(crate::ws::QuoteFrame {
            initial_discard: None,
            auxiliary_total: 0,
            raw_frame: "SECRET_TOKEN_AND_PROVIDER_FIELDS".into(),
            received_at_ms: 10,
            quotes: vec![crate::feedback::Quote {
                instrument: "IMOEXF@RTSX".into(),
                source_id: "SECRET_KEY".into(),
                price_ticks: 4400,
                event_at_ms: 10,
                received_at_ms: 10,
            }],
        });
        let mut wrong = Witness::new(Channel::Bars, 1);
        wrong.observe(&Observation::Handshake, 1);
        assert!(wrong.observe(&data, 10).is_none());
        for generation in [1, 2] {
            let mut w = Witness::new(Channel::Quotes, generation);
            assert!(w.observe(&data, 9).is_none());
            w.observe(&Observation::Handshake, 9);
            let receipt = w.observe(&data, 10).unwrap();
            let encoded = serde_json::to_string(&receipt).unwrap();
            assert!(!encoded.contains("SECRET"));
            assert_eq!(receipt.data_items, 1);
            assert_eq!(receipt.connection_generation, generation);
            assert_eq!(receipt.event, "first_validated_data");
            assert!(w.observe(&data, 11).is_none());
        }
    }
    #[test]
    fn heartbeat_and_empty_data_are_not_subscription_evidence() {
        let mut w = Witness::new(Channel::Bars, 1);
        assert!(w.observe(&Observation::Ignore, 1).is_none());
        assert!(w
            .observe(&Observation::ProvisionalBars(vec![]), 2)
            .is_none());
        let receipt = serde_json::to_value(w.observe(&Observation::Handshake, 3).unwrap()).unwrap();
        assert_eq!(receipt["provider_subscription_ack"], false);
        assert_eq!(receipt["finality_proven"], false);
        assert!(w.observe(&Observation::Handshake, 4).is_none());
    }
    #[test]
    fn reconnect_has_independent_witness_and_no_arbitrary_strings() {
        for channel in [Channel::Bars, Channel::Quotes] {
            let mut w = Witness::new(channel, 2);
            let receipt =
                serde_json::to_value(w.observe(&Observation::Handshake, 8).unwrap()).unwrap();
            assert_eq!(receipt["connection_generation"], 2);
            assert_eq!(receipt.as_object().unwrap().len(), 9);
            assert!(receipt.get("raw_frame").is_none());
            assert!(receipt.get("subscription_key").is_none());
        }
    }
}
