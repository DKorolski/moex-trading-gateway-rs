//! Receive loop for the explicitly selected candle-only policy.
use super::*;
use liveness::{TimeoutKind, Watchdog};
use std::time::Duration;

impl<S: AsyncRead + AsyncWrite + Unpin> Session<S> {
    pub(super) async fn observe_candle(&mut self) -> Result<Option<Observation>, TransportError> {
        let mut socket = self.socket.take().ok_or(TransportError::Held)?;
        self.receive_lease_valid()?;
        self.protocol.enable_candle_liveness();
        let io_budget = self.budget.io_budget();
        let protocol = &mut self.protocol;
        let connection = &self.connection;
        let lease = &self.lease;
        let subscribed = &mut self.subscription_sent;
        let silence_ms = self.silence_ms;
        let watch = &mut self.watchdog;
        let result = self.budget.bounded_wait(Duration::from_secs(3600), async {
            watch.check(tokio::time::Instant::now()).map_err(|e| timeout(watch, e))?;
            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(watch.next()) => {
                    let now = tokio::time::Instant::now();
                    watch.check(now).map_err(|e| timeout(watch, e))?;
                    if watch.warn(now) { return Ok(Some(Observation::Ignore)); }
                    if watch.ping_due(now) {
                        let payload = watch.payload();
                        tokio::time::timeout(io_budget, socket.send(Message::Ping(payload)))
                            .await.map_err(|_| timeout(watch, TimeoutKind::HeartbeatSend))?
                            .map_err(|_| failure(SocketStage::Heartbeat, SocketCode::Read))?;
                        watch.check(tokio::time::Instant::now()).map_err(|e| timeout(watch, e))?;
                        watch.sent(tokio::time::Instant::now());
                    }
                    Ok(Some(Observation::Ignore))
                }
                frame = socket.next() => {
                    // Check before accepting a late frame/Pong or resetting any clock.
                    watch.check(tokio::time::Instant::now()).map_err(|e| timeout(watch, e))?;
                    let frame = incoming(frame)?;
                    watch.received(tokio::time::Instant::now());
                    if !*subscribed { lease_valid(lease)?; }
                    match frame {
                        Message::Text(text) => {
                            let observation = decode_text(protocol, connection, &text, silence_ms)?;
                            if matches!(observation, Observation::Handshake) {
                                lease_valid(lease)?;
                                let request = protocol.subscription_request(connection, lease.token())
                                    .map_err(|e| protocol_failure(SocketStage::SubscriptionBuild, e))?;
                                tokio::time::timeout(io_budget, socket.send(Message::Text(request.to_string())))
                                    .await.map_err(|_| failure(SocketStage::SubscriptionSend, SocketCode::IoTimeout))?
                                    .map_err(|_| failure(SocketStage::SubscriptionSend, SocketCode::Read))?;
                                protocol.subscription_sent(connection)
                                    .map_err(|e| protocol_failure(SocketStage::SubscriptionCommit, e))?;
                                *subscribed = true;
                            }
                            watch.check(tokio::time::Instant::now()).map_err(|e| timeout(watch, e))?;
                            if !observation.live_bars().is_empty() { watch.data(tokio::time::Instant::now()); }
                            Ok(Some(observation))
                        }
                        Message::Ping(_) => {
                            tokio::time::timeout(io_budget, socket.flush()).await
                                .map_err(|_| timeout(watch, TimeoutKind::ServerPongFlush))?
                                .map_err(|_| failure(SocketStage::PongFlush, SocketCode::Read))?;
                            Ok(Some(Observation::Ignore))
                        }
                        Message::Pong(payload) => {
                            watch.pong(&payload, tokio::time::Instant::now());
                            Ok(Some(Observation::Ignore))
                        }
                        _ => Err(failure(SocketStage::Receive, SocketCode::UnexpectedMessage)),
                    }
                }
            }
        }).await;
        // Error or cancellation drops the owning socket. No automatic retry.
        if matches!(result, Err(TransportError::Deadline)) {
            self.watchdog.failed(TimeoutKind::RunDeadline);
        }
        let observation = result?;
        self.watchdog
            .check(tokio::time::Instant::now())
            .map_err(|e| timeout(&mut self.watchdog, e))?;
        self.receive_lease_valid()?;
        self.socket = Some(socket);
        Ok(observation)
    }
}

fn timeout(watch: &mut Watchdog, kind: TimeoutKind) -> TransportError {
    watch.failed(kind);
    failure(SocketStage::Heartbeat, SocketCode::Liveness(kind))
}

#[cfg(test)]
mod tests;
