//! Finam Trade API adapter surface.
//!
//! The first implementation slice is deliberately read-only. Order placement is
//! added only after auth, account, reference data, market data, and
//! reconciliation behavior are characterized.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamConfig {
    pub rest_base_url: String,
    pub grpc_endpoint: String,
    pub websocket_endpoint: String,
    pub source_app_id: Option<String>,
    pub prefer_http2: bool,
}

impl Default for FinamConfig {
    fn default() -> Self {
        Self {
            rest_base_url: "https://api.finam.ru".to_string(),
            grpc_endpoint: "https://api.finam.ru:443".to_string(),
            websocket_endpoint: "wss://api.finam.ru/ws".to_string(),
            source_app_id: None,
            prefer_http2: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamApiCapabilities {
    pub rest_auth: bool,
    pub rest_account_trades: bool,
    pub rest_orders: bool,
    pub rest_sltp_orders: bool,
    pub grpc_order_trade_stream: bool,
    pub grpc_jwt_renewal_stream: bool,
    pub websocket_market_data: bool,
}

impl Default for FinamApiCapabilities {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayEnabledFeatures {
    pub enable_readonly_probe: bool,
    pub enable_live_orders: bool,
    pub enable_stop_orders: bool,
    pub enable_sltp_orders: bool,
    pub enable_brackets: bool,
}

impl Default for GatewayEnabledFeatures {
    fn default() -> Self {
        Self {
            enable_readonly_probe: true,
            enable_live_orders: false,
            enable_stop_orders: false,
            enable_sltp_orders: false,
            enable_brackets: false,
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
        let mut builder = reqwest::Client::builder().https_only(true);
        if config.prefer_http2 {
            builder = builder.http2_adaptive_window(true);
        }
        Self {
            http: builder
                .build()
                .expect("reqwest client configuration must be valid"),
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

    pub async fn account(
        &self,
        token: &str,
        account_id: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "accounts", account_id])?;
        self.get_json(token, url).await
    }

    pub async fn account_trades(
        &self,
        token: &str,
        account_id: &str,
        query: HistoryQuery<'_>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "accounts", account_id, "trades"])?;
        query.append_to_url(&mut url);
        self.get_json(token, url).await
    }

    pub async fn account_transactions(
        &self,
        token: &str,
        account_id: &str,
        query: HistoryQuery<'_>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "accounts", account_id, "transactions"])?;
        query.append_to_url(&mut url);
        self.get_json(token, url).await
    }

    pub async fn account_orders(
        &self,
        token: &str,
        account_id: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "accounts", account_id, "orders"])?;
        self.get_json(token, url).await
    }

    pub async fn account_order(
        &self,
        token: &str,
        account_id: &str,
        order_id: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "accounts", account_id, "orders", order_id])?;
        self.get_json(token, url).await
    }

    pub async fn assets(&self, token: &str) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "assets"])?;
        self.get_json(token, url).await
    }

    pub async fn all_assets(
        &self,
        token: &str,
        query: AllAssetsQuery<'_>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "assets", "all"])?;
        query.append_to_url(&mut url);
        self.get_json(token, url).await
    }

    pub async fn clock(&self, token: &str) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "assets", "clock"])?;
        self.get_json(token, url).await
    }

    pub async fn exchanges(&self, token: &str) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "assets", "exchanges"])?;
        self.get_json(token, url).await
    }

    pub async fn asset(
        &self,
        token: &str,
        symbol: &str,
        account_id: Option<&str>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "assets", symbol])?;
        append_optional_query(&mut url, "account_id", account_id);
        self.get_json(token, url).await
    }

    pub async fn asset_params(
        &self,
        token: &str,
        symbol: &str,
        account_id: Option<&str>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "assets", symbol, "params"])?;
        append_optional_query(&mut url, "account_id", account_id);
        self.get_json(token, url).await
    }

    pub async fn asset_schedule(
        &self,
        token: &str,
        symbol: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "assets", symbol, "schedule"])?;
        self.get_json(token, url).await
    }

    pub async fn bars(
        &self,
        token: &str,
        symbol: &str,
        query: BarsQuery<'_>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "instruments", symbol, "bars"])?;
        query.append_to_url(&mut url);
        self.get_json(token, url).await
    }

    pub async fn last_quote(
        &self,
        token: &str,
        symbol: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "instruments", symbol, "quotes", "latest"])?;
        self.get_json(token, url).await
    }

    pub async fn latest_trades(
        &self,
        token: &str,
        symbol: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "instruments", symbol, "trades", "latest"])?;
        self.get_json(token, url).await
    }

    fn rest_url(&self, segments: &[&str]) -> Result<reqwest::Url, FinamError> {
        let base = format!("{}/", self.config.rest_base_url.trim_end_matches('/'));
        let mut url = reqwest::Url::parse(&base).map_err(|error| FinamError::InvalidBaseUrl {
            base_url: self.config.rest_base_url.clone(),
            error: error.to_string(),
        })?;
        {
            let mut path_segments =
                url.path_segments_mut()
                    .map_err(|_| FinamError::InvalidBaseUrl {
                        base_url: self.config.rest_base_url.clone(),
                        error: "base URL cannot be a base for path segments".to_string(),
                    })?;
            path_segments.clear();
            path_segments.extend(segments.iter().copied());
        }
        Ok(url)
    }

    async fn get_json(
        &self,
        token: &str,
        url: reqwest::Url,
    ) -> Result<serde_json::Value, FinamError> {
        if token.is_empty() {
            return Err(FinamError::MissingToken);
        }
        let response = self.http.get(url).bearer_auth(token).send().await?;
        decode_response(response).await
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryQuery<'a> {
    pub limit: Option<u32>,
    pub start_time: Option<&'a str>,
    pub end_time: Option<&'a str>,
}

impl HistoryQuery<'_> {
    fn append_to_url(self, url: &mut reqwest::Url) {
        append_optional_u32_query(url, "limit", self.limit);
        append_optional_query(url, "interval.start_time", self.start_time);
        append_optional_query(url, "interval.end_time", self.end_time);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AllAssetsQuery<'a> {
    pub cursor: Option<&'a str>,
    pub only_active: Option<bool>,
    pub only_disabled: Option<bool>,
}

impl AllAssetsQuery<'_> {
    fn append_to_url(self, url: &mut reqwest::Url) {
        append_optional_query(url, "cursor", self.cursor);
        append_optional_bool_query(url, "only_active", self.only_active);
        append_optional_bool_query(url, "only_disabled", self.only_disabled);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarsQuery<'a> {
    pub timeframe: &'a str,
    pub start_time: Option<&'a str>,
    pub end_time: Option<&'a str>,
}

impl<'a> BarsQuery<'a> {
    pub fn new(timeframe: &'a str) -> Self {
        Self {
            timeframe,
            start_time: None,
            end_time: None,
        }
    }

    fn append_to_url(self, url: &mut reqwest::Url) {
        url.query_pairs_mut()
            .append_pair("timeframe", self.timeframe);
        append_optional_query(url, "interval.start_time", self.start_time);
        append_optional_query(url, "interval.end_time", self.end_time);
    }
}

#[derive(Serialize)]
struct AuthRequest<'a> {
    secret: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_app_id: Option<&'a str>,
}

#[derive(Serialize)]
struct TokenDetailsRequest<'a> {
    token: &'a str,
}

#[derive(Clone, Deserialize)]
pub struct AuthResponse {
    pub token: String,
}

impl std::fmt::Debug for AuthResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthResponse")
            .field("token_present", &!self.token.is_empty())
            .field("token_len", &self.token.len())
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FinamError {
    #[error("finam http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("finam REST base URL is invalid ({base_url}): {error}")]
    InvalidBaseUrl { base_url: String, error: String },
    #[error("finam JWT/access token is missing")]
    MissingToken,
    #[error("finam api returned HTTP {status}: {body}")]
    Api { status: u16, body: String },
}

fn append_optional_query(url: &mut reqwest::Url, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        url.query_pairs_mut().append_pair(key, value);
    }
}

fn append_optional_u32_query(url: &mut reqwest::Url, key: &str, value: Option<u32>) {
    if let Some(value) = value {
        url.query_pairs_mut().append_pair(key, &value.to_string());
    }
}

fn append_optional_bool_query(url: &mut reqwest::Url, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        url.query_pairs_mut()
            .append_pair(key, if value { "true" } else { "false" });
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_url_encodes_path_segments() {
        let client = FinamRestClient::new(FinamConfig {
            rest_base_url: "https://api.finam.ru".to_string(),
            ..FinamConfig::default()
        });

        let url = client
            .rest_url(&["v1", "assets", "SBER@MISX", "params"])
            .expect("valid url");

        assert_eq!(
            url.as_str(),
            "https://api.finam.ru/v1/assets/SBER@MISX/params"
        );
    }

    #[test]
    fn auth_response_debug_is_redacted() {
        let response = AuthResponse {
            token: "secret-jwt-value".to_string(),
        };

        let debug = format!("{response:?}");

        assert!(debug.contains("token_present"));
        assert!(debug.contains("token_len"));
        assert!(!debug.contains("secret-jwt-value"));
    }

    #[test]
    fn history_query_uses_finam_interval_keys() {
        let mut url = reqwest::Url::parse("https://api.finam.ru/v1/accounts/A/trades").unwrap();
        HistoryQuery {
            limit: Some(1000),
            start_time: Some("2026-06-01T00:00:00Z"),
            end_time: Some("2026-06-27T23:59:59Z"),
        }
        .append_to_url(&mut url);

        assert!(url.as_str().contains("limit=1000"));
        assert!(url.as_str().contains("interval.start_time="));
        assert!(url.as_str().contains("interval.end_time="));
    }
}
