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
    pub side_present: bool,
    pub quantity_present: bool,
    pub order_type_present: bool,
    pub time_in_force_present: bool,
    pub client_order_id_present: bool,
    pub limit_price_present: bool,
    pub raw_body_exported: bool,
}

impl Default for M3d2LocalMockPlaceBodyDiagnostic {
    fn default() -> Self {
        Self {
            body_kind: M3d2LocalMockPlaceBodyKind::NotPlaceBody,
            json_object: false,
            symbol_present: false,
            side_present: false,
            quantity_present: false,
            order_type_present: false,
            time_in_force_present: false,
            client_order_id_present: false,
            limit_price_present: false,
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
    let side_present = object.contains_key("side");
    let quantity_present = object.contains_key("quantity");
    let order_type_present = object.contains_key("order_type") || object.contains_key("type");
    let time_in_force_present = object.contains_key("time_in_force");
    let client_order_id_present = object.contains_key("client_order_id");
    let limit_price_present = object.contains_key("limit_price");
    let order_type = object
        .get("order_type")
        .or_else(|| object.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_uppercase();
    let core_fields_present =
        symbol_present && side_present && quantity_present && order_type_present;
    let body_kind = if !core_fields_present {
        M3d2LocalMockPlaceBodyKind::Unknown
    } else if order_type.contains("LIMIT") && limit_price_present {
        M3d2LocalMockPlaceBodyKind::Limit
    } else if order_type.contains("MARKET") && !limit_price_present {
        M3d2LocalMockPlaceBodyKind::Market
    } else {
        M3d2LocalMockPlaceBodyKind::Unknown
    };

    M3d2LocalMockPlaceBodyDiagnostic {
        body_kind,
        json_object: true,
        symbol_present,
        side_present,
        quantity_present,
        order_type_present,
        time_in_force_present,
        client_order_id_present,
        limit_price_present,
        raw_body_exported: false,
    }
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
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

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

    fn place_limit_request() -> String {
        let body = r#"{"symbol":"IMOEXF","side":"BUY","quantity":{"value":"1"},"order_type":"LIMIT","limit_price":{"value":"3000.0"},"time_in_force":"TIME_IN_FORCE_DAY","client_order_id":"CID_TEST_0001"}"#;
        format!(
            "POST /v1/accounts/ACC_TEST_0001/orders HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer REDACTED_TEST_TOKEN\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn local_mock_endpoint_accepts_exact_place_limit_wire_request() {
        let (diagnostic, response) = run_local_mock_once(place_limit_request());

        assert!(response.starts_with("HTTP/1.1 200 OK"));
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
            Some("Bearer REDACTED_TEST_TOKEN".len())
        );
        assert!(diagnostic.content_type_json);
        assert_eq!(diagnostic.body.body_kind, M3d2LocalMockPlaceBodyKind::Limit);
        assert!(diagnostic.body.symbol_present);
        assert!(diagnostic.body.side_present);
        assert!(diagnostic.body.quantity_present);
        assert!(diagnostic.body.limit_price_present);
        assert!(diagnostic.accepted_by_m3d2a_mock);
        assert!(!format!("{diagnostic:?}").contains("REDACTED_TEST_TOKEN"));
    }

    #[test]
    fn local_mock_endpoint_accepts_exact_cancel_wire_request() {
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
}
