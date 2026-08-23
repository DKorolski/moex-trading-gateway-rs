//! Stage 8B-IT single adapter qualification surface.
//!
//! The adapter can be exercised only with private request parts that consumed
//! the accepted Stage 8B exact permit.  During IT its only constructible
//! endpoint authority is a loopback controlled server.  The production FINAM
//! policy is frozen and testable, but no production endpoint authority or
//! broker-effect entry point exists in this stage.

#![allow(
    dead_code,
    reason = "Stage 8B-IT adapter is reachable only from crate-private qualification tests before P"
)]

use super::classify_stage8b_transport_observation_with_stage8a3;
use super::stage8b_permit_capsule::{Stage8bApprovedRequestParts, Stage8bPrivateRequestSpec};
use crate::{Stage8a3ClassifiedObservation, Stage8a3EndpointContext, Stage8a3LocalHttpObservation};
use reqwest::{redirect::Policy, Url};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::time::Duration;
use zeroize::Zeroizing;

const FINAM_PRODUCTION_SCHEME: &str = "https";
const FINAM_PRODUCTION_HOST: &str = "api.finam.ru";
const PLACE_ROUTE_TEMPLATE: &str = "/v1/accounts/{account_id}/orders";
const CANCEL_ROUTE_TEMPLATE: &str = "/v1/accounts/{account_id}/orders/{order_id}";
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Stage8bItAdapterMethod {
    Post,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct Stage8bItAdapterDiagnostic {
    pub(super) method: Stage8bItAdapterMethod,
    pub(super) route_template: &'static str,
    pub(super) controlled_loopback: bool,
    pub(super) tls_required_for_production: bool,
    pub(super) production_host: &'static str,
    pub(super) redirects_disabled: bool,
    pub(super) proxy_disabled: bool,
    pub(super) automatic_retry_disabled: bool,
    pub(super) request_body_present: bool,
    pub(super) request_body_len: usize,
    pub(super) request_body_sha256: Option<String>,
    pub(super) response_status: Option<u16>,
    pub(super) response_body_len: usize,
    pub(super) possible_write: bool,
    pub(super) transport_attempts: u8,
}

pub(super) struct Stage8bItClassifiedObservation {
    pub(super) classified: Stage8a3ClassifiedObservation,
    pub(super) diagnostic: Stage8bItAdapterDiagnostic,
}

struct Stage8bItRawObservation {
    context: Stage8a3EndpointContext,
    observation: Stage8a3LocalHttpObservation,
    diagnostic: Stage8bItAdapterDiagnostic,
}

pub(super) struct Stage8bItQualificationEndpoint {
    base_url: Url,
}

impl Stage8bItQualificationEndpoint {
    #[cfg(test)]
    pub(super) fn controlled_loopback(raw: &str) -> Result<Self, Stage8bItAdapterError> {
        let url = Url::parse(raw).map_err(|_| Stage8bItAdapterError::InvalidEndpoint)?;
        let host = url
            .host_str()
            .ok_or(Stage8bItAdapterError::InvalidEndpoint)?;
        let ip = host
            .parse::<std::net::IpAddr>()
            .map_err(|_| Stage8bItAdapterError::InvalidEndpoint)?;
        if url.scheme() != "http"
            || !ip.is_loopback()
            || url.port().is_none()
            || url.username() != ""
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !matches!(url.path(), "" | "/")
        {
            return Err(Stage8bItAdapterError::InvalidEndpoint);
        }
        Ok(Self { base_url: url })
    }
}

pub(super) struct Stage8bItQualificationToken(Zeroizing<String>);

impl Stage8bItQualificationToken {
    #[cfg(test)]
    pub(super) fn controlled(value: &str) -> Result<Self, Stage8bItAdapterError> {
        if value.len() < 16 || value.bytes().any(|byte| byte.is_ascii_whitespace()) {
            return Err(Stage8bItAdapterError::InvalidQualificationToken);
        }
        Ok(Self(Zeroizing::new(value.to_string())))
    }
}

pub(super) struct Stage8bItAdapter {
    http: reqwest::Client,
}

impl Stage8bItAdapter {
    pub(super) fn qualified() -> Result<Self, Stage8bItAdapterError> {
        let http = reqwest::Client::builder()
            .retry(reqwest::retry::never())
            .redirect(Policy::none())
            .no_proxy()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .pool_max_idle_per_host(0)
            .build()
            .map_err(|_| Stage8bItAdapterError::ClientBuild)?;
        Ok(Self { http })
    }

    /// One controlled transport attempt.  Consuming `self`, the qualification
    /// endpoint, token and private request parts prevents reuse or retry.
    pub(super) async fn qualify_once(
        self,
        parts: Stage8bApprovedRequestParts,
        endpoint: Stage8bItQualificationEndpoint,
        token: Stage8bItQualificationToken,
    ) -> Stage8bItClassifiedObservation {
        let (request, _diagnostic, _permit_binding_sha256) = parts.into_adapter_payload();
        let (request, context, request_diagnostic) = match request {
            Stage8bPrivateRequestSpec::Place { spec, context } => {
                let body = serde_json::to_vec(&spec.body).ok();
                let url = exact_url(&endpoint.base_url, &spec.rest_path_segments());
                let request_diagnostic = base_diagnostic(
                    Stage8bItAdapterMethod::Post,
                    PLACE_ROUTE_TEMPLATE,
                    body.as_deref(),
                );
                let request = match url {
                    Ok(url) => self
                        .http
                        .post(url)
                        .bearer_auth(token.0.as_str())
                        .json(&spec.body),
                    Err(error) => {
                        return classify_raw_observation(failed_before_write(
                            context,
                            request_diagnostic,
                            error,
                        ));
                    }
                };
                (request, context, request_diagnostic)
            }
            Stage8bPrivateRequestSpec::Cancel { spec, context } => {
                let url = exact_url(&endpoint.base_url, &spec.rest_path_segments());
                let request_diagnostic =
                    base_diagnostic(Stage8bItAdapterMethod::Delete, CANCEL_ROUTE_TEMPLATE, None);
                let request = match url {
                    Ok(url) => self.http.delete(url).bearer_auth(token.0.as_str()),
                    Err(error) => {
                        return classify_raw_observation(failed_before_write(
                            context,
                            request_diagnostic,
                            error,
                        ));
                    }
                };
                (request, context, request_diagnostic)
            }
        };
        let result = request.send().await;
        classify_raw_observation(observe_response(context, request_diagnostic, result).await)
    }
}

fn exact_url(base: &Url, segments: &[String]) -> Result<Url, Stage8bItAdapterError> {
    let mut url = base.clone();
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| Stage8bItAdapterError::InvalidEndpoint)?;
        path.clear();
        for segment in segments {
            if segment.is_empty() || segment == "." || segment == ".." || segment.contains('/') {
                return Err(Stage8bItAdapterError::InvalidRouteSegment);
            }
            path.push(segment);
        }
    }
    Ok(url)
}

fn base_diagnostic(
    method: Stage8bItAdapterMethod,
    route_template: &'static str,
    body: Option<&[u8]>,
) -> Stage8bItAdapterDiagnostic {
    Stage8bItAdapterDiagnostic {
        method,
        route_template,
        controlled_loopback: true,
        tls_required_for_production: true,
        production_host: FINAM_PRODUCTION_HOST,
        redirects_disabled: true,
        proxy_disabled: true,
        automatic_retry_disabled: true,
        request_body_present: body.is_some(),
        request_body_len: body.map_or(0, <[u8]>::len),
        request_body_sha256: body.map(sha256_hex),
        response_status: None,
        response_body_len: 0,
        possible_write: false,
        transport_attempts: 0,
    }
}

fn failed_before_write(
    context: Stage8a3EndpointContext,
    diagnostic: Stage8bItAdapterDiagnostic,
    _error: Stage8bItAdapterError,
) -> Stage8bItRawObservation {
    Stage8bItRawObservation {
        context,
        observation: Stage8a3LocalHttpObservation::disconnected(),
        diagnostic,
    }
}

async fn observe_response(
    context: Stage8a3EndpointContext,
    mut diagnostic: Stage8bItAdapterDiagnostic,
    result: Result<reqwest::Response, reqwest::Error>,
) -> Stage8bItRawObservation {
    diagnostic.transport_attempts = 1;
    diagnostic.possible_write = true;
    let observation = match result {
        Err(error) if error.is_timeout() => Stage8a3LocalHttpObservation::timeout(),
        Err(_) => Stage8a3LocalHttpObservation::disconnected(),
        Ok(mut response) => {
            let status = response.status().as_u16();
            diagnostic.response_status = Some(status);
            let mut body = Vec::new();
            loop {
                match response.chunk().await {
                    Ok(Some(chunk))
                        if body.len().saturating_add(chunk.len()) <= MAX_RESPONSE_BYTES =>
                    {
                        body.extend_from_slice(&chunk);
                    }
                    Ok(Some(_)) | Err(_) => {
                        break Stage8a3LocalHttpObservation::body_read_failed(Some(status));
                    }
                    Ok(None) => {
                        diagnostic.response_body_len = body.len();
                        break Stage8a3LocalHttpObservation::response(status, body);
                    }
                }
            }
        }
    };
    Stage8bItRawObservation {
        context,
        observation,
        diagnostic,
    }
}

fn classify_raw_observation(raw: Stage8bItRawObservation) -> Stage8bItClassifiedObservation {
    Stage8bItClassifiedObservation {
        classified: classify_stage8b_transport_observation_with_stage8a3(
            raw.context,
            raw.observation,
        ),
        diagnostic: raw.diagnostic,
    }
}

fn production_policy_accepts(url: &Url) -> bool {
    url.scheme() == FINAM_PRODUCTION_SCHEME
        && url.host_str() == Some(FINAM_PRODUCTION_HOST)
        && matches!(url.port(), None | Some(443))
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && matches!(url.path(), "" | "/")
}

fn sha256_hex(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    let mut output = String::with_capacity(64);
    use std::fmt::Write as _;
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("hex formatting cannot fail");
    }
    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(super) enum Stage8bItAdapterError {
    #[error("Stage 8B-IT endpoint is not the controlled loopback authority")]
    InvalidEndpoint,
    #[error("Stage 8B-IT qualification token is invalid")]
    InvalidQualificationToken,
    #[error("Stage 8B-IT exact route contains an invalid segment")]
    InvalidRouteSegment,
    #[error("Stage 8B-IT HTTP client could not be built")]
    ClientBuild,
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn production_policy_is_exact_tls_finam_host_only() {
        assert!(production_policy_accepts(
            &Url::parse("https://api.finam.ru/").unwrap()
        ));
        for rejected in [
            "http://api.finam.ru/",
            "https://api.finam.ru.evil.invalid/",
            "https://api.finam.ru:444/",
            "https://user@api.finam.ru/",
            "https://api.finam.ru/v1",
            "https://api.finam.ru/?redirect=1",
        ] {
            assert!(!production_policy_accepts(&Url::parse(rejected).unwrap()));
        }
    }

    #[test]
    fn qualification_endpoint_accepts_only_explicit_loopback_port() {
        assert!(
            Stage8bItQualificationEndpoint::controlled_loopback("http://127.0.0.1:18080/").is_ok()
        );
        for rejected in [
            "https://api.finam.ru/",
            "http://localhost:18080/",
            "http://127.0.0.1/",
            "http://127.0.0.1:18080/path",
            "http://127.0.0.1:18080/?q=1",
        ] {
            assert!(Stage8bItQualificationEndpoint::controlled_loopback(rejected).is_err());
        }
    }
}
