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
        return Err(TransportError::Fetch);
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
        .map_err(|_| TransportError::Fetch)?;
    let mut token =
        HeaderValue::from_str(lease.token().as_str()).map_err(|_| TransportError::Fetch)?;
    token.set_sensitive(true);
    request.headers_mut().insert("Authorization", token);
    // Never expose tungstenite/request errors; they can contain secrets.
    budget
        .bounded(async {
            connect_async_with_config(request, Some(config()), true)
                .await
                .map(|(socket, _)| socket)
                .map_err(|_| TransportError::Fetch)
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
        // The local owner drops the socket on any error OR cancellation.
        let mut socket = self.socket.take().ok_or(TransportError::Held)?;
        lease_valid(&self.lease)?;
        let remaining_silence = self
            .protocol
            .remaining_silence(Utc::now())
            .map_err(|_| TransportError::Fetch)?;
        let protocol = &mut self.protocol;
        let connection = &self.connection;
        let lease = &self.lease;
        let observation = self
            .budget
            .bounded_wait(remaining_silence, async {
                let frame = socket
                    .next()
                    .await
                    .ok_or(TransportError::Fetch)?
                    .map_err(|_| TransportError::Fetch)?;
                lease_valid(lease)?;
                match frame {
                    Message::Text(text) => {
                        let observation = protocol
                            .receive(connection, &text, Utc::now())
                            .map_err(|_| TransportError::Fetch)?;
                        if matches!(observation, Observation::Handshake) {
                            let request = protocol
                                .subscription_request(connection, lease.token())
                                .map_err(|_| TransportError::Fetch)?;
                            socket
                                .send(Message::Text(request.to_string()))
                                .await
                                .map_err(|_| TransportError::Fetch)?;
                            protocol
                                .subscription_sent(connection)
                                .map_err(|_| TransportError::Fetch)?;
                        }
                        Ok(observation)
                    }
                    Message::Ping(_) => {
                        // tungstenite queues the matching pong; flush under the same bound.
                        socket.flush().await.map_err(|_| TransportError::Fetch)?;
                        protocol
                            .check_silence(Utc::now())
                            .map_err(|_| TransportError::Fetch)?;
                        Ok(Observation::Ignore)
                    }
                    Message::Pong(_) => {
                        protocol
                            .check_silence(Utc::now())
                            .map_err(|_| TransportError::Fetch)?;
                        Ok(Observation::Ignore)
                    }
                    _ => Err(TransportError::Fetch),
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
            .map_err(|_| TransportError::Fetch)?;
        let socket = open(&lease, &mut budget).await?;
        protocol
            .check_silence(Utc::now())
            .map_err(|_| TransportError::Fetch)?;
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

    /// Explicit, consuming reconnect. Drop old transport before starting another.
    /// Faulted/cancelled sessions cannot heal through this method.
    pub async fn reconnect(self, lease: AccessTokenLease) -> Result<Self, TransportError> {
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
            .map_err(|_| TransportError::Fetch)?;
        let connection = protocol
            .connected(Utc::now())
            .map_err(|_| TransportError::Fetch)?;
        let socket = open(&lease, &mut budget).await?;
        protocol
            .check_silence(Utc::now())
            .map_err(|_| TransportError::Fetch)?;
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
mod tests {
    use super::*;
    use crate::transport::StopHandle;
    use broker_finam::{AccessToken, FinamAuthPolicy};
    use std::time::Duration;
    use tokio::io::DuplexStream;
    use tokio_tungstenite::tungstenite::protocol::Role;

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
        assert_eq!(client.observe().await.unwrap_err(), TransportError::Fetch);
        assert!(client.socket.is_none());
    }
}
