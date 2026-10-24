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
}

pub(crate) struct Witness {
    channel: Channel,
    generation: u64,
    sent: bool,
    data_seen: bool,
    auth_generation: Option<u64>,
}

impl Witness {
    pub(crate) fn new(channel: Channel, generation: u64) -> Self {
        Self {
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
            Observation::ProvisionalBars(rows)
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
