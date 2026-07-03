//! Finam Trade API adapter surface.
//!
//! The first implementation slice is deliberately read-only. Order placement is
//! added only after auth, account, reference data, market data, and
//! reconciliation behavior are characterized.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub mod dto;
pub mod instrument_registry;
pub mod mapper;
pub mod order_request;
pub use dto::*;
pub use instrument_registry::*;
pub use mapper::*;
pub use order_request::*;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_JWT_TTL: Duration = Duration::from_secs(15 * 60);
const DEFAULT_JWT_RENEW_BEFORE: Duration = Duration::from_secs(2 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinamConfig {
    pub rest_base_url: String,
    pub grpc_endpoint: String,
    pub websocket_endpoint: String,
    pub source_app_id: Option<String>,
    pub prefer_http2: bool,
    pub request_timeout_ms: u64,
}

impl Default for FinamConfig {
    fn default() -> Self {
        Self {
            rest_base_url: "https://api.finam.ru".to_string(),
            grpc_endpoint: "https://api.finam.ru:443".to_string(),
            websocket_endpoint: "wss://api.finam.ru/ws".to_string(),
            source_app_id: None,
            prefer_http2: true,
            request_timeout_ms: DEFAULT_REQUEST_TIMEOUT.as_millis() as u64,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactedJsonKeyKind {
    SchemaField,
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedJsonKey {
    pub key_kind: RedactedJsonKeyKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub len: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

pub fn redact_json_key_for_diagnostics(key: &str) -> RedactedJsonKey {
    if is_safe_schema_field_name(key) {
        RedactedJsonKey {
            key_kind: RedactedJsonKeyKind::SchemaField,
            name: Some(key.to_string()),
            len: key.len(),
            sha256: None,
        }
    } else {
        RedactedJsonKey {
            key_kind: RedactedJsonKeyKind::Dynamic,
            name: None,
            len: key.len(),
            sha256: Some(sha256_hex(key.as_bytes())),
        }
    }
}

fn is_safe_schema_field_name(key: &str) -> bool {
    if key.is_empty() || key.len() > 64 {
        return false;
    }

    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

#[derive(Debug, Clone)]
pub struct FinamRestClient {
    http: reqwest::Client,
    config: FinamConfig,
}

impl FinamRestClient {
    pub fn new(config: FinamConfig) -> Self {
        Self::try_new(config).expect("reqwest client configuration must be valid")
    }

    pub fn try_new(config: FinamConfig) -> Result<Self, FinamError> {
        let mut builder = reqwest::Client::builder()
            .https_only(true)
            .timeout(Duration::from_millis(config.request_timeout_ms.max(1)));
        if config.prefer_http2 {
            builder = builder.http2_adaptive_window(true);
        }
        Ok(Self {
            http: builder.build()?,
            config,
        })
    }

    pub async fn auth(&self, secret: &SecretToken) -> Result<AuthResponse, FinamError> {
        let mut request = AuthRequest {
            secret: secret.as_str(),
            source_app_id: None,
        };
        if let Some(source_app_id) = self.config.source_app_id.as_deref() {
            request.source_app_id = Some(source_app_id);
        }
        let url = self.rest_url(&["v1", "sessions"])?;
        let response = self.http.post(url).json(&request).send().await?;
        decode_response(response).await
    }

    pub async fn token_details(
        &self,
        token: &AccessToken,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "sessions", "details"])?;
        let response = self
            .http
            .post(url)
            .json(&TokenDetailsRequest {
                token: token.as_str(),
            })
            .send()
            .await?;
        decode_response(response).await
    }

    pub async fn token_details_typed(
        &self,
        token: &AccessToken,
    ) -> Result<dto::TokenDetailsResponse, FinamError> {
        let url = self.rest_url(&["v1", "sessions", "details"])?;
        let response = self
            .http
            .post(url)
            .json(&TokenDetailsRequest {
                token: token.as_str(),
            })
            .send()
            .await?;
        decode_response(response).await
    }

    pub async fn account(
        &self,
        token: &AccessToken,
        account_id: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "accounts", account_id])?;
        self.get_json(token, url).await
    }

    pub async fn account_typed(
        &self,
        token: &AccessToken,
        account_id: &str,
    ) -> Result<dto::AccountResponse, FinamError> {
        let url = self.rest_url(&["v1", "accounts", account_id])?;
        self.get_typed(token, url).await
    }

    pub async fn account_trades(
        &self,
        token: &AccessToken,
        account_id: &str,
        query: HistoryQuery<'_>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "accounts", account_id, "trades"])?;
        query.append_to_url(&mut url);
        self.get_json(token, url).await
    }

    pub async fn account_trades_typed(
        &self,
        token: &AccessToken,
        account_id: &str,
        query: HistoryQuery<'_>,
    ) -> Result<dto::AccountTradesResponse, FinamError> {
        let mut url = self.rest_url(&["v1", "accounts", account_id, "trades"])?;
        query.append_to_url(&mut url);
        self.get_typed(token, url).await
    }

    pub async fn account_transactions(
        &self,
        token: &AccessToken,
        account_id: &str,
        query: HistoryQuery<'_>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "accounts", account_id, "transactions"])?;
        query.append_to_url(&mut url);
        self.get_json(token, url).await
    }

    pub async fn account_transactions_typed(
        &self,
        token: &AccessToken,
        account_id: &str,
        query: HistoryQuery<'_>,
    ) -> Result<dto::AccountTransactionsResponse, FinamError> {
        let mut url = self.rest_url(&["v1", "accounts", account_id, "transactions"])?;
        query.append_to_url(&mut url);
        self.get_typed(token, url).await
    }

    pub async fn account_orders(
        &self,
        token: &AccessToken,
        account_id: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "accounts", account_id, "orders"])?;
        self.get_json(token, url).await
    }

    pub async fn account_orders_typed(
        &self,
        token: &AccessToken,
        account_id: &str,
    ) -> Result<dto::AccountOrdersResponse, FinamError> {
        let url = self.rest_url(&["v1", "accounts", account_id, "orders"])?;
        self.get_typed(token, url).await
    }

    pub async fn account_order(
        &self,
        token: &AccessToken,
        account_id: &str,
        order_id: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "accounts", account_id, "orders", order_id])?;
        self.get_json(token, url).await
    }

    pub async fn account_order_typed(
        &self,
        token: &AccessToken,
        account_id: &str,
        order_id: &str,
    ) -> Result<dto::OrderState, FinamError> {
        let url = self.rest_url(&["v1", "accounts", account_id, "orders", order_id])?;
        self.get_typed(token, url).await
    }

    pub async fn assets(&self, token: &AccessToken) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "assets"])?;
        self.get_json(token, url).await
    }

    pub async fn assets_typed(
        &self,
        token: &AccessToken,
    ) -> Result<dto::AssetsResponse, FinamError> {
        let url = self.rest_url(&["v1", "assets"])?;
        self.get_typed(token, url).await
    }

    pub async fn all_assets(
        &self,
        token: &AccessToken,
        query: AllAssetsQuery<'_>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "assets", "all"])?;
        query.append_to_url(&mut url);
        self.get_json(token, url).await
    }

    pub async fn all_assets_typed(
        &self,
        token: &AccessToken,
        query: AllAssetsQuery<'_>,
    ) -> Result<dto::AllAssetsResponse, FinamError> {
        let mut url = self.rest_url(&["v1", "assets", "all"])?;
        query.append_to_url(&mut url);
        self.get_typed(token, url).await
    }

    pub async fn clock(&self, token: &AccessToken) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "assets", "clock"])?;
        self.get_json(token, url).await
    }

    pub async fn exchanges(&self, token: &AccessToken) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "exchanges"])?;
        self.get_json(token, url).await
    }

    pub async fn exchanges_typed(
        &self,
        token: &AccessToken,
    ) -> Result<dto::ExchangesResponse, FinamError> {
        let url = self.rest_url(&["v1", "exchanges"])?;
        self.get_typed(token, url).await
    }

    pub async fn asset(
        &self,
        token: &AccessToken,
        symbol: &str,
        account_id: Option<&str>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "assets", symbol])?;
        append_optional_query(&mut url, "account_id", account_id);
        self.get_json(token, url).await
    }

    pub async fn asset_typed(
        &self,
        token: &AccessToken,
        symbol: &str,
        account_id: Option<&str>,
    ) -> Result<dto::AssetResponse, FinamError> {
        let mut url = self.rest_url(&["v1", "assets", symbol])?;
        append_optional_query(&mut url, "account_id", account_id);
        self.get_typed(token, url).await
    }

    pub async fn asset_params(
        &self,
        token: &AccessToken,
        symbol: &str,
        account_id: Option<&str>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "assets", symbol, "params"])?;
        append_optional_query(&mut url, "account_id", account_id);
        self.get_json(token, url).await
    }

    pub async fn asset_params_typed(
        &self,
        token: &AccessToken,
        symbol: &str,
        account_id: Option<&str>,
    ) -> Result<dto::AssetParamsResponse, FinamError> {
        let mut url = self.rest_url(&["v1", "assets", symbol, "params"])?;
        append_optional_query(&mut url, "account_id", account_id);
        self.get_typed(token, url).await
    }

    pub async fn asset_schedule(
        &self,
        token: &AccessToken,
        symbol: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "assets", symbol, "schedule"])?;
        self.get_json(token, url).await
    }

    pub async fn asset_schedule_typed(
        &self,
        token: &AccessToken,
        symbol: &str,
    ) -> Result<dto::AssetScheduleResponse, FinamError> {
        let url = self.rest_url(&["v1", "assets", symbol, "schedule"])?;
        self.get_typed(token, url).await
    }

    pub async fn bars(
        &self,
        token: &AccessToken,
        symbol: &str,
        query: BarsQuery<'_>,
    ) -> Result<serde_json::Value, FinamError> {
        let mut url = self.rest_url(&["v1", "instruments", symbol, "bars"])?;
        query.append_to_url(&mut url);
        self.get_json(token, url).await
    }

    pub async fn bars_typed(
        &self,
        token: &AccessToken,
        symbol: &str,
        query: BarsQuery<'_>,
    ) -> Result<dto::BarsResponse, FinamError> {
        let mut url = self.rest_url(&["v1", "instruments", symbol, "bars"])?;
        query.append_to_url(&mut url);
        self.get_typed(token, url).await
    }

    pub async fn last_quote(
        &self,
        token: &AccessToken,
        symbol: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "instruments", symbol, "quotes", "latest"])?;
        self.get_json(token, url).await
    }

    pub async fn last_quote_typed(
        &self,
        token: &AccessToken,
        symbol: &str,
    ) -> Result<dto::LastQuoteResponse, FinamError> {
        let url = self.rest_url(&["v1", "instruments", symbol, "quotes", "latest"])?;
        self.get_typed(token, url).await
    }

    pub async fn latest_trades(
        &self,
        token: &AccessToken,
        symbol: &str,
    ) -> Result<serde_json::Value, FinamError> {
        let url = self.rest_url(&["v1", "instruments", symbol, "trades", "latest"])?;
        self.get_json(token, url).await
    }

    pub async fn latest_trades_typed(
        &self,
        token: &AccessToken,
        symbol: &str,
    ) -> Result<dto::LatestTradesResponse, FinamError> {
        let url = self.rest_url(&["v1", "instruments", symbol, "trades", "latest"])?;
        self.get_typed(token, url).await
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
        token: &AccessToken,
        url: reqwest::Url,
    ) -> Result<serde_json::Value, FinamError> {
        if token.is_empty() {
            return Err(FinamError::MissingToken);
        }
        let response = self
            .http
            .get(url)
            .bearer_auth(token.as_str())
            .send()
            .await?;
        decode_response(response).await
    }

    async fn get_typed<T: for<'de> Deserialize<'de>>(
        &self,
        token: &AccessToken,
        url: reqwest::Url,
    ) -> Result<T, FinamError> {
        if token.is_empty() {
            return Err(FinamError::MissingToken);
        }
        let response = self
            .http
            .get(url)
            .bearer_auth(token.as_str())
            .send()
            .await?;
        decode_response(response).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinamAuthPolicy {
    pub token_ttl: Duration,
    pub renew_before: Duration,
}

impl Default for FinamAuthPolicy {
    fn default() -> Self {
        Self {
            token_ttl: DEFAULT_JWT_TTL,
            renew_before: DEFAULT_JWT_RENEW_BEFORE,
        }
    }
}

#[derive(Clone)]
pub struct AccessTokenLease {
    token: AccessToken,
    issued_at: Instant,
    refresh_after: Instant,
    expires_at: Instant,
}

impl AccessTokenLease {
    pub fn new(token: AccessToken, issued_at: Instant, policy: FinamAuthPolicy) -> Self {
        let expires_at = issued_at + policy.token_ttl;
        let candidate_refresh_after = expires_at
            .checked_sub(policy.renew_before)
            .unwrap_or(issued_at);
        let refresh_after = if candidate_refresh_after < issued_at {
            issued_at
        } else {
            candidate_refresh_after
        };
        Self {
            token,
            issued_at,
            refresh_after,
            expires_at,
        }
    }

    pub fn token(&self) -> &AccessToken {
        &self.token
    }

    pub fn issued_at(&self) -> Instant {
        self.issued_at
    }

    pub fn refresh_after(&self) -> Instant {
        self.refresh_after
    }

    pub fn expires_at(&self) -> Instant {
        self.expires_at
    }

    pub fn should_refresh(&self, now: Instant) -> bool {
        now >= self.refresh_after
    }
}

impl std::fmt::Debug for AccessTokenLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccessTokenLease")
            .field("token_present", &!self.token.is_empty())
            .field("token_len", &self.token.len())
            .field("issued_at", &self.issued_at)
            .field("refresh_after", &self.refresh_after)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

pub struct FinamAuthManager {
    client: FinamRestClient,
    secret: SecretToken,
    policy: FinamAuthPolicy,
    cached_lease: Mutex<Option<AccessTokenLease>>,
}

impl FinamAuthManager {
    pub fn new(client: FinamRestClient, secret: SecretToken) -> Self {
        Self::with_policy(client, secret, FinamAuthPolicy::default())
    }

    pub fn with_policy(
        client: FinamRestClient,
        secret: SecretToken,
        policy: FinamAuthPolicy,
    ) -> Self {
        Self {
            client,
            secret,
            policy,
            cached_lease: Mutex::new(None),
        }
    }

    pub async fn access_token(&self) -> Result<AccessToken, FinamError> {
        let now = Instant::now();
        if let Some(token) = {
            let cache = self.lock_cached_lease()?;
            cache
                .as_ref()
                .filter(|lease| !lease.should_refresh(now))
                .map(|lease| lease.token().clone())
        } {
            return Ok(token);
        }

        let auth = self.client.auth(&self.secret).await?;
        let lease = AccessTokenLease::new(auth.token, now, self.policy);
        let token = lease.token().clone();
        *self.lock_cached_lease()? = Some(lease);
        Ok(token)
    }

    pub fn clear_cache(&self) -> Result<(), FinamError> {
        *self.lock_cached_lease()? = None;
        Ok(())
    }

    fn lock_cached_lease(&self) -> Result<MutexGuard<'_, Option<AccessTokenLease>>, FinamError> {
        self.cached_lease
            .lock()
            .map_err(|_| FinamError::InternalState {
                message: "auth cache mutex poisoned",
            })
    }
}

impl std::fmt::Debug for FinamAuthManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FinamAuthManager")
            .field("client", &self.client)
            .field("secret", &self.secret)
            .field("policy", &self.policy)
            .field(
                "cached_lease_present",
                &self
                    .cached_lease
                    .lock()
                    .map(|cache| cache.is_some())
                    .unwrap_or(false),
            )
            .finish()
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
    pub token: AccessToken,
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

/// FINAM JWT/access token.
///
/// This type intentionally does not implement `Serialize`, so accidental JSON
/// export of a live JWT fails at compile time:
///
/// ```compile_fail
/// let token = broker_finam::AccessToken::new("jwt");
/// let _ = serde_json::to_string(&token).unwrap();
/// ```
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct AccessToken(String);

impl AccessToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl std::ops::Deref for AccessToken {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for AccessToken {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Debug for AccessToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccessToken")
            .field("present", &!self.is_empty())
            .field("len", &self.len())
            .finish()
    }
}

impl std::fmt::Display for AccessToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "<redacted access token len={}>", self.len())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretToken(String);

impl SecretToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl std::fmt::Debug for SecretToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretToken")
            .field("present", &!self.is_empty())
            .field("len", &self.len())
            .finish()
    }
}

impl std::fmt::Display for SecretToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "<redacted secret token len={}>", self.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinamErrorKind {
    TransportTimeout,
    TransportConnect,
    TransportHttp,
    InvalidConfiguration,
    MissingToken,
    ApiBadRequest,
    ApiAuthentication,
    ApiAuthorization,
    ApiNotFound,
    ApiConflict,
    ApiRateLimited,
    ApiTimeout,
    ApiClient,
    ApiServer,
    ApiUnexpectedStatus,
    Decode,
    InternalState,
}

#[derive(Debug, thiserror::Error)]
pub enum FinamError {
    #[error("finam http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("finam REST base URL is invalid ({base_url}): {error}")]
    InvalidBaseUrl { base_url: String, error: String },
    #[error("finam JWT/access token is missing")]
    MissingToken,
    #[error("finam internal state error: {message}")]
    InternalState { message: &'static str },
    #[error("finam response decode error: status={status:?}")]
    Decode { status: Option<u16> },
    #[error(
        "finam api returned HTTP {status}: body_kind={body_kind:?}, body_keys={body_keys:?}, body_len={body_len}, body_sha256={body_sha256}"
    )]
    Api {
        status: u16,
        body_kind: Option<String>,
        body_keys: Vec<RedactedJsonKey>,
        body_len: usize,
        body_sha256: String,
    },
}

impl FinamError {
    pub fn kind(&self) -> FinamErrorKind {
        match self {
            FinamError::Http(error) if error.is_timeout() => FinamErrorKind::TransportTimeout,
            FinamError::Http(error) if error.is_connect() => FinamErrorKind::TransportConnect,
            FinamError::Http(_) => FinamErrorKind::TransportHttp,
            FinamError::InvalidBaseUrl { .. } => FinamErrorKind::InvalidConfiguration,
            FinamError::MissingToken => FinamErrorKind::MissingToken,
            FinamError::InternalState { .. } => FinamErrorKind::InternalState,
            FinamError::Decode { .. } => FinamErrorKind::Decode,
            FinamError::Api { status, .. } => match *status {
                400 => FinamErrorKind::ApiBadRequest,
                401 => FinamErrorKind::ApiAuthentication,
                403 => FinamErrorKind::ApiAuthorization,
                404 => FinamErrorKind::ApiNotFound,
                408 => FinamErrorKind::ApiTimeout,
                409 => FinamErrorKind::ApiConflict,
                429 => FinamErrorKind::ApiRateLimited,
                status if (400..=499).contains(&status) => FinamErrorKind::ApiClient,
                status if (500..=599).contains(&status) => FinamErrorKind::ApiServer,
                _ => FinamErrorKind::ApiUnexpectedStatus,
            },
        }
    }

    pub fn to_redacted_string(&self) -> String {
        match self {
            FinamError::Http(error) => {
                format!(
                    "finam http error: kind={:?}, is_timeout={}, is_connect={}, status={:?}",
                    self.kind(),
                    error.is_timeout(),
                    error.is_connect(),
                    error.status().map(|status| status.as_u16())
                )
            }
            FinamError::Decode { .. } => self.to_string(),
            _ => self.to_string(),
        }
    }
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
        let redacted = redact_api_body(&body);
        return Err(FinamError::Api {
            status: status.as_u16(),
            body_kind: redacted.kind,
            body_keys: redacted.keys,
            body_len: redacted.len,
            body_sha256: redacted.sha256,
        });
    }
    response
        .json::<T>()
        .await
        .map_err(|error| FinamError::Decode {
            status: error.status().map(|status| status.as_u16()),
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RedactedApiBody {
    kind: Option<String>,
    keys: Vec<RedactedJsonKey>,
    len: usize,
    sha256: String,
}

fn redact_api_body(body: &str) -> RedactedApiBody {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    let (kind, keys) = match parsed.as_ref() {
        Some(serde_json::Value::Object(object)) => (
            Some("object".to_string()),
            object
                .keys()
                .map(|key| redact_json_key_for_diagnostics(key))
                .collect::<Vec<_>>(),
        ),
        Some(serde_json::Value::Array(_)) => (Some("array".to_string()), Vec::new()),
        Some(serde_json::Value::String(_)) => (Some("string".to_string()), Vec::new()),
        Some(serde_json::Value::Number(_)) => (Some("number".to_string()), Vec::new()),
        Some(serde_json::Value::Bool(_)) => (Some("bool".to_string()), Vec::new()),
        Some(serde_json::Value::Null) => (Some("null".to_string()), Vec::new()),
        None => (None, Vec::new()),
    };

    RedactedApiBody {
        kind,
        keys,
        len: body.len(),
        sha256: sha256_hex(body.as_bytes()),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        write!(&mut output, "{byte:02x}").expect("writing to string cannot fail");
    }
    output
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
            .rest_url(&["v1", "assets", "SYNTH@TEST", "params"])
            .expect("valid url");

        assert_eq!(
            url.as_str(),
            "https://api.finam.ru/v1/assets/SYNTH@TEST/params"
        );
    }

    #[test]
    fn auth_response_debug_is_redacted() {
        let response = AuthResponse {
            token: AccessToken::new("secret-jwt-value"),
        };

        let debug = format!("{response:?}");

        assert!(debug.contains("token_present"));
        assert!(debug.contains("token_len"));
        assert!(!debug.contains("secret-jwt-value"));
    }

    #[test]
    fn access_token_debug_and_display_are_redacted() {
        let token = AccessToken::new("secret-jwt-value");

        assert!(!format!("{token:?}").contains("secret-jwt-value"));
        assert!(!format!("{token}").contains("secret-jwt-value"));
    }

    #[test]
    fn access_token_lease_refreshes_before_expiration() {
        let issued_at = Instant::now();
        let policy = FinamAuthPolicy {
            token_ttl: Duration::from_secs(900),
            renew_before: Duration::from_secs(120),
        };
        let lease = AccessTokenLease::new(AccessToken::new("secret-jwt-value"), issued_at, policy);

        assert_eq!(lease.issued_at(), issued_at);
        assert_eq!(lease.expires_at(), issued_at + Duration::from_secs(900));
        assert_eq!(lease.refresh_after(), issued_at + Duration::from_secs(780));
        assert!(!lease.should_refresh(issued_at + Duration::from_secs(779)));
        assert!(lease.should_refresh(issued_at + Duration::from_secs(780)));
        assert!(!format!("{lease:?}").contains("secret-jwt-value"));
    }

    #[test]
    fn access_token_lease_refresh_floor_is_issue_time() {
        let issued_at = Instant::now();
        let policy = FinamAuthPolicy {
            token_ttl: Duration::from_secs(30),
            renew_before: Duration::from_secs(120),
        };
        let lease = AccessTokenLease::new(AccessToken::new("secret-jwt-value"), issued_at, policy);

        assert_eq!(lease.refresh_after(), issued_at);
        assert!(lease.should_refresh(issued_at));
    }

    #[test]
    fn secret_token_debug_and_display_are_redacted() {
        let token = SecretToken::new("secret-token-value");

        assert!(!format!("{token:?}").contains("secret-token-value"));
        assert!(!format!("{token}").contains("secret-token-value"));
    }

    #[test]
    fn json_key_redaction_keeps_schema_names_only() {
        let schema_key = redact_json_key_for_diagnostics("account_id");
        let dynamic_account_key = redact_json_key_for_diagnostics("ACC_DYNAMIC_TEST_001");
        let dynamic_order_key = redact_json_key_for_diagnostics("ORDER_DYNAMIC_TEST_001");
        let dynamic_symbol_key = redact_json_key_for_diagnostics("SYNTH@TEST");

        assert_eq!(schema_key.key_kind, RedactedJsonKeyKind::SchemaField);
        assert_eq!(schema_key.name.as_deref(), Some("account_id"));
        assert!(schema_key.sha256.is_none());

        for redacted in [dynamic_account_key, dynamic_order_key, dynamic_symbol_key] {
            assert_eq!(redacted.key_kind, RedactedJsonKeyKind::Dynamic);
            assert!(redacted.name.is_none());
            assert!(redacted.sha256.is_some());
        }
    }

    #[test]
    fn api_body_redaction_keeps_shape_and_hash_but_not_raw_values() {
        let body = r#"{"message":"account 123 rejected","code":"NOPE"}"#;

        let redacted = redact_api_body(body);
        let error = FinamError::Api {
            status: 400,
            body_kind: redacted.kind,
            body_keys: redacted.keys,
            body_len: redacted.len,
            body_sha256: redacted.sha256,
        };
        let display = error.to_string();

        assert_eq!(error.kind(), FinamErrorKind::ApiBadRequest);
        assert!(display.contains("HTTP 400"));
        assert!(display.contains("message"));
        assert!(display.contains("code"));
        assert!(display.contains("body_sha256="));
        assert!(!display.contains("account 123"));
        assert!(!display.contains("NOPE"));
    }

    #[test]
    fn api_body_redaction_does_not_leak_dynamic_keys() {
        let body =
            r#"{"ACC_DYNAMIC_TEST_001":{"message":"account rejected"},"message":"bad request"}"#;

        let redacted = redact_api_body(body);
        let rendered = serde_json::to_string(&redacted.keys).expect("keys serialize");

        assert!(rendered.contains("schema_field"));
        assert!(rendered.contains("dynamic"));
        assert!(rendered.contains("sha256"));
        assert!(rendered.contains("message"));
        assert!(!rendered.contains("ACC_DYNAMIC_TEST_001"));
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

    #[test]
    fn api_error_kind_is_based_on_status_class() {
        let error = |status| FinamError::Api {
            status,
            body_kind: None,
            body_keys: Vec::new(),
            body_len: 0,
            body_sha256: sha256_hex(b""),
        };

        assert_eq!(error(401).kind(), FinamErrorKind::ApiAuthentication);
        assert_eq!(error(403).kind(), FinamErrorKind::ApiAuthorization);
        assert_eq!(error(404).kind(), FinamErrorKind::ApiNotFound);
        assert_eq!(error(429).kind(), FinamErrorKind::ApiRateLimited);
        assert_eq!(error(422).kind(), FinamErrorKind::ApiClient);
        assert_eq!(error(503).kind(), FinamErrorKind::ApiServer);
    }
}
