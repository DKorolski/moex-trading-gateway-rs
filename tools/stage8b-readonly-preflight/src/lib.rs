//! Separate non-authority Stage 8B-P read-only preflight helper.
//!
//! This crate is intentionally outside the effect workspace. It can perform
//! exactly two AuthService POSTs followed by three PLACE or four CANCEL GETs.
//! It has no order-effect request builder and exports redacted evidence only.

use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use zeroize::Zeroizing;

const RUN_AUTHORITY: &str =
    include_str!("../../../docs/stage-8/stage8b-p-r1b-run-identity-authority.json");
const CURRENT_SOURCE_AUTHORITY: &str =
    include_str!("../../../docs/stage-8/stage8b-p-r2a1-current-source-authority.json");
pub const PRODUCTION_BASE_URL: &str = "https://api.finam.ru";
pub const AUTH_REQUEST_BUDGET: usize = 2;
pub const PLACE_GET_BUDGET: usize = 3;
pub const CANCEL_GET_BUDGET: usize = 4;
pub const REQUEST_TIMEOUT_MS: u64 = 10_000;
pub const MIN_REQUEST_INTERVAL_MS: u64 = 250;
pub const TRADES_LIMIT: usize = 1_000;
pub const TRADES_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;
pub const QUERY_POLICY_ID: &str = "stage8b-p-r2a1-query-policy-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Operation {
    Place,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NetworkClass {
    AuthService,
    BrokerTruth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Source {
    GetOrder,
    OrdersSnapshot,
    TradesSnapshot,
    PositionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPlan {
    pub operation: Operation,
    pub run_identity_sha256: String,
    pub broker_order_id: Option<String>,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueryPolicyEvidence {
    pub policy_id: &'static str,
    pub orders_filter: &'static str,
    pub trades_limit: usize,
    pub trades_window_ms: i64,
    pub trades_time_basis: &'static str,
    pub pagination: &'static str,
    pub page_full_is_blocking: bool,
    pub caller_override_allowed: bool,
}

impl Default for QueryPolicyEvidence {
    fn default() -> Self {
        Self {
            policy_id: QUERY_POLICY_ID,
            orders_filter: "ClientSideAccountInstrumentAndOrderIdentity",
            trades_limit: TRADES_LIMIT,
            trades_window_ms: TRADES_WINDOW_MS,
            trades_time_basis: "RequestRequestedAt",
            pagination: "SinglePageNoCursor",
            page_full_is_blocking: true,
            caller_override_allowed: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentSourceEvidence {
    pub source_name: String,
    pub issuer: String,
    pub evidence_schema: String,
    pub observed_at_utc: DateTime<Utc>,
    pub payload_sha256: String,
    pub evidence_sha256: String,
    pub freshness_budget_key: String,
    pub skew_group: String,
    pub run_identity_sha256: String,
    pub selected_account_binding_sha256: String,
    pub execution_build_identity_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentSourcesEnvelope {
    pub schema_version: u8,
    pub sources: Vec<CurrentSourceEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CurrentSourcesEvidenceSummary {
    pub schema_version: u8,
    pub source_count: usize,
    pub source_digests: BTreeMap<String, String>,
    pub observed_at_utc: BTreeMap<String, DateTime<Utc>>,
    pub raw_payload_exported: bool,
    pub k2_authority_issued: bool,
}

pub struct ValidatedCurrentSources {
    summary: CurrentSourcesEvidenceSummary,
    run_identity_sha256: String,
    selected_account_binding_sha256: String,
    execution_build_identity_sha256: String,
}

impl ValidatedCurrentSources {
    pub fn summary(&self) -> &CurrentSourcesEvidenceSummary {
        &self.summary
    }
}

#[derive(Debug, Deserialize)]
struct CurrentSourceAuthority {
    required_inputs: Vec<CurrentSourceRule>,
    freshness_budgets_ms: BTreeMap<String, FreshnessBudget>,
    cross_source_budgets_ms: BTreeMap<String, i64>,
}

#[derive(Debug, Deserialize)]
struct CurrentSourceRule {
    source_name: String,
    issuer: String,
    evidence_schema: String,
    digest_domain: String,
    freshness_budget_key: String,
    skew_group: String,
}

#[derive(Debug, Deserialize)]
struct FreshnessBudget {
    max_age_ms: i64,
    max_future_skew_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttemptEvidence {
    pub ordinal: usize,
    pub network_class: NetworkClass,
    pub method: &'static str,
    pub route_template: &'static str,
    pub status: u16,
    pub response_body_len: usize,
    pub response_body_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadonlyEvidence {
    pub schema_version: u8,
    pub operation: Operation,
    pub run_identity_sha256: String,
    pub auth_request_count: usize,
    pub broker_get_count: usize,
    pub total_request_count: usize,
    pub request_order: Vec<AttemptEvidence>,
    pub query_policy: QueryPolicyEvidence,
    pub selected_account_binding_sha256: String,
    pub execution_build_identity_sha256: String,
    pub current_sources: CurrentSourcesEvidenceSummary,
    pub raw_account_exported: bool,
    pub token_exported: bool,
    pub raw_response_exported: bool,
    pub operator_arm_issued: bool,
    pub dispatch_attempt_recorded: bool,
    pub effect_transport_entered: bool,
    pub finam_order_post_delete_sent: bool,
    pub authorization_status: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum PreflightError {
    #[error("manifest is not a closed accepted R1B operation")]
    InvalidManifest,
    #[error("run identity does not match its canonical preimage")]
    RunIdentityMismatch,
    #[error("PLACE must not contain an order target")]
    PlaceOrderTargetForbidden,
    #[error("CANCEL requires the exact non-synthetic order target")]
    CancelOrderTargetInvalid,
    #[error("current-source evidence is invalid")]
    InvalidCurrentSources,
    #[error("network budget or order was violated")]
    NetworkBudget,
    #[error("authentication evidence is invalid")]
    Authentication,
    #[error("broker truth is incomplete or invalid")]
    IncompleteBrokerTruth,
    #[error("HTTP boundary failed")]
    Http(#[from] reqwest::Error),
    #[error("JSON boundary failed")]
    Json(#[from] serde_json::Error),
    #[error("URL boundary failed")]
    Url,
}

#[derive(Debug, Deserialize)]
struct RunAuthority {
    run_identity: RunIdentityAuthority,
}

#[derive(Debug, Deserialize)]
struct RunIdentityAuthority {
    domain_utf8: String,
    common_fields_in_exact_order_excluding_run_identity: Vec<String>,
    place_fields_in_exact_order: Vec<String>,
    cancel_fields_in_exact_order: Vec<String>,
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn digest_parts(domain: &str, parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain.as_bytes());
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn current_source_digest(rule: &CurrentSourceRule, source: &CurrentSourceEvidence) -> String {
    digest_parts(
        &rule.digest_domain,
        &[
            &source.source_name,
            &source.issuer,
            &source.evidence_schema,
            &source
                .observed_at_utc
                .to_rfc3339_opts(SecondsFormat::Millis, true),
            &source.payload_sha256,
            &source.freshness_budget_key,
            &source.skew_group,
            &source.run_identity_sha256,
            &source.selected_account_binding_sha256,
            &source.execution_build_identity_sha256,
        ],
    )
}

pub fn validate_current_sources(
    bytes: &[u8],
    plan: &ReadPlan,
    selected_account_binding_sha256: &str,
    execution_build_identity_sha256: &str,
    requested_at: DateTime<Utc>,
) -> Result<ValidatedCurrentSources, PreflightError> {
    if !is_lower_sha256(selected_account_binding_sha256)
        || !is_lower_sha256(execution_build_identity_sha256)
    {
        return Err(PreflightError::InvalidCurrentSources);
    }
    let envelope: CurrentSourcesEnvelope = serde_json::from_slice(bytes)?;
    let authority: CurrentSourceAuthority = serde_json::from_str(CURRENT_SOURCE_AUTHORITY)?;
    if envelope.schema_version != 1 || envelope.sources.len() != authority.required_inputs.len() {
        return Err(PreflightError::InvalidCurrentSources);
    }
    let supplied: BTreeMap<&str, &CurrentSourceEvidence> = envelope
        .sources
        .iter()
        .map(|source| (source.source_name.as_str(), source))
        .collect();
    if supplied.len() != envelope.sources.len() {
        return Err(PreflightError::InvalidCurrentSources);
    }
    let required: BTreeSet<&str> = authority
        .required_inputs
        .iter()
        .map(|rule| rule.source_name.as_str())
        .collect();
    if supplied.keys().copied().collect::<BTreeSet<_>>() != required {
        return Err(PreflightError::InvalidCurrentSources);
    }

    let mut digests = BTreeMap::new();
    let mut timestamps = BTreeMap::new();
    let mut skew_groups: BTreeMap<&str, Vec<DateTime<Utc>>> = BTreeMap::new();
    for rule in &authority.required_inputs {
        let source = supplied
            .get(rule.source_name.as_str())
            .copied()
            .ok_or(PreflightError::InvalidCurrentSources)?;
        let budget = authority
            .freshness_budgets_ms
            .get(&rule.freshness_budget_key)
            .ok_or(PreflightError::InvalidCurrentSources)?;
        let age_ms = requested_at
            .signed_duration_since(source.observed_at_utc)
            .num_milliseconds();
        if source.issuer != rule.issuer
            || source.evidence_schema != rule.evidence_schema
            || source.freshness_budget_key != rule.freshness_budget_key
            || source.skew_group != rule.skew_group
            || source.run_identity_sha256 != plan.run_identity_sha256
            || source.selected_account_binding_sha256 != selected_account_binding_sha256
            || source.execution_build_identity_sha256 != execution_build_identity_sha256
            || !is_lower_sha256(&source.payload_sha256)
            || !is_lower_sha256(&source.evidence_sha256)
            || current_source_digest(rule, source) != source.evidence_sha256
            || age_ms > budget.max_age_ms
            || age_ms < -budget.max_future_skew_ms
        {
            return Err(PreflightError::InvalidCurrentSources);
        }
        if authority
            .cross_source_budgets_ms
            .contains_key(rule.skew_group.as_str())
        {
            skew_groups
                .entry(rule.skew_group.as_str())
                .or_default()
                .push(source.observed_at_utc);
        }
        digests.insert(source.source_name.clone(), source.evidence_sha256.clone());
        timestamps.insert(source.source_name.clone(), source.observed_at_utc);
    }
    for (group, values) in skew_groups {
        let min = values
            .iter()
            .min()
            .ok_or(PreflightError::InvalidCurrentSources)?;
        let max = values
            .iter()
            .max()
            .ok_or(PreflightError::InvalidCurrentSources)?;
        let allowed = authority
            .cross_source_budgets_ms
            .get(group)
            .ok_or(PreflightError::InvalidCurrentSources)?;
        if max.signed_duration_since(*min).num_milliseconds() > *allowed {
            return Err(PreflightError::InvalidCurrentSources);
        }
    }
    Ok(ValidatedCurrentSources {
        summary: CurrentSourcesEvidenceSummary {
            schema_version: 1,
            source_count: digests.len(),
            source_digests: digests,
            observed_at_utc: timestamps,
            raw_payload_exported: false,
            k2_authority_issued: false,
        },
        run_identity_sha256: plan.run_identity_sha256.clone(),
        selected_account_binding_sha256: selected_account_binding_sha256.to_owned(),
        execution_build_identity_sha256: execution_build_identity_sha256.to_owned(),
    })
}

pub fn read_plan_from_manifest(bytes: &[u8]) -> Result<ReadPlan, PreflightError> {
    let manifest: Map<String, Value> = serde_json::from_slice(bytes)?;
    let operation = match manifest.get("operation").and_then(Value::as_str) {
        Some("PLACE") => Operation::Place,
        Some("CANCEL") => Operation::Cancel,
        _ => return Err(PreflightError::InvalidManifest),
    };
    let authority: RunAuthority = serde_json::from_str(RUN_AUTHORITY)?;
    let variant = match operation {
        Operation::Place => &authority.run_identity.place_fields_in_exact_order,
        Operation::Cancel => &authority.run_identity.cancel_fields_in_exact_order,
    };
    let ordered: Vec<&String> = authority
        .run_identity
        .common_fields_in_exact_order_excluding_run_identity
        .iter()
        .chain(variant.iter())
        .collect();
    let mut expected: BTreeSet<&str> = ordered.iter().map(|field| field.as_str()).collect();
    expected.insert("run_identity_sha256");
    if manifest.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected {
        return Err(PreflightError::InvalidManifest);
    }
    let mut values = Vec::with_capacity(ordered.len());
    for field in ordered {
        let value = manifest
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| value.is_ascii())
            .ok_or(PreflightError::InvalidManifest)?;
        values.push(value);
    }
    let computed = digest_parts(&authority.run_identity.domain_utf8, &values);
    let asserted = manifest
        .get("run_identity_sha256")
        .and_then(Value::as_str)
        .filter(|value| is_lower_sha256(value))
        .ok_or(PreflightError::InvalidManifest)?;
    if computed != asserted {
        return Err(PreflightError::RunIdentityMismatch);
    }

    match operation {
        Operation::Place => {
            if manifest.contains_key("broker_order_id")
                || manifest.contains_key("cancel_target_broker_order_id")
                || manifest.get("instrument").and_then(Value::as_str) != Some("IMOEXF@RTSX")
                || manifest.get("order_type").and_then(Value::as_str) != Some("ORDER_TYPE_LIMIT")
                || manifest.get("time_in_force").and_then(Value::as_str)
                    != Some("TIME_IN_FORCE_DAY")
                || manifest.get("quantity").and_then(Value::as_str) != Some("1")
            {
                return Err(PreflightError::PlaceOrderTargetForbidden);
            }
            Ok(ReadPlan {
                operation,
                run_identity_sha256: computed,
                broker_order_id: None,
                sources: vec![
                    Source::OrdersSnapshot,
                    Source::TradesSnapshot,
                    Source::PositionSnapshot,
                ],
            })
        }
        Operation::Cancel => {
            let order_id = manifest
                .get("cancel_target_broker_order_id")
                .and_then(Value::as_str)
                .filter(|value| {
                    !value.is_empty()
                        && !value.starts_with("SYNTHETIC")
                        && !value.starts_with("synthetic")
                })
                .ok_or(PreflightError::CancelOrderTargetInvalid)?;
            Ok(ReadPlan {
                operation,
                run_identity_sha256: computed,
                broker_order_id: Some(order_id.to_owned()),
                sources: vec![
                    Source::GetOrder,
                    Source::OrdersSnapshot,
                    Source::TradesSnapshot,
                    Source::PositionSnapshot,
                ],
            })
        }
    }
}

fn hardened_client(https_only: bool) -> Result<reqwest::Client, PreflightError> {
    Ok(reqwest::Client::builder()
        .https_only(https_only)
        .timeout(Duration::from_millis(REQUEST_TIMEOUT_MS))
        .retry(reqwest::retry::never())
        .redirect(Policy::none())
        .no_proxy()
        .build()?)
}

pub fn production_clients() -> Result<(reqwest::Client, reqwest::Client), PreflightError> {
    Ok((hardened_client(true)?, hardened_client(true)?))
}

fn append_segment(url: &mut reqwest::Url, segment: &str) -> Result<(), PreflightError> {
    url.path_segments_mut()
        .map_err(|_| PreflightError::Url)?
        .push(segment);
    Ok(())
}

fn base_url(value: &str, allow_http_for_test: bool) -> Result<reqwest::Url, PreflightError> {
    let url = reqwest::Url::parse(value).map_err(|_| PreflightError::Url)?;
    if (!allow_http_for_test && url.as_str() != "https://api.finam.ru/")
        || (allow_http_for_test && !matches!(url.scheme(), "http" | "https"))
    {
        return Err(PreflightError::Url);
    }
    Ok(url)
}

fn route(
    base: &reqwest::Url,
    source: Source,
    account_id: &str,
    order_id: Option<&str>,
    requested_at: DateTime<Utc>,
) -> Result<(reqwest::Url, &'static str), PreflightError> {
    let mut url = base.clone();
    url.set_path("");
    append_segment(&mut url, "v1")?;
    append_segment(&mut url, "accounts")?;
    append_segment(&mut url, account_id)?;
    let template = match source {
        Source::GetOrder => {
            append_segment(&mut url, "orders")?;
            append_segment(
                &mut url,
                order_id.ok_or(PreflightError::CancelOrderTargetInvalid)?,
            )?;
            "/v1/accounts/{account_id}/orders/{order_id}"
        }
        Source::OrdersSnapshot => {
            append_segment(&mut url, "orders")?;
            "/v1/accounts/{account_id}/orders"
        }
        Source::TradesSnapshot => {
            append_segment(&mut url, "trades")?;
            let start = requested_at
                .checked_sub_signed(ChronoDuration::milliseconds(TRADES_WINDOW_MS))
                .ok_or(PreflightError::Url)?;
            url.query_pairs_mut()
                .append_pair("limit", &TRADES_LIMIT.to_string())
                .append_pair(
                    "interval.start_time",
                    &start.to_rfc3339_opts(SecondsFormat::Secs, true),
                )
                .append_pair(
                    "interval.end_time",
                    &requested_at.to_rfc3339_opts(SecondsFormat::Secs, true),
                );
            "/v1/accounts/{account_id}/trades"
        }
        Source::PositionSnapshot => "/v1/accounts/{account_id}",
    };
    Ok((url, template))
}

#[derive(Debug, Deserialize)]
struct AuthResponse {
    token: String,
}

#[derive(Debug, Deserialize)]
struct TokenDetails {
    #[serde(default)]
    account_ids: Vec<String>,
    readonly: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct OrdersResponse {
    #[serde(default)]
    orders: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct TradesResponse {
    #[serde(default)]
    trades: Vec<Value>,
}

async fn body_and_evidence(
    ordinal: usize,
    class: NetworkClass,
    method: &'static str,
    template: &'static str,
    response: reqwest::Response,
) -> Result<(u16, Zeroizing<Vec<u8>>, AttemptEvidence), PreflightError> {
    let status = response.status().as_u16();
    let body = Zeroizing::new(response.bytes().await?.to_vec());
    let evidence = AttemptEvidence {
        ordinal,
        network_class: class,
        method,
        route_template: template,
        status,
        response_body_len: body.len(),
        response_body_sha256: sha256(&body),
    };
    Ok((status, body, evidence))
}

struct HttpBoundary<'a> {
    auth_client: &'a reqwest::Client,
    broker_client: &'a reqwest::Client,
    base: &'a str,
    allow_http_for_test: bool,
}

async fn execute_with_clients(
    boundary: HttpBoundary<'_>,
    secret: &str,
    account_id: &str,
    plan: &ReadPlan,
    current_sources: ValidatedCurrentSources,
    requested_at: DateTime<Utc>,
) -> Result<ReadonlyEvidence, PreflightError> {
    if secret.is_empty() || account_id.is_empty() {
        return Err(PreflightError::Authentication);
    }
    let account_binding_sha256 = sha256(account_id.as_bytes());
    if current_sources.run_identity_sha256 != plan.run_identity_sha256
        || current_sources.selected_account_binding_sha256 != account_binding_sha256
    {
        return Err(PreflightError::InvalidCurrentSources);
    }
    let base = base_url(boundary.base, boundary.allow_http_for_test)?;
    let mut attempts = Vec::new();

    let auth_url = base.join("v1/sessions").map_err(|_| PreflightError::Url)?;
    let response = boundary
        .auth_client
        .post(auth_url)
        .json(&serde_json::json!({"secret": secret}))
        .send()
        .await?;
    let (status, body, evidence) = body_and_evidence(
        1,
        NetworkClass::AuthService,
        "POST",
        "/v1/sessions",
        response,
    )
    .await?;
    attempts.push(evidence);
    if status != 200 {
        return Err(PreflightError::Authentication);
    }
    let auth: AuthResponse = serde_json::from_slice(&body)?;
    let token = Zeroizing::new(auth.token);
    if token.is_empty() {
        return Err(PreflightError::Authentication);
    }

    let details_url = base
        .join("v1/sessions/details")
        .map_err(|_| PreflightError::Url)?;
    let response = boundary
        .auth_client
        .post(details_url)
        .json(&serde_json::json!({"token": token.as_str()}))
        .send()
        .await?;
    let (status, body, evidence) = body_and_evidence(
        2,
        NetworkClass::AuthService,
        "POST",
        "/v1/sessions/details",
        response,
    )
    .await?;
    attempts.push(evidence);
    if status != 200 {
        return Err(PreflightError::Authentication);
    }
    let details: TokenDetails = serde_json::from_slice(&body)?;
    if details.readonly != Some(true)
        || details.account_ids.len() != 1
        || details.account_ids.first().map(String::as_str) != Some(account_id)
    {
        return Err(PreflightError::Authentication);
    }

    let expected_gets = match plan.operation {
        Operation::Place => PLACE_GET_BUDGET,
        Operation::Cancel => CANCEL_GET_BUDGET,
    };
    if plan.sources.len() != expected_gets {
        return Err(PreflightError::NetworkBudget);
    }
    for (index, source) in plan.sources.iter().copied().enumerate() {
        if index > 0 {
            tokio::time::sleep(Duration::from_millis(MIN_REQUEST_INTERVAL_MS)).await;
        }
        let (url, template) = route(
            &base,
            source,
            account_id,
            plan.broker_order_id.as_deref(),
            requested_at,
        )?;
        let response = boundary
            .broker_client
            .get(url)
            .bearer_auth(token.as_str())
            .send()
            .await?;
        let (status, body, evidence) = body_and_evidence(
            AUTH_REQUEST_BUDGET + index + 1,
            NetworkClass::BrokerTruth,
            "GET",
            template,
            response,
        )
        .await?;
        attempts.push(evidence);
        if status != 200 {
            return Err(PreflightError::IncompleteBrokerTruth);
        }
        match source {
            Source::TradesSnapshot => {
                let parsed: TradesResponse = serde_json::from_slice(&body)?;
                if parsed.trades.len() >= TRADES_LIMIT {
                    return Err(PreflightError::IncompleteBrokerTruth);
                }
            }
            Source::OrdersSnapshot => {
                let parsed: OrdersResponse = serde_json::from_slice(&body)?;
                let _count = parsed.orders.len();
            }
            Source::GetOrder | Source::PositionSnapshot => {
                let _: Value = serde_json::from_slice(&body)?;
            }
        }
    }
    if attempts.len() != AUTH_REQUEST_BUDGET + expected_gets {
        return Err(PreflightError::NetworkBudget);
    }

    Ok(ReadonlyEvidence {
        schema_version: 1,
        operation: plan.operation,
        run_identity_sha256: plan.run_identity_sha256.clone(),
        auth_request_count: AUTH_REQUEST_BUDGET,
        broker_get_count: expected_gets,
        total_request_count: attempts.len(),
        request_order: attempts,
        query_policy: QueryPolicyEvidence::default(),
        selected_account_binding_sha256: account_binding_sha256,
        execution_build_identity_sha256: current_sources.execution_build_identity_sha256,
        current_sources: current_sources.summary,
        raw_account_exported: false,
        token_exported: false,
        raw_response_exported: false,
        operator_arm_issued: false,
        dispatch_attempt_recorded: false,
        effect_transport_entered: false,
        finam_order_post_delete_sent: false,
        authorization_status: "NOT_ISSUED",
    })
}

pub async fn execute_production(
    secret: &str,
    account_id: &str,
    manifest: &[u8],
    current_sources: &[u8],
    execution_build_identity_sha256: &str,
    requested_at: DateTime<Utc>,
) -> Result<ReadonlyEvidence, PreflightError> {
    let plan = read_plan_from_manifest(manifest)?;
    let account_binding_sha256 = sha256(account_id.as_bytes());
    let current_sources = validate_current_sources(
        current_sources,
        &plan,
        &account_binding_sha256,
        execution_build_identity_sha256,
        requested_at,
    )?;
    let (auth, broker) = production_clients()?;
    execute_with_clients(
        HttpBoundary {
            auth_client: &auth,
            broker_client: &broker,
            base: PRODUCTION_BASE_URL,
            allow_http_for_test: false,
        },
        secret,
        account_id,
        &plan,
        current_sources,
        requested_at,
    )
    .await
}

pub fn source_plan(plan: &ReadPlan) -> BTreeMap<&'static str, &'static str> {
    let mut result = BTreeMap::new();
    result.insert("auth_1", "POST /v1/sessions");
    result.insert("auth_2", "POST /v1/sessions/details");
    match plan.operation {
        Operation::Place => {
            result.insert("broker_1", "GET /v1/accounts/{account_id}/orders");
            result.insert("broker_2", "GET /v1/accounts/{account_id}/trades");
            result.insert("broker_3", "GET /v1/accounts/{account_id}");
        }
        Operation::Cancel => {
            result.insert(
                "broker_1",
                "GET /v1/accounts/{account_id}/orders/{order_id}",
            );
            result.insert("broker_2", "GET /v1/accounts/{account_id}/orders");
            result.insert("broker_3", "GET /v1/accounts/{account_id}/trades");
            result.insert("broker_4", "GET /v1/accounts/{account_id}");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn golden_manifest(operation: &str) -> Vec<u8> {
        let authority: Value = serde_json::from_str(RUN_AUTHORITY).unwrap();
        let vector = &authority["golden_vectors"][operation];
        let mut manifest = vector["manifest_without_run_identity_sha256"]
            .as_object()
            .unwrap()
            .clone();
        manifest.insert(
            "run_identity_sha256".to_owned(),
            vector["run_identity_sha256"].clone(),
        );
        serde_json::to_vec(&manifest).unwrap()
    }

    fn current_sources(
        plan: &ReadPlan,
        account_sha256: &str,
        build_sha256: &str,
        observed_at: DateTime<Utc>,
    ) -> Vec<u8> {
        let authority: CurrentSourceAuthority =
            serde_json::from_str(CURRENT_SOURCE_AUTHORITY).unwrap();
        let sources = authority
            .required_inputs
            .iter()
            .map(|rule| {
                let mut source = CurrentSourceEvidence {
                    source_name: rule.source_name.clone(),
                    issuer: rule.issuer.clone(),
                    evidence_schema: rule.evidence_schema.clone(),
                    observed_at_utc: observed_at,
                    payload_sha256: sha256(rule.source_name.as_bytes()),
                    evidence_sha256: String::new(),
                    freshness_budget_key: rule.freshness_budget_key.clone(),
                    skew_group: rule.skew_group.clone(),
                    run_identity_sha256: plan.run_identity_sha256.clone(),
                    selected_account_binding_sha256: account_sha256.to_owned(),
                    execution_build_identity_sha256: build_sha256.to_owned(),
                };
                source.evidence_sha256 = current_source_digest(rule, &source);
                source
            })
            .collect();
        serde_json::to_vec(&CurrentSourcesEnvelope {
            schema_version: 1,
            sources,
        })
        .unwrap()
    }

    #[test]
    fn place_and_cancel_plans_are_distinct_and_exact() {
        let place = read_plan_from_manifest(&golden_manifest("PLACE")).unwrap();
        let cancel = read_plan_from_manifest(&golden_manifest("CANCEL")).unwrap();
        assert_eq!(place.operation, Operation::Place);
        assert_eq!(place.sources.len(), 3);
        assert_eq!(place.broker_order_id, None);
        assert!(!place.sources.contains(&Source::GetOrder));
        assert_eq!(cancel.operation, Operation::Cancel);
        assert_eq!(cancel.sources.len(), 4);
        assert!(cancel.broker_order_id.is_some());
        assert_eq!(cancel.sources[0], Source::GetOrder);
    }

    #[test]
    fn hidden_place_target_and_synthetic_cancel_are_rejected() {
        let mut place: Value = serde_json::from_slice(&golden_manifest("PLACE")).unwrap();
        place["broker_order_id"] = Value::String("hidden".to_owned());
        assert!(matches!(
            read_plan_from_manifest(&serde_json::to_vec(&place).unwrap()),
            Err(PreflightError::InvalidManifest)
        ));
        let mut cancel: Value = serde_json::from_slice(&golden_manifest("CANCEL")).unwrap();
        cancel["cancel_target_broker_order_id"] =
            Value::String("SYNTHETIC_PROBE_ORDER_0001".to_owned());
        let authority: RunAuthority = serde_json::from_str(RUN_AUTHORITY).unwrap();
        let order: Vec<&str> = authority
            .run_identity
            .common_fields_in_exact_order_excluding_run_identity
            .iter()
            .chain(authority.run_identity.cancel_fields_in_exact_order.iter())
            .map(|field| cancel[field].as_str().unwrap())
            .collect();
        cancel["run_identity_sha256"] =
            Value::String(digest_parts(&authority.run_identity.domain_utf8, &order));
        assert!(matches!(
            read_plan_from_manifest(&serde_json::to_vec(&cancel).unwrap()),
            Err(PreflightError::CancelOrderTargetInvalid)
        ));
    }

    async fn one_response_server(
        response: &'static str,
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&count);
        let task = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                observed.fetch_add(1, Ordering::SeqCst);
                let mut buffer = [0_u8; 2048];
                let _ = stream.read(&mut buffer).await;
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        (format!("http://{address}/"), count, task)
    }

    #[tokio::test]
    async fn redirect_is_returned_and_never_followed() {
        let (url, count, task) = one_response_server(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .await;
        let client = hardened_client(false).unwrap();
        let response = client.get(url).send().await.unwrap();
        assert_eq!(response.status().as_u16(), 302);
        task.await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn protocol_failure_is_not_retried() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&count);
        let task = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                observed.fetch_add(1, Ordering::SeqCst);
                drop(stream);
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        });
        let client = hardened_client(false).unwrap();
        assert!(client
            .get(format!("http://{address}/nack"))
            .send()
            .await
            .is_err());
        task.await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn system_proxy_environment_is_ignored() {
        let keys = ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"];
        let previous: Vec<_> = keys.iter().map(std::env::var_os).collect();
        for key in &keys[..3] {
            std::env::set_var(key, "http://127.0.0.1:9");
        }
        std::env::set_var("NO_PROXY", "");
        let (url, count, task) = one_response_server(
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        )
        .await;
        let client = hardened_client(false).unwrap();
        let status = client.get(url).send().await.unwrap().status();
        for (key, value) in keys.iter().zip(previous) {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        task.await.unwrap();
        assert_eq!(status.as_u16(), 200);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn query_policy_is_exact_and_non_overridable() {
        let policy = QueryPolicyEvidence::default();
        assert_eq!(policy.trades_limit, 1000);
        assert_eq!(policy.trades_window_ms, 86_400_000);
        assert_eq!(policy.trades_time_basis, "RequestRequestedAt");
        assert_eq!(policy.pagination, "SinglePageNoCursor");
        assert!(policy.page_full_is_blocking);
        assert!(!policy.caller_override_allowed);
    }

    #[test]
    fn all_seventeen_current_sources_are_exactly_bound() {
        let plan = read_plan_from_manifest(&golden_manifest("PLACE")).unwrap();
        let requested_at = "2026-08-25T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let account = sha256(b"ACCOUNT");
        let build = sha256(b"helper-build");
        let bytes = current_sources(&plan, &account, &build, requested_at);
        let validated =
            validate_current_sources(&bytes, &plan, &account, &build, requested_at).unwrap();
        assert_eq!(validated.summary().source_count, 17);
        assert!(!validated.summary().raw_payload_exported);
        assert!(!validated.summary().k2_authority_issued);

        let mut stale: CurrentSourcesEnvelope = serde_json::from_slice(&bytes).unwrap();
        stale.sources[0].observed_at_utc = requested_at - ChronoDuration::seconds(2);
        let stale = serde_json::to_vec(&stale).unwrap();
        assert!(matches!(
            validate_current_sources(&stale, &plan, &account, &build, requested_at),
            Err(PreflightError::InvalidCurrentSources)
        ));
    }

    async fn exact_mock_server(
        expected: usize,
    ) -> (
        String,
        Arc<tokio::sync::Mutex<Vec<String>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let observed = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&observed);
        let task = tokio::spawn(async move {
            for _ in 0..expected {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = [0_u8; 8192];
                let count = stream.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..count]);
                let first = request.lines().next().unwrap().to_owned();
                captured.lock().await.push(first.clone());
                let body = if first.starts_with("POST /v1/sessions/details ") {
                    r#"{"account_ids":["ACCOUNT"],"readonly":true}"#
                } else if first.starts_with("POST /v1/sessions ") {
                    r#"{"token":"short-lived-readonly-token"}"#
                } else if first.contains("/trades?") {
                    r#"{"trades":[]}"#
                } else if first.ends_with("/orders HTTP/1.1") {
                    r#"{"orders":[]}"#
                } else {
                    "{}"
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (format!("http://{address}/"), observed, task)
    }

    async fn assert_exact_source_level_topology(operation: &str, expected_gets: usize) {
        let plan = read_plan_from_manifest(&golden_manifest(operation)).unwrap();
        let total = AUTH_REQUEST_BUDGET + expected_gets;
        let (base, observed, task) = exact_mock_server(total).await;
        let client = hardened_client(false).unwrap();
        let requested_at = "2026-08-25T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let account = sha256(b"ACCOUNT");
        let build = sha256(b"helper-build");
        let sources = current_sources(&plan, &account, &build, requested_at);
        let validated =
            validate_current_sources(&sources, &plan, &account, &build, requested_at).unwrap();
        let evidence = execute_with_clients(
            HttpBoundary {
                auth_client: &client,
                broker_client: &client,
                base: &base,
                allow_http_for_test: true,
            },
            "secret",
            "ACCOUNT",
            &plan,
            validated,
            requested_at,
        )
        .await
        .unwrap();
        task.await.unwrap();
        let requests = observed.lock().await.clone();
        assert_eq!(evidence.auth_request_count, 2);
        assert_eq!(evidence.broker_get_count, expected_gets);
        assert_eq!(evidence.total_request_count, total);
        assert_eq!(requests.len(), total);
        assert!(requests[0].starts_with("POST /v1/sessions "));
        assert!(requests[1].starts_with("POST /v1/sessions/details "));
        assert!(requests[2..]
            .iter()
            .all(|request| request.starts_with("GET ")));
        assert!(!evidence.operator_arm_issued);
        assert!(!evidence.effect_transport_entered);
        assert_eq!(evidence.authorization_status, "NOT_ISSUED");
    }

    #[tokio::test]
    async fn place_is_exactly_two_auth_posts_then_three_gets() {
        assert_exact_source_level_topology("PLACE", PLACE_GET_BUDGET).await;
    }

    #[tokio::test]
    async fn cancel_is_exactly_two_auth_posts_then_four_gets() {
        assert_exact_source_level_topology("CANCEL", CANCEL_GET_BUDGET).await;
    }
}
