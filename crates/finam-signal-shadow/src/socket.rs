//! Fixed-endpoint socket owner. No tasks, order channels, Redis or credential discovery.
use crate::{
    transport::{ReadOnlyCollector, TransportError},
    ws::{BarsProtocol, Connection, Observation},
};
use broker_finam::AccessTokenLease;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig, Message,
    },
    MaybeTlsStream, WebSocketStream,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum SocketStage {
    Lease,
    Connect,
    Receive,
    ProtocolReceive,
    SubscriptionBuild,
    SubscriptionSend,
    SubscriptionCommit,
    PongFlush,
    Silence,
    Reconnect,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum SocketCode {
    Lease,
    Request,
    Connect,
    Eof,
    Read,
    Close,
    UnexpectedMessage,
    Protocol(crate::ws::WsError),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SocketFailure {
    pub stage: SocketStage,
    pub code: SocketCode,
}
fn failure(stage: SocketStage, code: SocketCode) -> TransportError {
    TransportError::Socket(SocketFailure { stage, code })
}
fn protocol_failure(stage: SocketStage, e: crate::ws::WsError) -> TransportError {
    failure(stage, SocketCode::Protocol(e))
}
// Drop every dynamic tungstenite payload; even error Debug/Display is forbidden.
fn incoming(
    frame: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
) -> Result<Message, TransportError> {
    match frame {
        None => Err(failure(SocketStage::Receive, SocketCode::Eof)),
        Some(Err(_)) => Err(failure(SocketStage::Receive, SocketCode::Read)),
        Some(Ok(Message::Close(_))) => Err(failure(SocketStage::Receive, SocketCode::Close)),
        Some(Ok(m)) => Ok(m),
    }
}
const ENDPOINT: &str = "wss://api.finam.ru/ws";

fn config() -> WebSocketConfig {
    WebSocketConfig {
        write_buffer_size: 0,
        max_write_buffer_size: 512 * 1024,
        max_message_size: Some(256 * 1024),
        max_frame_size: Some(256 * 1024),
        ..WebSocketConfig::default()
    }
}

fn lease_valid(lease: &AccessTokenLease) -> Result<(), TransportError> {
    if lease.token().is_empty() || lease.should_refresh(std::time::Instant::now()) {
        return Err(failure(SocketStage::Lease, SocketCode::Lease));
    }
    Ok(())
}

async fn open(
    lease: &AccessTokenLease,
    budget: &mut ReadOnlyCollector,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, TransportError> {
    lease_valid(lease)?;
    let mut request = ENDPOINT
        .into_client_request()
        .map_err(|_| failure(SocketStage::Connect, SocketCode::Request))?;
    let mut token = HeaderValue::from_str(lease.token().as_str())
        .map_err(|_| failure(SocketStage::Connect, SocketCode::Request))?;
    token.set_sensitive(true);
    request.headers_mut().insert("Authorization", token);
    // Never expose tungstenite/request errors; they can contain secrets.
    budget
        .bounded(async {
            connect_async_with_config(request, Some(config()), true)
                .await
                .map(|(socket, _)| socket)
                .map_err(|_| failure(SocketStage::Connect, SocketCode::Connect))
        })
        .await
}

struct Session<S> {
    socket: Option<WebSocketStream<S>>,
    protocol: BarsProtocol,
    connection: Connection,
    lease: AccessTokenLease,
    budget: ReadOnlyCollector,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Session<S> {
    async fn observe(&mut self) -> Result<Observation, TransportError> {
        self.observe_until(None).await?.ok_or(TransportError::Fetch)
    }

    async fn observe_until(
        &mut self,
        rotate_at: Option<std::time::Instant>,
    ) -> Result<Option<Observation>, TransportError> {
        // The local owner drops the socket on any error OR cancellation.
        let mut socket = self.socket.take().ok_or(TransportError::Held)?;
        lease_valid(&self.lease)?;
        let remaining_silence = self
            .protocol
            .remaining_silence(Utc::now())
            .map_err(|e| protocol_failure(SocketStage::Silence, e))?;
        let protocol = &mut self.protocol;
        let connection = &self.connection;
        let lease = &self.lease;
        let observation = self
            .budget
            .bounded_wait(remaining_silence, async {
                // Rotation interrupts only the cancel-safe Stream::next, not
                // subscription send, state mutation or the owning budget future.
                let rotation = async {
                    if let Some(at) = rotate_at {
                        tokio::time::sleep_until(at.into()).await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                };
                let frame = incoming(tokio::select! {
                    biased;
                    _ = rotation => return Ok(None),
                    frame = socket.next() => frame,
                })?;
                lease_valid(lease)?;
                match frame {
                    Message::Text(text) => {
                        let observation = protocol
                            .receive(connection, &text, Utc::now())
                            .map_err(|e| protocol_failure(SocketStage::ProtocolReceive, e))?;
                        if matches!(observation, Observation::Handshake) {
                            let request = protocol
                                .subscription_request(connection, lease.token())
                                .map_err(|e| protocol_failure(SocketStage::SubscriptionBuild, e))?;
                            socket
                                .send(Message::Text(request.to_string()))
                                .await
                                .map_err(|_| {
                                    failure(SocketStage::SubscriptionSend, SocketCode::Read)
                                })?;
                            protocol.subscription_sent(connection).map_err(|e| {
                                protocol_failure(SocketStage::SubscriptionCommit, e)
                            })?;
                        }
                        Ok(Some(observation))
                    }
                    Message::Ping(_) => {
                        // tungstenite queues the matching pong; flush under the same bound.
                        socket
                            .flush()
                            .await
                            .map_err(|_| failure(SocketStage::PongFlush, SocketCode::Read))?;
                        protocol
                            .check_silence(Utc::now())
                            .map_err(|e| protocol_failure(SocketStage::Silence, e))?;
                        Ok(Some(Observation::Ignore))
                    }
                    Message::Pong(_) => {
                        protocol
                            .check_silence(Utc::now())
                            .map_err(|e| protocol_failure(SocketStage::Silence, e))?;
                        Ok(Some(Observation::Ignore))
                    }
                    _ => Err(failure(SocketStage::Receive, SocketCode::UnexpectedMessage)),
                }
            })
            .await?;
        lease_valid(&self.lease)?;
        self.socket = Some(socket);
        Ok(observation)
    }
}

pub struct BarsSocket(Session<MaybeTlsStream<TcpStream>>);

impl BarsSocket {
    /// Caller retains the StopHandle returned when constructing budget.
    /// A successful connect is NOT a subscription or live readiness.
    pub async fn connect(
        lease: AccessTokenLease,
        budget: ReadOnlyCollector,
        silence_ms: u64,
        reconnect_ms: u64,
    ) -> Result<Self, TransportError> {
        let protocol =
            BarsProtocol::new(silence_ms, reconnect_ms).map_err(|_| TransportError::Bounds)?;
        Self::connect_protocol(lease, budget, protocol).await
    }

    async fn connect_protocol(
        lease: AccessTokenLease,
        mut budget: ReadOnlyCollector,
        mut protocol: BarsProtocol,
    ) -> Result<Self, TransportError> {
        let connection = protocol
            .connected(Utc::now())
            .map_err(|e| protocol_failure(SocketStage::Connect, e))?;
        let socket = open(&lease, &mut budget).await?;
        protocol
            .check_silence(Utc::now())
            .map_err(|e| protocol_failure(SocketStage::Silence, e))?;
        Ok(Self(Session {
            socket: Some(socket),
            protocol,
            connection,
            lease,
            budget,
        }))
    }

    pub async fn observe(&mut self) -> Result<Observation, TransportError> {
        self.0.observe().await
    }

    pub(crate) async fn observe_until(
        &mut self,
        at: std::time::Instant,
    ) -> Result<Option<Observation>, TransportError> {
        self.0.observe_until(Some(at)).await
    }

    /// Explicit, consuming reconnect. Drop old transport before starting another.
    /// Faulted/cancelled sessions cannot heal through this method.
    pub async fn reconnect(self, lease: AccessTokenLease) -> Result<Self, TransportError> {
        self.reconnect_kind(lease, false).await
    }

    pub(crate) async fn rotate_auth(self, lease: AccessTokenLease) -> Result<Self, TransportError> {
        self.reconnect_kind(lease, true).await
    }

    async fn reconnect_kind(
        self,
        lease: AccessTokenLease,
        auth: bool,
    ) -> Result<Self, TransportError> {
        let Session {
            socket,
            mut protocol,
            connection,
            mut budget,
            ..
        } = self.0;
        let socket = socket.ok_or(TransportError::Held)?;
        drop(socket);
        protocol
            .disconnected(&connection, Utc::now())
            .map_err(|e| protocol_failure(SocketStage::Reconnect, e))?;
        let connection = if auth {
            protocol.connected_for_auth(Utc::now())
        } else {
            protocol.connected(Utc::now())
        }
        .map_err(|e| protocol_failure(SocketStage::Reconnect, e))?;
        let socket = open(&lease, &mut budget).await?;
        protocol
            .check_silence(Utc::now())
            .map_err(|e| protocol_failure(SocketStage::Silence, e))?;
        Ok(Self(Session {
            socket: Some(socket),
            protocol,
            connection,
            lease,
            budget,
        }))
    }

    /// Immediate local transport drop, not a graceful peer-close guarantee.
    pub fn stop(mut self) {
        self.0.protocol.stop();
        self.0.socket.take();
    }
}

/// Same transport owner, dedicated immutable QUOTES channel.
pub struct QuotesSocket(BarsSocket);

impl QuotesSocket {
    pub(crate) async fn observe_until(
        &mut self,
        at: std::time::Instant,
    ) -> Result<Option<Observation>, TransportError> {
        self.0.observe_until(at).await
    }
    pub(crate) async fn rotate_auth(self, lease: AccessTokenLease) -> Result<Self, TransportError> {
        Ok(Self(self.0.rotate_auth(lease).await?))
    }
    pub async fn connect(
        lease: AccessTokenLease,
        budget: ReadOnlyCollector,
        silence_ms: u64,
        reconnect_ms: u64,
        quote_age_ms: u64,
        capacity: usize,
    ) -> Result<Self, TransportError> {
        let protocol = BarsProtocol::new_quotes(silence_ms, reconnect_ms, quote_age_ms, capacity)
            .map_err(|_| TransportError::Bounds)?;
        Ok(Self(
            BarsSocket::connect_protocol(lease, budget, protocol).await?,
        ))
    }
    pub async fn observe(&mut self) -> Result<Observation, TransportError> {
        self.0.observe().await
    }
    pub async fn reconnect(self, lease: AccessTokenLease) -> Result<Self, TransportError> {
        Ok(Self(self.0.reconnect(lease).await?))
    }
    pub fn stop(self) {
        self.0.stop();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::transport::StopHandle;
    use broker_finam::{AccessToken, FinamAuthPolicy};
    use std::time::Duration;
    use tokio::io::DuplexStream;
    use tokio_tungstenite::tungstenite::protocol::Role;

    #[tokio::test]
    async fn rotation_timer_is_not_a_cancelled_or_held_observe() {
        let (_stop, mut client, mut server) = pair().await;
        assert!(client
            .observe_until(Some(std::time::Instant::now()))
            .await
            .unwrap()
            .is_none());
        assert!(client.socket.is_some());
        server
            .send(Message::Text(
                r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
            ))
            .await
            .unwrap();
        assert!(matches!(
            client.observe().await.unwrap(),
            Observation::Handshake
        ));
        assert!(server.next().await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn auth_reconnect_sends_new_token_and_preserves_quote_dedup() {
        let (_stop, mut client, mut server) =
            pair_with_protocol(BarsProtocol::new_quotes(5000, 1000, 5000, 32).unwrap()).await;
        let handshake = r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#;
        server.send(Message::Text(handshake.into())).await.unwrap();
        client.observe().await.unwrap();
        server.next().await.unwrap().unwrap();
        let data = serde_json::json!({"type":"DATA","subscriptionType":"QUOTES","subscriptionKey":"old",
            "payload":{"quote":[{"symbol":"IMOEXF@RTSX","timestamp":Utc::now().to_rfc3339(),"last":{"value":"2200.5"}}]}}).to_string();
        server.send(Message::Text(data.clone())).await.unwrap();
        let Observation::Quotes(q) = client.observe().await.unwrap() else {
            panic!("quotes")
        };
        assert_eq!(q.len(), 1);
        client.socket.take();
        drop(server);
        client
            .protocol
            .disconnected(&client.connection, Utc::now())
            .unwrap();
        client.connection = client.protocol.connected_for_auth(Utc::now()).unwrap();
        let (a, b) = tokio::io::duplex(16384);
        client.socket =
            Some(WebSocketStream::from_raw_socket(a, Role::Client, Some(config())).await);
        let mut server = WebSocketStream::from_raw_socket(b, Role::Server, Some(config())).await;
        client.lease = AccessTokenLease::new(
            AccessToken::new("rotated-fixture"),
            std::time::Instant::now(),
            FinamAuthPolicy::default(),
        );
        server.send(Message::Text(handshake.into())).await.unwrap();
        client.observe().await.unwrap();
        let Message::Text(wire) = server.next().await.unwrap().unwrap() else {
            panic!("subscription")
        };
        let wire: serde_json::Value = serde_json::from_str(&wire).unwrap();
        assert_eq!(wire["token"], "rotated-fixture");
        server
            .send(Message::Text(data.replace("\"old\"", "\"new\"")))
            .await
            .unwrap();
        let Observation::Quotes(q) = client.observe().await.unwrap() else {
            panic!("quotes")
        };
        assert!(q.is_empty());
    }

    pub(crate) async fn protocol_error_fixture(quotes: bool) -> TransportError {
        error_fixture(quotes, 0).await
    }
    pub(crate) async fn error_fixture(quotes: bool, case: u8) -> TransportError {
        let protocol = if quotes {
            BarsProtocol::new_quotes(5000, 1000, 5000, 32).unwrap()
        } else {
            BarsProtocol::new(5000, 1000).unwrap()
        };
        let (stop, mut client, mut server) = pair_with_protocol(protocol).await;
        if case == 3 {
            stop.stop();
            return client.observe().await.unwrap_err();
        }
        if case == 2 {
            server
                .send(Message::Text(
                    r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
                ))
                .await
                .unwrap();
            client.observe().await.unwrap();
            server.next().await.unwrap().unwrap();
            let payload = if quotes {
                serde_json::json!({"quote":[{"symbol":"IMOEXF@RTSX",
                    "timestamp":Utc::now().to_rfc3339(),"last":{"value":"2200.25"}}]})
            } else {
                serde_json::json!({"symbol":"SECRET_RAW_ACCOUNT_TOKEN","bars":[]})
            };
            server
                .send(Message::Text(
                    serde_json::json!({
                        "type":"DATA", "subscriptionType":if quotes {"QUOTES"} else {"BARS"},
                        "subscriptionKey":"SECRET_RAW_ACCOUNT_TOKEN", "payload":payload
                    })
                    .to_string(),
                ))
                .await
                .unwrap();
            return client.observe().await.unwrap_err();
        }
        if case == 1 {
            server
                .send(Message::Text(
                    r#"{"type":"ERROR","errorInfo":{"message":"SECRET_RAW_ACCOUNT_TOKEN"}}"#.into(),
                ))
                .await
                .unwrap();
            return client.observe().await.unwrap_err();
        }
        server
            .send(Message::Text(
                "SECRET_RAW_ACCOUNT_TOKEN invalid JSON".into(),
            ))
            .await
            .unwrap();
        client.observe().await.unwrap_err()
    }

    #[tokio::test]
    async fn socket_failure_classes_remain_distinct_and_redacted() {
        for quotes in [false, true] {
            let error = protocol_error_fixture(quotes).await;
            assert!(matches!(
                error,
                TransportError::Socket(SocketFailure {
                    stage: SocketStage::ProtocolReceive,
                    code: SocketCode::Protocol(crate::ws::WsError::FrameAt(
                        crate::ws::FrameCode::EnvelopeJson
                    ))
                })
            ));
        }
        assert!(matches!(
            incoming(None),
            Err(TransportError::Socket(SocketFailure {
                code: SocketCode::Eof,
                ..
            }))
        ));
        assert!(matches!(
            incoming(Some(Err(tokio_tungstenite::tungstenite::Error::Io(
                std::io::Error::other("SECRET_RAW_ACCOUNT_TOKEN")
            )))),
            Err(TransportError::Socket(SocketFailure {
                code: SocketCode::Read,
                ..
            }))
        ));
        let (_stop, mut client, mut server) = pair().await;
        server.send(Message::Close(None)).await.unwrap();
        assert!(matches!(
            client.observe().await,
            Err(TransportError::Socket(SocketFailure {
                code: SocketCode::Close,
                ..
            }))
        ));
    }

    async fn pair() -> (
        StopHandle,
        Session<DuplexStream>,
        WebSocketStream<DuplexStream>,
    ) {
        pair_with_protocol(BarsProtocol::new(5000, 1000).unwrap()).await
    }

    async fn pair_with_protocol(
        mut protocol: BarsProtocol,
    ) -> (
        StopHandle,
        Session<DuplexStream>,
        WebSocketStream<DuplexStream>,
    ) {
        let (a, b) = tokio::io::duplex(16 * 1024);
        let socket = WebSocketStream::from_raw_socket(a, Role::Client, Some(config())).await;
        let server = WebSocketStream::from_raw_socket(b, Role::Server, Some(config())).await;
        let connection = protocol.connected(Utc::now()).unwrap();
        let lease = AccessTokenLease::new(
            AccessToken::new("synthetic-only"),
            std::time::Instant::now(),
            FinamAuthPolicy::default(),
        );
        let (stop, budget) = ReadOnlyCollector::new(
            Utc::now() + chrono::Duration::seconds(10),
            Duration::from_secs(1),
        )
        .unwrap();
        (
            stop,
            Session {
                socket: Some(socket),
                protocol,
                connection,
                lease,
                budget,
            },
            server,
        )
    }

    #[tokio::test]
    async fn quote_socket_normalizes_and_retains_duplicate_frame_evidence() {
        let (_stop, mut client, mut server) =
            pair_with_protocol(BarsProtocol::new_quotes(5000, 1000, 5000, 32).unwrap()).await;
        server
            .send(Message::Text(
                r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
            ))
            .await
            .unwrap();
        assert!(matches!(
            client.observe().await.unwrap(),
            Observation::Handshake
        ));
        let Message::Text(wire) = server.next().await.unwrap().unwrap() else {
            panic!("text")
        };
        let wire: serde_json::Value = serde_json::from_str(&wire).unwrap();
        assert_eq!(wire["type"], "QUOTES");
        let payload = serde_json::json!({"type":"DATA","subscriptionType":"QUOTES",
            "subscriptionKey":"q1","payload":{"quote":[{
                "symbol":"IMOEXF@RTSX","timestamp":Utc::now().to_rfc3339(),
                "last":{"value":"2200.5"}}]}})
        .to_string();
        let payload = format!(" \n{payload}\t\n");
        server.send(Message::Text(payload.clone())).await.unwrap();
        let before = Utc::now().timestamp_millis();
        let Observation::Quotes(quotes) = client.observe().await.unwrap() else {
            panic!("quotes")
        };
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].price_ticks, 4401);
        assert_eq!(quotes.raw_frame.as_bytes(), payload.as_bytes());
        assert!((before..=Utc::now().timestamp_millis()).contains(&quotes.received_at_ms));
        assert_eq!(quotes[0].received_at_ms, quotes.received_at_ms);
        server.send(Message::Text(payload.clone())).await.unwrap();
        let before = Utc::now().timestamp_millis();
        let Observation::Quotes(quotes) = client.observe().await.unwrap() else {
            panic!("quotes")
        };
        assert!(quotes.is_empty());
        assert_eq!(quotes.raw_frame.as_bytes(), payload.as_bytes());
        assert!((before..=Utc::now().timestamp_millis()).contains(&quotes.received_at_ms));
    }

    #[tokio::test]
    async fn handshake_sends_exact_subscription_and_ping_flushes_pong() {
        let (_stop, mut client, mut server) = pair().await;
        server
            .send(Message::Text(
                r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
            ))
            .await
            .unwrap();
        assert!(matches!(
            client.observe().await.unwrap(),
            Observation::Handshake
        ));
        let Message::Text(wire) = server.next().await.unwrap().unwrap() else {
            panic!("text")
        };
        let wire: serde_json::Value = serde_json::from_str(&wire).unwrap();
        assert_eq!(wire["type"], "BARS");
        assert_eq!(wire["data"]["symbol"], "IMOEXF@RTSX");
        assert_eq!(wire["data"]["timeframe"], "TIME_FRAME_M1");
        server.send(Message::Ping(vec![1, 2])).await.unwrap();
        assert!(matches!(
            client.observe().await.unwrap(),
            Observation::Ignore
        ));
        assert_eq!(
            server.next().await.unwrap().unwrap(),
            Message::Pong(vec![1, 2])
        );
    }

    pub(crate) async fn history_error_fixture(cap: bool) -> TransportError {
        let (stop, mut client, mut server) = pair().await;
        server
            .send(Message::Text(
                r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
            ))
            .await
            .unwrap();
        client.observe().await.unwrap();
        server.next().await.unwrap().unwrap();
        let frame = serde_json::json!({
            "type":"DATA","subscriptionType":"BARS","subscriptionKey":"SECRET",
            "payload":{"symbol":"IMOEXF@RTSX","timeframe":"TIME_FRAME_M1","bars":[{
                "timestamp":"2026-01-01T00:00:00Z","open":"2200","high":"2201",
                "low":"2199","close":"2200","volume":"1","is_data_snapshot":true
            }]}
        })
        .to_string();
        for _ in 0..if cap { 512 } else { 1 } {
            server.send(Message::Text(frame.clone())).await.unwrap();
            let observation = client.observe().await.unwrap();
            assert!(matches!(observation, Observation::InitialHistory { .. }));
            assert!(observation.live_bars().is_empty());
        }
        if cap {
            server.send(Message::Text(frame)).await.unwrap();
            let error = client.observe().await.unwrap_err();
            assert_eq!(client.observe().await.unwrap_err(), TransportError::Held);
            return error;
        }
        let task = tokio::spawn(async move {
            let error = client.observe().await.unwrap_err();
            assert_eq!(client.observe().await.unwrap_err(), TransportError::Held);
            error
        });
        stop.stop();
        let error = task.await.unwrap();
        assert_eq!(error, TransportError::Stopped);
        let eof = tokio::time::timeout(Duration::from_secs(1), server.next())
            .await
            .unwrap();
        assert!(eof.is_none() || eof.unwrap().is_err());
        error
    }

    #[tokio::test]
    async fn snapshot_discard_then_stop_drops_real_duplex_transport() {
        assert_eq!(history_error_fixture(false).await, TransportError::Stopped);
    }

    #[tokio::test]
    async fn stop_while_reading_drops_transport_and_prevents_reuse() {
        let (stop, mut client, mut server) = pair().await;
        let task = tokio::spawn(async move {
            let error = client.observe().await.unwrap_err();
            assert_eq!(client.observe().await.unwrap_err(), TransportError::Held);
            error
        });
        stop.stop();
        assert_eq!(task.await.unwrap(), TransportError::Stopped);
        let eof = tokio::time::timeout(Duration::from_secs(1), server.next())
            .await
            .unwrap();
        assert!(eof.is_none() || eof.unwrap().is_err());
    }

    #[tokio::test]
    async fn caller_cancellation_and_bad_frame_drop_socket() {
        let (_stop, mut client, _server) = pair().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(5), client.observe())
                .await
                .is_err()
        );
        assert!(client.socket.is_none());
        assert_eq!(client.observe().await.unwrap_err(), TransportError::Held);
        let (_stop, mut client, mut server) = pair().await;
        server.send(Message::Binary(vec![1])).await.unwrap();
        assert_eq!(
            client.observe().await.unwrap_err(),
            failure(SocketStage::Receive, SocketCode::UnexpectedMessage)
        );
        assert!(client.socket.is_none());
    }
}
