//! Finite episode policy and TWO independent latches. This controller does not
//! reset a model or clear a global STOP. The managed async driver owns the
//! connection tasks; this controller owns their bounded recovery authority.
use super::{Coordinator, PrimaryError};
use crate::{
    auth::PrimaryAuthClass,
    primary::{LiveDataStamp, ResolvedRecovery},
    socket::{
        primary::{Failure, Kind, Subscription},
        LivenessTimeout,
    },
    transport::StopHandle,
};
use serde::Serialize;
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Stage {
    Fault,
    Backoff,
    Auth,
    Connect,
    Subscribe,
    Bootstrap,
    FirstData,
    Resync,
    TransportReady,
    Ready,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Error {
    Rest(super::RestFailure),
    Binding,
    NotRecoverable,
    Stopped,
    RunDeadline,
    EpisodeDeadline,
    AttemptsExhausted,
    RunEpisodesExhausted,
    NoFreshData,
    NoCanonicalReceipt,
    RestTailRequestsExhausted,
    Cancelled,
    Auth(PrimaryAuthClass),
    Transport(Kind),
    Source(PrimaryError),
}
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub episode: u8,
    pub attempt: u8,
    pub generation: u64,
    pub stage: Stage,
    pub original: Failure,
    pub fault_observed_at_ms: i64,
    pub deadline_ms: i64,
    pub transport_unresolved: bool,
    pub canonical_unresolved: bool,
    pub last_error: Option<Error>,
    pub terminal: Option<Error>,
}
#[derive(Clone)]
pub struct Audit(Arc<Mutex<Snapshot>>);
impl Audit {
    fn update(&self, change: impl FnOnce(&mut Snapshot)) {
        change(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()));
    }
    pub fn snapshot(&self) -> Snapshot {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    /// Reconcile a candidate fault that raced with operator STOP/drain. An
    /// already completed episode remains immutable historical evidence.
    pub(crate) fn latch_candidate_fault(&self, failure: &Failure) {
        self.update(|s| {
            if s.generation == failure.generation && s.canonical_unresolved {
                s.transport_unresolved = true;
                if s.last_error.is_none() {
                    s.last_error = Some(match failure.kind {
                        Kind::Auth(class) => Error::Auth(class),
                        kind => Error::Transport(kind),
                    });
                }
                if s.stage == Stage::TransportReady {
                    s.stage = Stage::Failed;
                }
            }
        });
    }
}
pub struct RunBudget {
    episodes: u8,
    config_sha256: String,
    run_deadline_ms: i64,
    stop: StopHandle,
    last: Option<Audit>,
}
pub struct Attempt {
    owner: Arc<()>,
    ordinal: u8,
    generation: u64,
    deadline: Instant,
    reused_transport: bool,
}
impl Attempt {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn ordinal(&self) -> u8 {
        self.ordinal
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
}
pub struct Episode {
    owner: Arc<()>,
    audit: Audit,
    config_sha256: String,
    stop: StopHandle,
    deadline: Instant,
    wall_deadline_ms: i64,
    run_deadline_ms: i64,
    previous_generation: u64,
    attempt: u8,
    subscription: Option<Subscription>,
    completed: bool,
    last_wall_ms: i64,
    stage_mask: u8,
    data_wait: bool,
}
fn recoverable(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Auth(PrimaryAuthClass::Unavailable)
            | Kind::Connect
            | Kind::Read
            | Kind::Eof
            | Kind::Close
            | Kind::Send
            | Kind::Source(PrimaryError::Continuity)
            | Kind::Liveness(
                LivenessTimeout::Pong
                    | LivenessTimeout::TransportIdle
                    | LivenessTimeout::BarsDataSilence
                    | LivenessTimeout::HeartbeatSend
                    | LivenessTimeout::ServerPongFlush
            )
    )
}
fn retryable(error: Error) -> bool {
    matches!(
        error,
        Error::Auth(PrimaryAuthClass::Unavailable)
            | Error::Rest(super::RestFailure::Unavailable | super::RestFailure::Timeout)
            | Error::Transport(Kind::Connect | Kind::Read | Kind::Eof | Kind::Close | Kind::Send)
            | Error::NoFreshData
            | Error::Source(PrimaryError::ReferenceUnproven)
    ) || matches!(error,Error::Transport(kind) if recoverable(kind))
}
impl RunBudget {
    pub fn new(owner: &Coordinator) -> Result<Self, Error> {
        let (config, stop) = owner.session.primary_runtime().ok_or(Error::Binding)?;
        Ok(Self {
            episodes: 0,
            config_sha256: config.fingerprint().into(),
            run_deadline_ms: config.config().run_deadline_utc_ms.min(
                owner
                    .processor()
                    .calendar()
                    .map_or(i64::MAX, |c| c.eod_check_at_ms()),
            ),
            stop,
            last: None,
        })
    }
    pub fn last_audit(&self) -> Option<Audit> {
        self.last.clone()
    }
    /// Synchronous original-fault + canonical latch BEFORE the first await.
    pub fn begin(&mut self, owner: &mut Coordinator, failure: Failure) -> Result<Episode, Error> {
        self.begin_impl(owner, failure, false)
    }
    pub(super) fn begin_data(
        &mut self,
        owner: &mut Coordinator,
        failure: Failure,
    ) -> Result<Episode, Error> {
        if failure.kind != Kind::Source(crate::primary::PrimaryError::Continuity) {
            return Err(Error::Binding);
        }
        self.begin_impl(owner, failure, true)
    }
    pub(super) fn begin_impl_for_writer(
        &mut self,
        owner: &mut Coordinator,
        failure: Failure,
        data_only: bool,
    ) -> Result<Episode, Error> {
        if data_only {
            self.begin_data(owner, failure)
        } else {
            self.begin(owner, failure)
        }
    }
    fn begin_impl(
        &mut self,
        owner: &mut Coordinator,
        failure: Failure,
        data_only: bool,
    ) -> Result<Episode, Error> {
        if failure.generation != owner.processor().generation()
            || owner.processor().config_sha256() != self.config_sha256
        {
            return Err(Error::Binding);
        }
        if !recoverable(failure.kind) {
            return Err(Error::NotRecoverable);
        }
        if self.stop.is_stopped() {
            return Err(Error::Stopped);
        }
        let now = crate::timing::now_ms();
        if now >= self.run_deadline_ms {
            return Err(Error::RunDeadline);
        }
        // A valid new fault fences canonical authority even when no further
        // attempt is authorized. Budget refusal is not restoration success.
        if data_only {
            owner.latch_data_recovery()
        } else {
            owner.latch_recovery()
        }
        .map_err(|_| Error::Binding)?;
        if self.last.as_ref().is_some_and(|a| {
            let s = a.snapshot();
            s.transport_unresolved || s.canonical_unresolved
        }) {
            self.stop.stop();
            return Err(Error::Binding);
        }
        if self.episodes >= 3 {
            self.stop.stop();
            return Err(Error::RunEpisodesExhausted);
        }
        self.episodes += 1;
        let wall_deadline_ms = (now + 45000).min(self.run_deadline_ms);
        let audit = Audit(Arc::new(Mutex::new(Snapshot {
            episode: self.episodes,
            attempt: 0,
            generation: failure.generation,
            stage: Stage::Fault,
            original: failure.clone(),
            fault_observed_at_ms: now,
            deadline_ms: wall_deadline_ms,
            transport_unresolved: true,
            canonical_unresolved: true,
            last_error: None,
            terminal: None,
        })));
        self.last = Some(audit.clone());
        Ok(Episode {
            owner: Arc::new(()),
            audit,
            config_sha256: self.config_sha256.clone(),
            stop: self.stop.clone(),
            deadline: Instant::now() + Duration::from_millis((wall_deadline_ms - now) as u64),
            wall_deadline_ms,
            run_deadline_ms: self.run_deadline_ms,
            previous_generation: failure.generation,
            attempt: 0,
            subscription: None,
            completed: false,
            last_wall_ms: now,
            stage_mask: 0,
            data_wait: owner.processor().data_wait_recovery(),
        })
    }
}
impl Episode {
    pub(crate) fn attempt_failed(&self, error: Error) -> Error {
        self.fail(error)
    }
    pub(crate) fn deadline_ms(&self) -> i64 {
        self.wall_deadline_ms
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    fn deadline_error(&self) -> Error {
        if self.wall_deadline_ms == self.run_deadline_ms {
            Error::RunDeadline
        } else {
            Error::EpisodeDeadline
        }
    }
    pub fn audit(&self) -> Audit {
        self.audit.clone()
    }
    fn check(&mut self) -> Result<(), Error> {
        if self.completed {
            return Err(Error::Binding);
        }
        if let Some(error) = self.audit.snapshot().terminal {
            return Err(error);
        }
        if self.stop.is_stopped() {
            return Err(self.fail(Error::Stopped));
        }
        let now = crate::timing::now_ms();
        if now < self.last_wall_ms {
            return Err(self.fail(Error::Source(PrimaryError::Clock)));
        }
        self.last_wall_ms = now;
        if now >= self.run_deadline_ms {
            return Err(self.fail(Error::RunDeadline));
        }
        if now >= self.wall_deadline_ms || Instant::now() >= self.deadline {
            return Err(self.fail(Error::EpisodeDeadline));
        }
        Ok(())
    }
    fn fail(&self, error: Error) -> Error {
        self.audit.update(|s| {
            if matches!(error, Error::Transport(_) | Error::Auth(_)) {
                s.transport_unresolved = true;
                s.canonical_unresolved = true;
            }
            s.last_error = Some(error);
            s.stage = Stage::Failed;
            if !retryable(error) {
                s.terminal = Some(error);
            }
        });
        error
    }
    fn permit(&self, attempt: &Attempt) -> bool {
        Arc::ptr_eq(&self.owner, &attempt.owner)
            && attempt.ordinal == self.attempt
            && attempt.generation
                == self.previous_generation
                    + if attempt.reused_transport {
                        0
                    } else {
                        u64::from(self.attempt)
                    }
            && (!attempt.reused_transport || self.data_wait)
            && attempt.deadline == self.deadline
    }
    pub async fn next_attempt(&mut self) -> Result<Attempt, Error> {
        self.check()?;
        if self.attempt >= 3 {
            return Err(self.fail(Error::AttemptsExhausted));
        }
        if self.attempt > 0 && self.audit.snapshot().stage != Stage::Failed {
            return Err(self.fail(Error::Binding));
        }
        self.attempt += 1;
        self.stage_mask = 0;
        self.subscription = None;
        let generation = self
            .previous_generation
            .checked_add(u64::from(self.attempt))
            .ok_or_else(|| self.fail(Error::Binding))?;
        self.audit.update(|s| {
            s.attempt = self.attempt;
            s.generation = generation;
            s.stage = Stage::Backoff;
            s.last_error = None;
            s.transport_unresolved = true;
            s.canonical_unresolved = true;
        });
        let backoff = Duration::from_secs([1, 2, 4][usize::from(self.attempt) - 1]);
        tokio::select! {biased;
            _=self.stop.cancelled()=>return Err(self.fail(Error::Stopped)),
            _=tokio::time::sleep_until(self.deadline)=>return Err(self.fail(self.deadline_error())),
            _=tokio::time::sleep(backoff)=>(),
        }
        self.check()?;
        Ok(Attempt {
            owner: self.owner.clone(),
            ordinal: self.attempt,
            generation,
            deadline: self.deadline,
            reused_transport: false,
        })
    }
    pub(crate) fn reuse_bound_attempt(&mut self) -> Result<Attempt, Error> {
        self.check()?;
        if !self.data_wait || self.attempt != 0 {
            return Err(self.fail(Error::Binding));
        }
        self.attempt = 1;
        self.audit.update(|s| {
            s.attempt = 1;
            s.generation = self.previous_generation;
            s.stage = Stage::Resync;
        });
        Ok(Attempt {
            owner: self.owner.clone(),
            ordinal: 1,
            generation: self.previous_generation,
            deadline: self.deadline,
            reused_transport: true,
        })
    }
    /// A physical SUBSCRIBE permits GET resync, NOT LIVE/readiness by history.
    pub(crate) fn bound_for_resync(
        &mut self,
        attempt: &Attempt,
        subscription: &Subscription,
    ) -> Result<(), Error> {
        self.check()?;
        if !self.data_wait
            || !self.permit(attempt)
            || subscription.generation() != attempt.generation
        {
            return Err(self.fail(Error::Binding));
        }
        self.subscription = Some(subscription.retained_for_recovery());
        self.audit.update(|s| s.stage = Stage::Resync);
        Ok(())
    }
    pub(crate) fn waiting_for_data(&mut self) -> Result<(), Error> {
        self.check()?;
        if !self.data_wait || self.subscription.is_none() {
            return Err(self.fail(Error::Binding));
        }
        self.audit.update(|s| s.stage = Stage::FirstData);
        Ok(())
    }
    /// Auth/connect/subscribe/bootstrap/first-DATA and REST proof consume the
    /// SAME episode/run budget. The factory still validates each command grant.
    pub async fn bounded<T>(
        &mut self,
        attempt: &Attempt,
        stage: Stage,
        work: impl Future<Output = Result<T, Error>>,
    ) -> Result<T, Error> {
        self.check()?;
        if !self.permit(attempt) {
            return Err(self.fail(Error::Binding));
        }
        if self.audit.snapshot().stage == Stage::Failed {
            return Err(self.audit.snapshot().last_error.unwrap_or(Error::Binding));
        }
        if !matches!(
            stage,
            Stage::Auth
                | Stage::Connect
                | Stage::Subscribe
                | Stage::Bootstrap
                | Stage::FirstData
                | Stage::Resync
        ) {
            return Err(self.fail(Error::Binding));
        }
        let bit = match stage {
            Stage::Auth => 1,
            Stage::Connect => 2,
            Stage::Subscribe => 4,
            Stage::Bootstrap => 8,
            Stage::FirstData => 16,
            Stage::Resync => 32,
            _ => unreachable!(),
        };
        if self.stage_mask & bit != 0 {
            return Err(self.fail(Error::Binding));
        }
        self.stage_mask |= bit;
        self.audit.update(|s| s.stage = stage);
        let command_deadline = self.deadline.min(Instant::now() + Duration::from_secs(5));
        let result = tokio::select! {biased;
            _=self.stop.cancelled()=>Err(Error::Stopped),
            _=tokio::time::sleep_until(command_deadline)=>Err(if Instant::now()>=self.deadline{self.deadline_error()}else if stage==Stage::FirstData{Error::NoFreshData}else{Error::Cancelled}),
            result=work=>result,
        };
        match result {
            Ok(value) => {
                self.check()?;
                Ok(value)
            }
            Err(error) => Err(self.fail(error)),
        }
    }
    pub fn transport_ready(
        &mut self,
        attempt: &Attempt,
        subscription: Subscription,
        data: Option<&LiveDataStamp>,
    ) -> Result<(), Error> {
        self.check()?;
        if !self.permit(attempt) {
            return Err(self.fail(Error::Binding));
        }
        if self.audit.snapshot().stage == Stage::Failed {
            return Err(self.audit.snapshot().last_error.unwrap_or(Error::Binding));
        }
        let Some(data) = data else {
            return Err(self.fail(Error::NoFreshData));
        };
        let now = crate::timing::now_ms();
        if subscription.generation() != attempt.generation
            || data.generation != attempt.generation
            || data
                .owner
                .as_ref()
                .map_or(true, |owner| !Arc::ptr_eq(owner, &subscription.owner))
            || data.received_at_ms < subscription.sent_at_ms()
            || data.received_at_ms > now
            || data.received_mono > Instant::now()
            || (!self.data_wait && data.received_at_ms - subscription.sent_at_ms() > 5000)
            || (self.data_wait
                && (data.received_at_ms >= self.wall_deadline_ms
                    || now - data.received_at_ms >= 1_200_000))
        {
            return Err(self.fail(Error::Binding));
        }
        self.subscription = Some(subscription);
        self.audit.update(|s| {
            s.transport_unresolved = false;
            s.stage = Stage::TransportReady;
        });
        Ok(())
    }
    pub fn canonical_ready(
        &mut self,
        owner: &Coordinator,
        attempt: &Attempt,
        resolved: &ResolvedRecovery,
    ) -> Result<(), Error> {
        self.check()?;
        if !self.permit(attempt) {
            return Err(self.fail(Error::Binding));
        }
        let subscription = self
            .subscription
            .as_ref()
            .ok_or(Error::NoCanonicalReceipt)?;
        if resolved.config_sha256() != self.config_sha256
            || !resolved.matches_subscription(subscription)
            || !owner.processor().accepts_resolution(resolved)
        {
            return Err(self.fail(Error::Binding));
        }
        self.audit.update(|s| {
            s.canonical_unresolved = false;
            s.stage = Stage::Ready;
            s.last_error = None;
        });
        self.completed = true;
        Ok(())
    }
}
impl Drop for Episode {
    fn drop(&mut self) {
        if !self.completed {
            self.audit.update(|s| {
                if s.terminal.is_none() {
                    s.terminal = Some(Error::Cancelled);
                    s.stage = Stage::Cancelled;
                }
            });
            self.stop.stop();
        }
    }
}

#[cfg(test)]
mod tests;
