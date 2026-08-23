//! Test-only controlled rustls qualification server for the exact Stage 8B adapter.
//!
//! This module owns no client, request builder, FINAM endpoint, credential or
//! production authority.  It binds an ephemeral loopback listener, presents a
//! locally generated CA-signed certificate and records one HTTP/2 request.

use super::stage8b_adapter::TLS_QUALIFICATION_HOST;
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use hyper::{body::Incoming, service::service_fn, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rcgen::{
    date_time_ymd, BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::{
    pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer},
    ServerConfig,
};
use std::{convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, sync::Mutex};
use tokio_rustls::TlsAcceptor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage8bTlsCertificateProfile {
    Valid,
    WrongHostname,
    Expired,
    NotYetValid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage8bTlsServerBehavior {
    ServiceUnavailable,
    ResponseLost,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stage8bTlsCapturedRequest {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) authorization_present: bool,
    pub(super) body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stage8bTlsServerResult {
    pub(super) handshake_completed: bool,
    pub(super) negotiated_alpn: Option<Vec<u8>>,
    pub(super) request: Option<Stage8bTlsCapturedRequest>,
}

pub(super) struct Stage8bControlledTlsServer {
    pub(super) address: SocketAddr,
    pub(super) root_certificate_der: Vec<u8>,
    pub(super) task: tokio::task::JoinHandle<Stage8bTlsServerResult>,
}

pub(super) async fn spawn_controlled_tls_server(
    profile: Stage8bTlsCertificateProfile,
    behavior: Stage8bTlsServerBehavior,
) -> Stage8bControlledTlsServer {
    let (root_certificate_der, server_config) = issue_server_configuration(profile);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("controlled TLS listener must bind loopback");
    let address = listener
        .local_addr()
        .expect("controlled TLS listener address must exist");
    let task = tokio::spawn(async move {
        let (socket, _) = listener
            .accept()
            .await
            .expect("exact TLS client must connect once");
        let tls = match TlsAcceptor::from(Arc::new(server_config))
            .accept(socket)
            .await
        {
            Ok(tls) => tls,
            Err(_) => {
                return Stage8bTlsServerResult {
                    handshake_completed: false,
                    negotiated_alpn: None,
                    request: None,
                };
            }
        };
        let negotiated_alpn = tls.get_ref().1.alpn_protocol().map(ToOwned::to_owned);
        match behavior {
            Stage8bTlsServerBehavior::ResponseLost => Stage8bTlsServerResult {
                handshake_completed: true,
                negotiated_alpn,
                request: None,
            },
            Stage8bTlsServerBehavior::Timeout => {
                tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                Stage8bTlsServerResult {
                    handshake_completed: true,
                    negotiated_alpn,
                    request: None,
                }
            }
            Stage8bTlsServerBehavior::ServiceUnavailable => {
                let captured = Arc::new(Mutex::new(None));
                let captured_for_service = Arc::clone(&captured);
                let service = service_fn(move |request: Request<Incoming>| {
                    let captured = Arc::clone(&captured_for_service);
                    async move {
                        let (parts, body) = request.into_parts();
                        let body = body
                            .collect()
                            .await
                            .expect("controlled request body must be readable")
                            .to_bytes()
                            .to_vec();
                        *captured.lock().await = Some(Stage8bTlsCapturedRequest {
                            method: parts.method.to_string(),
                            path: parts.uri.path().to_string(),
                            authorization_present: parts.headers.contains_key("authorization"),
                            body,
                        });
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(StatusCode::SERVICE_UNAVAILABLE)
                                .header("content-type", "application/json")
                                .body(Full::new(Bytes::from_static(b"{}")))
                                .expect("controlled response must build"),
                        )
                    }
                });
                let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(tls), service)
                    .await;
                let request = captured.lock().await.take();
                Stage8bTlsServerResult {
                    handshake_completed: true,
                    negotiated_alpn,
                    request,
                }
            }
        }
    });
    Stage8bControlledTlsServer {
        address,
        root_certificate_der,
        task,
    }
}

pub(super) fn unrelated_root_certificate_der() -> Vec<u8> {
    issue_server_configuration(Stage8bTlsCertificateProfile::Valid).0
}

fn issue_server_configuration(profile: Stage8bTlsCertificateProfile) -> (Vec<u8>, ServerConfig) {
    let mut ca_params = CertificateParams::new(Vec::new()).expect("empty CA SAN list is valid");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Stage 8B controlled qualification CA");
    ca_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    ca_params.not_before = date_time_ymd(2020, 1, 1);
    ca_params.not_after = date_time_ymd(2040, 1, 1);
    let ca_key = KeyPair::generate().expect("controlled CA key generation must succeed");
    let ca_certificate = ca_params
        .self_signed(&ca_key)
        .expect("controlled CA certificate must build");
    let root_certificate_der = ca_certificate.der().to_vec();
    let issuer = Issuer::new(ca_params, ca_key);

    let dns_name = match profile {
        Stage8bTlsCertificateProfile::WrongHostname => "wrong-stage8b.invalid",
        _ => TLS_QUALIFICATION_HOST,
    };
    let mut server_params = CertificateParams::new(vec![dns_name.to_string()])
        .expect("controlled DNS name must be valid");
    server_params
        .distinguished_name
        .push(DnType::CommonName, dns_name);
    server_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    match profile {
        Stage8bTlsCertificateProfile::Expired => {
            server_params.not_before = date_time_ymd(2020, 1, 1);
            server_params.not_after = date_time_ymd(2021, 1, 1);
        }
        Stage8bTlsCertificateProfile::NotYetValid => {
            server_params.not_before = date_time_ymd(2035, 1, 1);
            server_params.not_after = date_time_ymd(2036, 1, 1);
        }
        Stage8bTlsCertificateProfile::Valid | Stage8bTlsCertificateProfile::WrongHostname => {
            server_params.not_before = date_time_ymd(2020, 1, 1);
            server_params.not_after = date_time_ymd(2035, 1, 1);
        }
    }
    let server_key = KeyPair::generate().expect("controlled server key generation must succeed");
    let server_certificate = server_params
        .signed_by(&server_key, &issuer)
        .expect("controlled server certificate must build");
    let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(server_key.serialize_der()));
    let mut server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![server_certificate.der().clone()], private_key)
        .expect("controlled rustls server config must build");
    server_config.alpn_protocols = vec![b"h2".to_vec()];
    (root_certificate_der, server_config)
}
