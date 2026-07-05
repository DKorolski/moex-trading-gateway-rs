use std::str::FromStr;

use broker_core::account::{CashPosition, PortfolioSnapshot, Position};
use broker_core::broker::BrokerKind;
use broker_core::event::{
    Bar as CoreBar, LatestMarketTrade, MarketDataSourceKind, Quote as CoreQuote,
};
use broker_core::ids::{BrokerAccountId, BrokerOrderId, BrokerTradeId, ClientOrderId};
use broker_core::instrument::{
    BrokerSymbol, Exchange, InstrumentId, InstrumentMapEntry, InternalSymbol, Market, Money,
};
use broker_core::operational_config::{
    BrokerFeedFreshness, BrokerMarketSessionState, BrokerReadinessSnapshot,
};
use broker_core::operational_snapshot::{
    BrokerCashSnapshot, BrokerInstrumentSpec, BrokerOrderSnapshot, BrokerPositionSnapshot,
    BrokerTradeSnapshot, BrokerTruthSnapshot,
};
use broker_core::order::{
    Order, OrderSide, OrderStatus, OrderType, RedactedValueFingerprint, TimeInForce, Trade,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::dto;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedScalarValue {
    pub len: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderSnapshotClass {
    Active,
    Terminal,
    BlockingUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinamOrderStatusClass {
    ActiveOrPending,
    CancelPending,
    TerminalFilled,
    TerminalCanceled,
    TerminalRejected,
    TerminalExpired,
    NeedsPolicy,
    ManualOrDegraded,
    BlockingUnknown,
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
    #[error("finam mapper invalid timeframe: timeframe_sec must be greater than zero")]
    InvalidTimeframe,
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

pub fn map_time_in_force(native: &str) -> Result<TimeInForce, FinamMapperError> {
    match native {
        "TIME_IN_FORCE_DAY" | "DAY" => Ok(TimeInForce::Day),
        "TIME_IN_FORCE_GOOD_TILL_CANCEL" | "GOOD_TILL_CANCEL" | "GTC" => {
            Ok(TimeInForce::GoodTillCancel)
        }
        "TIME_IN_FORCE_GOOD_TILL_DATE" | "GOOD_TILL_DATE" | "GTD" => Ok(TimeInForce::GoodTillDate),
        "TIME_IN_FORCE_FOK" | "FOK" => Ok(TimeInForce::FillOrKill),
        "TIME_IN_FORCE_IOC" | "IOC" => Ok(TimeInForce::ImmediateOrCancel),
        _ => Err(unsupported_value("order.time_in_force", native)),
    }
}

fn normalized_order_status(native: &str) -> &str {
    native
        .strip_prefix("ORDER_STATUS_")
        .unwrap_or(native)
        .trim()
}

pub fn classify_finam_order_status(native: &str) -> FinamOrderStatusClass {
    match normalized_order_status(native) {
        "NEW" | "ACCEPTED" | "ACTIVE" | "WORKING" | "MATCHING" | "WAIT" | "FORWARDING"
        | "WATCHING" | "PENDING_NEW" | "PARTIALLY_FILLED" => FinamOrderStatusClass::ActiveOrPending,
        "PENDING_CANCEL" => FinamOrderStatusClass::CancelPending,
        "FILLED" | "EXECUTED" | "SL_EXECUTED" | "TP_EXECUTED" => {
            FinamOrderStatusClass::TerminalFilled
        }
        "CANCELED" | "CANCELLED" => FinamOrderStatusClass::TerminalCanceled,
        "REJECTED" | "FAILED" | "DENIED_BY_BROKER" | "REJECTED_BY_EXCHANGE" => {
            FinamOrderStatusClass::TerminalRejected
        }
        "EXPIRED" => FinamOrderStatusClass::TerminalExpired,
        "DONE_FOR_DAY" | "REPLACED" | "LINK_WAIT" | "SL_GUARD_TIME" | "SL_FORWARDING"
        | "TP_GUARD_TIME" | "TP_CORRECTION" | "TP_FORWARDING" | "TP_CORR_GUARD_TIME" => {
            FinamOrderStatusClass::NeedsPolicy
        }
        "SUSPENDED" | "DISABLED" => FinamOrderStatusClass::ManualOrDegraded,
        _ => FinamOrderStatusClass::BlockingUnknown,
    }
}

pub fn map_order_status(native: &str) -> OrderStatus {
    match normalized_order_status(native) {
        "NEW" | "ACCEPTED" => OrderStatus::New,
        "ACTIVE" | "WORKING" | "MATCHING" | "WAIT" | "FORWARDING" | "WATCHING" | "PENDING_NEW" => {
            OrderStatus::Working
        }
        "PENDING_CANCEL" => OrderStatus::Working,
        "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
        "FILLED" | "EXECUTED" | "SL_EXECUTED" | "TP_EXECUTED" => OrderStatus::Filled,
        "CANCELED" | "CANCELLED" => OrderStatus::Canceled,
        "REJECTED" | "FAILED" | "DENIED_BY_BROKER" | "REJECTED_BY_EXCHANGE" => {
            OrderStatus::Rejected
        }
        "EXPIRED" => OrderStatus::Expired,
        value => OrderStatus::Unknown(value.to_string()),
    }
}

pub fn classify_order_status(status: &OrderStatus) -> OrderSnapshotClass {
    match status {
        OrderStatus::New | OrderStatus::Working | OrderStatus::PartiallyFilled => {
            OrderSnapshotClass::Active
        }
        OrderStatus::Filled
        | OrderStatus::Canceled
        | OrderStatus::Rejected
        | OrderStatus::Expired => OrderSnapshotClass::Terminal,
        OrderStatus::Unknown(_) => OrderSnapshotClass::BlockingUnknown,
    }
}

pub fn classify_native_order_status(native: &str) -> OrderSnapshotClass {
    match classify_finam_order_status(native) {
        FinamOrderStatusClass::ActiveOrPending | FinamOrderStatusClass::CancelPending => {
            OrderSnapshotClass::Active
        }
        FinamOrderStatusClass::TerminalFilled
        | FinamOrderStatusClass::TerminalCanceled
        | FinamOrderStatusClass::TerminalRejected
        | FinamOrderStatusClass::TerminalExpired => OrderSnapshotClass::Terminal,
        FinamOrderStatusClass::NeedsPolicy
        | FinamOrderStatusClass::ManualOrDegraded
        | FinamOrderStatusClass::BlockingUnknown => OrderSnapshotClass::BlockingUnknown,
    }
}

pub fn has_blocking_unknown_order_statuses(orders: &[Order]) -> bool {
    orders
        .iter()
        .any(|order| classify_order_status(&order.status) == OrderSnapshotClass::BlockingUnknown)
}

pub fn active_orders(orders: &[Order]) -> impl Iterator<Item = &Order> {
    orders
        .iter()
        .filter(|order| classify_order_status(&order.status) == OrderSnapshotClass::Active)
}

pub fn terminal_orders(orders: &[Order]) -> impl Iterator<Item = &Order> {
    orders
        .iter()
        .filter(|order| classify_order_status(&order.status) == OrderSnapshotClass::Terminal)
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
    let positions = account
        .positions
        .iter()
        .map(|position| map_account_position(&account.account_id, position))
        .filter(|position| {
            position
                .as_ref()
                .map(|position| position.qty != Decimal::ZERO)
                .unwrap_or(true)
        })
        .collect::<Result<Vec<_>, FinamMapperError>>()?;

    Ok(PortfolioSnapshot {
        account_id: BrokerAccountId::new(account.account_id.clone()),
        positions,
        cash,
        source_ts: None,
        received_ts,
    })
}

pub fn map_finam_broker_truth_snapshot(
    account: &dto::AccountResponse,
    orders: &dto::AccountOrdersResponse,
    received_ts: DateTime<Utc>,
) -> Result<BrokerTruthSnapshot, FinamMapperError> {
    map_finam_broker_truth_snapshot_with_readonly_artifacts(account, orders, None, &[], received_ts)
}

#[derive(Debug, Clone, Copy)]
pub struct FinamInstrumentSpecArtifacts<'a> {
    pub asset: &'a dto::AssetResponse,
    pub params: &'a dto::AssetParamsResponse,
    pub schedule: &'a dto::AssetScheduleResponse,
}

pub fn map_finam_broker_truth_snapshot_with_readonly_artifacts(
    account: &dto::AccountResponse,
    orders: &dto::AccountOrdersResponse,
    trades: Option<&dto::AccountTradesResponse>,
    instruments: &[FinamInstrumentSpecArtifacts<'_>],
    received_ts: DateTime<Utc>,
) -> Result<BrokerTruthSnapshot, FinamMapperError> {
    let account_id = BrokerAccountId::new(account.account_id.clone());
    let positions = account
        .positions
        .iter()
        .map(|position| {
            map_account_position_to_broker_position(&account.account_id, position, received_ts)
        })
        .filter(|position| {
            position
                .as_ref()
                .map(|position| position.qty != Decimal::ZERO)
                .unwrap_or(true)
        })
        .collect::<Result<Vec<_>, FinamMapperError>>()?;
    let orders = orders
        .orders
        .iter()
        .map(|order| map_order_state_to_broker_order_snapshot(order, received_ts))
        .collect::<Result<Vec<_>, FinamMapperError>>()?;
    let cash = Some(map_account_cash_snapshot(account, received_ts)?);
    let trades = trades
        .map(|trades| {
            trades
                .trades
                .iter()
                .map(|trade| {
                    map_account_trade_to_broker_trade_snapshot(
                        &account.account_id,
                        trade,
                        received_ts,
                    )
                })
                .collect::<Result<Vec<_>, FinamMapperError>>()
        })
        .transpose()?
        .unwrap_or_default();
    let instruments = instruments
        .iter()
        .map(|artifacts| {
            map_finam_instrument_spec(artifacts.asset, artifacts.params, artifacts.schedule)
        })
        .collect::<Result<Vec<_>, FinamMapperError>>()?;

    Ok(BrokerTruthSnapshot {
        account_id,
        orders,
        positions,
        cash,
        trades,
        instruments,
        received_ts,
    })
}

pub fn map_finam_instrument_spec(
    asset: &dto::AssetResponse,
    params: &dto::AssetParamsResponse,
    schedule: &dto::AssetScheduleResponse,
) -> Result<BrokerInstrumentSpec, FinamMapperError> {
    let ticker = asset
        .ticker
        .as_deref()
        .ok_or(FinamMapperError::MissingField {
            field: "asset.ticker",
        })?;
    let mic = asset
        .mic
        .as_deref()
        .ok_or(FinamMapperError::MissingField { field: "asset.mic" })?;
    let venue_symbol = format!("{ticker}@{mic}");
    if params.symbol != venue_symbol {
        return Err(unsupported_value("asset_params.symbol", &params.symbol));
    }
    if schedule.symbol != venue_symbol {
        return Err(unsupported_value("asset_schedule.symbol", &schedule.symbol));
    }
    let market = market_from_asset_type(asset.asset_type.as_deref());
    let price_step = asset_price_step(asset)?;
    let lot_size = asset_lot_size(asset)?;
    let step_value = asset_step_value(asset)?;
    let currency = asset
        .quote_currency
        .clone()
        .ok_or(FinamMapperError::MissingField {
            field: "asset.quote_currency",
        })?;
    let expiration_date = asset
        .future_details
        .as_ref()
        .and_then(|future| {
            future
                .expiration_date
                .as_deref()
                .or(future.last_trade_date.as_deref())
        })
        .map(|value| parse_date("asset.future_details.expiration_date", value))
        .transpose()?;
    let is_tradable = params.tradeable.or(params.is_tradable).unwrap_or(false);

    Ok(BrokerInstrumentSpec {
        instrument: InstrumentMapEntry {
            internal_symbol: InternalSymbol(ticker.to_string()),
            broker: BrokerKind::Finam,
            broker_symbol: BrokerSymbol(venue_symbol),
            exchange: exchange_from_mic(mic),
            market,
            price_step,
            qty_step: Decimal::ONE,
            lot_size,
            min_qty: Decimal::ONE,
            step_value,
            currency,
            schedule_id: asset.board.clone().unwrap_or_else(|| mic.to_string()),
            expiration_date,
            is_tradable,
        },
    })
}

pub fn map_finam_broker_readiness_snapshot(
    account: &dto::AccountResponse,
    orders: &dto::AccountOrdersResponse,
    trades: Option<&dto::AccountTradesResponse>,
    quote: Option<&dto::LastQuoteResponse>,
    instrument_specs: &[BrokerInstrumentSpec],
    schedule: Option<&dto::AssetScheduleResponse>,
    received_ts: DateTime<Utc>,
) -> Result<BrokerReadinessSnapshot, FinamMapperError> {
    let quote_observed_ts = quote
        .and_then(|quote| quote.quote.timestamp.as_deref())
        .map(|timestamp| parse_timestamp("quote.timestamp", timestamp))
        .transpose()?
        .or_else(|| quote.map(|_| received_ts));
    let market_session = schedule
        .map(|schedule| finam_schedule_market_session(schedule, received_ts))
        .unwrap_or(BrokerMarketSessionState::Unknown);
    let unknown_order_count = orders
        .orders
        .iter()
        .filter(|order| {
            classify_native_order_status(&order.status) == OrderSnapshotClass::BlockingUnknown
        })
        .count();
    let cash_margin_present = account.portfolio_mc.as_ref().is_some_and(|portfolio| {
        portfolio.available_cash.is_some()
            && (portfolio.initial_margin.is_some() || portfolio.maintenance_margin.is_some())
    });
    let instrument_spec_validated = !instrument_specs.is_empty()
        && instrument_specs
            .iter()
            .all(|spec| spec.instrument.is_tradable);

    Ok(BrokerReadinessSnapshot {
        account: freshness(Some(received_ts), 120_000),
        positions: freshness(Some(received_ts), 120_000),
        orders: freshness(Some(received_ts), 120_000),
        trades: freshness(trades.map(|_| received_ts), 120_000),
        quotes: freshness(quote_observed_ts, 30_000),
        instrument_spec: freshness(
            (!instrument_specs.is_empty()).then_some(received_ts),
            86_400_000,
        ),
        schedule: freshness(schedule.map(|_| received_ts), 86_400_000),
        market_session,
        unknown_order_count,
        cash_margin_present,
        instrument_spec_validated,
    })
}

pub fn map_account_position_to_broker_position(
    account_id: &str,
    position: &dto::AccountPosition,
    received_ts: DateTime<Utc>,
) -> Result<BrokerPositionSnapshot, FinamMapperError> {
    let position = map_account_position(account_id, position)?;
    Ok(BrokerPositionSnapshot {
        account_id: position.account_id,
        instrument: position.instrument,
        qty: position.qty,
        avg_price: position.avg_price,
        unrealized_pnl: position.unrealized_pnl,
        source_ts: position.source_ts,
        received_ts,
    })
}

pub fn map_account_cash_snapshot(
    account: &dto::AccountResponse,
    received_ts: DateTime<Utc>,
) -> Result<BrokerCashSnapshot, FinamMapperError> {
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
    let equity = optional_decimal("account.equity", account.equity.as_ref())?;
    let free_cash = account
        .portfolio_mc
        .as_ref()
        .and_then(|portfolio| portfolio.available_cash.as_ref())
        .map(|value| decimal_value("account.portfolio_mc.available_cash", value))
        .transpose()?;
    let initial_margin = account
        .portfolio_mc
        .as_ref()
        .and_then(|portfolio| portfolio.initial_margin.as_ref())
        .map(|value| decimal_value("account.portfolio_mc.initial_margin", value))
        .transpose()?;
    let maintenance_margin = account
        .portfolio_mc
        .as_ref()
        .and_then(|portfolio| portfolio.maintenance_margin.as_ref())
        .map(|value| decimal_value("account.portfolio_mc.maintenance_margin", value))
        .transpose()?;

    Ok(BrokerCashSnapshot {
        account_id: BrokerAccountId::new(account.account_id.clone()),
        cash,
        equity,
        free_cash,
        initial_margin,
        maintenance_margin,
        source_ts: None,
        received_ts,
    })
}

pub fn map_account_position(
    account_id: &str,
    position: &dto::AccountPosition,
) -> Result<Position, FinamMapperError> {
    let symbol = position
        .symbol
        .as_deref()
        .ok_or(FinamMapperError::MissingField {
            field: "position.symbol",
        })?;
    let qty = required_decimal(
        "position.quantity",
        position.quantity.as_ref().or(position.balance.as_ref()),
    )?;
    let avg_price = optional_decimal(
        "position.average_price",
        position
            .average_price
            .as_ref()
            .or(position.avg_price.as_ref()),
    )?;
    let unrealized_pnl = optional_decimal(
        "position.unrealized_profit",
        position.unrealized_profit.as_ref(),
    )?;

    Ok(Position {
        account_id: BrokerAccountId::new(account_id),
        instrument: instrument_id_from_symbol(symbol, position.asset_type.as_deref()),
        qty,
        avg_price,
        unrealized_pnl,
        source_ts: None,
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
            .and_then(map_client_order_id_if_core_safe),
        broker_client_order_id_fingerprint: order
            .order
            .client_order_id
            .as_deref()
            .map(redact_core_value),
        instrument: instrument_id_from_symbol(&order.order.symbol, None),
        side: map_side(&order.order.side)?,
        order_type: map_order_type(&order.order.order_type)?,
        status: map_order_status(&order.status),
        qty,
        filled_qty,
        limit_price,
        stop_price: None,
        comment_fingerprint: order.order.comment.as_deref().map(redact_core_value),
        comment: None,
        source_ts,
        received_ts,
    })
}

pub fn map_order_state_to_broker_order_snapshot(
    order: &dto::OrderState,
    received_ts: DateTime<Utc>,
) -> Result<BrokerOrderSnapshot, FinamMapperError> {
    let mapped = map_order_state(order, received_ts)?;
    let remaining_qty = optional_decimal(
        "order.remaining_quantity",
        order.remaining_quantity.as_ref(),
    )?;
    let time_in_force = order
        .order
        .time_in_force
        .as_deref()
        .map(map_time_in_force)
        .transpose()?;
    let lifecycle = BrokerOrderSnapshot::lifecycle_for(&mapped.status);

    Ok(BrokerOrderSnapshot {
        account_id: mapped.account_id,
        broker_order_id: mapped.order_id,
        client_order_id: mapped.client_order_id,
        instrument: mapped.instrument,
        side: mapped.side,
        order_type: mapped.order_type,
        time_in_force,
        status: mapped.status,
        lifecycle,
        qty: mapped.qty,
        filled_qty: mapped.filled_qty,
        remaining_qty,
        limit_price: mapped.limit_price,
        source_ts: mapped.source_ts,
        received_ts: mapped.received_ts,
    })
}

pub fn map_latest_market_trade(
    symbol: &str,
    trade: &dto::LatestTrade,
    received_ts: DateTime<Utc>,
) -> Result<LatestMarketTrade, FinamMapperError> {
    Ok(LatestMarketTrade {
        instrument: instrument_id_from_symbol(symbol, None),
        source_kind: MarketDataSourceKind::ReadOnlyPoll,
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
            .and_then(map_client_order_id_if_core_safe),
        broker_client_order_id_fingerprint: trade.client_order_id.as_deref().map(redact_core_value),
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

pub fn map_account_trade_to_broker_trade_snapshot(
    account_id: &str,
    trade: &dto::AccountTrade,
    received_ts: DateTime<Utc>,
) -> Result<BrokerTradeSnapshot, FinamMapperError> {
    let mapped = map_account_trade(account_id, trade, received_ts)?;
    Ok(BrokerTradeSnapshot {
        account_id: mapped.account_id,
        broker_trade_id: mapped.trade_id,
        broker_order_id: mapped.order_id,
        client_order_id: mapped.client_order_id,
        instrument: mapped.instrument,
        side: mapped.side,
        qty: mapped.qty,
        price: mapped.price,
        gross_amount: mapped.gross_amount,
        commission: mapped.commission,
        source_ts: mapped.source_ts,
        received_ts: mapped.received_ts,
    })
}

pub fn map_bar(
    symbol: &str,
    bar: &dto::Bar,
    timeframe_sec: u32,
) -> Result<CoreBar, FinamMapperError> {
    if timeframe_sec == 0 {
        return Err(FinamMapperError::InvalidTimeframe);
    }
    let open_ts = parse_timestamp("bar.timestamp", &bar.timestamp)?;
    Ok(CoreBar {
        instrument: instrument_id_from_symbol(symbol, None),
        source_kind: MarketDataSourceKind::HistoricalPoll,
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
        source_kind: MarketDataSourceKind::ReadOnlyPoll,
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

fn decimal_like(
    field: &'static str,
    value: &dto::DecimalLike,
) -> Result<Decimal, FinamMapperError> {
    match value {
        dto::DecimalLike::Value(value) => decimal_value(field, value),
        dto::DecimalLike::String(value) => parse_decimal(field, value),
    }
}

fn parse_decimal(field: &'static str, value: &str) -> Result<Decimal, FinamMapperError> {
    Decimal::from_str(value).map_err(|_| invalid_decimal(field, value))
}

fn parse_date(field: &'static str, value: &str) -> Result<NaiveDate, FinamMapperError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid_timestamp(field, value))
}

fn asset_price_step(asset: &dto::AssetResponse) -> Result<Decimal, FinamMapperError> {
    asset
        .future_details
        .as_ref()
        .and_then(|future| future.min_step.as_ref())
        .or(asset.min_step.as_ref())
        .ok_or(FinamMapperError::MissingField {
            field: "asset.min_step",
        })
        .and_then(|value| decimal_like("asset.min_step", value))
}

fn asset_lot_size(asset: &dto::AssetResponse) -> Result<Decimal, FinamMapperError> {
    asset
        .future_details
        .as_ref()
        .and_then(|future| future.lot_size.as_ref())
        .or(asset.lot_size.as_ref())
        .ok_or(FinamMapperError::MissingField {
            field: "asset.lot_size",
        })
        .and_then(|value| decimal_value("asset.lot_size", value))
}

fn asset_step_value(asset: &dto::AssetResponse) -> Result<Decimal, FinamMapperError> {
    asset
        .future_details
        .as_ref()
        .and_then(|future| future.step_price.as_ref())
        .ok_or(FinamMapperError::MissingField {
            field: "asset.future_details.step_price",
        })
        .and_then(|value| decimal_like("asset.future_details.step_price", value))
}

fn exchange_from_mic(mic: &str) -> Exchange {
    match mic {
        "RTSX" | "MISX" | "MOEX" => Exchange::Moex,
        value => Exchange::Other(value.to_string()),
    }
}

fn market_from_asset_type(asset_type: Option<&str>) -> Market {
    match asset_type {
        Some(value) if value.contains("FUT") => Market::Futures,
        Some(value) if value.contains("OPTION") => Market::Options,
        Some(value) if value.contains("STOCK") || value.contains("SHARE") => Market::Stocks,
        Some(value) if value.contains("CURRENC") || value.contains("FOREX") => Market::Currency,
        Some(value) if value.contains("FUND") => Market::Funds,
        Some(value) => Market::Other(value.to_string()),
        None => Market::Other("unknown".to_string()),
    }
}

fn finam_schedule_market_session(
    schedule: &dto::AssetScheduleResponse,
    checked_at: DateTime<Utc>,
) -> BrokerMarketSessionState {
    if schedule.sessions.is_empty() {
        return BrokerMarketSessionState::Unknown;
    }
    let has_open_session = schedule.sessions.iter().any(|session| {
        let Some(interval) = &session.interval else {
            return false;
        };
        let Some(start_time) = interval.start_time.as_deref() else {
            return false;
        };
        let Some(end_time) = interval.end_time.as_deref() else {
            return false;
        };
        let Ok(start) = DateTime::parse_from_rfc3339(start_time) else {
            return false;
        };
        let Ok(end) = DateTime::parse_from_rfc3339(end_time) else {
            return false;
        };
        checked_at >= start.with_timezone(&Utc) && checked_at < end.with_timezone(&Utc)
    });
    if has_open_session {
        BrokerMarketSessionState::Open
    } else {
        BrokerMarketSessionState::Closed
    }
}

fn freshness(observed_ts: Option<DateTime<Utc>>, max_age_ms: u64) -> BrokerFeedFreshness {
    BrokerFeedFreshness {
        observed_ts,
        max_age_ms,
    }
}

fn map_client_order_id_if_core_safe(value: &str) -> Option<ClientOrderId> {
    ClientOrderId::new(value).ok()
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

fn redact_core_value(value: &str) -> RedactedValueFingerprint {
    RedactedValueFingerprint {
        len: value.len(),
        sha256: crate::sha256_hex(value.as_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::instrument_identity_matches;
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
        assert_eq!(map_order_status("ORDER_STATUS_WAIT"), OrderStatus::Working);
        assert_eq!(map_order_status("WAIT"), OrderStatus::Working);
        assert_eq!(
            map_order_status("ORDER_STATUS_DENIED_BY_BROKER"),
            OrderStatus::Rejected
        );
        assert_eq!(
            map_order_status("ORDER_STATUS_SL_EXECUTED"),
            OrderStatus::Filled
        );
        assert_eq!(
            map_order_status("BROKER_NEW_STATUS"),
            OrderStatus::Unknown("BROKER_NEW_STATUS".to_string())
        );
    }

    #[test]
    fn classifies_finam_order_statuses_by_explicit_m3d1_buckets() {
        for status in [
            "ORDER_STATUS_WAIT",
            "ORDER_STATUS_FORWARDING",
            "ORDER_STATUS_WATCHING",
            "ORDER_STATUS_PENDING_NEW",
        ] {
            assert_eq!(
                classify_finam_order_status(status),
                FinamOrderStatusClass::ActiveOrPending,
                "{status}"
            );
        }
        assert_eq!(
            classify_finam_order_status("ORDER_STATUS_PENDING_CANCEL"),
            FinamOrderStatusClass::CancelPending
        );
        for status in [
            "ORDER_STATUS_FAILED",
            "ORDER_STATUS_DENIED_BY_BROKER",
            "ORDER_STATUS_REJECTED_BY_EXCHANGE",
        ] {
            assert_eq!(
                classify_finam_order_status(status),
                FinamOrderStatusClass::TerminalRejected,
                "{status}"
            );
        }
        for status in [
            "ORDER_STATUS_EXECUTED",
            "ORDER_STATUS_SL_EXECUTED",
            "TP_EXECUTED",
        ] {
            assert_eq!(
                classify_finam_order_status(status),
                FinamOrderStatusClass::TerminalFilled,
                "{status}"
            );
        }
        assert_eq!(
            classify_finam_order_status("ORDER_STATUS_DONE_FOR_DAY"),
            FinamOrderStatusClass::NeedsPolicy
        );
        assert_eq!(
            classify_finam_order_status("ORDER_STATUS_REPLACED"),
            FinamOrderStatusClass::NeedsPolicy
        );
        assert_eq!(
            classify_finam_order_status("ORDER_STATUS_SUSPENDED"),
            FinamOrderStatusClass::ManualOrDegraded
        );
        assert_eq!(
            classify_finam_order_status("ORDER_STATUS_DISABLED"),
            FinamOrderStatusClass::ManualOrDegraded
        );
        assert_eq!(
            classify_finam_order_status("BROKER_NEW_STATUS"),
            FinamOrderStatusClass::BlockingUnknown
        );
    }

    #[test]
    fn classifies_order_status_for_snapshot_readiness() {
        assert_eq!(
            classify_native_order_status("ORDER_STATUS_ACTIVE"),
            OrderSnapshotClass::Active
        );
        assert_eq!(
            classify_native_order_status("ORDER_STATUS_PARTIALLY_FILLED"),
            OrderSnapshotClass::Active
        );
        assert_eq!(
            classify_native_order_status("ORDER_STATUS_CANCELED"),
            OrderSnapshotClass::Terminal
        );
        assert_eq!(
            classify_native_order_status("ORDER_STATUS_REJECTED"),
            OrderSnapshotClass::Terminal
        );
        assert_eq!(
            classify_native_order_status("ORDER_STATUS_PENDING_CANCEL"),
            OrderSnapshotClass::Active
        );
        assert_eq!(
            classify_native_order_status("ORDER_STATUS_DONE_FOR_DAY"),
            OrderSnapshotClass::BlockingUnknown
        );
        assert_eq!(
            classify_native_order_status("ORDER_STATUS_DISABLED"),
            OrderSnapshotClass::BlockingUnknown
        );
        assert_eq!(
            classify_native_order_status("BROKER_NEW_STATUS"),
            OrderSnapshotClass::BlockingUnknown
        );
    }

    #[test]
    fn pinned_finam_spec_fixture_contains_statuses_used_by_classifier() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/finam_spec/order_contract_enums_v2026_07_03.json"
        ))
        .expect("fixture json");
        let values = fixture["order_status"]
            .as_array()
            .expect("order_status array");

        for expected in [
            "ORDER_STATUS_WAIT",
            "ORDER_STATUS_FORWARDING",
            "ORDER_STATUS_WATCHING",
            "ORDER_STATUS_PENDING_NEW",
            "ORDER_STATUS_PENDING_CANCEL",
            "ORDER_STATUS_FAILED",
            "ORDER_STATUS_DENIED_BY_BROKER",
            "ORDER_STATUS_REJECTED_BY_EXCHANGE",
            "ORDER_STATUS_EXECUTED",
            "ORDER_STATUS_SL_EXECUTED",
            "ORDER_STATUS_TP_EXECUTED",
            "ORDER_STATUS_DONE_FOR_DAY",
            "ORDER_STATUS_REPLACED",
            "ORDER_STATUS_SUSPENDED",
            "ORDER_STATUS_DISABLED",
        ] {
            assert!(
                values.iter().any(|value| value == expected),
                "missing pinned FINAM order status enum {expected}"
            );
        }
    }

    fn expected_status_class_from_policy(policy: &str) -> FinamOrderStatusClass {
        match policy {
            "active_or_pending" => FinamOrderStatusClass::ActiveOrPending,
            "cancel_pending" => FinamOrderStatusClass::CancelPending,
            "terminal_filled" => FinamOrderStatusClass::TerminalFilled,
            "terminal_canceled" => FinamOrderStatusClass::TerminalCanceled,
            "terminal_rejected" => FinamOrderStatusClass::TerminalRejected,
            "terminal_expired" => FinamOrderStatusClass::TerminalExpired,
            "needs_policy" => FinamOrderStatusClass::NeedsPolicy,
            "manual_or_degraded" => FinamOrderStatusClass::ManualOrDegraded,
            "blocking_unknown" => FinamOrderStatusClass::BlockingUnknown,
            other => panic!("unknown pinned status policy {other}"),
        }
    }

    #[test]
    fn every_pinned_finam_order_status_has_explicit_m3d1_policy() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/finam_spec/order_contract_enums_v2026_07_03.json"
        ))
        .expect("fixture json");
        let statuses = fixture["order_status"]
            .as_array()
            .expect("order_status array");
        let policy = fixture["order_status_policy"]
            .as_object()
            .expect("order_status_policy object");

        assert_eq!(statuses.len(), policy.len());
        for status in statuses {
            let status = status.as_str().expect("status string");
            let expected_policy = policy
                .get(status)
                .and_then(|value| value.as_str())
                .unwrap_or_else(|| panic!("missing policy for pinned FINAM status {status}"));
            let expected_class = expected_status_class_from_policy(expected_policy);
            assert_eq!(
                classify_finam_order_status(status),
                expected_class,
                "{status}"
            );

            if expected_class == FinamOrderStatusClass::BlockingUnknown {
                assert_eq!(
                    status, "ORDER_STATUS_UNSPECIFIED",
                    "only ORDER_STATUS_UNSPECIFIED may remain BlockingUnknown by reviewed policy"
                );
            }
        }
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
                "account_id": "ACC_TEST_0001",
                "client_order_id": "ABC123",
                "comment": "manual test",
                "legs": [],
                "limit_price": {"value": "1000.5"},
                "quantity": {"value": "1"},
                "side": "SIDE_BUY",
                "symbol": "TESTFUT@TEST",
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

        assert_eq!(mapped.account_id.as_str(), "ACC_TEST_0001");
        assert_eq!(mapped.instrument.symbol, "TESTFUT");
        assert_eq!(
            mapped.instrument.venue_symbol.as_deref(),
            Some("TESTFUT@TEST")
        );
        assert_eq!(mapped.side, OrderSide::Buy);
        assert_eq!(mapped.order_type, OrderType::Limit);
        assert_eq!(mapped.status, OrderStatus::Canceled);
        assert_eq!(mapped.qty, Decimal::ONE);
        assert_eq!(mapped.filled_qty, Decimal::ZERO);
        assert_eq!(mapped.limit_price, Some(Decimal::new(10005, 1)));
        assert_eq!(
            mapped
                .broker_client_order_id_fingerprint
                .as_ref()
                .map(|fp| fp.len),
            Some("ABC123".len())
        );
        assert!(mapped.comment.is_none());
        let comment_fingerprint = mapped
            .comment_fingerprint
            .as_ref()
            .expect("redacted comment fingerprint");
        assert_eq!(comment_fingerprint.len, "manual test".len());
        assert_eq!(comment_fingerprint.sha256.len(), 64);
    }

    #[test]
    fn long_broker_client_order_id_does_not_block_readonly_order_mapping() {
        let order: dto::OrderState = serde_json::from_value(serde_json::json!({
            "executed_quantity": {"value": "0"},
            "initial_quantity": {"value": "1"},
            "order": {
                "account_id": "ACC_TEST_0001",
                "client_order_id": "THIS-CLIENT-ORDER-ID-IS-TOO-LONG",
                "legs": [],
                "quantity": {"value": "1"},
                "side": "SIDE_BUY",
                "symbol": "TESTFUT@TEST",
                "type": "ORDER_TYPE_LIMIT"
            },
            "status": "ORDER_STATUS_CANCELED"
        }))
        .expect("order dto");

        let received_ts = parse_timestamp("test", "2026-06-29T09:10:01Z").expect("timestamp");
        let mapped = map_order_state(&order, received_ts).expect("mapped order");

        assert!(mapped.client_order_id.is_none());
        let fingerprint = mapped
            .broker_client_order_id_fingerprint
            .expect("broker-native id fingerprint");
        assert_eq!(fingerprint.len, "THIS-CLIENT-ORDER-ID-IS-TOO-LONG".len());
        assert_eq!(fingerprint.sha256.len(), 64);
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

        let mapped = map_bar("TESTFUT@TEST", &bar, 60).expect("mapped bar");

        assert_eq!(mapped.instrument.symbol, "TESTFUT");
        assert_eq!(mapped.source_kind, MarketDataSourceKind::HistoricalPoll);
        assert_eq!(mapped.timeframe_sec, 60);
        assert_eq!(mapped.close_ts, mapped.open_ts + Duration::seconds(60));
        assert!(mapped.is_final);
    }

    #[test]
    fn map_bar_rejects_zero_timeframe() {
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

        assert_eq!(
            map_bar("TESTFUT@TEST", &bar, 0).expect_err("zero timeframe"),
            FinamMapperError::InvalidTimeframe
        );
    }

    #[test]
    fn maps_empty_account_snapshot_cash() {
        let account: dto::AccountResponse = serde_json::from_value(serde_json::json!({
            "account_id": "ACC_TEST_0001",
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

        assert_eq!(snapshot.account_id.as_str(), "ACC_TEST_0001");
        assert!(snapshot.positions.is_empty());
        assert_eq!(snapshot.cash[0].currency, "RUB");
        assert_eq!(snapshot.cash[0].amount, Decimal::new(6000, 0));
    }

    #[test]
    fn maps_non_flat_account_position_into_snapshot() {
        let account: dto::AccountResponse = serde_json::from_value(serde_json::json!({
            "account_id": "ACC_TEST_0001",
            "cash": [
                {"currency_code": "RUB", "units": "1000", "nanos": 0}
            ],
            "positions": [
                {
                    "symbol": "TESTFUT@TEST",
                    "asset_type": "FUTURES",
                    "quantity": {"value": "2"},
                    "average_price": {"value": "5000.5"},
                    "unrealized_profit": {"value": "-12.5"}
                }
            ],
            "status": "ACCOUNT_ACTIVE",
            "type": "UNION"
        }))
        .expect("account dto");
        let received_ts = parse_timestamp("test", "2026-06-29T09:10:01Z").expect("timestamp");

        let snapshot = map_portfolio_snapshot(&account, received_ts).expect("snapshot");

        assert_eq!(snapshot.positions.len(), 1);
        let position = &snapshot.positions[0];
        assert_eq!(position.account_id.as_str(), "ACC_TEST_0001");
        assert_eq!(position.instrument.symbol, "TESTFUT");
        assert_eq!(position.instrument.market, Market::Futures);
        assert_eq!(position.qty, Decimal::new(2, 0));
        assert_eq!(position.avg_price, Some(Decimal::new(50005, 1)));
        assert_eq!(position.unrealized_pnl, Some(Decimal::new(-125, 1)));
    }

    #[test]
    fn maps_zero_quantity_account_position_as_flat_snapshot() {
        let account: dto::AccountResponse = serde_json::from_value(serde_json::json!({
            "account_id": "ACC_TEST_0001",
            "cash": [
                {"currency_code": "RUB", "units": "6000", "nanos": 0}
            ],
            "positions": [
                {
                    "symbol": "TESTFUT@TEST",
                    "asset_type": "FUTURES",
                    "quantity": {"value": "0"},
                    "average_price": {"value": "5000.5"},
                    "unrealized_profit": {"value": "0"}
                }
            ],
            "status": "ACCOUNT_ACTIVE",
            "type": "UNION"
        }))
        .expect("account dto");
        let received_ts = parse_timestamp("test", "2026-06-29T09:10:01Z").expect("timestamp");

        let snapshot = map_portfolio_snapshot(&account, received_ts).expect("snapshot");

        assert!(snapshot.positions.is_empty());
    }

    #[test]
    fn m4_2b_finam_fixtures_map_to_canonical_broker_truth_summary() {
        let account: dto::AccountResponse = serde_json::from_str(include_str!(
            "../../../fixtures/finam/equivalent_positions_snapshot_zero_qty.json"
        ))
        .expect("finam account fixture");
        let orders: dto::AccountOrdersResponse = serde_json::from_str(include_str!(
            "../../../fixtures/finam/equivalent_orders_active_terminal.json"
        ))
        .expect("finam orders fixture");
        let expected_position: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/canonical_truth_zero_qty_flat_summary.json"
        ))
        .expect("expected position summary");
        let expected_order: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/expected/canonical_truth_order_summary.json"
        ))
        .expect("expected order summary");
        let received_ts = parse_timestamp("test", "2026-07-04T15:00:00Z").expect("timestamp");

        let truth = map_finam_broker_truth_snapshot(&account, &orders, received_ts).expect("truth");
        let target = instrument_id_from_symbol("IMOEXF@RTSX", Some("FUTURES"));
        let summary = truth.summarize_for_instrument(&target);

        assert_eq!(truth.account_id.as_str(), "ACC_TEST_0001");
        assert_eq!(
            summary.target_open_positions_count,
            expected_position["target_open_positions_count"]
                .as_u64()
                .expect("target open positions") as usize
        );
        assert_eq!(
            summary.account_open_positions_count,
            expected_position["account_open_positions_count"]
                .as_u64()
                .expect("account open positions") as usize
        );
        assert_eq!(
            summary.target_active_orders_count,
            expected_order["target_active_orders_count"]
                .as_u64()
                .expect("target active orders") as usize
        );
        assert_eq!(
            summary.target_terminal_orders_count,
            expected_order["target_terminal_orders_count"]
                .as_u64()
                .expect("target terminal orders") as usize
        );
        assert_eq!(
            summary.account_active_orders_count,
            expected_order["account_active_orders_count"]
                .as_u64()
                .expect("account active orders") as usize
        );
        assert_eq!(
            summary.other_symbol_active_orders_count,
            expected_order["other_symbol_active_orders_count"]
                .as_u64()
                .expect("other symbol active orders") as usize
        );
    }

    #[test]
    fn m4_2b_finam_canonical_order_mapper_preserves_remaining_qty_truth() {
        let order: dto::OrderState = serde_json::from_value(serde_json::json!({
            "executed_quantity": {"value": "1"},
            "initial_quantity": {"value": "1"},
            "remaining_quantity": {"value": "0"},
            "order": {
                "account_id": "ACC_TEST_0001",
                "legs": [],
                "quantity": {"value": "1"},
                "side": "SIDE_BUY",
                "symbol": "IMOEXF@RTSX",
                "time_in_force": "TIME_IN_FORCE_DAY",
                "type": "ORDER_TYPE_MARKET"
            },
            "order_id": "BROKER-ORDER-FILLED",
            "status": "ORDER_STATUS_FILLED",
            "transact_at": "2026-07-04T15:00:00Z"
        }))
        .expect("order dto");
        let received_ts = parse_timestamp("test", "2026-07-04T15:00:01Z").expect("timestamp");

        let mapped =
            map_order_state_to_broker_order_snapshot(&order, received_ts).expect("broker order");

        assert_eq!(mapped.remaining_qty, Some(Decimal::ZERO));
        assert_eq!(mapped.time_in_force, Some(TimeInForce::Day));
        assert_eq!(
            mapped.lifecycle,
            broker_core::BrokerOrderLifecycle::Terminal
        );
        assert!(!mapped.is_active_for_lifecycle());
    }

    fn m4_2d_account() -> dto::AccountResponse {
        serde_json::from_value(serde_json::json!({
            "account_id": "ACC_TEST_0001",
            "cash": [
                {"currency_code": "RUB", "units": "6000", "nanos": 0}
            ],
            "equity": {"value": "6000"},
            "portfolio_mc": {
                "available_cash": {"value": "6000"},
                "initial_margin": {"value": "5000"},
                "maintenance_margin": {"value": "4000"}
            },
            "positions": [
                {
                    "symbol": "IMOEXF@RTSX",
                    "asset_type": "FUTURES",
                    "quantity": {"value": "0"},
                    "average_price": {"value": "2227.5"},
                    "unrealized_profit": {"value": "0"}
                }
            ],
            "status": "ACCOUNT_ACTIVE",
            "type": "UNION"
        }))
        .expect("account dto")
    }

    fn m4_2d_orders() -> dto::AccountOrdersResponse {
        serde_json::from_value(serde_json::json!({
            "orders": [
                {
                    "executed_quantity": {"value": "1"},
                    "initial_quantity": {"value": "1"},
                    "remaining_quantity": {"value": "0"},
                    "order": {
                        "account_id": "ACC_TEST_0001",
                        "legs": [],
                        "quantity": {"value": "1"},
                        "side": "SIDE_BUY",
                        "symbol": "IMOEXF@RTSX",
                        "time_in_force": "TIME_IN_FORCE_DAY",
                        "type": "ORDER_TYPE_MARKET"
                    },
                    "order_id": "BROKER-ENTRY",
                    "status": "ORDER_STATUS_FILLED",
                    "transact_at": "2026-07-04T14:57:17Z"
                }
            ]
        }))
        .expect("orders dto")
    }

    fn m4_2d_trades() -> dto::AccountTradesResponse {
        serde_json::from_value(serde_json::json!({
            "trades": [
                {
                    "trade_id": "TRADE-BUY",
                    "order_id": "BROKER-ENTRY",
                    "account_id": "ACC_TEST_0001",
                    "symbol": "IMOEXF@RTSX",
                    "side": "SIDE_BUY",
                    "quantity": {"value": "1"},
                    "price": {"value": "2227.5"},
                    "amount": {"value": "22275"},
                    "commission": {"currency_code": "RUB", "units": "1", "nanos": 0},
                    "timestamp": "2026-07-04T14:57:17Z"
                },
                {
                    "trade_id": "TRADE-SELL",
                    "order_id": "BROKER-EXIT",
                    "account_id": "ACC_TEST_0001",
                    "symbol": "IMOEXF@RTSX",
                    "side": "SIDE_SELL",
                    "quantity": {"value": "1"},
                    "price": {"value": "2227.0"},
                    "amount": {"value": "22270"},
                    "commission": {"currency_code": "RUB", "units": "1", "nanos": 0},
                    "timestamp": "2026-07-04T14:57:18Z"
                }
            ]
        }))
        .expect("trades dto")
    }

    fn m4_2d_asset() -> dto::AssetResponse {
        serde_json::from_value(serde_json::json!({
            "board": "RTSX",
            "decimals": 1,
            "future_details": {
                "contract_size": {"value": "1"},
                "expiration_date": "2026-09-17",
                "first_trade_date": "2026-06-01",
                "last_trade_date": "2026-09-17",
                "lot_size": {"value": "1"},
                "min_step": "0.5",
                "step_price": "5"
            },
            "id": "ASSET_IMOEXF_TEST",
            "lot_size": {"value": "1"},
            "mic": "RTSX",
            "min_step": "0.5",
            "name": "Synthetic IMOEX future",
            "quote_currency": "RUB",
            "ticker": "IMOEXF",
            "type": "ASSET_TYPE_FUTURES"
        }))
        .expect("asset dto")
    }

    fn m4_2d_params() -> dto::AssetParamsResponse {
        serde_json::from_value(serde_json::json!({
            "account_id": "ACC_TEST_0001",
            "is_tradable": true,
            "long_initial_margin": {"currency_code": "RUB", "units": "5000", "nanos": 0},
            "price_type": "PRICE_TYPE_PRICE",
            "short_initial_margin": {"currency_code": "RUB", "units": "5000", "nanos": 0},
            "symbol": "IMOEXF@RTSX",
            "tradeable": true
        }))
        .expect("params dto")
    }

    fn m4_2d_schedule() -> dto::AssetScheduleResponse {
        serde_json::from_value(serde_json::json!({
            "symbol": "IMOEXF@RTSX",
            "sessions": [
                {
                    "interval": {
                        "start_time": "2026-07-04T06:00:00Z",
                        "end_time": "2026-07-04T20:00:00Z"
                    },
                    "type": "SESSION_TYPE_MAIN"
                }
            ]
        }))
        .expect("schedule dto")
    }

    fn m4_2d_quote() -> dto::LastQuoteResponse {
        serde_json::from_value(serde_json::json!({
            "symbol": "IMOEXF@RTSX",
            "quote": {
                "ask": {"value": "2227.5"},
                "bid": {"value": "2227.0"},
                "last": {"value": "2227.5"},
                "symbol": "IMOEXF@RTSX",
                "timestamp": "2026-07-04T14:57:16Z"
            }
        }))
        .expect("quote dto")
    }

    #[test]
    fn m4_2d_enriched_broker_truth_maps_trades_instrument_spec_and_readiness() {
        let received_ts = parse_timestamp("test", "2026-07-04T14:57:18Z").expect("timestamp");
        let account = m4_2d_account();
        let orders = m4_2d_orders();
        let trades = m4_2d_trades();
        let asset = m4_2d_asset();
        let params = m4_2d_params();
        let schedule = m4_2d_schedule();
        let quote = m4_2d_quote();
        let instrument_artifacts = [FinamInstrumentSpecArtifacts {
            asset: &asset,
            params: &params,
            schedule: &schedule,
        }];

        let truth = map_finam_broker_truth_snapshot_with_readonly_artifacts(
            &account,
            &orders,
            Some(&trades),
            &instrument_artifacts,
            received_ts,
        )
        .expect("enriched broker truth");
        let target = instrument_id_from_symbol("IMOEXF@RTSX", Some("FUTURES"));
        let summary = truth.summarize_for_instrument(&target);
        let readiness = map_finam_broker_readiness_snapshot(
            &account,
            &orders,
            Some(&trades),
            Some(&quote),
            &truth.instruments,
            Some(&schedule),
            received_ts,
        )
        .expect("readiness");

        assert!(truth.target_is_flat(&target));
        assert_eq!(summary.target_open_positions_count, 0);
        assert_eq!(truth.trades.len(), 2);
        assert_eq!(truth.instruments.len(), 1);
        assert_eq!(
            truth.instruments[0].instrument.price_step,
            Decimal::new(5, 1)
        );
        assert_eq!(
            truth.instruments[0].instrument.step_value,
            Decimal::new(5, 0)
        );
        assert_eq!(truth.cash_by_currency("RUB"), Some(Decimal::new(6000, 0)));
        assert_eq!(readiness.market_session, BrokerMarketSessionState::Open);
        assert!(readiness.broker_truth_is_fresh(received_ts));
        assert_eq!(readiness.unknown_order_count, 0);
        assert!(readiness.cash_margin_present);
        assert!(readiness.instrument_spec_validated);
    }

    #[test]
    fn m4_2d_same_ticker_different_mic_is_not_same_instrument() {
        let rtsx = instrument_id_from_symbol("IMOEXF@RTSX", Some("FUTURES"));
        let misx = instrument_id_from_symbol("IMOEXF@MISX", Some("FUTURES"));

        assert_eq!(rtsx.symbol, misx.symbol);
        assert_ne!(rtsx.venue_symbol, misx.venue_symbol);
        assert!(!instrument_identity_matches(&rtsx, &misx));
    }

    #[test]
    fn m4_2d_round_trip_trades_explain_flat_position_delta() {
        let received_ts = parse_timestamp("test", "2026-07-04T14:57:18Z").expect("timestamp");
        let trades = m4_2d_trades();
        let mapped = trades
            .trades
            .iter()
            .map(|trade| {
                map_account_trade_to_broker_trade_snapshot("ACC_TEST_0001", trade, received_ts)
            })
            .collect::<Result<Vec<_>, FinamMapperError>>()
            .expect("mapped trades");
        let net_qty: Decimal = mapped
            .iter()
            .map(|trade| match trade.side {
                OrderSide::Buy => trade.qty,
                OrderSide::Sell => -trade.qty,
            })
            .sum();

        assert_eq!(net_qty, Decimal::ZERO);
    }
}
