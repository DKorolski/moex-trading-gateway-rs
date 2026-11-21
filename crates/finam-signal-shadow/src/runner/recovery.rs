//! Bounded BARS-only recovery. No model, REST snapshot, strategy or fill access.
use super::Event;
use crate::{
    auth::Grant,
    s5::{Channel, Witness},
    socket::{BarsSocket, LivenessTimeout, RecoveryFactory, SocketCode},
    transport::{StopHandle, TransportError},
    ws::Observation,
};
use chrono::Utc;
use std::{future::Future, time::Duration};
use tokio::{sync::mpsc::Sender, time::Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(super) enum Reason {
    Eof,
    Read,
    Close,
    Connect,
    Provider,
    Pong,
    TransportIdle,
    BarsDataSilence,
    HeartbeatIo,
}

impl Reason {
    pub(super) fn classify(e: &TransportError) -> Option<Self> {
        use SocketCode as C;
        match e {
            TransportError::Socket(s) => match s.code {
                C::Eof => Some(Self::Eof),
                C::Read => Some(Self::Read),
                C::Close => Some(Self::Close),
                C::Connect => Some(Self::Connect),
                C::Protocol(crate::ws::WsError::Provider) => Some(Self::Provider),
                C::Liveness(LivenessTimeout::Pong) => Some(Self::Pong),
                C::Liveness(LivenessTimeout::TransportIdle) => Some(Self::TransportIdle),
                C::Liveness(LivenessTimeout::BarsDataSilence) => Some(Self::BarsDataSilence),
                C::Liveness(LivenessTimeout::HeartbeatSend | LivenessTimeout::ServerPongFlush) => {
                    Some(Self::HeartbeatIo)
                }
                _ => None,
            },
            _ => None,
        }
    }
}

#[derive(Default)]
pub(super) struct RunBudget {
    episodes: u8,
    audit: Audit,
}

#[derive(Clone, Default)]
pub(super) struct Audit(std::sync::Arc<std::sync::Mutex<AuditState>>);
#[derive(Clone, Default, serde::Serialize)]
struct AuditState {
    episode: u8,
    attempt: u64,
    generation: u64,
    stage: &'static str,
    last_reason: Option<Reason>,
    last_error: Option<serde_json::Value>,
    terminal_code: Option<&'static str>,
    last_episode_succeeded: bool,
    fault: Option<Fault>,
    fault_resolved: bool,
    resolved_generation: Option<u64>,
    resolved_at_ms: Option<i64>,
    cancelled_during: Option<&'static str>,
}

#[derive(Clone, serde::Serialize)]
pub(super) struct Fault {
    id: u64,
    at_ms: i64,
    pub error: TransportError,
    pub auth_generation: u64,
    pub connection_generation: u64,
    pub liveness: Option<crate::socket::LivenessSnapshot>,
}

/// Created synchronously by the worker BEFORE its first recovery await.
/// Owned by that worker, so abort_all/drop records cancellation without polling
/// restore's stop branch. A successfully resolved episode is never re-poisoned.
pub(super) struct FaultGuard {
    audit: Audit,
    id: u64,
}
impl Drop for FaultGuard {
    fn drop(&mut self) {
        self.audit.update(|a| {
            if a.fault.as_ref().is_some_and(|f| f.id == self.id)
                && !a.fault_resolved
                && a.terminal_code.is_none()
            {
                a.cancelled_during = Some(a.stage);
                a.stage = "cancelled";
                a.terminal_code = Some("runner_bars_recovery_cancelled");
                a.last_episode_succeeded = false;
            }
        });
    }
}

impl Audit {
    pub(super) fn latch_fault(
        &self,
        error: &TransportError,
        auth: u64,
        connection: u64,
        liveness: Option<crate::socket::LivenessSnapshot>,
    ) -> FaultGuard {
        let mut id = 0;
        self.update(|a| {
            if let Some(fault) = a.fault.as_ref().filter(|_| !a.fault_resolved) {
                id = fault.id;
            } else {
                id = a.fault.as_ref().map_or(1, |f| {
                    f.id.checked_add(1).expect("bounded recovery fault ids")
                });
                a.fault = Some(Fault {
                    id,
                    at_ms: Utc::now().timestamp_millis(),
                    error: error.clone(),
                    auth_generation: auth,
                    connection_generation: connection,
                    liveness,
                });
                a.fault_resolved = false;
                a.resolved_generation = None;
                a.resolved_at_ms = None;
                a.cancelled_during = None;
                a.terminal_code = None;
                a.stage = "fault_observed";
                a.last_episode_succeeded = false;
            }
        });
        FaultGuard {
            audit: self.clone(),
            id,
        }
    }
    pub(super) fn unresolved_fault(&self) -> Option<Fault> {
        let a = self.0.lock().unwrap_or_else(|e| e.into_inner());
        (!a.fault_resolved).then(|| a.fault.clone()).flatten()
    }
    fn resolve(&self, id: u64, generation: u64) -> bool {
        let mut resolved = false;
        self.update(|a| {
            if !a.fault_resolved
                && a.fault
                    .as_ref()
                    .is_some_and(|f| f.id == id && generation > f.connection_generation)
            {
                a.fault_resolved = true;
                a.resolved_generation = Some(generation);
                a.resolved_at_ms = Some(Utc::now().timestamp_millis());
                a.last_episode_succeeded = true;
                a.stage = "ready";
                a.terminal_code = None;
                resolved = true;
            }
        });
        resolved
    }
    fn error(&self, error: &TransportError) {
        self.update(|a| {
            a.last_reason = Reason::classify(error);
            a.last_error = Some(serde_json::to_value(error).expect("closed transport error"));
        });
    }
    fn update(&self, change: impl FnOnce(&mut AuditState)) {
        change(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()));
    }
    pub(super) fn snapshot(&self) -> serde_json::Value {
        serde_json::to_value(&*self.0.lock().unwrap_or_else(|e| e.into_inner()))
            .expect("closed recovery fields")
    }
}
impl RunBudget {
    pub(super) fn with_audit(audit: Audit) -> Self {
        Self { episodes: 0, audit }
    }
}
pub(super) struct Restored {
    pub socket: BarsSocket,
    pub grant: Grant,
    pub generation: u64,
    pub witness: Witness,
}

async fn emit(
    tx: &Sender<Event>,
    event: &'static str,
    fields: serde_json::Value,
) -> Result<(), &'static str> {
    tx.send(Event::Liveness(serde_json::json!({"event":event,"at_ms":Utc::now().timestamp_millis(),"detail":fields,"channel":"BARS","canonical_gap_replay":false})))
        .await.map_err(|_|"runner_queue")
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn restore<F, Fut>(
    factory: RecoveryFactory,
    mut grant: F,
    stop: StopHandle,
    tx: &Sender<Event>,
    run: &mut RunBudget,
    previous_generation: u64,
    reason: Reason,
    first_data_budget: Duration,
) -> Result<Restored, &'static str>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Grant, &'static str>>,
{
    let fault_id = run
        .audit
        .unresolved_fault()
        .ok_or("runner_bars_recovery_unlatched")?
        .id;
    if run.episodes >= 3 {
        run.audit.update(|a| {
            a.last_episode_succeeded = false;
            a.stage = "terminal";
            a.terminal_code = Some("runner_bars_recovery_run_budget");
        });
        return Err("runner_bars_recovery_run_budget");
    }
    run.episodes += 1;
    let episode = run.episodes;
    let audit = run.audit.clone();
    audit.update(|a| {
        a.episode = episode;
        a.attempt = 0;
        a.generation = previous_generation;
        a.stage = "backoff";
        a.last_reason = Some(reason);
        a.last_error = None;
        a.last_episode_succeeded = false;
        a.terminal_code = None;
    });
    let end = Instant::now() + Duration::from_secs(45);
    let work = async {
        emit(tx,"bars_recovery_started",serde_json::json!({"episode":episode,"reason":reason,"previous_generation":previous_generation,"attempt_limit":3,"episode_budget_ms":45000,"run_episode_limit":3})).await?;
        for attempt in 1..=3u64 {
            audit.update(|a| {
                a.attempt = attempt;
                a.stage = "backoff";
            });
            tokio::time::sleep(Duration::from_secs(1 << (attempt - 1))).await;
            let generation = previous_generation
                .checked_add(attempt)
                .ok_or("runner_bars_generation")?;
            emit(tx,"bars_recovery_attempt",serde_json::json!({"episode":episode,"attempt":attempt,"connection_generation":generation})).await?;
            // Validated shared auth is obtained for EVERY new command. Auth error
            // or cancellation remains terminal; it is never retried as a WS error.
            audit.update(|a| {
                a.stage = "auth";
                a.generation = generation;
            });
            let current = grant().await?;
            audit.update(|a| a.stage = "connect");
            let mut socket = match factory.connect(current.lease.clone()).await {
                Ok(socket) => socket,
                Err(error) => {
                    audit.error(&error);
                    if error == TransportError::Stopped {
                        return Err("runner_stopped");
                    }
                    if error != TransportError::Deadline && Reason::classify(&error).is_none() {
                        return Err("runner_bars_recovery_connect_fatal");
                    }
                    emit(tx,"bars_recovery_attempt_failed",serde_json::json!({"episode":episode,"attempt":attempt,"connection_generation":generation,"stage":"connect","reason":Reason::classify(&error)})).await?;
                    continue;
                }
            };
            tx.send(Event::AuthValidated(current.generation, current.expires_at))
                .await
                .map_err(|_| "runner_queue")?;
            let mut witness = Witness::new_auth(Channel::Bars, generation, current.generation);
            audit.update(|a| a.stage = "first_data");
            let ready = tokio::time::timeout(first_data_budget, async {
                loop {
                    let observation = socket.observe().await?;
                    if matches!(observation, Observation::Handshake) {
                        tx.send(Event::BarsSubscription(
                            generation,
                            current.generation,
                            "bounded_recovery",
                        ))
                        .await
                        .map_err(|_| TransportError::Held)?;
                    }
                    for receipt in witness.receipts(&observation, Utc::now().timestamp_millis()) {
                        tx.send(Event::Witness(receipt))
                            .await
                            .map_err(|_| TransportError::Held)?;
                    }
                    let live = !observation.live_bars().is_empty();
                    tx.send(Event::Socket(observation))
                        .await
                        .map_err(|_| TransportError::Held)?;
                    if live {
                        return Ok::<_, TransportError>(());
                    }
                }
            })
            .await;
            match &ready {
                Ok(Err(error)) => audit.error(error),
                Err(_) => audit.update(|a| {
                    a.last_error = Some(serde_json::json!({"Recovery":"FirstDataDeadline"}))
                }),
                _ => (),
            }
            match ready {
                Ok(Ok(())) => {
                    emit(tx,"bars_recovery_succeeded",serde_json::json!({"episode":episode,"attempt":attempt,"connection_generation":generation,"subscription_auth_generation":current.generation,"fresh_data_observed":true})).await?;
                    // No await after resolution: publication and new-generation
                    // DATA are complete, and cancellation cannot re-open this fault.
                    if !audit.resolve(fault_id, generation) {
                        return Err("runner_bars_recovery_fault_mismatch");
                    }
                    return Ok(Restored {
                        socket,
                        grant: current,
                        generation,
                        witness,
                    });
                }
                Ok(Err(error)) if Reason::classify(&error).is_none() => {
                    return Err("runner_bars_recovery_protocol")
                }
                other => {
                    // Drop the entire candidate owner before another attempt.
                    drop(socket);
                    emit(tx,"bars_recovery_attempt_failed",serde_json::json!({"episode":episode,"attempt":attempt,"connection_generation":generation,"stage":"first_data","timed_out":other.is_err()})).await?;
                }
            }
        }
        Err("runner_bars_recovery_attempts_exhausted")
    };
    let result = tokio::select! {
        biased;
        _=stop.cancelled()=>Err("runner_stopped"),
        _=tokio::time::sleep_until(end)=>Err("runner_bars_recovery_deadline"),
        result=work=>result,
    };
    if let Err(code) = result.as_ref() {
        audit.update(|a| {
            a.last_episode_succeeded = false;
            a.stage = "terminal";
            a.terminal_code = Some(code);
        });
    }
    result
}

#[cfg(test)]
mod finalization_tests;
#[cfg(test)]
mod tests;
