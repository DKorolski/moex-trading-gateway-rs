//! Finam Trade API adapter surface.
//!
//! M0 intentionally exposes configuration and capability declarations only.
//! Network clients and order placement will be added after read-only API
//! characterization.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamConfig {
    pub rest_base_url: String,
    pub grpc_endpoint: String,
    pub websocket_endpoint: String,
    pub source_app_id: Option<String>,
}

impl Default for FinamConfig {
    fn default() -> Self {
        Self {
            rest_base_url: "https://api.finam.ru".to_string(),
            grpc_endpoint: "https://api.finam.ru:443".to_string(),
            websocket_endpoint: "wss://api.finam.ru/ws".to_string(),
            source_app_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamCapabilities {
    pub rest_auth: bool,
    pub rest_account_trades: bool,
    pub rest_orders: bool,
    pub rest_sltp_orders: bool,
    pub grpc_order_trade_stream: bool,
    pub grpc_jwt_renewal_stream: bool,
    pub websocket_market_data: bool,
}

impl Default for FinamCapabilities {
    fn default() -> Self {
        Self {
            rest_auth: true,
            rest_account_trades: true,
            rest_orders: true,
            rest_sltp_orders: true,
            grpc_order_trade_stream: true,
            grpc_jwt_renewal_stream: true,
            websocket_market_data: true,
        }
    }
}
