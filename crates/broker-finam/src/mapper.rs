use std::str::FromStr;

use broker_core::account::{CashPosition, PortfolioSnapshot};
use broker_core::event::{Bar as CoreBar, LatestMarketTrade, Quote as CoreQuote};
use broker_core::ids::{BrokerAccountId, BrokerOrderId, BrokerTradeId, ClientOrderId};
use broker_core::instrument::{Exchange, InstrumentId, Market, Money};
use broker_core::order::{Order, OrderSide, OrderStatus, OrderType, Trade};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::dto;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedScalarValue {
    pub len: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FinamMapperError {
    #[error("finam mapper missing required field: {field}")]
    MissingField { field: &'static str },
    #[error("finam mapper invalid decimal in {field}: value_len={value_len}, value_sha256={value_sha256}")]
    InvalidDecimal {
        field: &'static str,
        value_len: usize,
        value_sha256: String,
    },
    #[error("finam mapper invalid timestamp in {field}: value_len={value_len}, value_sha256={value_sha256}")]
    InvalidTimestamp {
        field: &'static str,
        value_len: usize,
        value_sha256: String,
    },
    #[error("finam mapper unsupported broker value in {field}: value_len={value_len}, value_sha256={value_sha256}")]
    UnsupportedBrokerValue {
        field: &'static str,
        value_len: usize,
        value_sha256: String,
    },
    #[error("finam mapper invalid client_order_id: {reason}")]
    InvalidClientOrderId { reason: String },
}

pub fn map_side(native: &str) -> Result<OrderSide, FinamMapperError> {
    match native {
        "SIDE_BUY" | "BUY" => Ok(OrderSide::Buy),
        "SIDE_SELL" | "SELL" => Ok(OrderSide::Sell),
        _ => Err(unsupported_value("side", native)),
    }
}

pub fn map_order_type(native: &str) -> Result<OrderType, FinamMapperError> {
    match native {
        "ORDER_TYPE_MARKET" | "MARKET" => Ok(OrderType::Market),
        "ORDER_TYPE_LIMIT" | "LIMIT" => Ok(OrderType::Limit),
        "ORDER_TYPE_STOP" | "STOP" => Ok(OrderType::Stop),
        "ORDER_TYPE_STOP_LIMIT" | "STOP_LIMIT" => Ok(OrderType::StopLimit),
        "ORDER_TYPE_TAKE_PROFIT" | "TAKE_PROFIT" => Ok(OrderType::TakeProfit),
        "ORDER_TYPE_TAKE_PROFIT_LIMIT" | "TAKE_PROFIT_LIMIT" => Ok(OrderType::TakeProfitLimit),
        _ => Err(unsupported_value("order.type", native)),
    }
}

pub fn map_order_status(native: &str) -> OrderStatus {
    match native {
        "ORDER_STATUS_NEW" | "ORDER_STATUS_ACCEPTED" => OrderStatus::New,
        "ORDER_STATUS_ACTIVE" | "ORDER_STATUS_WORKING" | "ORDER_STATUS_MATCHING" => {
            OrderStatus::Working
        }
        "ORDER_STATUS_PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
        "ORDER_STATUS_FILLED" | "ORDER_STATUS_EXECUTED" => OrderStatus::Filled,
        "ORDER_STATUS_CANCELED" | "ORDER_STATUS_CANCELLED" => OrderStatus::Canceled,
        "ORDER_STATUS_REJECTED" => OrderStatus::Rejected,
        "ORDER_STATUS_EXPIRED" => OrderStatus::Expired,
        value => OrderStatus::Unknown(value.to_string()),
    }
}

pub fn decimal_value(
    field: &'static str,
    value: &dto::DecimalValue,
) -> Result<Decimal, FinamMapperError> {
    parse_decimal(field, &value.value)
}

pub fn money_amount(
    field: &'static str,
    value: &dto::MoneyAmount,
) -> Result<Money, FinamMapperError> {
    let units = parse_decimal(field, &value.units)?;
    let nanos = Decimal::from_i128_with_scale(value.nanos as i128, 9);
    Ok(units + nanos)
}

pub fn parse_timestamp(
    field: &'static str,
    value: &str,
) -> Result<DateTime<Utc>, FinamMapperError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| invalid_timestamp(field, value))
}

pub fn instrument_id_from_symbol(symbol: &str, asset_type: Option<&str>) -> InstrumentId {
    let (ticker, mic) = symbol.split_once('@').unwrap_or((symbol, ""));
    InstrumentId {
        symbol: ticker.to_string(),
        venue_symbol: Some(symbol.to_string()),
        exchange: match mic {
            "RTSX" | "MISX" | "MOEX" => Exchange::Moex,
            "" => Exchange::Other("unknown".to_string()),
            value => Exchange::Other(value.to_string()),
        },
        market: match asset_type {
            Some("FUTURES") => Market::Futures,
            Some("OPTIONS") => Market::Options,
            Some("EQUITIES") => Market::Stocks,
            Some("CURRENCIES") | Some("FOREX") => Market::Currency,
            Some("FUNDS") => Market::Funds,
            Some(value) => Market::Other(value.to_string()),
            None => Market::Other("unknown".to_string()),
        },
    }
}

pub fn map_portfolio_snapshot(
    account: &dto::AccountResponse,
    received_ts: DateTime<Utc>,
) -> Result<PortfolioSnapshot, FinamMapperError> {
    let cash = account
        .cash
        .iter()
        .map(|cash| {
            Ok(CashPosition {
                currency: cash.currency_code.clone(),
                amount: money_amount("cash", cash)?,
            })
        })
        .collect::<Result<Vec<_>, FinamMapperError>>()?;

    Ok(PortfolioSnapshot {
        account_id: BrokerAccountId::new(account.account_id.clone()),
        positions: Vec::new(),
        cash,
        source_ts: None,
        received_ts,
    })
}

pub fn map_order_state(
    order: &dto::OrderState,
    received_ts: DateTime<Utc>,
) -> Result<Order, FinamMapperError> {
    let qty = required_decimal(
        "order.initial_quantity",
        order
            .initial_quantity
            .as_ref()
            .or(order.order.quantity.as_ref()),
    )?;
    let filled_qty = optional_decimal("order.executed_quantity", order.executed_quantity.as_ref())?
        .unwrap_or(Decimal::ZERO);
    let limit_price = optional_decimal("order.limit_price", order.order.limit_price.as_ref())?;
    let source_ts = order
        .transact_at
        .as_deref()
        .map(|value| parse_timestamp("order.transact_at", value))
        .transpose()?;

    Ok(Order {
        account_id: BrokerAccountId::new(order.order.account_id.clone()),
        order_id: order.order_id.clone().map(BrokerOrderId::new),
        client_order_id: order
            .order
            .client_order_id
            .as_deref()
            .map(map_client_order_id)
            .transpose()?,
        instrument: instrument_id_from_symbol(&order.order.symbol, None),
        side: map_side(&order.order.side)?,
        order_type: map_order_type(&order.order.order_type)?,
        status: map_order_status(&order.status),
        qty,
        filled_qty,
        limit_price,
        stop_price: None,
        comment: order.order.comment.clone(),
        source_ts,
        received_ts,
    })
}

pub fn map_latest_market_trade(
    symbol: &str,
    trade: &dto::LatestTrade,
    received_ts: DateTime<Utc>,
) -> Result<LatestMarketTrade, FinamMapperError> {
    Ok(LatestMarketTrade {
        instrument: instrument_id_from_symbol(symbol, None),
        price: decimal_value("latest_trade.price", &trade.price)?,
        qty: decimal_value("latest_trade.size", &trade.size)?,
        source_ts: parse_timestamp("latest_trade.timestamp", &trade.timestamp)?,
        received_ts,
    })
}

pub fn map_account_trade(
    account_id: &str,
    trade: &dto::AccountTrade,
    received_ts: DateTime<Utc>,
) -> Result<Trade, FinamMapperError> {
    let trade_id = trade
        .trade_id
        .as_deref()
        .ok_or(FinamMapperError::MissingField {
            field: "trade.trade_id",
        })?;
    let symbol = trade
        .symbol
        .as_deref()
        .ok_or(FinamMapperError::MissingField {
            field: "trade.symbol",
        })?;
    let side = trade
        .side
        .as_deref()
        .ok_or(FinamMapperError::MissingField {
            field: "trade.side",
        })?;
    let timestamp = trade
        .timestamp
        .as_deref()
        .or(trade.transact_at.as_deref())
        .ok_or(FinamMapperError::MissingField {
            field: "trade.timestamp",
        })?;

    Ok(Trade {
        account_id: BrokerAccountId::new(trade.account_id.as_deref().unwrap_or(account_id)),
        trade_id: BrokerTradeId::new(trade_id),
        order_id: trade.order_id.clone().map(BrokerOrderId::new),
        client_order_id: trade
            .client_order_id
            .as_deref()
            .map(map_client_order_id)
            .transpose()?,
        instrument: instrument_id_from_symbol(symbol, None),
        side: map_side(side)?,
        qty: required_decimal(
            "trade.quantity",
            trade.quantity.as_ref().or(trade.size.as_ref()),
        )?,
        price: required_decimal("trade.price", trade.price.as_ref())?,
        gross_amount: optional_decimal("trade.amount", trade.amount.as_ref())?,
        commission: trade
            .commission
            .as_ref()
            .map(|commission| money_amount("trade.commission", commission))
            .transpose()?,
        source_ts: parse_timestamp("trade.timestamp", timestamp)?,
        received_ts,
    })
}

pub fn map_bar(
    symbol: &str,
    bar: &dto::Bar,
    timeframe_sec: u32,
) -> Result<CoreBar, FinamMapperError> {
    let open_ts = parse_timestamp("bar.timestamp", &bar.timestamp)?;
    Ok(CoreBar {
        instrument: instrument_id_from_symbol(symbol, None),
        timeframe_sec,
        open_ts,
        close_ts: open_ts + Duration::seconds(i64::from(timeframe_sec)),
        open: decimal_value("bar.open", &bar.open)?,
        high: decimal_value("bar.high", &bar.high)?,
        low: decimal_value("bar.low", &bar.low)?,
        close: decimal_value("bar.close", &bar.close)?,
        volume: decimal_value("bar.volume", &bar.volume)?,
        is_final: true,
    })
}

pub fn map_quote(
    quote: &dto::LastQuoteResponse,
    received_ts: DateTime<Utc>,
) -> Result<CoreQuote, FinamMapperError> {
    Ok(CoreQuote {
        instrument: instrument_id_from_symbol(&quote.symbol, None),
        bid: optional_decimal("quote.bid", quote.quote.bid.as_ref())?,
        ask: optional_decimal("quote.ask", quote.quote.ask.as_ref())?,
        last: optional_decimal("quote.last", quote.quote.last.as_ref())?,
        source_ts: quote
            .quote
            .timestamp
            .as_deref()
            .map(|timestamp| parse_timestamp("quote.timestamp", timestamp))
            .transpose()?,
        received_ts,
    })
}

fn required_decimal(
    field: &'static str,
    value: Option<&dto::DecimalValue>,
) -> Result<Decimal, FinamMapperError> {
    value
        .ok_or(FinamMapperError::MissingField { field })
        .and_then(|value| decimal_value(field, value))
}

fn optional_decimal(
    field: &'static str,
    value: Option<&dto::DecimalValue>,
) -> Result<Option<Decimal>, FinamMapperError> {
    value.map(|value| decimal_value(field, value)).transpose()
}

fn parse_decimal(field: &'static str, value: &str) -> Result<Decimal, FinamMapperError> {
    Decimal::from_str(value).map_err(|_| invalid_decimal(field, value))
}

fn map_client_order_id(value: &str) -> Result<ClientOrderId, FinamMapperError> {
    ClientOrderId::new(value).map_err(|error| FinamMapperError::InvalidClientOrderId {
        reason: error.to_string(),
    })
}

fn invalid_decimal(field: &'static str, value: &str) -> FinamMapperError {
    let redacted = redact_scalar_value(value);
    FinamMapperError::InvalidDecimal {
        field,
        value_len: redacted.len,
        value_sha256: redacted.sha256,
    }
}

fn invalid_timestamp(field: &'static str, value: &str) -> FinamMapperError {
    let redacted = redact_scalar_value(value);
    FinamMapperError::InvalidTimestamp {
        field,
        value_len: redacted.len,
        value_sha256: redacted.sha256,
    }
}

fn unsupported_value(field: &'static str, value: &str) -> FinamMapperError {
    let redacted = redact_scalar_value(value);
    FinamMapperError::UnsupportedBrokerValue {
        field,
        value_len: redacted.len,
        value_sha256: redacted.sha256,
    }
}

fn redact_scalar_value(value: &str) -> RedactedScalarValue {
    RedactedScalarValue {
        len: value.len(),
        sha256: crate::sha256_hex(value.as_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn maps_known_order_enums_and_preserves_unknown_status() {
        assert_eq!(map_side("SIDE_BUY").expect("buy"), OrderSide::Buy);
        assert_eq!(map_side("SIDE_SELL").expect("sell"), OrderSide::Sell);
        assert_eq!(
            map_order_type("ORDER_TYPE_LIMIT").expect("limit"),
            OrderType::Limit
        );
        assert_eq!(
            map_order_status("ORDER_STATUS_CANCELED"),
            OrderStatus::Canceled
        );
        assert_eq!(
            map_order_status("BROKER_NEW_STATUS"),
            OrderStatus::Unknown("BROKER_NEW_STATUS".to_string())
        );
    }

    #[test]
    fn mapper_errors_redact_raw_values() {
        let error = map_side("SECRET_NATIVE_SIDE").expect_err("unsupported");
        let display = error.to_string();

        assert!(display.contains("value_sha256="));
        assert!(!display.contains("SECRET_NATIVE_SIDE"));
    }

    #[test]
    fn maps_order_state_from_finam_shape() {
        let order: dto::OrderState = serde_json::from_value(serde_json::json!({
            "executed_quantity": {"value": "0"},
            "initial_quantity": {"value": "1"},
            "order": {
                "account_id": "1909892",
                "client_order_id": "ABC123",
                "comment": "manual test",
                "legs": [],
                "limit_price": {"value": "1000.5"},
                "quantity": {"value": "1"},
                "side": "SIDE_BUY",
                "symbol": "IMOEXF@RTSX",
                "time_in_force": "TIME_IN_FORCE_DAY",
                "type": "ORDER_TYPE_LIMIT",
                "valid_before": "2026-06-29T23:59:59Z"
            },
            "order_id": "BROKER-ORDER-1",
            "remaining_quantity": {"value": "1"},
            "status": "ORDER_STATUS_CANCELED",
            "transact_at": "2026-06-29T09:10:00Z"
        }))
        .expect("order dto");

        let received_ts = parse_timestamp("test", "2026-06-29T09:10:01Z").expect("timestamp");
        let mapped = map_order_state(&order, received_ts).expect("mapped order");

        assert_eq!(mapped.account_id.as_str(), "1909892");
        assert_eq!(mapped.instrument.symbol, "IMOEXF");
        assert_eq!(
            mapped.instrument.venue_symbol.as_deref(),
            Some("IMOEXF@RTSX")
        );
        assert_eq!(mapped.side, OrderSide::Buy);
        assert_eq!(mapped.order_type, OrderType::Limit);
        assert_eq!(mapped.status, OrderStatus::Canceled);
        assert_eq!(mapped.qty, Decimal::ONE);
        assert_eq!(mapped.filled_qty, Decimal::ZERO);
        assert_eq!(mapped.limit_price, Some(Decimal::new(10005, 1)));
    }

    #[test]
    fn maps_bar_timestamp_as_open_and_close_from_timeframe() {
        let bar = dto::Bar {
            open: dto::DecimalValue {
                value: "100".into(),
            },
            high: dto::DecimalValue {
                value: "105".into(),
            },
            low: dto::DecimalValue { value: "99".into() },
            close: dto::DecimalValue {
                value: "101".into(),
            },
            volume: dto::DecimalValue { value: "10".into() },
            timestamp: "2026-06-29T09:10:00Z".into(),
        };

        let mapped = map_bar("IMOEXF@RTSX", &bar, 60).expect("mapped bar");

        assert_eq!(mapped.instrument.symbol, "IMOEXF");
        assert_eq!(mapped.timeframe_sec, 60);
        assert_eq!(mapped.close_ts, mapped.open_ts + Duration::seconds(60));
        assert!(mapped.is_final);
    }

    #[test]
    fn maps_empty_account_snapshot_cash() {
        let account: dto::AccountResponse = serde_json::from_value(serde_json::json!({
            "account_id": "1909892",
            "cash": [
                {"currency_code": "RUB", "units": "6000", "nanos": 0}
            ],
            "positions": [],
            "status": "ACCOUNT_ACTIVE",
            "type": "UNION"
        }))
        .expect("account dto");
        let received_ts = parse_timestamp("test", "2026-06-29T09:10:01Z").expect("timestamp");

        let snapshot = map_portfolio_snapshot(&account, received_ts).expect("snapshot");

        assert_eq!(snapshot.account_id.as_str(), "1909892");
        assert!(snapshot.positions.is_empty());
        assert_eq!(snapshot.cash[0].currency, "RUB");
        assert_eq!(snapshot.cash[0].amount, Decimal::new(6000, 0));
    }
}
