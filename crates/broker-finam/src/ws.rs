use broker_core::event::{MarketDataEvent, MarketDataSourceKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::dto;
use crate::mapper::{map_bar, map_quote, FinamMapperError};
use crate::AccessToken;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinamWsAction {
    Subscribe,
    Unsubscribe,
    UnsubscribeAll,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinamWsSubscriptionType {
    Bars,
    Quotes,
}

impl FinamWsSubscriptionType {
    pub fn as_wire_str(&self) -> &'static str {
        match self {
            Self::Bars => "BARS",
            Self::Quotes => "QUOTES",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FinamWsEnvelope {
    #[serde(rename = "type", alias = "message_type", alias = "messageType")]
    pub envelope_type: String,
    #[serde(alias = "subscriptionKey")]
    pub subscription_key: Option<String>,
    #[serde(alias = "subscriptionType")]
    pub subscription_type: Option<String>,
    pub timestamp: Option<serde_json::Value>,
    #[serde(alias = "data")]
    pub payload: Option<serde_json::Value>,
    #[serde(alias = "errorInfo")]
    pub error_info: Option<serde_json::Value>,
    #[serde(alias = "eventInfo")]
    pub event_info: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct FinamWsQuotePayload {
    #[serde(default, alias = "quotes")]
    quote: Vec<dto::Quote>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct FinamWsBarsPayload {
    symbol: Option<String>,
    #[serde(default, alias = "bar")]
    bars: Vec<dto::Bar>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FinamWsMapperError {
    #[error("websocket envelope is not DATA")]
    NotData,
    #[error("websocket envelope subscription type is unsupported: {0}")]
    UnsupportedSubscriptionType(String),
    #[error("websocket DATA envelope is missing payload")]
    MissingPayload,
    #[error("websocket DATA envelope is missing symbol")]
    MissingSymbol,
    #[error("websocket payload decode failed: {0}")]
    Decode(String),
    #[error(transparent)]
    Mapper(#[from] FinamMapperError),
}

pub fn build_ws_subscribe_bars_request(
    symbol: &str,
    timeframe: &str,
    token: &AccessToken,
) -> serde_json::Value {
    json!({
        "action": "SUBSCRIBE",
        "type": "BARS",
        "data": {
            "symbol": symbol,
            "timeframe": timeframe,
        },
        "token": token.as_str(),
    })
}

pub fn build_ws_subscribe_quotes_request(
    symbols: &[String],
    token: &AccessToken,
) -> serde_json::Value {
    json!({
        "action": "SUBSCRIBE",
        "type": "QUOTES",
        "data": {
            "symbols": symbols,
        },
        "token": token.as_str(),
    })
}

pub fn map_ws_market_data_events(
    envelope: &FinamWsEnvelope,
    fallback_symbol: Option<&str>,
    timeframe_sec: u32,
    received_ts: DateTime<Utc>,
) -> Result<Vec<MarketDataEvent>, FinamWsMapperError> {
    if !envelope.envelope_type.eq_ignore_ascii_case("DATA") {
        return Err(FinamWsMapperError::NotData);
    }
    let subscription_type = envelope
        .subscription_type
        .as_deref()
        .ok_or_else(|| FinamWsMapperError::UnsupportedSubscriptionType("<missing>".to_string()))?;
    let payload = envelope
        .payload
        .clone()
        .ok_or(FinamWsMapperError::MissingPayload)?;

    match subscription_type {
        subscription if subscription.eq_ignore_ascii_case("QUOTES") => {
            map_ws_quote_events(payload, fallback_symbol, received_ts)
        }
        subscription if subscription.eq_ignore_ascii_case("BARS") => {
            map_ws_bar_events(payload, fallback_symbol, timeframe_sec, received_ts)
        }
        other => Err(FinamWsMapperError::UnsupportedSubscriptionType(
            other.to_string(),
        )),
    }
}

fn map_ws_quote_events(
    payload: serde_json::Value,
    fallback_symbol: Option<&str>,
    received_ts: DateTime<Utc>,
) -> Result<Vec<MarketDataEvent>, FinamWsMapperError> {
    let payload: FinamWsQuotePayload = serde_json::from_value(payload)
        .map_err(|error| FinamWsMapperError::Decode(error.to_string()))?;
    payload
        .quote
        .into_iter()
        .map(|quote| {
            let symbol = quote
                .symbol
                .as_deref()
                .or(fallback_symbol)
                .ok_or(FinamWsMapperError::MissingSymbol)?
                .to_string();
            let response = dto::LastQuoteResponse { quote, symbol };
            let mut mapped = map_quote(&response, received_ts)?;
            mapped.source_kind = MarketDataSourceKind::LiveStream;
            Ok(MarketDataEvent::Quote(mapped))
        })
        .collect()
}

fn map_ws_bar_events(
    payload: serde_json::Value,
    fallback_symbol: Option<&str>,
    timeframe_sec: u32,
    received_ts: DateTime<Utc>,
) -> Result<Vec<MarketDataEvent>, FinamWsMapperError> {
    let payload: FinamWsBarsPayload = serde_json::from_value(payload)
        .map_err(|error| FinamWsMapperError::Decode(error.to_string()))?;
    let symbol = payload
        .symbol
        .as_deref()
        .or(fallback_symbol)
        .ok_or(FinamWsMapperError::MissingSymbol)?;
    payload
        .bars
        .into_iter()
        .map(|bar| {
            let mut mapped = map_bar(symbol, &bar, timeframe_sec)?;
            mapped.source_kind = MarketDataSourceKind::LiveStream;
            mapped.is_final = mapped.close_ts <= received_ts;
            Ok(MarketDataEvent::Bar(mapped))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn websocket_subscribe_requests_keep_expected_wire_shape() {
        let token = AccessToken::new("jwt-test-token");
        let bars = build_ws_subscribe_bars_request("IMOEXF@RTSX", "TIME_FRAME_M1", &token);
        assert_eq!(bars["action"], "SUBSCRIBE");
        assert_eq!(bars["type"], "BARS");
        assert_eq!(bars["data"]["symbol"], "IMOEXF@RTSX");
        assert_eq!(bars["data"]["timeframe"], "TIME_FRAME_M1");
        assert_eq!(bars["token"], "jwt-test-token");

        let quotes = build_ws_subscribe_quotes_request(&["IMOEXF@RTSX".to_string()], &token);
        assert_eq!(quotes["action"], "SUBSCRIBE");
        assert_eq!(quotes["type"], "QUOTES");
        assert_eq!(quotes["data"]["symbols"][0], "IMOEXF@RTSX");
    }

    #[test]
    fn websocket_quote_envelope_maps_to_live_stream_market_data() {
        let envelope: FinamWsEnvelope = serde_json::from_value(json!({
            "type": "DATA",
            "subscription_type": "QUOTES",
            "timestamp": 1783255200,
            "payload": {
                "quote": [{
                    "symbol": "IMOEXF@RTSX",
                    "timestamp": "2026-07-05T12:40:00Z",
                    "bid": {"value": "2248.0"},
                    "ask": {"value": "2248.5"},
                    "last": {"value": "2248.0"}
                }]
            }
        }))
        .expect("envelope");

        let events = map_ws_market_data_events(
            &envelope,
            None,
            60,
            Utc.with_ymd_and_hms(2026, 7, 5, 12, 40, 1)
                .single()
                .expect("ts"),
        )
        .expect("mapped");

        assert_eq!(events.len(), 1);
        match &events[0] {
            MarketDataEvent::Quote(quote) => {
                assert_eq!(quote.source_kind, MarketDataSourceKind::LiveStream);
                assert_eq!(
                    quote.instrument.venue_symbol.as_deref(),
                    Some("IMOEXF@RTSX")
                );
                assert_eq!(quote.last.expect("last").to_string(), "2248.0");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn websocket_envelope_accepts_camel_case_aliases() {
        let envelope: FinamWsEnvelope = serde_json::from_value(json!({
            "messageType": "EVENT",
            "subscriptionKey": "synthetic-key",
            "subscriptionType": "QUOTES",
            "timestamp": 1783255200,
            "data": {"event": "HANDSHAKE_SUCCESS"},
            "eventInfo": {"kind": "synthetic"}
        }))
        .expect("envelope");

        assert_eq!(envelope.envelope_type, "EVENT");
        assert_eq!(envelope.subscription_key.as_deref(), Some("synthetic-key"));
        assert_eq!(envelope.subscription_type.as_deref(), Some("QUOTES"));
        assert!(envelope.payload.is_some());
        assert!(envelope.event_info.is_some());
    }

    #[test]
    fn websocket_bar_envelope_marks_only_closed_bars_final() {
        let envelope: FinamWsEnvelope = serde_json::from_value(json!({
            "type": "DATA",
            "subscription_type": "BARS",
            "timestamp": 1783255200,
            "payload": {
                "symbol": "IMOEXF@RTSX",
                "bars": [{
                    "timestamp": "2026-07-05T12:40:00Z",
                    "open": {"value": "2248.0"},
                    "high": {"value": "2249.0"},
                    "low": {"value": "2248.0"},
                    "close": {"value": "2248.5"},
                    "volume": {"value": "10"}
                }]
            }
        }))
        .expect("envelope");

        let forming = map_ws_market_data_events(
            &envelope,
            None,
            60,
            Utc.with_ymd_and_hms(2026, 7, 5, 12, 40, 30)
                .single()
                .expect("ts"),
        )
        .expect("forming");
        let closed = map_ws_market_data_events(
            &envelope,
            None,
            60,
            Utc.with_ymd_and_hms(2026, 7, 5, 12, 41, 0)
                .single()
                .expect("ts"),
        )
        .expect("closed");

        match (&forming[0], &closed[0]) {
            (MarketDataEvent::Bar(forming), MarketDataEvent::Bar(closed)) => {
                assert_eq!(forming.source_kind, MarketDataSourceKind::LiveStream);
                assert!(!forming.is_final);
                assert!(closed.is_final);
            }
            other => panic!("unexpected events: {other:?}"),
        }
    }
}
