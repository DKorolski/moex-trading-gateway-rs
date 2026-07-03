//! M3d-2a local mock endpoint wire harness.
//!
//! This module does not add a production FINAM order endpoint client and does
//! not perform real broker calls. It only provides redacted diagnostics for
//! local mock endpoint tests that prove the exact-two-route wire shape can be
//! observed on loopback before any real implementation is reviewed.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const M3D2_PLACE_ORDER_ROUTE_TEMPLATE: &str = "/v1/accounts/{account_id}/orders";
pub const M3D2_CANCEL_ORDER_ROUTE_TEMPLATE: &str = "/v1/accounts/{account_id}/orders/{order_id}";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum M3d2LocalMockOrderOperation {
    PlaceOrder,
    CancelOrder,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum M3d2LocalMockPlaceBodyKind {
    Market,
    Limit,
    Unknown,
    NotJson,
    NotPlaceBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct M3d2LocalMockPlaceBodyDiagnostic {
    pub body_kind: M3d2LocalMockPlaceBodyKind,
    pub json_object: bool,
    pub symbol_present: bool,
    pub symbol_format_ok: bool,
    pub side_present: bool,
    pub side_official_finam_enum: bool,
    pub quantity_present: bool,
    pub quantity_value_decimal_string: bool,
    pub order_type_present: bool,
    pub order_type_official_finam_enum: bool,
    pub time_in_force_present: bool,
    pub time_in_force_plain_order_allowed: bool,
    pub client_order_id_present: bool,
    pub client_order_id_finam_safe: bool,
    pub limit_price_present: bool,
    pub limit_price_value_decimal_string: bool,
    pub forbidden_plain_order_field_present: bool,
    pub comment_present: bool,
    pub comment_string: bool,
    pub strict_contract_rejection: Option<String>,
    pub raw_body_exported: bool,
}

impl Default for M3d2LocalMockPlaceBodyDiagnostic {
    fn default() -> Self {
        Self {
            body_kind: M3d2LocalMockPlaceBodyKind::NotPlaceBody,
            json_object: false,
            symbol_present: false,
            symbol_format_ok: false,
            side_present: false,
            side_official_finam_enum: false,
            quantity_present: false,
            quantity_value_decimal_string: false,
            order_type_present: false,
            order_type_official_finam_enum: false,
            time_in_force_present: false,
            time_in_force_plain_order_allowed: false,
            client_order_id_present: false,
            client_order_id_finam_safe: false,
            limit_price_present: false,
            limit_price_value_decimal_string: false,
            forbidden_plain_order_field_present: false,
            comment_present: false,
            comment_string: false,
            strict_contract_rejection: None,
            raw_body_exported: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct M3d2LocalMockWireDiagnostic {
    pub operation: M3d2LocalMockOrderOperation,
    pub method_name: String,
    pub route_template: Option<String>,
    pub route_template_exported_as_design_data_only: bool,
    pub raw_path_exported: bool,
    pub account_id_present: bool,
    pub account_id_len: Option<usize>,
    pub order_id_present: bool,
    pub order_id_len: Option<usize>,
    pub authorization_present: bool,
    pub authorization_len: Option<usize>,
    pub raw_authorization_exported: bool,
    pub content_type_json: bool,
    pub body_len: usize,
    pub body_sha256: String,
    pub body: M3d2LocalMockPlaceBodyDiagnostic,
    pub cancel_body_empty: bool,
    pub accepted_by_m3d2a_mock: bool,
    pub rejection_reason: Option<String>,
}

pub fn classify_m3d2_local_mock_wire_request(raw_request: &str) -> M3d2LocalMockWireDiagnostic {
    let (head, body) = split_head_body(raw_request);
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut request_line_parts = request_line.split_whitespace();
    let method_name = request_line_parts.next().unwrap_or_default().to_string();
    let raw_path = request_line_parts.next().unwrap_or_default();
    let headers = parse_headers(lines);
    let route = classify_route(raw_path);
    let authorization_len = headers.get("authorization").map(|value| value.len());
    let content_type_json = headers
        .get("content-type")
        .map(|value| value.to_ascii_lowercase().contains("application/json"))
        .unwrap_or(false);
    let body_sha256 = sha256_hex(body.as_bytes());
    let place_body = if route.operation == M3d2LocalMockOrderOperation::PlaceOrder {
        classify_place_body(body)
    } else {
        M3d2LocalMockPlaceBodyDiagnostic::default()
    };
    let cancel_body_empty =
        route.operation == M3d2LocalMockOrderOperation::CancelOrder && body.is_empty();

    let mut rejection_reason = None;
    if route.operation == M3d2LocalMockOrderOperation::PlaceOrder {
        if method_name != "POST" {
            rejection_reason = Some("place_method_not_post".to_string());
        } else if authorization_len.is_none() {
            rejection_reason = Some("authorization_missing".to_string());
        } else if !content_type_json {
            rejection_reason = Some("content_type_not_json".to_string());
        } else if !matches!(
            place_body.body_kind,
            M3d2LocalMockPlaceBodyKind::Market | M3d2LocalMockPlaceBodyKind::Limit
        ) {
            rejection_reason = Some("place_body_shape_not_supported".to_string());
        }
    } else if route.operation == M3d2LocalMockOrderOperation::CancelOrder {
        if method_name != "DELETE" {
            rejection_reason = Some("cancel_method_not_delete".to_string());
        } else if authorization_len.is_none() {
            rejection_reason = Some("authorization_missing".to_string());
        } else if !cancel_body_empty {
            rejection_reason = Some("cancel_body_not_empty".to_string());
        }
    } else {
        rejection_reason = Some("route_not_in_exact_two_route_allowlist".to_string());
    }

    M3d2LocalMockWireDiagnostic {
        operation: route.operation,
        method_name,
        route_template: route.route_template.map(str::to_string),
        route_template_exported_as_design_data_only: true,
        raw_path_exported: false,
        account_id_present: route.account_id_len.is_some(),
        account_id_len: route.account_id_len,
        order_id_present: route.order_id_len.is_some(),
        order_id_len: route.order_id_len,
        authorization_present: authorization_len.is_some(),
        authorization_len,
        raw_authorization_exported: false,
        content_type_json,
        body_len: body.len(),
        body_sha256,
        body: place_body,
        cancel_body_empty,
        accepted_by_m3d2a_mock: rejection_reason.is_none(),
        rejection_reason,
    }
}

pub fn classify_m3d2_local_mock_place_spec(
    spec: &broker_finam::FinamPlaceOrderRequestSpec,
) -> M3d2LocalMockWireDiagnostic {
    let body = serde_json::to_string(&spec.body).unwrap_or_else(|_| "{}".to_string());
    let path = format!("/{}", spec.rest_path_segments().join("/"));
    let raw_request = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer REDACTED_LOCAL_MOCK_TOKEN\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    classify_m3d2_local_mock_wire_request(&raw_request)
}

pub fn classify_m3d2_local_mock_cancel_spec(
    spec: &broker_finam::FinamCancelOrderRequestSpec,
) -> M3d2LocalMockWireDiagnostic {
    let path = format!("/{}", spec.rest_path_segments().join("/"));
    let raw_request = format!(
        "DELETE {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer REDACTED_LOCAL_MOCK_TOKEN\r\nContent-Length: 0\r\n\r\n"
    );
    classify_m3d2_local_mock_wire_request(&raw_request)
}

struct RouteClassification {
    operation: M3d2LocalMockOrderOperation,
    route_template: Option<&'static str>,
    account_id_len: Option<usize>,
    order_id_len: Option<usize>,
}

fn classify_route(path: &str) -> RouteClassification {
    let Some(rest) = path.strip_prefix("/v1/accounts/") else {
        return unknown_route();
    };
    let parts = rest.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        [account_id, "orders"] if !account_id.is_empty() => RouteClassification {
            operation: M3d2LocalMockOrderOperation::PlaceOrder,
            route_template: Some(M3D2_PLACE_ORDER_ROUTE_TEMPLATE),
            account_id_len: Some(account_id.len()),
            order_id_len: None,
        },
        [account_id, "orders", order_id] if !account_id.is_empty() && !order_id.is_empty() => {
            RouteClassification {
                operation: M3d2LocalMockOrderOperation::CancelOrder,
                route_template: Some(M3D2_CANCEL_ORDER_ROUTE_TEMPLATE),
                account_id_len: Some(account_id.len()),
                order_id_len: Some(order_id.len()),
            }
        }
        _ => unknown_route(),
    }
}

fn unknown_route() -> RouteClassification {
    RouteClassification {
        operation: M3d2LocalMockOrderOperation::Unknown,
        route_template: None,
        account_id_len: None,
        order_id_len: None,
    }
}

fn parse_headers<'a>(lines: impl Iterator<Item = &'a str>) -> BTreeMap<String, String> {
    lines
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect()
}

fn split_head_body(raw_request: &str) -> (&str, &str) {
    raw_request
        .split_once("\r\n\r\n")
        .or_else(|| raw_request.split_once("\n\n"))
        .unwrap_or((raw_request, ""))
}

fn classify_place_body(body: &str) -> M3d2LocalMockPlaceBodyDiagnostic {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return M3d2LocalMockPlaceBodyDiagnostic {
            body_kind: M3d2LocalMockPlaceBodyKind::NotJson,
            ..M3d2LocalMockPlaceBodyDiagnostic::default()
        };
    };
    let Some(object) = value.as_object() else {
        return M3d2LocalMockPlaceBodyDiagnostic {
            body_kind: M3d2LocalMockPlaceBodyKind::NotJson,
            json_object: false,
            ..M3d2LocalMockPlaceBodyDiagnostic::default()
        };
    };

    let symbol_present = object.contains_key("symbol");
    let symbol_format_ok = object
        .get("symbol")
        .and_then(Value::as_str)
        .is_some_and(is_pinned_broker_symbol);
    let side_present = object.contains_key("side");
    let side_official_finam_enum = object
        .get("side")
        .and_then(Value::as_str)
        .is_some_and(|value| matches!(value, "SIDE_BUY" | "SIDE_SELL"));
    let quantity_present = object.contains_key("quantity");
    let quantity_value_decimal_string = decimal_value_field_ok(object.get("quantity"));
    let order_type_present = object.contains_key("type");
    let order_type_official_finam_enum = object
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|value| matches!(value, "ORDER_TYPE_MARKET" | "ORDER_TYPE_LIMIT"));
    let time_in_force_present = object.contains_key("time_in_force");
    let time_in_force_plain_order_allowed = object
        .get("time_in_force")
        .and_then(Value::as_str)
        .is_some_and(|value| {
            matches!(
                value,
                "TIME_IN_FORCE_DAY"
                    | "TIME_IN_FORCE_GOOD_TILL_CANCEL"
                    | "TIME_IN_FORCE_IOC"
                    | "TIME_IN_FORCE_FOK"
            )
        });
    let client_order_id_present = object.contains_key("client_order_id");
    let client_order_id_finam_safe = object
        .get("client_order_id")
        .and_then(Value::as_str)
        .is_some_and(is_finam_safe_client_order_id);
    let limit_price_present = object.contains_key("limit_price");
    let limit_price_value_decimal_string = decimal_value_field_ok(object.get("limit_price"));
    let forbidden_plain_order_field_present = object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "symbol"
                | "quantity"
                | "side"
                | "type"
                | "time_in_force"
                | "limit_price"
                | "client_order_id"
                | "comment"
        ) || matches!(
            key.as_str(),
            "order_type"
                | "stop_price"
                | "stop_condition"
                | "stop_loss"
                | "take_profit"
                | "legs"
                | "sltp"
                | "valid_before"
        )
    });
    let comment_present = object.contains_key("comment");
    let comment_string = object.get("comment").is_some_and(Value::is_string);
    let order_type = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let core_fields_present =
        symbol_present && side_present && quantity_present && order_type_present;
    let strict_contract_rejection = strict_place_body_rejection(
        core_fields_present,
        symbol_format_ok,
        side_official_finam_enum,
        quantity_value_decimal_string,
        order_type_official_finam_enum,
        time_in_force_present,
        time_in_force_plain_order_allowed,
        client_order_id_present,
        client_order_id_finam_safe,
        limit_price_present,
        limit_price_value_decimal_string,
        forbidden_plain_order_field_present,
        comment_present,
        comment_string,
        &order_type,
    );
    let body_kind = if strict_contract_rejection.is_some() {
        M3d2LocalMockPlaceBodyKind::Unknown
    } else if order_type == "ORDER_TYPE_LIMIT" {
        M3d2LocalMockPlaceBodyKind::Limit
    } else if order_type == "ORDER_TYPE_MARKET" {
        M3d2LocalMockPlaceBodyKind::Market
    } else {
        M3d2LocalMockPlaceBodyKind::Unknown
    };

    M3d2LocalMockPlaceBodyDiagnostic {
        body_kind,
        json_object: true,
        symbol_present,
        symbol_format_ok,
        side_present,
        side_official_finam_enum,
        quantity_present,
        quantity_value_decimal_string,
        order_type_present,
        order_type_official_finam_enum,
        time_in_force_present,
        time_in_force_plain_order_allowed,
        client_order_id_present,
        client_order_id_finam_safe,
        limit_price_present,
        limit_price_value_decimal_string,
        forbidden_plain_order_field_present,
        comment_present,
        comment_string,
        strict_contract_rejection,
        raw_body_exported: false,
    }
}

#[allow(clippy::too_many_arguments)]
fn strict_place_body_rejection(
    core_fields_present: bool,
    symbol_format_ok: bool,
    side_official_finam_enum: bool,
    quantity_value_decimal_string: bool,
    order_type_official_finam_enum: bool,
    time_in_force_present: bool,
    time_in_force_plain_order_allowed: bool,
    client_order_id_present: bool,
    client_order_id_finam_safe: bool,
    limit_price_present: bool,
    limit_price_value_decimal_string: bool,
    forbidden_plain_order_field_present: bool,
    comment_present: bool,
    comment_string: bool,
    order_type: &str,
) -> Option<String> {
    if !core_fields_present {
        return Some("required_plain_order_field_missing".to_string());
    }
    if forbidden_plain_order_field_present {
        return Some("plain_order_field_not_allowed".to_string());
    }
    if !symbol_format_ok {
        return Some("symbol_format_not_pinned_broker_symbol".to_string());
    }
    if !quantity_value_decimal_string {
        return Some("quantity_value_not_decimal_string".to_string());
    }
    if !side_official_finam_enum {
        return Some("side_not_official_finam_enum".to_string());
    }
    if !order_type_official_finam_enum {
        return Some("order_type_not_official_finam_enum".to_string());
    }
    if !time_in_force_present {
        return Some("time_in_force_missing".to_string());
    }
    if !time_in_force_plain_order_allowed {
        return Some("time_in_force_not_plain_order_allowed".to_string());
    }
    if !client_order_id_present {
        return Some("client_order_id_missing".to_string());
    }
    if !client_order_id_finam_safe {
        return Some("client_order_id_not_finam_safe".to_string());
    }
    if comment_present && !comment_string {
        return Some("comment_not_string".to_string());
    }
    if order_type == "ORDER_TYPE_LIMIT" && !limit_price_present {
        return Some("limit_price_missing_for_limit".to_string());
    }
    if order_type == "ORDER_TYPE_LIMIT" && !limit_price_value_decimal_string {
        return Some("limit_price_value_not_decimal_string".to_string());
    }
    if order_type == "ORDER_TYPE_MARKET" && limit_price_present {
        return Some("limit_price_forbidden_for_market".to_string());
    }
    None
}

fn decimal_value_field_ok(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_object)
        .and_then(|object| object.get("value"))
        .and_then(Value::as_str)
        .is_some_and(is_decimal_like_string)
}

fn is_decimal_like_string(value: &str) -> bool {
    if value.is_empty() || value.starts_with('-') || value.ends_with('.') {
        return false;
    }
    let mut dot_count = 0usize;
    let mut digit_count = 0usize;
    for character in value.chars() {
        if character == '.' {
            dot_count += 1;
            if dot_count > 1 {
                return false;
            }
        } else if character.is_ascii_digit() {
            digit_count += 1;
        } else {
            return false;
        }
    }
    digit_count > 0
}

fn is_finam_safe_client_order_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= broker_core::CLIENT_ORDER_ID_MAX_LEN
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn is_pinned_broker_symbol(value: &str) -> bool {
    let Some((symbol, market)) = value.split_once('@') else {
        return false;
    };
    !symbol.is_empty()
        && !market.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '@' | '.')
        })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("write to String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        AccountId, BrokerOrderId, CancelOrder, CancelPreflightApproval, ClientOrderId, Exchange,
        InstrumentId, Market, OperatorArm, OrderPathEvent, OrderPathRecord, OrderPathState,
        OrderPreflightContext, OrderPreflightPolicy, OrderReferencePrice, OrderSide, OrderType,
        PlaceOrder, StrategyRequestId, TimeInForce,
    };
    use broker_finam::{
        build_cancel_order_request, build_place_order_request,
        classify_order_endpoint_local_http_response_for_context, FinamOrderEndpointContext,
        FinamOrderEndpointLocalHttpResponse, FinamOrderEndpointMappedResult,
        FinamOrderEndpointResponseKind, FinamOrderExecutionOutcome,
    };
    use chrono::{DateTime, TimeZone, Utc};
    use rust_decimal::Decimal;
    use serde_json::{json, Value};
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use uuid::Uuid;

    fn run_local_mock_once(raw_request: String) -> (M3d2LocalMockWireDiagnostic, String) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local mock endpoint");
        let address = listener.local_addr().expect("local mock address");
        let captured_diagnostic = Arc::new(Mutex::new(None));
        let server_diagnostic = Arc::clone(&captured_diagnostic);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept local mock request");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set local mock read timeout");
            let mut raw = String::new();
            stream
                .read_to_string(&mut raw)
                .expect("read local mock request");
            let diagnostic = classify_m3d2_local_mock_wire_request(&raw);
            let response_body = if diagnostic.accepted_by_m3d2a_mock {
                r#"{"status":"accepted","broker_order_id":"BROKER_TEST_ORDER"}"#
            } else {
                r#"{"status":"rejected"}"#
            };
            let status_line = if diagnostic.accepted_by_m3d2a_mock {
                "HTTP/1.1 200 OK"
            } else {
                "HTTP/1.1 400 Bad Request"
            };
            let response = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{response_body}",
                response_body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write local mock response");
            *server_diagnostic
                .lock()
                .expect("lock local mock diagnostic") = Some(diagnostic);
        });

        let mut client = TcpStream::connect(address).expect("connect local mock endpoint");
        client
            .write_all(raw_request.as_bytes())
            .expect("write local mock request");
        client
            .shutdown(Shutdown::Write)
            .expect("half-close local mock request");
        let mut response = String::new();
        client
            .read_to_string(&mut response)
            .expect("read local mock response");
        server.join().expect("join local mock server");
        let diagnostic = captured_diagnostic
            .lock()
            .expect("lock captured local mock diagnostic")
            .clone()
            .expect("captured local mock diagnostic");
        (diagnostic, response)
    }

    fn request_id(n: u128) -> StrategyRequestId {
        StrategyRequestId::from(Uuid::from_u128(n))
    }

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "TESTFUT".to_string(),
            venue_symbol: Some("TESTFUT@TEST".to_string()),
            exchange: Exchange::Other("TEST".to_string()),
            market: Market::Futures,
        }
    }

    fn place_order(order_type: OrderType) -> PlaceOrder {
        let limit_price = if order_type == OrderType::Limit {
            Some(Decimal::new(10_050, 2))
        } else {
            None
        };
        PlaceOrder {
            request_id: request_id(10),
            created_ts: Utc
                .with_ymd_and_hms(2026, 7, 3, 9, 10, 0)
                .single()
                .expect("timestamp"),
            ttl_ms: Some(1_000),
            account_id: AccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::new("CID000000000000010").expect("client id"),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type,
            qty: Decimal::new(1, 0),
            limit_price,
            time_in_force: TimeInForce::Day,
            comment: None,
        }
    }

    fn sample_arm(now: DateTime<Utc>) -> OperatorArm {
        OperatorArm {
            session_id: "ARM_TEST_M3D2".to_string(),
            armed_until: now + chrono::Duration::minutes(5),
            endpoint_calls_enabled: true,
            one_shot: false,
            endpoint_attempted: false,
            preflight_digest: "digest-test".to_string(),
        }
    }

    fn preflight_policy(now: DateTime<Utc>) -> OrderPreflightPolicy {
        OrderPreflightPolicy {
            allowed_accounts: vec![AccountId::new("ACC_TEST_0001")],
            allowed_venue_symbols: vec!["TESTFUT@TEST".to_string()],
            allowed_order_types: vec![OrderType::Market, OrderType::Limit],
            allowed_time_in_force: vec![TimeInForce::Day],
            min_qty: Decimal::new(1, 0),
            qty_step: Decimal::new(1, 0),
            max_qty: Decimal::new(3, 0),
            price_step: Some(Decimal::new(1, 2)),
            max_market_qty: Decimal::new(1, 0),
            max_notional_per_order: None,
            max_notional_per_run: None,
            max_limit_deviation_bps: None,
            max_reference_age_ms: 1_000,
            allow_cancel_by_broker_order_id_without_mapping: false,
            operator_arm: sample_arm(now),
        }
    }

    fn approve_place(order: &PlaceOrder) -> broker_core::PreflightApprovedPlaceOrder {
        let now = order.created_ts + chrono::Duration::milliseconds(1);
        let context = OrderPreflightContext {
            reference_price: Some(OrderReferencePrice {
                price: order.limit_price.unwrap_or(Decimal::new(10_050, 2)),
                received_ts: now,
            }),
            current_run_notional: Decimal::ZERO,
        };
        preflight_policy(now)
            .approve_place_order_with_context(order, now, &context)
            .expect("place preflight approval")
    }

    fn approve_cancel(cancel: &CancelOrder) -> broker_core::PreflightApprovedCancelOrder {
        let now = cancel.created_ts + chrono::Duration::milliseconds(1);
        let mut existing = OrderPathRecord::from_place_order(
            &place_order(OrderType::Limit),
            cancel.created_ts,
            None,
        );
        existing.broker_order_id = Some(cancel.order_id.clone());
        existing
            .transition(OrderPathEvent::BeginSubmit, now)
            .expect("begin submit");
        existing
            .transition(OrderPathEvent::SubmitAccepted, now)
            .expect("submitted");
        assert_eq!(existing.state, OrderPathState::Submitted);
        match preflight_policy(now)
            .approve_cancel_order(cancel, now, Some(&existing))
            .expect("cancel preflight approval")
        {
            CancelPreflightApproval::Submit(approved) => approved,
            CancelPreflightApproval::AlreadyTerminal => panic!("expected submit approval"),
        }
    }

    fn place_limit_request() -> String {
        request_from_body(strict_limit_body(), true, Some("application/json"))
    }

    fn request_from_body(body: Value, authorization: bool, content_type: Option<&str>) -> String {
        let body = serde_json::to_string(&body).expect("body json");
        raw_request_from_body(
            "POST",
            "/v1/accounts/ACC_TEST_0001/orders",
            authorization,
            content_type,
            &body,
        )
    }

    fn raw_request_from_body(
        method: &str,
        path: &str,
        authorization: bool,
        content_type: Option<&str>,
        body: &str,
    ) -> String {
        let authorization_header = if authorization {
            "Authorization: Bearer REDACTED_TEST_TOKEN\r\n"
        } else {
            ""
        };
        let content_type_header = content_type
            .map(|value| format!("Content-Type: {value}\r\n"))
            .unwrap_or_default();
        format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{authorization_header}{content_type_header}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    fn strict_limit_body() -> Value {
        json!({
            "symbol": "TESTFUT@TEST",
            "quantity": { "value": "1" },
            "side": "SIDE_BUY",
            "type": "ORDER_TYPE_LIMIT",
            "time_in_force": "TIME_IN_FORCE_DAY",
            "limit_price": { "value": "100.5" },
            "client_order_id": "CID000000000000010"
        })
    }

    fn strict_market_body() -> Value {
        json!({
            "symbol": "TESTFUT@TEST",
            "quantity": { "value": "1" },
            "side": "SIDE_SELL",
            "type": "ORDER_TYPE_MARKET",
            "time_in_force": "TIME_IN_FORCE_DAY",
            "client_order_id": "CID000000000000011"
        })
    }

    fn assert_place_rejected(request: String, body_rejection: Option<&str>) {
        let (diagnostic, response) = run_local_mock_once(request);
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert_eq!(
            diagnostic.operation,
            M3d2LocalMockOrderOperation::PlaceOrder
        );
        assert!(!diagnostic.accepted_by_m3d2a_mock);
        if let Some(expected) = body_rejection {
            assert_eq!(
                diagnostic.body.strict_contract_rejection.as_deref(),
                Some(expected)
            );
        }
        assert!(!diagnostic.raw_path_exported);
        assert!(!diagnostic.raw_authorization_exported);
        assert!(!diagnostic.body.raw_body_exported);
    }

    #[test]
    fn local_mock_endpoint_accepts_exact_place_limit_finam_body() {
        let order = place_order(OrderType::Limit);
        let approved = approve_place(&order);
        let spec = build_place_order_request(&approved, None).expect("request spec");
        let diagnostic = classify_m3d2_local_mock_place_spec(&spec);

        assert_eq!(
            diagnostic.operation,
            M3d2LocalMockOrderOperation::PlaceOrder
        );
        assert_eq!(diagnostic.method_name, "POST");
        assert_eq!(
            diagnostic.route_template.as_deref(),
            Some(M3D2_PLACE_ORDER_ROUTE_TEMPLATE)
        );
        assert!(diagnostic.account_id_present);
        assert_eq!(diagnostic.account_id_len, Some("ACC_TEST_0001".len()));
        assert!(!diagnostic.order_id_present);
        assert!(diagnostic.authorization_present);
        assert_eq!(
            diagnostic.authorization_len,
            Some("Bearer REDACTED_LOCAL_MOCK_TOKEN".len())
        );
        assert!(diagnostic.content_type_json);
        assert_eq!(diagnostic.body.body_kind, M3d2LocalMockPlaceBodyKind::Limit);
        assert!(diagnostic.body.symbol_present);
        assert!(diagnostic.body.symbol_format_ok);
        assert!(diagnostic.body.side_present);
        assert!(diagnostic.body.side_official_finam_enum);
        assert!(diagnostic.body.quantity_present);
        assert!(diagnostic.body.quantity_value_decimal_string);
        assert!(diagnostic.body.time_in_force_plain_order_allowed);
        assert!(diagnostic.body.client_order_id_finam_safe);
        assert!(diagnostic.body.limit_price_present);
        assert!(diagnostic.body.limit_price_value_decimal_string);
        assert_eq!(diagnostic.body.strict_contract_rejection, None);
        assert!(diagnostic.accepted_by_m3d2a_mock);
        assert!(!format!("{diagnostic:?}").contains("REDACTED_LOCAL_MOCK_TOKEN"));
        let (wire_diagnostic, response) = run_local_mock_once(place_limit_request());
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(
            wire_diagnostic.body.body_kind,
            M3d2LocalMockPlaceBodyKind::Limit
        );
    }

    #[test]
    fn local_mock_endpoint_accepts_exact_place_market_finam_body() {
        let mut order = place_order(OrderType::Market);
        order.side = OrderSide::Sell;
        order.client_order_id = ClientOrderId::new("CID000000000000011").expect("client id");
        let approved = approve_place(&order);
        let spec = build_place_order_request(&approved, None).expect("request spec");
        let diagnostic = classify_m3d2_local_mock_place_spec(&spec);

        assert_eq!(
            diagnostic.operation,
            M3d2LocalMockOrderOperation::PlaceOrder
        );
        assert_eq!(diagnostic.method_name, "POST");
        assert_eq!(
            diagnostic.body.body_kind,
            M3d2LocalMockPlaceBodyKind::Market
        );
        assert!(diagnostic.body.symbol_format_ok);
        assert!(diagnostic.body.quantity_value_decimal_string);
        assert!(diagnostic.body.side_official_finam_enum);
        assert!(diagnostic.body.order_type_official_finam_enum);
        assert!(diagnostic.body.time_in_force_plain_order_allowed);
        assert!(diagnostic.body.client_order_id_finam_safe);
        assert!(!diagnostic.body.limit_price_present);
        assert!(diagnostic.accepted_by_m3d2a_mock);
        let (wire_diagnostic, response) = run_local_mock_once(request_from_body(
            strict_market_body(),
            true,
            Some("application/json"),
        ));
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(
            wire_diagnostic.body.body_kind,
            M3d2LocalMockPlaceBodyKind::Market
        );
    }

    #[test]
    fn local_mock_endpoint_accepts_exact_cancel_wire_request() {
        let cancel = CancelOrder {
            request_id: request_id(11),
            created_ts: Utc
                .with_ymd_and_hms(2026, 7, 3, 9, 11, 0)
                .single()
                .expect("timestamp"),
            ttl_ms: Some(1_000),
            account_id: AccountId::new("ACC_TEST_0001"),
            order_id: BrokerOrderId::new("BROKER_TEST_ORDER"),
            client_order_id: Some(ClientOrderId::new("CID000000000000010").expect("client id")),
        };
        let approved = approve_cancel(&cancel);
        let spec = build_cancel_order_request(&approved).expect("cancel spec");
        let spec_diagnostic = classify_m3d2_local_mock_cancel_spec(&spec);
        assert!(spec_diagnostic.accepted_by_m3d2a_mock);

        let request = "DELETE /v1/accounts/ACC_TEST_0001/orders/BROKER_TEST_ORDER HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer REDACTED_TEST_TOKEN\r\nContent-Length: 0\r\n\r\n".to_string();
        let (diagnostic, response) = run_local_mock_once(request);

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(
            diagnostic.operation,
            M3d2LocalMockOrderOperation::CancelOrder
        );
        assert_eq!(diagnostic.method_name, "DELETE");
        assert_eq!(
            diagnostic.route_template.as_deref(),
            Some(M3D2_CANCEL_ORDER_ROUTE_TEMPLATE)
        );
        assert_eq!(diagnostic.account_id_len, Some("ACC_TEST_0001".len()));
        assert_eq!(diagnostic.order_id_len, Some("BROKER_TEST_ORDER".len()));
        assert!(diagnostic.authorization_present);
        assert!(diagnostic.cancel_body_empty);
        assert!(diagnostic.accepted_by_m3d2a_mock);
        assert!(!format!("{diagnostic:?}").contains("BROKER_TEST_ORDER"));
        assert!(!format!("{diagnostic:?}").contains("REDACTED_TEST_TOKEN"));
    }

    #[test]
    fn local_mock_endpoint_rejects_wrong_method_without_raw_secret_export() {
        let request = place_limit_request().replacen("POST ", "GET ", 1);
        let (diagnostic, response) = run_local_mock_once(request);

        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert_eq!(
            diagnostic.operation,
            M3d2LocalMockOrderOperation::PlaceOrder
        );
        assert_eq!(diagnostic.method_name, "GET");
        assert!(!diagnostic.accepted_by_m3d2a_mock);
        assert_eq!(
            diagnostic.rejection_reason.as_deref(),
            Some("place_method_not_post")
        );
        assert!(!diagnostic.raw_path_exported);
        assert!(!diagnostic.raw_authorization_exported);
        assert!(!diagnostic.body.raw_body_exported);
        assert!(!format!("{diagnostic:?}").contains("REDACTED_TEST_TOKEN"));
    }

    #[test]
    fn local_mock_endpoint_rejects_missing_client_order_id() {
        let mut body = strict_limit_body();
        body.as_object_mut()
            .expect("object")
            .remove("client_order_id");
        assert_place_rejected(
            request_from_body(body, true, Some("application/json")),
            Some("client_order_id_missing"),
        );
    }

    #[test]
    fn local_mock_endpoint_rejects_missing_time_in_force() {
        let mut body = strict_limit_body();
        body.as_object_mut()
            .expect("object")
            .remove("time_in_force");
        assert_place_rejected(
            request_from_body(body, true, Some("application/json")),
            Some("time_in_force_missing"),
        );
    }

    #[test]
    fn local_mock_endpoint_rejects_side_buy_alias_instead_of_finam_enum() {
        let mut body = strict_limit_body();
        body["side"] = json!("BUY");
        assert_place_rejected(
            request_from_body(body, true, Some("application/json")),
            Some("side_not_official_finam_enum"),
        );
    }

    #[test]
    fn local_mock_endpoint_rejects_order_type_limit_alias_instead_of_finam_enum() {
        let mut body = strict_limit_body();
        body["type"] = json!("LIMIT");
        assert_place_rejected(
            request_from_body(body, true, Some("application/json")),
            Some("order_type_not_official_finam_enum"),
        );
    }

    #[test]
    fn local_mock_endpoint_rejects_market_with_limit_price() {
        let mut body = strict_market_body();
        body["limit_price"] = json!({ "value": "100.5" });
        assert_place_rejected(
            request_from_body(body, true, Some("application/json")),
            Some("limit_price_forbidden_for_market"),
        );
    }

    #[test]
    fn local_mock_endpoint_rejects_limit_without_limit_price() {
        let mut body = strict_limit_body();
        body.as_object_mut().expect("object").remove("limit_price");
        assert_place_rejected(
            request_from_body(body, true, Some("application/json")),
            Some("limit_price_missing_for_limit"),
        );
    }

    #[test]
    fn local_mock_endpoint_rejects_stop_price_legs_and_sltp_fields() {
        for forbidden_field in ["stop_price", "legs", "sltp", "valid_before"] {
            let mut body = strict_limit_body();
            body[forbidden_field] = json!("forbidden");
            assert_place_rejected(
                request_from_body(body, true, Some("application/json")),
                Some("plain_order_field_not_allowed"),
            );
        }
    }

    #[test]
    fn local_mock_endpoint_rejects_wrong_route_extra_segment() {
        let request = raw_request_from_body(
            "POST",
            "/v1/accounts/ACC_TEST_0001/orders/EXTRA",
            true,
            Some("application/json"),
            &serde_json::to_string(&strict_limit_body()).expect("body"),
        );
        let (diagnostic, response) = run_local_mock_once(request);
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert_eq!(
            diagnostic.operation,
            M3d2LocalMockOrderOperation::CancelOrder
        );
        assert_eq!(
            diagnostic.rejection_reason.as_deref(),
            Some("cancel_method_not_delete")
        );
        assert!(!diagnostic.raw_path_exported);
    }

    #[test]
    fn local_mock_endpoint_rejects_missing_authorization() {
        assert_place_rejected(
            request_from_body(strict_limit_body(), false, Some("application/json")),
            None,
        );
    }

    #[test]
    fn local_mock_endpoint_rejects_wrong_content_type() {
        assert_place_rejected(
            request_from_body(strict_limit_body(), true, Some("text/plain")),
            None,
        );
    }

    #[test]
    fn local_mock_endpoint_rejects_non_json_body() {
        assert_place_rejected(
            raw_request_from_body(
                "POST",
                "/v1/accounts/ACC_TEST_0001/orders",
                true,
                Some("application/json"),
                "not-json",
            ),
            None,
        );
    }

    #[test]
    fn local_mock_response_matrix_covers_required_endpoint_outcomes() {
        let classify = |context, response| {
            classify_order_endpoint_local_http_response_for_context(context, &response)
        };

        let accepted = classify(
            FinamOrderEndpointContext::Place,
            FinamOrderEndpointLocalHttpResponse::Response {
                status: 200,
                body: r#"{"broker_order_id":"BROKER_TEST_ORDER"}"#.to_string(),
                retry_after_ms: None,
            },
        );
        assert_eq!(
            accepted.diagnostic.kind,
            FinamOrderEndpointResponseKind::Accepted
        );
        assert!(accepted.diagnostic.broker_order_id_present);
        assert!(matches!(
            accepted.result,
            FinamOrderEndpointMappedResult::Execution(FinamOrderExecutionOutcome::Accepted { .. })
        ));

        let accepted_without_id = classify(
            FinamOrderEndpointContext::Place,
            FinamOrderEndpointLocalHttpResponse::Response {
                status: 202,
                body: "{}".to_string(),
                retry_after_ms: None,
            },
        );
        assert_eq!(
            accepted_without_id.diagnostic.kind,
            FinamOrderEndpointResponseKind::Accepted
        );
        assert!(!accepted_without_id.diagnostic.broker_order_id_present);

        let malformed_2xx = classify(
            FinamOrderEndpointContext::Place,
            FinamOrderEndpointLocalHttpResponse::Response {
                status: 200,
                body: "not-json".to_string(),
                retry_after_ms: None,
            },
        );
        assert_eq!(
            malformed_2xx.diagnostic.kind,
            FinamOrderEndpointResponseKind::DecodeError
        );

        let rejected_400 = classify(
            FinamOrderEndpointContext::Place,
            FinamOrderEndpointLocalHttpResponse::Response {
                status: 400,
                body: r#"{"error":"bad_request"}"#.to_string(),
                retry_after_ms: None,
            },
        );
        assert_eq!(
            rejected_400.diagnostic.kind,
            FinamOrderEndpointResponseKind::Rejected
        );

        let unauthorized = classify(
            FinamOrderEndpointContext::Place,
            FinamOrderEndpointLocalHttpResponse::Response {
                status: 401,
                body: r#"{"error":"unauthorized"}"#.to_string(),
                retry_after_ms: None,
            },
        );
        assert_eq!(
            unauthorized.diagnostic.kind,
            FinamOrderEndpointResponseKind::Unauthorized
        );

        for status in [404, 409, 410] {
            let reconciliation = classify(
                FinamOrderEndpointContext::Cancel,
                FinamOrderEndpointLocalHttpResponse::Response {
                    status,
                    body: r#"{"error":"requires_reconciliation"}"#.to_string(),
                    retry_after_ms: None,
                },
            );
            assert_eq!(
                reconciliation.diagnostic.kind,
                FinamOrderEndpointResponseKind::ReconciliationRequired
            );
        }

        let rate_limited = classify(
            FinamOrderEndpointContext::Place,
            FinamOrderEndpointLocalHttpResponse::Response {
                status: 429,
                body: r#"{"error":"rate_limit"}"#.to_string(),
                retry_after_ms: Some(1_000),
            },
        );
        assert_eq!(
            rate_limited.diagnostic.kind,
            FinamOrderEndpointResponseKind::RateLimited
        );
        assert!(rate_limited.diagnostic.retry_after_ms_present);

        for status in [500, 503] {
            let maintenance = classify(
                FinamOrderEndpointContext::Place,
                FinamOrderEndpointLocalHttpResponse::Response {
                    status,
                    body: r#"{"error":"maintenance"}"#.to_string(),
                    retry_after_ms: None,
                },
            );
            assert_eq!(
                maintenance.diagnostic.kind,
                FinamOrderEndpointResponseKind::Maintenance
            );
        }

        for response in [
            FinamOrderEndpointLocalHttpResponse::Response {
                status: 504,
                body: r#"{"error":"deadline"}"#.to_string(),
                retry_after_ms: None,
            },
            FinamOrderEndpointLocalHttpResponse::Timeout,
        ] {
            let timeout = classify(FinamOrderEndpointContext::Place, response);
            assert_eq!(
                timeout.diagnostic.kind,
                FinamOrderEndpointResponseKind::Timeout
            );
        }

        let closed_connection = classify(
            FinamOrderEndpointContext::Place,
            FinamOrderEndpointLocalHttpResponse::BodyReadFailed { status: Some(200) },
        );
        assert_eq!(
            closed_connection.diagnostic.kind,
            FinamOrderEndpointResponseKind::DecodeError
        );
    }
}
