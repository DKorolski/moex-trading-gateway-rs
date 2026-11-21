//! Fixed-endpoint socket owner. No tasks, order channels, Redis or credential discovery.
use crate::{
    transport::{ReadOnlyCollector, TransportError},
    ws::{BarsProtocol, Connection, Observation},
};
use broker_finam::AccessTokenLease;
use chrono::Utc;
mod candle;
mod liveness;
pub mod primary;
mod timestamp;
use futures_util::{SinkExt, StreamExt};
pub(crate) use liveness::policy as candle_liveness_policy;
pub use liveness::{Snapshot as LivenessSnapshot, TimeoutKind as LivenessTimeout};
pub use timestamp::TimestampBoundsDetail;
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
    Heartbeat,
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
    IoTimeout,
    Liveness(liveness::TimeoutKind),
    Protocol(crate::ws::WsError),
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SocketFailure {
    pub stage: SocketStage,
    pub code: SocketCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_bounds: Option<TimestampBoundsDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_conflict: Option<Box<crate::quote::QuoteConflictDetail>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_time: Option<Box<crate::quote::QuoteTimeDetail>>,
}
fn failure(stage: SocketStage, code: SocketCode) -> TransportError {
    TransportError::Socket(SocketFailure {
        stage,
        code,
        timestamp_bounds: None,
        quote_conflict: None,
        quote_time: None,
    })
}
fn protocol_failure(stage: SocketStage, e: crate::ws::WsError) -> TransportError {
    failure(stage, SocketCode::Protocol(e))
}

fn decode_text(
    protocol: &mut BarsProtocol,
    connection: &Connection,
    text: &str,
    silence_ms: u64,
) -> Result<Observation, TransportError> {
    let now = Utc::now();
    protocol.receive(connection, text, now).map_err(|e| {
        let timestamp_bounds = (e
            == crate::ws::WsError::FrameAt(crate::ws::FrameCode::TimestampBounds))
        .then(|| timestamp::detail(text, now, silence_ms))
        .flatten();
        TransportError::Socket(SocketFailure {
            stage: SocketStage::ProtocolReceive,
            code: SocketCode::Protocol(e),
            timestamp_bounds,
            quote_time: if e == crate::ws::WsError::Quote(crate::quote::QuoteError::Time) {
                protocol.quote_time_detail().map(Box::new)
            } else {
                None
            },
            quote_conflict: if e == crate::ws::WsError::Quote(crate::quote::QuoteError::Conflict) {
                protocol.quote_conflict_detail().map(Box::new)
            } else {
                None
            },
        })
    })
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
    watchdog: liveness::Watchdog,
    // Only enabled by the candle-only runner. Never authorizes a new command.
    independent_subscription: bool,
    subscription_sent: bool,
    silence_ms: u64,
    socket: Option<WebSocketStream<S>>,
    protocol: BarsProtocol,
    connection: Connection,
    lease: AccessTokenLease,
    budget: ReadOnlyCollector,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Session<S> {
    fn receive_lease_valid(&self) -> Result<(), TransportError> {
        if self.independent_subscription && self.subscription_sent {
            Ok(())
        } else {
            lease_valid(&self.lease)
        }
    }
    async fn observe(&mut self) -> Result<Observation, TransportError> {
        self.observe_until(None).await?.ok_or(TransportError::Fetch)
    }

    async fn observe_until(
        &mut self,
        rotate_at: Option<std::time::Instant>,
    ) -> Result<Option<Observation>, TransportError> {
        if self.independent_subscription {
            return self.observe_candle().await;
        }
        // The local owner drops the socket on any error OR cancellation.
        let mut socket = self.socket.take().ok_or(TransportError::Held)?;
        self.receive_lease_valid()?;
        let remaining_silence = self
            .protocol
            .remaining_silence(Utc::now())
            .map_err(|e| protocol_failure(SocketStage::Silence, e))?;
        let protocol = &mut self.protocol;
        let connection = &self.connection;
        let lease = &self.lease;
        let independent = self.independent_subscription;
        let subscription_sent = &mut self.subscription_sent;
        let silence_ms = self.silence_ms;
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
                if !independent || !*subscription_sent {
                    lease_valid(lease)?;
                }
                match frame {
                    Message::Text(text) => {
                        let observation = decode_text(protocol, connection, &text, silence_ms)?;
                        if matches!(observation, Observation::Handshake) {
                            // CONNECT validation does not authorize a delayed SUBSCRIBE.
                            lease_valid(lease)?;
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
                            *subscription_sent = true;
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
        self.receive_lease_valid()?;
        self.socket = Some(socket);
        Ok(observation)
    }
}

pub struct BarsSocket(Session<MaybeTlsStream<TcpStream>>);

/// A fresh bounded receive owner after a classified fault, never a reset of a
/// held request or reuse of the old socket. Absolute deadline/stop are inherited.
pub(crate) struct RecoveryFactory {
    budget: ReadOnlyCollector,
    silence_ms: u64,
    reconnect_ms: u64,
    #[cfg(test)]
    endpoint: Option<std::net::SocketAddr>,
}

impl RecoveryFactory {
    pub(crate) async fn connect(
        &self,
        lease: AccessTokenLease,
    ) -> Result<BarsSocket, TransportError> {
        #[cfg(test)]
        if let Some(endpoint) = self.endpoint {
            lease_valid(&lease)?;
            let mut budget = self.budget.sibling();
            let socket = budget
                .bounded(async {
                    let tcp = TcpStream::connect(endpoint)
                        .await
                        .map_err(|_| failure(SocketStage::Connect, SocketCode::Connect))?;
                    tokio_tungstenite::client_async(
                        "ws://local.fixture/ws",
                        MaybeTlsStream::Plain(tcp),
                    )
                    .await
                    .map(|(ws, _)| ws)
                    .map_err(|_| failure(SocketStage::Connect, SocketCode::Connect))
                })
                .await?;
            let mut protocol = BarsProtocol::new(self.silence_ms, self.reconnect_ms)
                .map_err(|_| TransportError::Bounds)?;
            let connection = protocol
                .connected(Utc::now())
                .map_err(|e| protocol_failure(SocketStage::Connect, e))?;
            return Ok(BarsSocket(Session {
                watchdog: liveness::Watchdog::new(),
                independent_subscription: true,
                subscription_sent: false,
                silence_ms: self.silence_ms,
                socket: Some(socket),
                protocol,
                connection,
                lease,
                budget,
            }));
        }
        BarsSocket::connect(
            lease,
            self.budget.sibling(),
            self.silence_ms,
            self.reconnect_ms,
        )
        .await
        .map(BarsSocket::independent_subscription)
    }

    #[cfg(test)]
    pub(crate) fn local_fixture(budget: ReadOnlyCollector, endpoint: std::net::SocketAddr) -> Self {
        Self {
            budget,
            silence_ms: 60000,
            reconnect_ms: 5000,
            endpoint: Some(endpoint),
        }
    }
}

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
        Self::connect_protocol(lease, budget, protocol, silence_ms).await
    }

    async fn connect_protocol(
        lease: AccessTokenLease,
        mut budget: ReadOnlyCollector,
        mut protocol: BarsProtocol,
        silence_ms: u64,
    ) -> Result<Self, TransportError> {
        let connection = protocol
            .connected(Utc::now())
            .map_err(|e| protocol_failure(SocketStage::Connect, e))?;
        let socket = open(&lease, &mut budget).await?;
        protocol
            .check_silence(Utc::now())
            .map_err(|e| protocol_failure(SocketStage::Silence, e))?;
        Ok(Self(Session {
            watchdog: liveness::Watchdog::new(),
            independent_subscription: false,
            subscription_sent: false,
            silence_ms,
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

    /// Candle-only receive policy; CONNECT and SUBSCRIBE still require a lease.
    pub(crate) fn independent_subscription(mut self) -> Self {
        self.0.independent_subscription = true;
        self
    }

    pub(crate) fn liveness(&self) -> Option<LivenessSnapshot> {
        self.0
            .independent_subscription
            .then(|| self.0.watchdog.snapshot())
    }

    pub(crate) fn recovery_factory(&self) -> Option<RecoveryFactory> {
        self.0.independent_subscription.then(|| RecoveryFactory {
            budget: self.0.budget.sibling(),
            silence_ms: self.0.silence_ms,
            reconnect_ms: self.0.protocol.reconnect_budget_ms(),
            #[cfg(test)]
            endpoint: None,
        })
    }

    pub(crate) fn take_liveness_event(&mut self) -> Option<serde_json::Value> {
        self.0.watchdog.take_event()
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
            independent_subscription,
            silence_ms,
            socket,
            mut protocol,
            connection,
            mut budget,
            mut watchdog,
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
        watchdog.reconnected();
        protocol
            .check_silence(Utc::now())
            .map_err(|e| protocol_failure(SocketStage::Silence, e))?;
        Ok(Self(Session {
            watchdog,
            independent_subscription,
            subscription_sent: false,
            silence_ms,
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
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn connect_diagnostic_initial(
        lease: AccessTokenLease,
        budget: ReadOnlyCollector,
        silence_ms: u64,
        reconnect_ms: u64,
        quote_age_ms: u64,
        capacity: usize,
        enabled: bool,
    ) -> Result<Self, TransportError> {
        let protocol = BarsProtocol::new_quotes(silence_ms, reconnect_ms, quote_age_ms, capacity)
            .map_err(|_| TransportError::Bounds)?;
        let protocol = if enabled {
            protocol.diagnostic_initial_quotes()
        } else {
            protocol
        };
        Ok(Self(
            BarsSocket::connect_protocol(lease, budget, protocol, silence_ms).await?,
        ))
    }
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
            BarsSocket::connect_protocol(lease, budget, protocol, silence_ms).await?,
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

    fn expired_lease() -> AccessTokenLease {
        AccessTokenLease::new(
            AccessToken::new("expired-fixture"),
            std::time::Instant::now() - Duration::from_secs(2),
            FinamAuthPolicy {
                token_ttl: Duration::from_secs(1),
                renew_before: Duration::ZERO,
            },
        )
    }

    pub(super) async fn subscribed_pair() -> (
        StopHandle,
        Session<DuplexStream>,
        WebSocketStream<DuplexStream>,
    ) {
        let (stop, mut client, mut server) = pair().await;
        client.independent_subscription = true;
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
        assert!(matches!(
            server.next().await.unwrap().unwrap(),
            Message::Text(_)
        ));
        (stop, client, server)
    }

    pub(crate) fn fresh_bar() -> Message {
        Message::Text(
            serde_json::json!({
                "type":"DATA", "subscriptionType":"BARS", "subscriptionKey":"fixture",
                "payload":{"symbol":"IMOEXF@RTSX","timeframe":"TIME_FRAME_M1","bars":[{
                "timestamp":chrono::DateTime::from_timestamp(Utc::now().timestamp()/60*60,0).unwrap().to_rfc3339(),"open":"2200","high":"2201",
                    "low":"2199","close":"2200","volume":"1"
                }]}
            })
            .to_string(),
        )
    }

    pub(crate) async fn refresh_without_socket_replacement<F: std::future::Future<Output = ()>>(
        refresh: F,
    ) {
        let (_stop, mut client, mut server) = subscribed_pair().await;
        let original_connection = client.connection.clone();
        let Message::Text(frame) = fresh_bar() else {
            unreachable!()
        };
        server.send(Message::Text(frame.clone())).await.unwrap();
        assert_eq!(client.observe().await.unwrap().live_bars().len(), 1);
        refresh.await;
        // Keep the expired subscription credential, not the new REST grant.
        client.lease = expired_lease();
        // Reusing the original connection capability still passes the protocol's
        // exact-generation check. Provisional bars never drive model finality.
        assert_eq!(
            client
                .protocol
                .receive(&original_connection, &frame, Utc::now())
                .unwrap()
                .live_bars()
                .len(),
            1
        );
        server.send(fresh_bar()).await.unwrap();
        assert_eq!(client.observe().await.unwrap().live_bars().len(), 1);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), server.next())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn candle_subscription_receives_after_real_expiry_without_resubscribe() {
        let (_stop, mut client, mut server) = subscribed_pair().await;
        client.lease = AccessTokenLease::new(
            AccessToken::new("short-fixture"),
            std::time::Instant::now(),
            FinamAuthPolicy {
                token_ttl: Duration::from_millis(10),
                renew_before: Duration::ZERO,
            },
        );
        let observation = client.observe();
        let publish = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            server.send(fresh_bar()).await.unwrap();
        };
        let (result, ()) = tokio::join!(observation, publish);
        assert_eq!(result.unwrap().live_bars().len(), 1);
        assert!(lease_valid(&client.lease).is_err());
        assert!(client.socket.is_some());
        assert!(
            tokio::time::timeout(Duration::from_millis(10), server.next())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn expired_connect_and_delayed_subscribe_remain_forbidden() {
        let (_stop, mut client, mut server) = pair().await;
        client.independent_subscription = true;
        client.lease = expired_lease();
        assert!(matches!(
            open(&client.lease, &mut client.budget).await,
            Err(TransportError::Socket(SocketFailure {
                code: SocketCode::Lease,
                ..
            }))
        ));
        server
            .send(Message::Text(
                r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
            ))
            .await
            .unwrap();
        assert_eq!(
            client.observe().await.unwrap_err(),
            failure(SocketStage::Lease, SocketCode::Lease)
        );
        assert!(!client.subscription_sent);
        assert!(client.socket.is_none());
    }

    #[tokio::test]
    async fn established_candle_bad_frame_close_eof_and_provider_error_still_fail() {
        for case in 0..4 {
            let (_stop, mut client, mut server) = subscribed_pair().await;
            client.lease = expired_lease();
            match case {
                0 => server.send(Message::Text("not-json".into())).await.unwrap(),
                1 => server.send(Message::Close(None)).await.unwrap(),
                2 => {
                    drop(server);
                }
                _ => server
                    .send(Message::Text(
                        r#"{"type":"ERROR","errorInfo":{"code":"fixture"}}"#.into(),
                    ))
                    .await
                    .unwrap(),
            }
            let error = client.observe().await.unwrap_err();
            assert_ne!(error, failure(SocketStage::Lease, SocketCode::Lease));
            assert!(client.socket.is_none());
            assert_eq!(client.observe().await.unwrap_err(), TransportError::Held);
        }
    }

    #[tokio::test]
    async fn expiry_does_not_turn_bootstrap_into_live_or_disable_silence() {
        let (_stop, mut client, mut server) = subscribed_pair().await;
        client.lease = expired_lease();
        let Message::Text(frame) = fresh_bar() else {
            unreachable!()
        };
        let mut frame: serde_json::Value = serde_json::from_str(&frame).unwrap();
        frame["payload"]["bars"][0]["isDataSnapshot"] = true.into();
        frame["payload"]["bars"][0]["timestamp"] = "2026-01-01T00:00:00Z".into();
        server.send(Message::Text(frame.to_string())).await.unwrap();
        let observation = client.observe().await.unwrap();
        assert!(matches!(observation, Observation::InitialHistory { .. }));
        assert!(observation.live_bars().is_empty());
        let error = client.observe().await.unwrap_err();
        assert_ne!(error, failure(SocketStage::Lease, SocketCode::Lease));
        assert!(client.socket.is_none());
    }

    #[tokio::test]
    async fn established_candle_stop_does_not_need_auth_or_valid_jwt() {
        let (stop, mut client, mut server) = subscribed_pair().await;
        client.lease = expired_lease();
        let job = tokio::spawn(async move { client.observe().await });
        stop.stop();
        assert_eq!(job.await.unwrap().unwrap_err(), TransportError::Stopped);
        let peer = tokio::time::timeout(Duration::from_secs(1), server.next())
            .await
            .unwrap();
        assert!(peer.is_none() || peer.unwrap().is_err());
    }

    #[tokio::test]
    async fn legacy_receive_policy_still_rejects_expiry() {
        let (_stop, mut client, _server) = subscribed_pair().await;
        client.independent_subscription = false;
        client.lease = expired_lease();
        assert_eq!(
            client.observe().await.unwrap_err(),
            failure(SocketStage::Lease, SocketCode::Lease)
        );
    }

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
        if case == 2 || case == 4 || case == 5 || case == 6 {
            server
                .send(Message::Text(
                    r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
                ))
                .await
                .unwrap();
            client.observe().await.unwrap();
            server.next().await.unwrap().unwrap();
            let stamp = Utc::now().to_rfc3339();
            if case == 5 {
                server.send(Message::Text(serde_json::json!({
                    "type":"DATA", "subscriptionType":"QUOTES", "subscriptionKey":"SECRET_RAW_ACCOUNT_TOKEN",
                    "payload":{"quote":[{"symbol":"IMOEXF@RTSX", "timestamp":stamp,
                    "last":{"value":"2200"}, "bid":{"value":"2199"}, "ask":{"value":"2201"}}]}
                }).to_string())).await.unwrap();
                let Observation::Quotes(q) = client.observe().await.unwrap() else {
                    panic!("quotes");
                };
                assert_eq!(q.len(), 1);
            }
            let payload = if case == 6 {
                serde_json::json!({"quote":[{"symbol":"IMOEXF@RTSX",
                    "timestamp":(Utc::now()-chrono::Duration::seconds(30)).to_rfc3339(),
                    "last":{"value":"2200"},"ignored":"SECRET_RAW_ACCOUNT_TOKEN"}]})
            } else if case == 5 {
                serde_json::json!({"quote":[{"symbol":"IMOEXF@RTSX", "timestamp":stamp,
                    "last":{"value":"2200.5"}, "bid":{"value":"2199"}, "ask":{"value":"2201"}, "ignored":"SECRET_RAW_ACCOUNT_TOKEN"}]})
            } else if quotes {
                serde_json::json!({"quote":[{"symbol":"IMOEXF@RTSX",
                    "timestamp":Utc::now().to_rfc3339(),"last":{"value":"2200.25"}}]})
            } else if case == 4 {
                serde_json::json!({"symbol":"IMOEXF@RTSX","bars":[{
                    "timestamp":(Utc::now()+chrono::Duration::minutes(2)).to_rfc3339(),
                    "isDataSnapshot":true,"open":"2200","high":"2201","low":"2199",
                    "close":"2200","volume":"1","ignored":"SECRET_RAW_ACCOUNT_TOKEN"
                }]})
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
            let error = client.observe().await.unwrap_err();
            assert!(client.socket.is_none());
            assert_eq!(client.observe().await.unwrap_err(), TransportError::Held);
            return error;
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
                    )),
                    ..
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
                watchdog: liveness::Watchdog::new(),
                independent_subscription: false,
                subscription_sent: false,
                silence_ms: 5000,
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
