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

#[derive(Debug, Clone)]
pub struct FinamRestClient {
    http: reqwest::Client,
    config: FinamConfig,
}

impl FinamRestClient {
    pub fn new(config: FinamConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }

    pub async fn auth(&self, secret: &str) -> Result<AuthResponse, FinamError> {
        let mut request = AuthRequest {
            secret,
            source_app_id: None,
        };
        if let Some(source_app_id) = self.config.source_app_id.as_deref() {
            request.source_app_id = Some(source_app_id);
        }
        let url = format!("{}/v1/sessions", self.config.rest_base_url);
        let response = self.http.post(url).json(&request).send().await?;
        decode_response(response).await
    }

    pub async fn token_details(&self, token: &str) -> Result<serde_json::Value, FinamError> {
        let url = format!("{}/v1/sessions/details", self.config.rest_base_url);
        let response = self
            .http
            .post(url)
            .json(&TokenDetailsRequest { token })
            .send()
            .await?;
        decode_response(response).await
    }
}

#[derive(Debug, Serialize)]
struct AuthRequest<'a> {
    secret: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_app_id: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct TokenDetailsRequest<'a> {
    token: &'a str,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthResponse {
    pub token: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FinamError {
    #[error("finam http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("finam api returned HTTP {status}: {body}")]
    Api { status: u16, body: String },
}

async fn decode_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T, FinamError> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(FinamError::Api {
            status: status.as_u16(),
            body,
        });
    }
    Ok(response.json::<T>().await?)
}
