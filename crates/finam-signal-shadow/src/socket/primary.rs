//! Fixed FINAM BARS transport for WS-primary. History is normalized but never
//! admitted as live here. Heartbeat runs independently of the model/fsync path.
use super::*;
use crate::{
    auth::PrimaryAuthGrant,
    primary::{IngressSender, LiveDataStamp, PrimaryError, WireFrame},
    transport::StopHandle,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{oneshot, watch};
use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Kind {
    Auth(crate::auth::PrimaryAuthClass),
    Connect,
    Protocol,
    Source(PrimaryError),
    Lease,
    Read,
    Eof,
    Close,
    Liveness(LivenessTimeout),
    RunDeadline,
    Clock,
    OwnerGone,
    Cancelled,
    Send,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct Failure {
    pub generation: u64,
    pub kind: Kind,
    pub liveness: Option<LivenessSnapshot>,
}
#[derive(Clone, Default)]
pub struct FailureSlot(
    Arc<Mutex<Option<Failure>>>,
    Option<crate::primary::ContinuityGate>,
    Option<StopHandle>,
);
impl FailureSlot {
    pub(crate) fn with_gate(gate: crate::primary::ContinuityGate, root: StopHandle) -> Self {
        Self(Arc::new(Mutex::new(None)), Some(gate), Some(root))
    }
    pub(crate) fn record(&self, value: Failure) {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(value.clone());
        }
        drop(slot);
        if value.kind == Kind::Cancelled && self.2.as_ref().is_some_and(StopHandle::is_stopped) {
            return;
        }
        if let Some(gate) = &self.1 {
            gate.fault(value);
        }
    }
    pub fn first(&self) -> Option<Failure> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Produced only after an actual SUBSCRIBE send. It is not a provider ACK.
#[derive(Debug)]
pub struct Subscription {
    generation: u64,
    sent_at_ms: i64,
    auth_generation: u64,
    pub(crate) owner: Arc<()>,
}
impl Subscription {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn sent_at_ms(&self) -> i64 {
        self.sent_at_ms
    }
    pub fn auth_generation(&self) -> u64 {
        self.auth_generation
    }
}
#[derive(Debug, serde::Serialize)]
pub struct Report {
    pub generation: u64,
    pub frames_forwarded: u64,
    pub liveness: LivenessSnapshot,
    pub stopped: bool,
}

pub struct PrimarySocket {
    driver: Driver<MaybeTlsStream<TcpStream>>,
}
struct ConnectGuard {
    slot: FailureSlot,
    stop: StopHandle,
    generation: u64,
    completed: bool,
}
impl Drop for ConnectGuard {
    fn drop(&mut self) {
        if !self.completed {
            self.slot.record(Failure {
                generation: self.generation,
                kind: Kind::Cancelled,
                liveness: None,
            });
            self.stop.stop();
        }
    }
}
impl PrimarySocket {
    pub async fn connect(
        grant: PrimaryAuthGrant,
        generation: u64,
        stop: StopHandle,
        deadline: chrono::DateTime<Utc>,
        run_deadline: chrono::DateTime<Utc>,
        failure_slot: FailureSlot,
    ) -> Result<Self, Failure> {
        let fail = |kind| Failure {
            generation,
            kind,
            liveness: None,
        };
        if generation == 0 || deadline > run_deadline {
            return Err(fail(Kind::Protocol));
        }
        let mut guard = ConnectGuard {
            slot: failure_slot.clone(),
            stop: stop.clone(),
            generation,
            completed: false,
        };
        let mut budget =
            match ReadOnlyCollector::for_shared_stop(&stop, deadline, Duration::from_secs(5)) {
                Ok(budget) => budget,
                Err(_) => {
                    let error = fail(Kind::RunDeadline);
                    failure_slot.record(error.clone());
                    stop.stop();
                    guard.completed = true;
                    return Err(error);
                }
            };
        let socket = match open(&grant.grant.lease, &mut budget).await {
            Ok(socket) => socket,
            Err(cause) => {
                let kind = match cause {
                    TransportError::Stopped => Kind::Cancelled,
                    TransportError::Deadline => Kind::RunDeadline,
                    TransportError::Clock => Kind::Clock,
                    TransportError::Socket(error) if error.code == SocketCode::Lease => Kind::Lease,
                    _ => Kind::Connect,
                };
                let error = fail(kind);
                failure_slot.record(error.clone());
                stop.stop();
                guard.completed = true;
                return Err(error);
            }
        };
        guard.completed = true;
        Ok(Self {
            driver: Driver::new(
                socket,
                grant.grant.lease,
                grant.grant.generation,
                generation,
                stop,
                run_deadline,
                failure_slot,
            ),
        })
    }
    pub async fn pump(
        self,
        ingress: IngressSender,
        feedback: watch::Receiver<Option<LiveDataStamp>>,
        started: oneshot::Sender<Subscription>,
    ) -> Result<Report, Failure> {
        self.driver.pump(ingress, feedback, started).await
    }
}

struct Driver<S> {
    socket: WebSocketStream<S>,
    lease: AccessTokenLease,
    auth_generation: u64,
    generation: u64,
    stop: StopHandle,
    wall_deadline: chrono::DateTime<Utc>,
    deadline: Instant,
    handshake_deadline: Instant,
    watch: liveness::Watchdog,
    subscribed: bool,
    key: Option<String>,
    last_wall_ms: i64,
    last_ack: Option<Instant>,
    slot: FailureSlot,
    finished: bool,
    frames: u64,
    owner: Arc<()>,
}
impl<S> Drop for Driver<S> {
    fn drop(&mut self) {
        if !self.finished {
            self.slot.record(Failure {
                generation: self.generation,
                kind: Kind::Cancelled,
                liveness: Some(self.watch.snapshot()),
            });
            self.stop.stop();
        }
    }
}
impl<S: AsyncRead + AsyncWrite + Unpin> Driver<S> {
    fn new(
        socket: WebSocketStream<S>,
        lease: AccessTokenLease,
        auth_generation: u64,
        generation: u64,
        stop: StopHandle,
        wall_deadline: chrono::DateTime<Utc>,
        slot: FailureSlot,
    ) -> Self {
        let remaining = Duration::from_millis(
            wall_deadline
                .timestamp_millis()
                .saturating_sub(crate::timing::now_ms())
                .max(0) as u64,
        );
        let now = Instant::now();
        Self {
            socket,
            lease,
            auth_generation,
            generation,
            stop,
            wall_deadline,
            deadline: now + remaining,
            handshake_deadline: now + Duration::from_secs(5),
            watch: liveness::Watchdog::new(),
            subscribed: false,
            key: None,
            last_wall_ms: crate::timing::now_ms(),
            last_ack: None,
            slot,
            finished: false,
            frames: 0,
            owner: Arc::new(()),
        }
    }
    fn fail(&self, kind: Kind) -> Failure {
        Failure {
            generation: self.generation,
            kind,
            liveness: Some(self.watch.snapshot()),
        }
    }
    async fn send(&mut self, message: Message) -> Result<(), Kind> {
        tokio::select! {biased;
            _=self.stop.cancelled()=>Err(Kind::Cancelled),
            result=tokio::time::timeout_at(self.deadline.min(Instant::now()+Duration::from_secs(5)),self.socket.send(message))=>
                result.map_err(|_|Kind::Send)?.map_err(|_|Kind::Send),
        }
    }
    async fn pump(
        mut self,
        ingress: IngressSender,
        mut feedback: watch::Receiver<Option<LiveDataStamp>>,
        started: oneshot::Sender<Subscription>,
    ) -> Result<Report, Failure> {
        let mut started = Some(started);
        let result = self.run(&ingress, &mut feedback, &mut started).await;
        match result {
            Ok(()) | Err(Kind::Cancelled) if self.stop.is_stopped() => {
                self.finished = true;
                // Owner-detected continuity/Lmax faults can precede connection
                // retirement. Preserve that fault; a drained pump must not panic.
                if let Some(failure) = self.slot.first() {
                    return Err(failure);
                }
                Ok(Report {
                    generation: self.generation,
                    frames_forwarded: self.frames,
                    liveness: self.watch.snapshot(),
                    stopped: true,
                })
            }
            Err(kind) => {
                let failure = self.fail(kind);
                self.slot.record(failure.clone());
                self.stop.stop();
                self.finished = true;
                Err(failure)
            }
            Ok(()) => unreachable!("normal socket exit requires the shared stop latch"),
        }
    }
    async fn run(
        &mut self,
        ingress: &IngressSender,
        feedback: &mut watch::Receiver<Option<LiveDataStamp>>,
        started: &mut Option<oneshot::Sender<Subscription>>,
    ) -> Result<(), Kind> {
        loop {
            if self.stop.is_stopped() {
                return Ok(());
            }
            let wall = crate::timing::now_ms();
            if wall < self.last_wall_ms {
                return Err(Kind::Clock);
            }
            self.last_wall_ms = wall;
            if wall >= self.wall_deadline.timestamp_millis() || Instant::now() >= self.deadline {
                return Err(Kind::RunDeadline);
            }
            self.watch.check(Instant::now()).map_err(Kind::Liveness)?;
            let due = if self.subscribed {
                self.watch.next()
            } else {
                self.watch.next().min(self.handshake_deadline)
            };
            tokio::select! {biased;
                _=self.stop.cancelled()=>return Ok(()),
                _=tokio::time::sleep_until(self.deadline)=>return Err(Kind::RunDeadline),
                changed=feedback.changed()=>{
                    changed.map_err(|_|Kind::OwnerGone)?;
                    if let Some(ack)=feedback.borrow_and_update().clone(){
                        if ack.generation<self.generation{continue;}
                        if ack.generation!=self.generation||ack.owner.as_ref().map_or(true,|owner|!Arc::ptr_eq(owner,&self.owner)){return Err(Kind::Source(PrimaryError::Generation));}
                        if ack.received_mono>Instant::now()||ack.received_at_ms>crate::timing::now_ms(){return Err(Kind::Clock);}
                        if self.last_ack.map_or(true,|last|ack.received_mono>last){
                            self.watch.data_observed(ack.received_mono,ack.received_at_ms);self.last_ack=Some(ack.received_mono);
                        }
                    }
                }
                _=tokio::time::sleep_until(due)=>{
                    if !self.subscribed&&Instant::now()>=self.handshake_deadline{return Err(Kind::Protocol);}
                    self.watch.check(Instant::now()).map_err(Kind::Liveness)?;
                    if self.watch.warn(Instant::now()){continue;}
                    if self.watch.ping_due(Instant::now()){
                        let payload=self.watch.payload();self.send(Message::Ping(payload)).await?;
                        self.watch.check(Instant::now()).map_err(Kind::Liveness)?;self.watch.sent(Instant::now());
                    }
                }
                message=self.socket.next()=>{
                    let received_mono=Instant::now();let received_ms=crate::timing::now_ms();
                    self.watch.check(received_mono).map_err(Kind::Liveness)?;
                    let message=match message{None=>return Err(Kind::Eof),Some(Err(_))=>return Err(Kind::Read),Some(Ok(m))=>m};
                    self.watch.received(received_mono);
                    match message{
                        Message::Text(text)=>{
                            if text.len()>256*1024{return Err(Kind::Protocol);}
                            let envelope:broker_finam::FinamWsEnvelope=serde_json::from_str(&text).map_err(|_|Kind::Protocol)?;
                            if envelope.error_info.as_ref().is_some_and(|v|!v.is_null())||envelope.envelope_type=="ERROR"{return Err(Kind::Protocol);}
                            if envelope.envelope_type=="EVENT"{
                                let event=envelope.event_info.as_ref().and_then(|v|v.get("event")).and_then(serde_json::Value::as_str);
                                match event{
                                    Some("HANDSHAKE_SUCCESS")=>{
                                        if self.subscribed{return Err(Kind::Protocol);}
                                        lease_valid(&self.lease).map_err(|_|Kind::Lease)?;
                                        let request=broker_finam::build_ws_subscribe_bars_request("IMOEXF@RTSX","TIME_FRAME_M1",self.lease.token());
                                        self.send(Message::Text(request.to_string())).await?;
                                        self.subscribed=true;
                                        let receipt=Subscription{generation:self.generation,sent_at_ms:crate::timing::now_ms(),auth_generation:self.auth_generation,owner:self.owner.clone()};
                                        started.take().ok_or(Kind::Protocol)?.send(receipt).map_err(|_|Kind::OwnerGone)?;
                                    }
                                    Some("CONNECTION_CLOSED")=>return Err(Kind::Close),_=>(),
                                }
                                continue;
                            }
                            if !self.subscribed||envelope.envelope_type!="DATA"||envelope.subscription_type.as_deref()!=Some("BARS"){return Err(Kind::Protocol);}
                            let key=envelope.subscription_key.as_deref().filter(|key|!key.is_empty()&&key.len()<=256).ok_or(Kind::Protocol)?;
                            if self.key.as_ref().is_some_and(|old|old!=key){return Err(Kind::Protocol);}
                            let received=chrono::DateTime::from_timestamp_millis(received_ms).ok_or(Kind::Clock)?;
                            let mut frame=WireFrame::normalize(&text,self.generation,key,received).map_err(Kind::Source)?;
                            frame.received_mono=received_mono;
                            frame.owner=Some(self.owner.clone());
                            self.key=Some(key.to_owned());ingress.offer(frame).map_err(Kind::Source)?;self.frames+=1;
                        }
                        Message::Ping(_)=>{
                            tokio::select!{biased;_=self.stop.cancelled()=>return Ok(()),
                                result=tokio::time::timeout_at(self.deadline.min(Instant::now()+Duration::from_secs(5)),self.socket.flush())=>{result.map_err(|_|Kind::Send)?.map_err(|_|Kind::Send)?;}}
                        }
                        Message::Pong(payload)=>self.watch.pong(&payload,received_mono),
                        Message::Close(_)=>return Err(Kind::Close),_=>return Err(Kind::Protocol),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn fixture_subscription(generation: u64, sent_at_ms: i64) -> Subscription {
    Subscription {
        generation,
        sent_at_ms,
        auth_generation: 1,
        owner: Arc::new(()),
    }
}

#[cfg(test)]
pub(crate) async fn fixture_pump<S: AsyncRead + AsyncWrite + Unpin>(
    socket: WebSocketStream<S>,
    stop: StopHandle,
    slot: FailureSlot,
    ingress: IngressSender,
    feedback: watch::Receiver<Option<LiveDataStamp>>,
    started: oneshot::Sender<Subscription>,
) -> Result<Report, Failure> {
    fixture_pump_generation(socket, 1, stop, slot, ingress, feedback, started).await
}
#[cfg(test)]
pub(crate) async fn fixture_pump_generation<S: AsyncRead + AsyncWrite + Unpin>(
    socket: WebSocketStream<S>,
    generation: u64,
    stop: StopHandle,
    slot: FailureSlot,
    ingress: IngressSender,
    feedback: watch::Receiver<Option<LiveDataStamp>>,
    started: oneshot::Sender<Subscription>,
) -> Result<Report, Failure> {
    fixture_pump_until(
        socket,
        generation,
        stop,
        slot,
        ingress,
        feedback,
        started,
        Utc::now() + chrono::Duration::hours(24),
    )
    .await
}
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn fixture_pump_until<S: AsyncRead + AsyncWrite + Unpin>(
    socket: WebSocketStream<S>,
    generation: u64,
    stop: StopHandle,
    slot: FailureSlot,
    ingress: IngressSender,
    feedback: watch::Receiver<Option<LiveDataStamp>>,
    started: oneshot::Sender<Subscription>,
    run_deadline: chrono::DateTime<Utc>,
) -> Result<Report, Failure> {
    let lease = AccessTokenLease::new(
        broker_finam::AccessToken::new("synthetic-only"),
        std::time::Instant::now(),
        broker_finam::FinamAuthPolicy::default(),
    );
    Driver::new(
        socket,
        lease,
        generation,
        generation,
        stop,
        run_deadline,
        slot,
    )
    .pump(ingress, feedback, started)
    .await
}
