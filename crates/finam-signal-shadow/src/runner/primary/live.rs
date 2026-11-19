//! Async composition. Fixed FINAM transport + bounded HTTP roles + one blocking
//! canonical writer; managed reconnect/resync lives in continuous. Post-join
//! completion is semantic only, not native/cgroup/operational acceptance. Explicit
//! entry uses the separate v2 CLI. The Stage-A driver is test-only regression.
use super::{
    bootstrap::BootstrapTicket,
    http::{HttpOutcome, HttpReceipt},
    writer::{Operation, Update, Writer},
    *,
};
use crate::{
    auth::AuthLifecycle,
    primary::{bounded_ingress, IngressSender, LiveDataStamp, PrimaryPolicy},
    socket::primary::{Failure, FailureSlot, PrimarySocket, Report as SocketReport, Subscription},
    transport::StopHandle,
};
use broker_finam::{sparse_m10::ClosedM1RequestPlan, Stage8bP1fO2GetOnlyClientV1};
use chrono::{DateTime, Utc};
use std::{future::Future, sync::Mutex, time::Duration};
use tokio::{
    sync::{oneshot, watch},
    task::{AbortHandle, JoinHandle},
    time::Instant,
};

#[derive(Debug, Clone, serde::Serialize)]
pub enum Cause {
    EndOfSession,
    Calendar(crate::primary::calendar::CalendarError),
    Recovery(super::reconnect::Error),
    RequestedStop,
    RunDeadline,
    Cancelled,
    Binding,
    Socket(Box<Failure>),
    Coordinator(CoordinatorError),
    Http(RestFailure),
    HttpCancelled,
    WriterPanic,
    WriterGone,
    WorkerGone,
    DrainTimeout,
    RoleOverlap,
}
#[derive(Clone, Default)]
pub(super) struct Faults(Arc<Mutex<Option<Cause>>>);
impl Faults {
    pub fn record(&self, cause: Cause) {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(cause);
        }
    }
    pub fn first(&self) -> Option<Cause> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub(crate) enum Role {
    SessionEnd,
    Recovery,
    Bootstrap,
    Proof,
    Audit,
}
#[derive(Clone)]
pub(crate) enum Job {
    Recovery(crate::primary::RecoveryPlan),
    Bootstrap(BootstrapTicket),
    Proof(RestTicket),
    Audit(RestTicket),
}
impl Job {
    pub fn role(&self) -> Role {
        match self {
            Self::Recovery(_) => Role::Recovery,
            Self::Bootstrap(_) => Role::Bootstrap,
            Self::Proof(t) if t.reason() == Some(ExceptionReason::SessionEnd) => Role::SessionEnd,
            Self::Proof(_) => Role::Proof,
            Self::Audit(_) => Role::Audit,
        }
    }
    pub(crate) fn plan(&self) -> Result<ClosedM1RequestPlan, PrimaryError> {
        match self {
            Self::Recovery(t) => Ok(t.request().clone()),
            Self::Bootstrap(t) => Ok(t.plan().clone()),
            Self::Proof(t) | Self::Audit(t) => t.request_plan(),
        }
    }
    pub(crate) fn deadline_ms(&self) -> i64 {
        match self {
            Self::Recovery(t) => t.deadline_ms(),
            Self::Bootstrap(t) => t.deadline_ms(),
            Self::Proof(t) | Self::Audit(t) => t.deadline_ms(),
        }
    }
}
#[derive(Debug, Default, serde::Serialize)]
pub struct RoleTotals {
    pub scheduled: u64,
    pub completed: u64,
    pub cancelled_or_unaccounted: u64,
    /// Lower bound only if cancelled_or_unaccounted is nonzero.
    pub known_get_send_attempts: u64,
    pub complete_response_body_bytes: u64,
    pub completed_command_elapsed_ms: u64,
}
#[derive(Debug, Default, serde::Serialize)]
pub struct HttpTotals {
    pub session_end: RoleTotals,
    pub recovery: RoleTotals,
    pub bootstrap: RoleTotals,
    pub proof: RoleTotals,
    pub audit: RoleTotals,
}
impl HttpTotals {
    fn role(&mut self, role: Role) -> &mut RoleTotals {
        match role {
            Role::SessionEnd => &mut self.session_end,
            Role::Recovery => &mut self.recovery,
            Role::Bootstrap => &mut self.bootstrap,
            Role::Proof => &mut self.proof,
            Role::Audit => &mut self.audit,
        }
    }
    fn completed(&mut self, job: &Job, outcome: Option<&HttpOutcome>) {
        let stats = self.role(job.role());
        if let Some(value) = outcome {
            if value.receipt.executor_completed {
                stats.completed += 1;
            } else {
                stats.cancelled_or_unaccounted += 1;
            }
            stats.known_get_send_attempts += value.receipt.get_attempts;
            stats.completed_command_elapsed_ms += value
                .receipt
                .finished_at_ms
                .saturating_sub(value.receipt.started_at_ms)
                .max(0) as u64;
            if let Some(body) = &value.receipt.body {
                stats.complete_response_body_bytes += body.bytes as u64;
            } else if let Ok(source) = &value.result {
                stats.complete_response_body_bytes += source
                    .evidence()
                    .parts
                    .iter()
                    .map(|p| p.raw_body.len() as u64)
                    .sum::<u64>();
            }
        } else {
            stats.cancelled_or_unaccounted += 1;
        }
    }
}
#[derive(Debug, serde::Serialize)]
pub struct RunReport {
    pub completion: Option<crate::primary::eod::Completion>,
    pub recovery: Vec<super::reconnect::Snapshot>,
    pub unresolved_transport_fault: Option<Failure>,
    pub cause: Cause,
    pub canonical_commits: u64,
    pub http: HttpTotals,
    pub socket_drained: bool,
    pub http_drained: bool,
    pub writer_drained: bool,
    pub all_workers_drained: bool,
    pub session_finalized: bool,
    pub socket: Option<SocketReport>,
    pub full_session_qualified: bool,
}

/// No arbitrary endpoint/transport seam is public. The caller must already own
/// the admitted primary session. Explicit fresh/restart composition is in entry.
pub async fn run(
    owner: Coordinator,
    calendar: crate::primary::calendar::AdmittedCalendar,
    auth: Arc<AuthLifecycle>,
    client: Arc<Stage8bP1fO2GetOnlyClientV1>,
) -> Result<RunReport, Cause> {
    continuous::run(owner, calendar, auth, client).await
}

struct Scope {
    stop: StopHandle,
    faults: Faults,
    socket: Option<AbortHandle>,
    critical: Option<AbortHandle>,
    audit: Option<AbortHandle>,
    completed: bool,
}
impl Scope {
    fn new(stop: StopHandle, faults: Faults) -> Self {
        Self {
            stop,
            faults,
            socket: None,
            critical: None,
            audit: None,
            completed: false,
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if !self.completed {
            self.faults.record(Cause::Cancelled);
        }
        self.stop.stop();
        for handle in [&self.socket, &self.critical, &self.audit]
            .into_iter()
            .flatten()
        {
            handle.abort();
        }
    }
}
struct HttpJob {
    job: Job,
    worker: JoinHandle<Option<HttpOutcome>>,
}
#[cfg(test)]
fn launch<F, Fut>(
    job: Job,
    fetch: F,
    stop: StopHandle,
    faults: Faults,
    run_deadline: i64,
) -> HttpJob
where
    F: Fn(Job, StopHandle) -> Fut + Send + 'static,
    Fut: Future<Output = HttpOutcome> + Send + 'static,
{
    let request = job.clone();
    let deadline_ms = job.deadline_ms().min(run_deadline);
    let deadline = Instant::now()
        + Duration::from_millis(deadline_ms.saturating_sub(crate::timing::now_ms()).max(0) as u64);
    let worker = tokio::spawn(async move {
        let outcome = tokio::select! {biased;
            _=stop.cancelled()=>None,
            // The executor owns the exact deadline/classification; this outer
            // watchdog allows one millisecond for its completed outcome. If it
            // is instead cancelled/unknown, never relabel it as audit timeout.
            _=tokio::time::sleep_until(deadline+Duration::from_millis(1))=>{
                faults.record(Cause::HttpCancelled);stop.stop();None
            }
            value=fetch(request,stop.clone())=>Some(value),
        };
        if let Some(value) = &outcome {
            if let Err(error) = &value.result {
                if !matches!(error, RestFailure::Timeout | RestFailure::Unavailable) {
                    faults.record(Cause::Http(*error));
                    stop.stop();
                }
            }
        }
        outcome
    });
    HttpJob { job, worker }
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn schedule<F, Fut>(
    update: Update,
    feedback: &watch::Sender<Option<LiveDataStamp>>,
    fetch: &F,
    scope: &mut Scope,
    critical: &mut Option<HttpJob>,
    audit: &mut Option<HttpJob>,
    totals: &mut HttpTotals,
    run_deadline: i64,
) -> Result<(bool, u64, Instant), Cause>
where
    F: Fn(Job, StopHandle) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = HttpOutcome> + Send + 'static,
{
    feedback.send(update.data).map_err(|_| Cause::WorkerGone)?;
    let mandatory = match (update.bootstrap, update.proof) {
        (Some(t), None) => Some(Job::Bootstrap(t)),
        (None, Some(t)) => Some(Job::Proof(t)),
        (None, None) => None,
        _ => return Err(Cause::RoleOverlap),
    };
    if let Some(job) = mandatory {
        if critical.is_some() {
            return Err(Cause::RoleOverlap);
        }
        totals.role(job.role()).scheduled += 1;
        let job = launch(
            job,
            fetch.clone(),
            scope.stop.clone(),
            scope.faults.clone(),
            run_deadline,
        );
        scope.critical = Some(job.worker.abort_handle());
        *critical = Some(job);
    }
    if let Some(ticket) = update.audit {
        if audit.is_some() {
            return Err(Cause::RoleOverlap);
        }
        totals.audit.scheduled += 1;
        let job = launch(
            Job::Audit(ticket),
            fetch.clone(),
            scope.stop.clone(),
            scope.faults.clone(),
            run_deadline,
        );
        scope.audit = Some(job.worker.abort_handle());
        *audit = Some(job);
    }
    let wake = Instant::now()
        + Duration::from_millis(
            update
                .next_wake_ms
                .saturating_sub(crate::timing::now_ms())
                .max(1) as u64,
        );
    Ok((update.bootstrap_pending, update.committed, wake))
}
async fn poll_http(
    job: &mut Option<HttpJob>,
) -> Option<Result<Option<HttpOutcome>, tokio::task::JoinError>> {
    match job {
        Some(job) => Some((&mut job.worker).await),
        None => std::future::pending().await,
    }
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
async fn drive<S, SFut, F, Fut>(
    owner: Coordinator,
    reference_start: DateTime<Utc>,
    policy: PrimaryPolicy,
    stop: StopHandle,
    slot: FailureSlot,
    faults: Faults,
    socket: S,
    fetch: F,
) -> Result<RunReport, Cause>
where
    S: FnOnce(
            IngressSender,
            watch::Receiver<Option<LiveDataStamp>>,
            oneshot::Sender<Subscription>,
        ) -> SFut
        + Send
        + 'static,
    SFut: Future<Output = Result<SocketReport, Failure>> + Send + 'static,
    F: Fn(Job, StopHandle) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = HttpOutcome> + Send + 'static,
{
    let (config, _) = owner.session.primary_runtime().ok_or(Cause::Binding)?;
    let deadline_ms = config.config().run_deadline_utc_ms;
    let run_deadline = Instant::now()
        + Duration::from_millis(deadline_ms.saturating_sub(crate::timing::now_ms()).max(0) as u64);
    let (tx, mut ingress) = bounded_ingress(&policy, owner.processor.generation())
        .map_err(|e| Cause::Coordinator(CoordinatorError::Source(e)))?;
    let mut writer = Writer::spawn(
        owner,
        reference_start,
        stop.clone(),
        faults.clone(),
        slot.clone(),
    )?;
    let mut scope = Scope::new(stop.clone(), faults.clone());
    let (feedback, acks) = watch::channel(None);
    let (start, mut started) = oneshot::channel();
    let socket_faults = faults.clone();
    let mut socket_job = Some(tokio::spawn(async move {
        let result = socket(tx, acks, start).await;
        if let Err(error) = &result {
            socket_faults.record(Cause::Socket(Box::new(error.clone())));
        }
        result
    }));
    scope.socket = socket_job.as_ref().map(|h| h.abort_handle());
    let mut critical = None;
    let mut audit = None;
    let mut totals = HttpTotals::default();
    let mut committed = 0;
    let mut socket_report = None;
    let start_result = tokio::select! {biased;
        _=stop.cancelled()=>Err(Cause::RequestedStop),
        _=tokio::time::sleep_until(run_deadline)=>Err(Cause::RunDeadline),
        value=&mut started=>match value{Ok(receipt)=>writer.call(Operation::Bind(receipt)).await,Err(_)=>Err(Cause::WorkerGone)},
    };
    let cause = match start_result {
        Err(cause) => cause,
        Ok(update) => {
            let mut state = schedule(
                update,
                &feedback,
                &fetch,
                &mut scope,
                &mut critical,
                &mut audit,
                &mut totals,
                deadline_ms,
            );
            loop {
                let (pending, done, wake) = match state {
                    Ok(value) => value,
                    Err(cause) => break cause,
                };
                committed = done;
                let operation = tokio::select! {biased;
                    _=stop.cancelled()=>Err(Cause::RequestedStop),
                    _=tokio::time::sleep_until(run_deadline)=>Err(Cause::RunDeadline),
                    joined=async{match &mut socket_job{Some(h)=>h.await,None=>std::future::pending().await}}=>{
                        socket_job.take();scope.socket=None;
                        match joined{Ok(Ok(report))=>{socket_report=Some(report);Err(Cause::WorkerGone)},Ok(Err(error))=>Err(Cause::Socket(Box::new(error))),Err(_)=>Err(Cause::WorkerGone)}
                    }
                    result=poll_http(&mut critical)=>{
                        let job=critical.take().ok_or(Cause::WorkerGone)?;scope.critical=None;
                        match result{Some(Ok(Some(outcome)))=>{totals.completed(&job.job,Some(&outcome));Ok(Operation::Rest(job.job,Box::new(outcome)))},
                            _=>{totals.completed(&job.job,None);Err(Cause::HttpCancelled)}}
                    }
                    result=poll_http(&mut audit)=>{
                        let job=audit.take().ok_or(Cause::WorkerGone)?;scope.audit=None;
                        match result{Some(Ok(Some(outcome)))=>{totals.completed(&job.job,Some(&outcome));Ok(Operation::Rest(job.job,Box::new(outcome)))},
                            _=>{totals.completed(&job.job,None);Err(Cause::HttpCancelled)}}
                    }
                    frame=ingress.receive(&stop),if !pending=>match frame{
                        Ok(Some(frame))=>Ok(Operation::Frame(frame)),Ok(None)=>Err(Cause::RequestedStop),Err(e)=>Err(Cause::Coordinator(CoordinatorError::Source(e))),
                    },
                    _=tokio::time::sleep_until(wake),if !pending=>Ok(Operation::Tick),
                };
                let operation = match operation {
                    Ok(op) => op,
                    Err(cause) => break cause,
                };
                state = match writer.call(operation).await {
                    Ok(update) => schedule(
                        update,
                        &feedback,
                        &fetch,
                        &mut scope,
                        &mut critical,
                        &mut audit,
                        &mut totals,
                        deadline_ms,
                    ),
                    Err(cause) => Err(cause),
                };
            }
        }
    };
    if !matches!(cause, Cause::RequestedStop) {
        faults.record(cause);
    }
    if let Some(fault) = slot.first() {
        faults.record(Cause::Socket(Box::new(fault)));
    }
    stop.stop();
    let drain_deadline = writer.drain_deadline();
    let mut http_drained = true;
    for job in [&mut critical, &mut audit]
        .into_iter()
        .filter_map(Option::take)
    {
        let mut job = job;
        match tokio::time::timeout_at(drain_deadline, &mut job.worker).await {
            Ok(Ok(outcome)) => totals.completed(&job.job, outcome.as_ref()),
            Ok(Err(_)) => {
                totals.completed(&job.job, None);
                faults.record(Cause::WorkerGone);
            }
            Err(_) => {
                http_drained = false;
                totals.completed(&job.job, None);
                faults.record(Cause::DrainTimeout);
                job.worker.abort();
            }
        }
    }
    scope.critical = None;
    scope.audit = None;
    let socket_drained = if let Some(mut job) = socket_job.take() {
        match tokio::time::timeout_at(drain_deadline, &mut job).await {
            Ok(Ok(Ok(report))) => {
                socket_report = Some(report);
                true
            }
            Ok(Ok(Err(error))) => {
                faults.record(Cause::Socket(Box::new(error)));
                true
            }
            Ok(Err(_)) => {
                faults.record(Cause::WorkerGone);
                true
            }
            Err(_) => {
                faults.record(Cause::DrainTimeout);
                job.abort();
                false
            }
        }
    } else {
        true
    };
    scope.socket = None;
    let cause = faults.first().unwrap_or(Cause::RequestedStop);
    let final_result = if Instant::now() < drain_deadline {
        writer
            .call(Operation::Final(
                serde_json::json!({"event":"primary_async_shutdown","cause":cause,
            "socket_drained":socket_drained,"http_drained":http_drained,"http":totals,
            "canonical_commits":committed,"full_session_qualified":false}),
            ))
            .await
            .ok()
    } else {
        None
    };
    let session_finalized = final_result.as_ref().is_some_and(|update| update.finalized);
    if let Some(update) = final_result {
        committed = update.committed;
    }
    let writer_drained = writer.join(drain_deadline).await;
    let all_workers_drained = socket_drained && http_drained && writer_drained;
    scope.completed = all_workers_drained;
    Ok(RunReport {
        completion: None,
        recovery: vec![],
        unresolved_transport_fault: None,
        cause: faults.first().unwrap_or(cause),
        canonical_commits: committed,
        http: totals,
        socket_drained,
        http_drained,
        writer_drained,
        all_workers_drained,
        session_finalized,
        socket: socket_report,
        full_session_qualified: false,
    })
}

pub(crate) mod continuous;
#[cfg(test)]
mod tests;
