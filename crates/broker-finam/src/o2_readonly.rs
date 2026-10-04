//! Stage 8B-P1-f O2 dedicated GET-only FINAM transport.
//!
//! The transport accepts a closed route enum rather than an HTTP method or an
//! arbitrary URL. Redirects and system proxies are disabled at construction.

use std::collections::BTreeSet;
use std::time::Duration;

use reqwest::redirect::Policy;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::AccessToken;

mod observed;

pub const STAGE8B_P1F_O2_FINAM_BASE_URL: &str = "https://api.finam.ru";
pub const STAGE8B_P1F_O2_VENUE_SYMBOL: &str = "IMOEXF@RTSX";
pub const STAGE8B_P1F_O2_BARS_TIMEFRAME: &str = "TIME_FRAME_M1";
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage8bP1fO2GetRouteV1 {
    Account,
    AccountOrders,
    AssetParams,
    AssetSchedule,
    Bars {
        start_time: String,
        end_time: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1fO2GetRouteKindV1 {
    Account,
    AccountOrders,
    AssetParams,
    AssetSchedule,
    Bars,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1fO2GetObservationV1 {
    pub route: Stage8bP1fO2GetRouteKindV1,
    pub request_sha256: String,
    pub response_sha256: String,
    pub exact_response_bytes: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1fO2GetErrorV1 {
    #[error("O2 GET guard rejected method, route, query or identity")]
    GuardRejected,
    #[error("O2 GET access token is missing")]
    MissingToken,
    #[error("O2 GET transport failed")]
    Transport,
    #[error("O2 GET response status was not successful")]
    HttpStatus,
    #[error("O2 GET response exceeded the accepted bound")]
    ResponseTooLarge,
    #[error("O2 GET response did not match its typed schema")]
    Decode,
}

#[derive(Clone)]
pub struct Stage8bP1fO2GetOnlyClientV1 {
    http: reqwest::Client,
    account_id: String,
    venue_symbol: String,
}

impl std::fmt::Debug for Stage8bP1fO2GetOnlyClientV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stage8bP1fO2GetOnlyClientV1")
            .field("base_url", &STAGE8B_P1F_O2_FINAM_BASE_URL)
            .field("account_id_len", &self.account_id.len())
            .field("venue_symbol", &self.venue_symbol)
            .field("redirects_allowed", &false)
            .field("system_proxy_allowed", &false)
            .finish()
    }
}

impl Stage8bP1fO2GetOnlyClientV1 {
    pub fn new(account_id: impl Into<String>) -> Result<Self, Stage8bP1fO2GetErrorV1> {
        let account_id = account_id.into();
        if !canonical_identity(&account_id) {
            return Err(Stage8bP1fO2GetErrorV1::GuardRejected);
        }
        let http = reqwest::Client::builder()
            .https_only(true)
            .redirect(Policy::none())
            .no_proxy()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| Stage8bP1fO2GetErrorV1::Transport)?;
        Ok(Self {
            http,
            account_id,
            venue_symbol: STAGE8B_P1F_O2_VENUE_SYMBOL.to_string(),
        })
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn render_and_validate(
        &self,
        route: &Stage8bP1fO2GetRouteV1,
    ) -> Result<reqwest::Url, Stage8bP1fO2GetErrorV1> {
        let mut url = reqwest::Url::parse(STAGE8B_P1F_O2_FINAM_BASE_URL)
            .map_err(|_| Stage8bP1fO2GetErrorV1::GuardRejected)?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| Stage8bP1fO2GetErrorV1::GuardRejected)?;
            match route {
                Stage8bP1fO2GetRouteV1::Account => {
                    segments.extend(["v1", "accounts", &self.account_id]);
                }
                Stage8bP1fO2GetRouteV1::AccountOrders => {
                    segments.extend(["v1", "accounts", &self.account_id, "orders"]);
                }
                Stage8bP1fO2GetRouteV1::AssetParams => {
                    segments.extend(["v1", "assets", &self.venue_symbol, "params"]);
                }
                Stage8bP1fO2GetRouteV1::AssetSchedule => {
                    segments.extend(["v1", "assets", &self.venue_symbol, "schedule"]);
                }
                Stage8bP1fO2GetRouteV1::Bars { .. } => {
                    segments.extend(["v1", "instruments", &self.venue_symbol, "bars"]);
                }
            }
        }
        match route {
            Stage8bP1fO2GetRouteV1::AssetParams => {
                url.query_pairs_mut()
                    .append_pair("account_id", &self.account_id);
            }
            Stage8bP1fO2GetRouteV1::Bars {
                start_time,
                end_time,
            } => {
                if start_time.is_empty() || end_time.is_empty() || start_time >= end_time {
                    return Err(Stage8bP1fO2GetErrorV1::GuardRejected);
                }
                url.query_pairs_mut()
                    .append_pair("timeframe", STAGE8B_P1F_O2_BARS_TIMEFRAME)
                    .append_pair("interval.start_time", start_time)
                    .append_pair("interval.end_time", end_time);
            }
            _ => {}
        }
        let query = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        validate_stage8b_p1f_o2_get_request_v1(
            "GET",
            url.path(),
            &query,
            &self.account_id,
            &self.venue_symbol,
        )?;
        Ok(url)
    }

    pub async fn fetch(
        &self,
        token: &AccessToken,
        route: Stage8bP1fO2GetRouteV1,
    ) -> Result<Stage8bP1fO2GetObservationV1, Stage8bP1fO2GetErrorV1> {
        if token.is_empty() {
            return Err(Stage8bP1fO2GetErrorV1::MissingToken);
        }
        let url = self.render_and_validate(&route)?;
        let request_sha256 = request_sha256(&url);
        let response = self
            .http
            .get(url)
            .bearer_auth(token.as_str())
            .send()
            .await
            .map_err(|_| Stage8bP1fO2GetErrorV1::Transport)?;
        if !response.status().is_success() {
            return Err(Stage8bP1fO2GetErrorV1::HttpStatus);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(Stage8bP1fO2GetErrorV1::ResponseTooLarge);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| Stage8bP1fO2GetErrorV1::Transport)?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(Stage8bP1fO2GetErrorV1::ResponseTooLarge);
        }
        Ok(Stage8bP1fO2GetObservationV1 {
            route: route_kind(&route),
            request_sha256,
            response_sha256: lower_hex(&Sha256::digest(&bytes)),
            exact_response_bytes: bytes.to_vec(),
        })
    }

    pub async fn fetch_typed<T: DeserializeOwned>(
        &self,
        token: &AccessToken,
        route: Stage8bP1fO2GetRouteV1,
    ) -> Result<(T, Stage8bP1fO2GetObservationV1), Stage8bP1fO2GetErrorV1> {
        let observation = self.fetch(token, route).await?;
        let parsed = serde_json::from_slice(&observation.exact_response_bytes)
            .map_err(|_| Stage8bP1fO2GetErrorV1::Decode)?;
        Ok((parsed, observation))
    }
}

fn request_sha256(url: &reqwest::Url) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"GET\0");
    hasher.update(url.as_str().as_bytes());
    lower_hex(&hasher.finalize())
}

pub fn validate_stage8b_p1f_o2_get_request_v1(
    method: &str,
    encoded_path: &str,
    query: &[(String, String)],
    expected_account_id: &str,
    expected_venue_symbol: &str,
) -> Result<Stage8bP1fO2GetRouteKindV1, Stage8bP1fO2GetErrorV1> {
    if method != "GET"
        || !canonical_identity(expected_account_id)
        || expected_venue_symbol != STAGE8B_P1F_O2_VENUE_SYMBOL
    {
        return Err(Stage8bP1fO2GetErrorV1::GuardRejected);
    }
    let account_path = encoded_path_for(&["v1", "accounts", expected_account_id])?;
    let orders_path = encoded_path_for(&["v1", "accounts", expected_account_id, "orders"])?;
    let params_path = encoded_path_for(&["v1", "assets", expected_venue_symbol, "params"])?;
    let schedule_path = encoded_path_for(&["v1", "assets", expected_venue_symbol, "schedule"])?;
    let bars_path = encoded_path_for(&["v1", "instruments", expected_venue_symbol, "bars"])?;
    let unique_keys = query
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<BTreeSet<_>>();
    if unique_keys.len() != query.len() {
        return Err(Stage8bP1fO2GetErrorV1::GuardRejected);
    }
    match encoded_path {
        path if path == account_path && query.is_empty() => Ok(Stage8bP1fO2GetRouteKindV1::Account),
        path if path == orders_path && query.is_empty() => {
            Ok(Stage8bP1fO2GetRouteKindV1::AccountOrders)
        }
        path if path == params_path
            && query == [("account_id".to_string(), expected_account_id.to_string())] =>
        {
            Ok(Stage8bP1fO2GetRouteKindV1::AssetParams)
        }
        path if path == schedule_path && query.is_empty() => {
            Ok(Stage8bP1fO2GetRouteKindV1::AssetSchedule)
        }
        path if path == bars_path && valid_bars_query(query) => {
            Ok(Stage8bP1fO2GetRouteKindV1::Bars)
        }
        _ => Err(Stage8bP1fO2GetErrorV1::GuardRejected),
    }
}

fn valid_bars_query(query: &[(String, String)]) -> bool {
    query.len() == 3
        && query[0]
            == (
                "timeframe".to_string(),
                STAGE8B_P1F_O2_BARS_TIMEFRAME.to_string(),
            )
        && query[1].0 == "interval.start_time"
        && query[2].0 == "interval.end_time"
        && !query[1].1.is_empty()
        && query[1].1 < query[2].1
}

fn route_kind(route: &Stage8bP1fO2GetRouteV1) -> Stage8bP1fO2GetRouteKindV1 {
    match route {
        Stage8bP1fO2GetRouteV1::Account => Stage8bP1fO2GetRouteKindV1::Account,
        Stage8bP1fO2GetRouteV1::AccountOrders => Stage8bP1fO2GetRouteKindV1::AccountOrders,
        Stage8bP1fO2GetRouteV1::AssetParams => Stage8bP1fO2GetRouteKindV1::AssetParams,
        Stage8bP1fO2GetRouteV1::AssetSchedule => Stage8bP1fO2GetRouteKindV1::AssetSchedule,
        Stage8bP1fO2GetRouteV1::Bars { .. } => Stage8bP1fO2GetRouteKindV1::Bars,
    }
}

fn encoded_path_for(segments: &[&str]) -> Result<String, Stage8bP1fO2GetErrorV1> {
    let mut url = reqwest::Url::parse(STAGE8B_P1F_O2_FINAM_BASE_URL)
        .map_err(|_| Stage8bP1fO2GetErrorV1::GuardRejected)?;
    url.path_segments_mut()
        .map_err(|_| Stage8bP1fO2GetErrorV1::GuardRejected)?
        .extend(segments.iter().copied());
    Ok(url.path().to_string())
}

fn canonical_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_allowlist_accepts_all_five_routes() {
        let client = Stage8bP1fO2GetOnlyClientV1::new("ACC_TEST_0001").unwrap();
        for route in [
            Stage8bP1fO2GetRouteV1::Account,
            Stage8bP1fO2GetRouteV1::AccountOrders,
            Stage8bP1fO2GetRouteV1::AssetParams,
            Stage8bP1fO2GetRouteV1::AssetSchedule,
            Stage8bP1fO2GetRouteV1::Bars {
                start_time: "2026-09-27T06:00:00Z".into(),
                end_time: "2026-09-27T07:00:00Z".into(),
            },
        ] {
            client.render_and_validate(&route).unwrap();
        }
    }

    #[test]
    fn write_foreign_identity_extra_query_and_unlisted_routes_fail_closed() {
        let account = "ACC_TEST_0001";
        let symbol = STAGE8B_P1F_O2_VENUE_SYMBOL;
        let orders = encoded_path_for(&["v1", "accounts", account, "orders"]).unwrap();
        assert!(
            validate_stage8b_p1f_o2_get_request_v1("POST", &orders, &[], account, symbol).is_err()
        );
        let foreign = encoded_path_for(&["v1", "accounts", "ACC_OTHER", "orders"]).unwrap();
        assert!(
            validate_stage8b_p1f_o2_get_request_v1("GET", &foreign, &[], account, symbol).is_err()
        );
        assert!(validate_stage8b_p1f_o2_get_request_v1(
            "GET",
            &orders,
            &[("limit".into(), "1".into())],
            account,
            symbol
        )
        .is_err());
        let arbitrary = encoded_path_for(&["v1", "accounts", account, "trades"]).unwrap();
        assert!(
            validate_stage8b_p1f_o2_get_request_v1("GET", &arbitrary, &[], account, symbol)
                .is_err()
        );
    }

    #[test]
    fn duplicate_and_incomplete_bars_queries_fail_closed() {
        let path =
            encoded_path_for(&["v1", "instruments", STAGE8B_P1F_O2_VENUE_SYMBOL, "bars"]).unwrap();
        let duplicate = vec![
            ("timeframe".into(), STAGE8B_P1F_O2_BARS_TIMEFRAME.into()),
            ("timeframe".into(), STAGE8B_P1F_O2_BARS_TIMEFRAME.into()),
            ("interval.start_time".into(), "2026-09-27T06:00:00Z".into()),
            ("interval.end_time".into(), "2026-09-27T07:00:00Z".into()),
        ];
        assert!(validate_stage8b_p1f_o2_get_request_v1(
            "GET",
            &path,
            &duplicate,
            "ACC_TEST_0001",
            STAGE8B_P1F_O2_VENUE_SYMBOL
        )
        .is_err());
        assert!(validate_stage8b_p1f_o2_get_request_v1(
            "GET",
            &path,
            &[("timeframe".into(), STAGE8B_P1F_O2_BARS_TIMEFRAME.into())],
            "ACC_TEST_0001",
            STAGE8B_P1F_O2_VENUE_SYMBOL
        )
        .is_err());
    }
}
