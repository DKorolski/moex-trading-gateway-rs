use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenDetailsResponse {
    #[serde(default)]
    pub account_ids: Vec<String>,
    pub created_at: Option<String>,
    pub expires_at: Option<String>,
    #[serde(default)]
    pub md_permissions: Vec<MarketDataPermission>,
    pub readonly: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketDataPermission {
    pub delay_minutes: Option<i64>,
    pub mic: Option<String>,
    pub quote_level: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecimalValue {
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DecimalLike {
    Value(DecimalValue),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoneyAmount {
    pub currency_code: String,
    pub units: String,
    pub nanos: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountResponse {
    pub account_id: String,
    #[serde(default)]
    pub cash: Vec<MoneyAmount>,
    #[serde(default)]
    pub equity: Option<DecimalValue>,
    pub first_non_trade_date: Option<String>,
    pub open_account_date: Option<String>,
    #[serde(default)]
    pub portfolio_mc: Option<PortfolioMarginCall>,
    #[serde(default)]
    pub positions: Vec<AccountPosition>,
    pub status: Option<String>,
    #[serde(rename = "type")]
    pub account_type: Option<String>,
    #[serde(default)]
    pub unrealized_profit: Option<DecimalValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioMarginCall {
    pub available_cash: Option<DecimalValue>,
    pub initial_margin: Option<DecimalValue>,
    pub maintenance_margin: Option<DecimalValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountPosition {
    pub symbol: Option<String>,
    pub quantity: Option<DecimalValue>,
    pub balance: Option<DecimalValue>,
    pub average_price: Option<DecimalValue>,
    pub current_price: Option<DecimalValue>,
    pub unrealized_profit: Option<DecimalValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountOrdersResponse {
    #[serde(default)]
    pub orders: Vec<OrderState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderState {
    pub exec_id: Option<String>,
    pub executed_quantity: Option<DecimalValue>,
    pub initial_quantity: Option<DecimalValue>,
    pub order: OrderRequestSnapshot,
    pub order_id: Option<String>,
    pub remaining_quantity: Option<DecimalValue>,
    pub status: String,
    pub transact_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderRequestSnapshot {
    pub account_id: String,
    pub client_order_id: Option<String>,
    pub comment: Option<String>,
    #[serde(default)]
    pub legs: Vec<serde_json::Value>,
    pub limit_price: Option<DecimalValue>,
    pub quantity: Option<DecimalValue>,
    pub side: String,
    pub stop_condition: Option<String>,
    pub symbol: String,
    pub time_in_force: Option<String>,
    #[serde(rename = "type")]
    pub order_type: String,
    pub valid_before: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountTradesResponse {
    #[serde(default)]
    pub trades: Vec<AccountTrade>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountTrade {
    pub trade_id: Option<String>,
    pub order_id: Option<String>,
    pub client_order_id: Option<String>,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
    pub side: Option<String>,
    pub price: Option<DecimalValue>,
    pub quantity: Option<DecimalValue>,
    pub size: Option<DecimalValue>,
    pub amount: Option<DecimalValue>,
    pub commission: Option<MoneyAmount>,
    pub timestamp: Option<String>,
    pub transact_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountTransactionsResponse {
    #[serde(default)]
    pub transactions: Vec<AccountTransaction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountTransaction {
    pub category: Option<String>,
    pub change: Option<MoneyAmount>,
    pub id: Option<String>,
    pub symbol: Option<String>,
    pub timestamp: Option<String>,
    pub transaction_category: Option<String>,
    pub transaction_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetsResponse {
    #[serde(default)]
    pub assets: Vec<AssetSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AllAssetsResponse {
    #[serde(default)]
    pub assets: Vec<AssetSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetSummary {
    pub id: Option<String>,
    pub is_archived: Option<bool>,
    pub isin: Option<String>,
    pub mic: Option<String>,
    pub name: Option<String>,
    pub symbol: String,
    pub ticker: Option<String>,
    #[serde(rename = "type")]
    pub asset_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetResponse {
    pub board: Option<String>,
    pub decimals: Option<u32>,
    pub future_details: Option<FutureDetails>,
    pub id: Option<String>,
    pub isin: Option<String>,
    pub lot_size: Option<DecimalValue>,
    pub mic: Option<String>,
    pub min_step: Option<DecimalLike>,
    pub name: Option<String>,
    pub quote_currency: Option<String>,
    pub ticker: Option<String>,
    #[serde(rename = "type")]
    pub asset_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FutureDetails {
    pub contract_size: Option<DecimalValue>,
    pub expiration_date: Option<String>,
    pub first_trade_date: Option<String>,
    pub last_trade_date: Option<String>,
    pub lot_size: Option<DecimalValue>,
    pub min_step: Option<DecimalLike>,
    pub step_price: Option<DecimalLike>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetParamsResponse {
    pub account_id: Option<String>,
    pub is_tradable: Option<bool>,
    pub long_collateral: Option<MoneyAmount>,
    pub long_initial_margin: Option<MoneyAmount>,
    pub long_risk_rate: Option<DecimalValue>,
    pub longable: Option<AvailabilityFlag>,
    pub price_type: Option<String>,
    pub short_collateral: Option<MoneyAmount>,
    pub short_initial_margin: Option<MoneyAmount>,
    pub short_risk_rate: Option<DecimalValue>,
    pub shortable: Option<AvailabilityFlag>,
    pub symbol: String,
    pub tradeable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvailabilityFlag {
    pub halted_days: Option<i64>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetScheduleResponse {
    #[serde(default)]
    pub sessions: Vec<ScheduleSession>,
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleSession {
    pub interval: Option<TimeInterval>,
    #[serde(rename = "type")]
    pub session_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeInterval {
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarsResponse {
    #[serde(default)]
    pub bars: Vec<Bar>,
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub close: DecimalValue,
    pub high: DecimalValue,
    pub low: DecimalValue,
    pub open: DecimalValue,
    pub timestamp: String,
    pub volume: DecimalValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastQuoteResponse {
    pub quote: Quote,
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    pub ask: Option<DecimalValue>,
    pub ask_size: Option<DecimalValue>,
    pub bid: Option<DecimalValue>,
    pub bid_size: Option<DecimalValue>,
    pub change: Option<DecimalValue>,
    pub close: Option<DecimalValue>,
    pub high: Option<DecimalValue>,
    pub last: Option<DecimalValue>,
    pub last_size: Option<DecimalValue>,
    pub low: Option<DecimalValue>,
    pub open: Option<DecimalValue>,
    pub open_interest: Option<DecimalValue>,
    pub option: Option<QuoteOption>,
    pub symbol: Option<String>,
    pub timestamp: Option<String>,
    pub turnover: Option<DecimalValue>,
    pub volume: Option<DecimalValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuoteOption {
    pub open_interest: Option<DecimalValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LatestTradesResponse {
    pub symbol: String,
    #[serde(default)]
    pub trades: Vec<LatestTrade>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LatestTrade {
    pub mpid: Option<String>,
    pub open_interest: Option<DecimalValue>,
    pub price: DecimalValue,
    pub side: Option<String>,
    pub size: DecimalValue,
    pub timestamp: String,
    pub trade_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExchangesResponse {
    #[serde(default)]
    pub exchanges: Vec<ExchangeSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExchangeSummary {
    pub mic: Option<String>,
    pub name: Option<String>,
}
