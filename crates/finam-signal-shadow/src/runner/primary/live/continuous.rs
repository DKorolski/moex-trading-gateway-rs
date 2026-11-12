//! Managed generations. Network loss closes the shared live fence, not the
//! independent run STOP. All model/replay/journal work remains in one writer.
use super::*;
use crate::{
    primary::{ContinuityGate, IngressReceiver},
    runner::primary::reconnect::{self, Attempt, Episode},
};

pub(super) struct Open {
    pub generation: u64,
    /// Establishment/recovery budget, not the subscription lifetime.
    pub deadline_ms: i64,
    pub run_deadline_ms: i64,
    pub stop: StopHandle,
    pub slot: FailureSlot,
    pub tx: IngressSender,
    pub feedback: watch::Receiver<Option<LiveDataStamp>>,
    pub started: oneshot::Sender<Subscription>,
}
pub(super) async fn run(
    mut owner: Coordinator,
    calendar: crate::primary::calendar::AdmittedCalendar,
    auth: Arc<AuthLifecycle>,
    client: Arc<Stage8bP1fO2GetOnlyClientV1>,
) -> Result<RunReport, Cause> {
    if !auth.binds(client.account_id()) {
        return Err(Cause::Binding);
    }
    let ws_first_proof = owner.processor().rest_precommit_ws_first();
    let nonblocking_rest = owner.processor().rest_nonblocking();
    let reference_start =
        DateTime::from_timestamp_millis(calendar.reference_start_ms()).ok_or(Cause::Binding)?;
    calendar
        .validate_entry(crate::timing::now_ms())
        .map_err(Cause::Calendar)?;
    owner.bind_calendar(calendar).map_err(Cause::Coordinator)?;
    let connection_auth = auth.clone();
    let factory = move |request: Open| {
        let auth = connection_auth.clone();
        async move {
            let error = |kind| Failure {
                generation: request.generation,
                kind,
                liveness: None,
            };
            let remaining = request
                .deadline_ms
                .saturating_sub(crate::timing::now_ms())
                .min(5000);
            if remaining <= 0 {
                return Err(error(crate::socket::primary::Kind::RunDeadline));
            }
            let grant = tokio::select! {biased;
                _=request.stop.cancelled()=>return Err(error(crate::socket::primary::Kind::Cancelled)),
                grant=auth.grant_primary_budget(Duration::from_millis(remaining as u64))=>grant.map_err(|e|error(crate::socket::primary::Kind::Auth(e.class)))?,
            };
            let socket = PrimarySocket::connect(
                grant,
                request.generation,
                request.stop,
                DateTime::from_timestamp_millis(request.deadline_ms)
                    .ok_or_else(|| error(crate::socket::primary::Kind::Clock))?,
                DateTime::from_timestamp_millis(request.run_deadline_ms)
                    .ok_or_else(|| error(crate::socket::primary::Kind::Clock))?,
                request.slot,
            )
            .await?;
            socket
                .pump(request.tx, request.feedback, request.started)
                .await
        }
    };
    let fetch = move |job: Job, stop: StopHandle| {
        let auth = auth.clone();
        let client = client.clone();
        async move {
            match job.plan() {
                Ok(plan) => {
                    if nonblocking_rest && matches!(&job, Job::Proof(_) | Job::Audit(_)) {
                        super::super::http::execute_nonblocking_comparison(
                            &auth,
                            &client,
                            &plan,
                            job.deadline_ms(),
                            &stop,
                        )
                        .await
                    } else if ws_first_proof && matches!(&job, Job::Proof(_)) {
                        super::super::http::execute_ws_first_proof(
                            &auth,
                            &client,
                            &plan,
                            job.deadline_ms(),
                            &stop,
                        )
                        .await
                    } else {
                        super::super::http::execute(&auth, &client, &plan, job.deadline_ms(), &stop)
                            .await
                    }
                }
                Err(_) => HttpOutcome {
                    result: Err(RestFailure::Integrity),
                    receipt: HttpReceipt {
                        started_at_ms: crate::timing::now_ms(),
                        finished_at_ms: crate::timing::now_ms(),
                        deadline_ms: job.deadline_ms(),
                        get_attempts: 0,
                        auth_generation: None,
                        body: None,
                        executor_completed: true,
                    },
                },
            }
        }
    };
    drive(owner, reference_start, factory, fetch).await
}

struct WorkGuard {
    slot: FailureSlot,
    generation: u64,
    root: StopHandle,
    complete: bool,
}
impl Drop for WorkGuard {
    fn drop(&mut self) {
        if !self.complete && !self.root.is_stopped() {
            self.slot.record(Failure {
                generation: self.generation,
                kind: crate::socket::primary::Kind::Cancelled,
                liveness: None,
            });
        }
    }
}
struct Connection {
    worker: Option<JoinHandle<Result<SocketReport, Failure>>>,
    stop: StopHandle,
    slot: FailureSlot,
    ingress: IngressReceiver,
    feedback: watch::Sender<Option<LiveDataStamp>>,
    started: Option<oneshot::Receiver<Subscription>>,
}
impl Connection {
    fn open<G, GF>(
        generation: u64,
        deadline_ms: i64,
        run_deadline_ms: i64,
        root: StopHandle,
        slot: FailureSlot,
        policy: &PrimaryPolicy,
        factory: G,
    ) -> Result<Self, Cause>
    where
        G: Fn(Open) -> GF + Send + 'static,
        GF: Future<Output = Result<SocketReport, Failure>> + Send + 'static,
    {
        let stop = StopHandle::connection_scope();
        let (tx, ingress) = bounded_ingress(policy, generation)
            .map_err(|e| Cause::Coordinator(CoordinatorError::Source(e)))?;
        let (feedback, acks) = watch::channel(None);
        let (started, srx) = oneshot::channel();
        let request = Open {
            generation,
            deadline_ms,
            run_deadline_ms,
            stop: stop.clone(),
            slot: slot.clone(),
            tx,
            feedback: acks,
            started,
        };
        let worker_stop = stop.clone();
        let worker_slot = slot.clone();
        let worker = tokio::spawn(async move {
            let mut guard = WorkGuard {
                slot: worker_slot.clone(),
                generation,
                root: root.clone(),
                complete: false,
            };
            let future = factory(request);
            tokio::pin!(future);
            let result = tokio::select! {biased;
                _=root.cancelled()=>{worker_stop.stop();(&mut future).await},
                result=&mut future=>result,
            };
            if let Err(failure) = &result {
                if !root.is_stopped() {
                    worker_slot.record(failure.clone());
                }
            }
            guard.complete = true;
            result
        });
        Ok(Self {
            worker: Some(worker),
            stop,
            slot,
            ingress,
            feedback,
            started: Some(srx),
        })
    }
    async fn drain(&mut self, deadline: Instant) -> (bool, Option<SocketReport>) {
        self.stop.stop();
        let Some(mut worker) = self.worker.take() else {
            return (true, None);
        };
        match tokio::time::timeout_at(deadline, &mut worker).await {
            Ok(Ok(Ok(report))) => (true, Some(report)),
            Ok(Ok(Err(failure))) => {
                self.slot.record(failure);
                (true, None)
            }
            Ok(Err(_)) => (false, None),
            Err(_) => {
                worker.abort();
                (false, None)
            }
        }
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.stop.stop();
        if let Some(worker) = &self.worker {
            worker.abort();
        }
    }
}

fn launch_managed_mode<F, FF>(
    job: Job,
    fetch: F,
    child: StopHandle,
    root: StopHandle,
    faults: Faults,
    nonblocking: bool,
) -> HttpJob
where
    F: Fn(Job, StopHandle) -> FF + Send + 'static,
    FF: Future<Output = HttpOutcome> + Send + 'static,
{
    let request = job.clone();
    let started = crate::timing::now_ms();
    let deadline = Instant::now()
        + Duration::from_millis(
            job.deadline_ms()
                .saturating_sub(crate::timing::now_ms())
                .max(0) as u64,
        )
        + Duration::from_millis(1);
    let worker = tokio::spawn(async move {
        // Cancellation is classified by the executor, so a fully validated
        // grant is distinguishable from a cancelled/partially issued grant.
        // Optional REST comparison has a command-local cancellation scope.
        // A data error must not close the healthy WS connection. Global STOP
        // and connection retirement are still relayed and every task joined.
        let command = if nonblocking {
            StopHandle::connection_scope()
        } else {
            child.clone()
        };
        let future = fetch(request.clone(), command.clone());
        tokio::pin!(future);
        let operation = async {
            tokio::select! {biased;
                _=root.cancelled()=>{command.stop();(&mut future).await},
                _=child.cancelled()=>{command.stop();(&mut future).await},
                result=&mut future=>result,
            }
        };
        let result = tokio::time::timeout_at(deadline, operation).await;
        match result {
            Ok(outcome) => {
                if let Err(error) = &outcome.result {
                    let harmless_get_cancel = *error == RestFailure::Cancelled
                        && child.is_stopped()
                        && outcome.receipt.auth_generation.is_some();
                    if !root.is_stopped()
                        && !harmless_get_cancel
                        && (!nonblocking || outcome.authority_rejected())
                        && !matches!(error, RestFailure::Unavailable | RestFailure::Timeout)
                    {
                        faults.record(Cause::Http(*error));
                        root.stop();
                    }
                }
                Some(outcome)
            }
            Err(_) => {
                command.stop();
                if nonblocking {
                    return Some(HttpOutcome {
                        result: Err(RestFailure::Timeout),
                        receipt: HttpReceipt {
                            started_at_ms: started,
                            finished_at_ms: crate::timing::now_ms(),
                            deadline_ms: request.deadline_ms(),
                            get_attempts: 0,
                            auth_generation: None,
                            body: None,
                            executor_completed: false,
                        },
                    });
                }
                if !root.is_stopped() {
                    faults.record(Cause::HttpCancelled);
                    root.stop();
                }
                None
            }
        }
    });
    HttpJob { job, worker }
}

struct Engine<F> {
    writer: Writer,
    connection: Connection,
    fetch: F,
    root: StopHandle,
    faults: Faults,
    gate: ContinuityGate,
    scope: Scope,
    policy: PrimaryPolicy,
    deadline: Instant,
    run_deadline_ms: i64,
    eod_at: Option<Instant>,
    eod_ms: Option<i64>,
    config_sha256: String,
    calendar_sha256: Option<String>,
    clock_uncertainty_ms: u64,
    critical: Option<HttpJob>,
    audit: Option<HttpJob>,
    totals: HttpTotals,
    data: Option<LiveDataStamp>,
    bootstrap_pending: bool,
    wake: Instant,
    committed: u64,
    recovery: Vec<reconnect::Audit>,
    socket_report: Option<SocketReport>,
    all_connections_drained: bool,
    drained_http: Vec<serde_json::Value>,
}
impl<F, FF> Engine<F>
where
    F: Fn(Job, StopHandle) -> FF + Clone + Send + 'static,
    FF: Future<Output = HttpOutcome> + Send + 'static,
{
    async fn apply(&mut self, op: Operation, limit: Option<Instant>) -> Result<Update, Cause> {
        let update = self
            .writer
            .call_until(op, Some(limit.unwrap_or(self.deadline).min(self.deadline)))
            .await?;
        self.committed = update.committed;
        self.data = update.data.clone();
        self.bootstrap_pending = update.bootstrap_pending;
        self.wake = Instant::now()
            + Duration::from_millis(
                update
                    .next_wake_ms
                    .saturating_sub(crate::timing::now_ms())
                    .max(1) as u64,
            );
        if let Some(failure) = &update.transport_failure {
            return Err(Cause::Socket(Box::new(failure.clone())));
        }
        if update.episode.is_none() {
            if let Some(error) = update.recovery_error {
                return Err(Cause::Recovery(error));
            }
        }
        if !self.connection.stop.is_stopped() && self.gate.snapshot().failure.is_none() {
            self.connection
                .feedback
                .send(update.data.clone())
                .map_err(|_| Cause::WorkerGone)?;
        }
        let mandatory = match (&update.bootstrap, &update.proof) {
            (Some(t), None) => Some(Job::Bootstrap(t.clone())),
            (None, Some(t)) => Some(Job::Proof(t.clone())),
            (None, None) => None,
            _ => return Err(Cause::RoleOverlap),
        };
        if let Some(job) = mandatory {
            self.schedule(job)?;
        }
        if let Some(ticket) = &update.audit {
            self.schedule(Job::Audit(ticket.clone()))?;
        }
        Ok(update)
    }
    fn schedule(&mut self, job: Job) -> Result<(), Cause> {
        let audit = matches!(job, Job::Audit(_));
        let target = if audit {
            &mut self.audit
        } else {
            &mut self.critical
        };
        if target.is_some() {
            return Err(Cause::RoleOverlap);
        }
        self.totals.role(job.role()).scheduled += 1;
        let nonblocking =
            self.policy.rest_nonblocking() && matches!(&job, Job::Proof(_) | Job::Audit(_));
        let job = launch_managed_mode(
            job,
            self.fetch.clone(),
            self.connection.stop.clone(),
            self.root.clone(),
            self.faults.clone(),
            nonblocking,
        );
        if audit {
            self.scope.audit = Some(job.worker.abort_handle());
        } else {
            self.scope.critical = Some(job.worker.abort_handle());
        }
        *target = Some(job);
        Ok(())
    }
    async fn poll(&mut self, phase: Option<Instant>, tick: bool) -> Result<Update, Cause> {
        if self.root.is_stopped() {
            return Err(self.faults.first().unwrap_or(Cause::RequestedStop));
        }
        if let Some(failure) = self.gate.snapshot().failure {
            return Err(Cause::Socket(Box::new(failure)));
        }
        if self.eod_ms.is_some_and(|t| crate::timing::now_ms() >= t) {
            return Err(Cause::EndOfSession);
        }
        let operation = tokio::select! {biased;
            _=self.root.cancelled()=>return Err(self.faults.first().unwrap_or(Cause::RequestedStop)),
            _=async{match self.eod_at{Some(at)=>tokio::time::sleep_until(at).await,None=>std::future::pending().await}}=>return Err(Cause::EndOfSession),
            _=tokio::time::sleep_until(self.deadline)=>return Err(Cause::RunDeadline),
            _=async{match phase{Some(at)=>tokio::time::sleep_until(at).await,None=>std::future::pending().await}}=>return Err(Cause::Recovery(reconnect::Error::NoFreshData)),
            joined=async{match &mut self.connection.worker{Some(h)=>h.await,None=>std::future::pending().await}}=>{
                self.connection.worker.take();self.scope.socket=None;
                match joined{Ok(Err(failure))=>return Err(Cause::Socket(Box::new(failure))),Ok(Ok(report))=>{self.socket_report=Some(report);return Err(Cause::WorkerGone);},Err(_)=>return Err(Cause::WorkerGone)}
            }
            receipt=async{match &mut self.connection.started{Some(rx)=>Some(rx.await),None=>std::future::pending().await}}=>{
                self.connection.started.take();Operation::Bind(receipt.ok_or(Cause::WorkerGone)?.map_err(|_|self.gate.snapshot().failure.map(|e|Cause::Socket(Box::new(e))).unwrap_or(Cause::WorkerGone))?)
            }
            result=poll_http(&mut self.critical)=>{
                let job=self.critical.take().ok_or(Cause::WorkerGone)?;self.scope.critical=None;
                match result{Some(Ok(Some(outcome)))=>{self.totals.completed(&job.job,Some(&outcome));Operation::Rest(job.job,Box::new(outcome))},_=>{self.totals.completed(&job.job,None);return Err(Cause::HttpCancelled);}}
            }
            result=poll_http(&mut self.audit)=>{
                let job=self.audit.take().ok_or(Cause::WorkerGone)?;self.scope.audit=None;
                match result{Some(Ok(Some(outcome)))=>{self.totals.completed(&job.job,Some(&outcome));Operation::Rest(job.job,Box::new(outcome))},_=>{self.totals.completed(&job.job,None);return Err(Cause::HttpCancelled);}}
            }
            frame=self.connection.ingress.receive(&self.root),if !self.bootstrap_pending=>match frame{
                Ok(Some(frame))=>Operation::Frame(frame),Ok(None)=>return Err(Cause::RequestedStop),Err(error)=>return Err(ingress_failure(&self.policy,&self.gate,&self.connection.slot,error)),
            },
            _=tokio::time::sleep_until(self.wake),if tick&&!self.bootstrap_pending=>Operation::Tick,
        };
        let phase = if matches!(&operation, Operation::Rest(Job::Recovery(_), _)) {
            Some(
                phase
                    .unwrap_or(self.deadline)
                    .min(Instant::now() + Duration::from_secs(10)),
            )
        } else {
            phase
        };
        self.apply(operation, phase).await
    }
    async fn drain_jobs(&mut self, deadline: Instant, audit_for_eod: bool) -> bool {
        let mut drained = true;
        for job in [&mut self.critical, &mut self.audit]
            .into_iter()
            .filter_map(Option::take)
        {
            let mut job = job;
            match tokio::time::timeout_at(deadline, &mut job.worker).await {
                Ok(Ok(outcome)) => {
                    self.totals.completed(&job.job, outcome.as_ref());
                    if let Some(outcome) = outcome {
                        self.drained_http.push(serde_json::json!({"role":job.job.role(),"class":outcome.result.as_ref().err(),"receipt":outcome.receipt,
                            "source_sha256":outcome.result.as_ref().ok().map(|s|s.sha256())}));
                        let safe = (self.policy.rest_nonblocking()
                            && matches!(&job.job, Job::Proof(_) | Job::Audit(_))
                            && !outcome.authority_rejected())
                            || matches!(
                                &outcome.result,
                                Ok(_) | Err(RestFailure::Unavailable | RestFailure::Timeout)
                            )
                            || (matches!(&outcome.result, Err(RestFailure::Cancelled))
                                && outcome.receipt.auth_generation.is_some());
                        if !safe {
                            self.faults.record(Cause::Http(
                                outcome
                                    .result
                                    .as_ref()
                                    .err()
                                    .copied()
                                    .unwrap_or(RestFailure::Unknown),
                            ));
                        }
                        if audit_for_eod {
                            if let Job::Audit(ticket) = job.job {
                                if let Err(cause) = self
                                    .writer
                                    .call(Operation::DrainAudit(ticket, Box::new(outcome)))
                                    .await
                                {
                                    self.faults.record(cause);
                                }
                            }
                        }
                    } else {
                        self.faults.record(Cause::HttpCancelled);
                    }
                }
                Ok(Err(_)) => {
                    self.totals.completed(&job.job, None);
                    self.faults.record(Cause::WorkerGone);
                }
                Err(_) => {
                    self.totals.completed(&job.job, None);
                    job.worker.abort();
                    drained = false;
                    self.faults.record(Cause::DrainTimeout);
                }
            }
        }
        self.scope.critical = None;
        self.scope.audit = None;
        drained
    }
    async fn retire(&mut self, deadline: Instant) -> Result<(), Cause> {
        self.connection.stop.stop();
        let jobs = self.drain_jobs(deadline, false).await;
        let (socket, report) = self.connection.drain(deadline).await;
        self.scope.socket = None;
        if let Some(report) = report {
            self.socket_report = Some(report);
        }
        self.all_connections_drained &= socket && jobs;
        if !socket || !jobs {
            return Err(Cause::DrainTimeout);
        }
        if self.root.is_stopped() {
            return Err(self.faults.first().unwrap_or(Cause::RequestedStop));
        }
        Ok(())
    }
    async fn recover<G, GF>(&mut self, failure: Failure, factory: &G) -> Result<(), Cause>
    where
        G: Fn(Open) -> GF + Clone + Send + 'static,
        GF: Future<Output = Result<SocketReport, Failure>> + Send + 'static,
    {
        // The physical driver's slot already latched the cross-task fault.
        // The writer now atomically retires old tickets and opens the episode.
        let mut update = self.apply(Operation::BeginRecovery(failure), None).await?;
        let mut episode = update.episode.take().ok_or(Cause::Binding)?;
        self.recovery.push(episode.audit());
        self.retire(
            episode
                .deadline()
                .min(Instant::now() + Duration::from_secs(5)),
        )
        .await?;
        loop {
            let attempt = episode.next_attempt().await.map_err(Cause::Recovery)?;
            let slot = FailureSlot::with_gate(self.gate.clone(), self.root.clone());
            self.apply(
                Operation::PrepareAttempt {
                    generation: attempt.generation(),
                    slot: slot.clone(),
                },
                Some(episode.deadline()),
            )
            .await?;
            self.connection = Connection::open(
                attempt.generation(),
                episode.deadline_ms(),
                self.run_deadline_ms,
                self.root.clone(),
                slot,
                &self.policy,
                factory.clone(),
            )?;
            self.scope.socket = self.connection.worker.as_ref().map(|h| h.abort_handle());
            let result = self.recover_attempt(&mut episode, &attempt).await;
            match result {
                Ok(()) => {
                    let mut update = self
                        .apply(
                            Operation::ResolveRecovery(Box::new((episode, attempt))),
                            None,
                        )
                        .await?;
                    if let Some(returned) = update.episode.take() {
                        episode = returned;
                        episode.attempt_failed(
                            update.recovery_error.unwrap_or(reconnect::Error::Source(
                                PrimaryError::ReferenceUnproven,
                            )),
                        );
                    } else {
                        return Ok(());
                    }
                }
                Err(cause) => {
                    if self.root.is_stopped() {
                        return Err(self.faults.first().unwrap_or(cause));
                    }
                    let error = match cause {
                        Cause::Socket(failure) => match failure.kind {
                            crate::socket::primary::Kind::Auth(class) => {
                                reconnect::Error::Auth(class)
                            }
                            kind => reconnect::Error::Transport(kind),
                        },
                        Cause::Recovery(error) => error,
                        other => return Err(other),
                    };
                    episode.attempt_failed(error);
                }
            }
            self.retire(
                episode
                    .deadline()
                    .min(Instant::now() + Duration::from_secs(5)),
            )
            .await?;
        }
    }
    async fn recover_attempt(
        &mut self,
        episode: &mut Episode,
        attempt: &Attempt,
    ) -> Result<(), Cause> {
        let subscription = loop {
            let mut update = self.poll(Some(episode.deadline()), false).await?;
            if let Some(receipt) = update.subscription.take() {
                break receipt;
            }
        };
        let first_data_deadline = (Instant::now()
            + Duration::from_millis(
                (subscription.sent_at_ms() + 5000 - crate::timing::now_ms()).max(0) as u64,
            ))
        .min(episode.deadline());
        while self.data.is_none() {
            self.poll(Some(first_data_deadline), false).await?;
        }
        episode
            .transport_ready(attempt, subscription, self.data.as_ref())
            .map_err(Cause::Recovery)?;
        let cutover = crate::timing::now_ms().div_euclid(600000) * 600000;
        if self.policy.rest_tail_recovery() {
            return self.recover_rest_tail(episode, cutover).await;
        }
        let mut update = self
            .apply(Operation::RecoveryPlan(cutover), Some(episode.deadline()))
            .await?;
        let plan = update.recovery_plan.take().ok_or(Cause::Binding)?;
        self.schedule(Job::Recovery(plan))?;
        while self.critical.is_some() {
            self.poll(Some(episode.deadline()), false).await?;
        }
        Ok(())
    }
    async fn recover_rest_tail(&mut self, episode: &Episode, cutover: i64) -> Result<(), Cause> {
        let mut previous = None;
        loop {
            let mut update = self
                .apply(
                    Operation::RecoveryRequest { cutover, previous },
                    Some(episode.deadline()),
                )
                .await?;
            let plan = update.recovery_plan.take().ok_or(Cause::Binding)?;
            self.schedule(Job::Recovery(plan.clone()))?;
            let mut completion = None;
            while self.critical.is_some() {
                completion = Some(self.poll(Some(episode.deadline()), false).await?);
            }
            let update = completion.ok_or(Cause::Binding)?;
            if update.recovery_ready {
                return Ok(());
            }
            if update.recovery_wait.is_none() {
                return Err(Cause::Binding);
            }
            let scope = plan.scope().ok_or(Cause::Binding)?;
            if scope.ordinal >= crate::primary::MAX_REST_READS {
                return Err(Cause::Recovery(reconnect::Error::RestTailRequestsExhausted));
            }
            let delay = crate::primary::REST_BACKOFF_MS[usize::from(scope.ordinal - 1)];
            let wake = (Instant::now() + Duration::from_millis(delay)).min(episode.deadline());
            // Keep servicing this same connection and STOP while waiting for
            // source availability. Neither latch is reset by Pending.
            loop {
                tokio::select! {biased;
                    _=tokio::time::sleep_until(wake)=>break,
                    result=self.poll(Some(episode.deadline()),false)=>{ result?; }
                }
            }
            previous = Some(Box::new(plan));
        }
    }
    async fn finish(mut self, cause: Cause) -> RunReport {
        let requested_eod = matches!(cause, Cause::EndOfSession) && !self.root.is_stopped();
        if !matches!(cause, Cause::RequestedStop | Cause::EndOfSession) {
            self.faults.record(cause.clone());
        }
        let stop_sample = crate::timing::sample_clock();
        self.root.stop();
        self.connection.stop.stop();
        let stop_budget_ms = self.writer.stop_budget_ms();
        let deadline = if requested_eod {
            self.writer.cap_drain_deadline(
                self.eod_at.unwrap_or_else(Instant::now) + Duration::from_millis(stop_budget_ms),
            )
        } else {
            self.writer.drain_deadline()
        };
        let budget_started = deadline
            .checked_sub(Duration::from_millis(stop_budget_ms))
            .unwrap_or_else(Instant::now);
        let http_drained = self.drain_jobs(deadline, requested_eod).await;
        let (socket_drained, report) = self.connection.drain(deadline).await;
        self.scope.socket = None;
        if let Some(report) = report {
            self.socket_report = Some(report);
        }
        let state = self.gate.snapshot();
        if state.lost {
            if let Some(failure) = &state.failure {
                self.faults.record(Cause::Socket(Box::new(failure.clone())));
            }
        }
        if let (Some(failure), Some(audit)) = (state.failure.as_ref(), self.recovery.last()) {
            audit.latch_candidate_fault(failure);
        }
        let recovery: Vec<_> = self.recovery.iter().map(|audit| audit.snapshot()).collect();
        let unresolved_transport_fault = if state.lost {
            state
                .failure
                .or_else(|| recovery.last().map(|r| r.original.clone()))
        } else {
            None
        };
        let recovery_resolved = recovery
            .iter()
            .all(|r| !r.transport_unresolved && !r.canonical_unresolved && r.terminal.is_none());
        let cause = self.faults.first().unwrap_or(cause);
        let update = if Instant::now() < deadline {
            let event = serde_json::json!({"event":"primary_async_shutdown",
            "cause":cause,"recovery":recovery,"continuity_lost":state.lost,"continuity_generation":state.generation,
            "unresolved_transport_fault":unresolved_transport_fault,"socket_drained":socket_drained&&self.all_connections_drained,
            "http_drained":http_drained,"http":self.totals,"drained_http":self.drained_http,
            "requested_calendar_eod":requested_eod,"stop_started_at_ms":stop_sample.0,"full_session_qualified":false});
            self.writer
                .call(Operation::Finalize(crate::primary::eod::FinishFacts {
                    event,
                    requested_eod: requested_eod && self.faults.first().is_none(),
                    io_drained: socket_drained && self.all_connections_drained && http_drained,
                    continuity_resolved: !state.lost,
                    recovery_resolved,
                }))
                .await
                .ok()
        } else {
            None
        };
        let session_finalized = update.as_ref().is_some_and(|u| u.finalized);
        let model_prepared = update.as_ref().is_some_and(|u| u.eod_prepared);
        let journal = update.as_ref().and_then(|u| u.journal_seal.clone());
        if let Some(update) = &update {
            self.committed = update.committed;
        }
        let writer_drained = self.writer.join(deadline).await;
        let all_workers_drained =
            socket_drained && self.all_connections_drained && http_drained && writer_drained;
        self.scope.completed = all_workers_drained;
        let joined = crate::timing::sample_clock();
        let elapsed_stop_ms = u64::try_from(
            Instant::now()
                .saturating_duration_since(budget_started)
                .as_millis(),
        )
        .unwrap_or(u64::MAX);
        if elapsed_stop_ms > stop_budget_ms {
            self.faults.record(Cause::DrainTimeout);
        }
        let clock_consistent = joined.0 >= stop_sample.0
            && joined
                .1
                .checked_duration_since(stop_sample.1)
                .is_some_and(|d| {
                    (i128::from(joined.0) - i128::from(stop_sample.0) - d.as_millis() as i128)
                        .unsigned_abs()
                        <= u128::from(self.clock_uncertainty_ms)
                })
            && (!requested_eod
                || self.eod_ms.is_some_and(|eod| {
                    let utc_elapsed = i128::from(joined.0) - i128::from(eod);
                    utc_elapsed >= 0
                        && utc_elapsed
                            <= i128::from(stop_budget_ms) + i128::from(self.clock_uncertainty_ms)
                        && (utc_elapsed - i128::from(elapsed_stop_ms)).unsigned_abs()
                            <= u128::from(self.clock_uncertainty_ms)
                }));
        let completion = self.calendar_sha256.as_ref().map(|calendar_sha256| {
            let mut result = crate::primary::eod::Completion {
                schema_version: 1,
                config_sha256: self.config_sha256.clone(),
                calendar_sha256: calendar_sha256.clone(),
                journal,
                calendar_eod_trigger: requested_eod,
                eod_check_at_ms: self.eod_ms.unwrap_or_default(),
                stop_started_at_ms: stop_sample.0,
                joined_at_ms: joined.0,
                stop_budget_ms,
                elapsed_stop_ms,
                clock_consistent,
                socket_drained: socket_drained && self.all_connections_drained,
                http_drained,
                writer_drained,
                fault_free: self.faults.first().is_none(),
                continuity_resolved: !state.lost,
                recovery_resolved,
                model_prepared,
                semantic_eod_passed: false,
                cgroup_qualified: false,
                network_qualified: false,
            };
            result.semantic_eod_passed = result.qualifies();
            result
        });
        RunReport {
            completion,
            cause: self.faults.first().unwrap_or(cause),
            canonical_commits: self.committed,
            http: std::mem::take(&mut self.totals),
            socket_drained: socket_drained && self.all_connections_drained,
            http_drained,
            writer_drained,
            all_workers_drained,
            session_finalized,
            socket: self.socket_report.take(),
            recovery,
            unresolved_transport_fault,
            full_session_qualified: false,
        }
    }
}

pub(super) async fn drive<G, GF, F, FF>(
    owner: Coordinator,
    reference_start: DateTime<Utc>,
    factory: G,
    fetch: F,
) -> Result<RunReport, Cause>
where
    G: Fn(Open) -> GF + Clone + Send + 'static,
    GF: Future<Output = Result<SocketReport, Failure>> + Send + 'static,
    F: Fn(Job, StopHandle) -> FF + Clone + Send + 'static,
    FF: Future<Output = HttpOutcome> + Send + 'static,
{
    let (config, root) = owner.session.primary_runtime().ok_or(Cause::Binding)?;
    let deadline_ms = config.config().run_deadline_utc_ms;
    let clock_uncertainty_ms = config.config().clock_uncertainty_ms;
    let config_sha256 = config.fingerprint().to_owned();
    let calendar_sha256 = owner.processor().calendar().map(|c| c.sha256().to_owned());
    let eod_ms = owner.processor().calendar().map(|c| c.eod_check_at_ms());
    let policy = config.primary_policy().ok_or(Cause::Binding)?.clone();
    let remaining = deadline_ms
        .checked_sub(crate::timing::now_ms())
        .filter(|ms| *ms > 0 && *ms <= 86400000)
        .ok_or(Cause::Binding)?;
    let now_ms = crate::timing::now_ms();
    let eod_at = eod_ms
        .map(|t| Instant::now() + Duration::from_millis(t.saturating_sub(now_ms).max(0) as u64));
    let gate = ContinuityGate::new(owner.processor().generation());
    let faults = Faults::default();
    let generation = owner.processor().generation();
    let slot = FailureSlot::with_gate(gate.clone(), root.clone());
    let writer = Writer::spawn_mode(
        owner,
        reference_start,
        root.clone(),
        faults.clone(),
        slot.clone(),
        Some(gate.clone()),
    )?;
    let connection = Connection::open(
        generation,
        deadline_ms,
        deadline_ms,
        root.clone(),
        slot,
        &policy,
        factory.clone(),
    )?;
    let mut scope = Scope::new(root.clone(), faults.clone());
    scope.socket = connection.worker.as_ref().map(|h| h.abort_handle());
    let mut engine = Engine {
        writer,
        connection,
        fetch,
        root,
        faults,
        gate,
        scope,
        policy,
        run_deadline_ms: deadline_ms,
        deadline: (Instant::now() + Duration::from_millis(remaining as u64)).min(
            eod_at.unwrap_or_else(|| Instant::now() + Duration::from_millis(remaining as u64)),
        ),
        eod_at,
        eod_ms,
        config_sha256,
        calendar_sha256,
        clock_uncertainty_ms,
        critical: None,
        audit: None,
        totals: HttpTotals::default(),
        data: None,
        bootstrap_pending: false,
        wake: Instant::now() + Duration::from_secs(60),
        committed: 0,
        recovery: vec![],
        socket_report: None,
        all_connections_drained: true,
        drained_http: vec![],
    };
    let cause = loop {
        match engine.poll(None, true).await {
            Ok(_) => (),
            Err(Cause::Socket(failure)) if !engine.root.is_stopped() => {
                if let Err(cause) = engine.recover(*failure, &factory).await {
                    break cause;
                }
            }
            Err(cause) => break cause,
        }
    };
    Ok(engine.finish(cause).await)
}

#[cfg(test)]
mod tests;

// Called only by the serial engine before submitting a frame to the writer.
// A stale dequeued frame never reaches a callback. Clock faults are not loss
// of transport continuity and must not be hidden behind reconnect retries.
fn ingress_failure(
    policy: &PrimaryPolicy,
    gate: &ContinuityGate,
    slot: &FailureSlot,
    error: crate::primary::PrimaryError,
) -> Cause {
    if policy.consistent_sources() && error == crate::primary::PrimaryError::Clock {
        return Cause::Coordinator(CoordinatorError::Source(error));
    }
    if policy.consistent_sources() && error == crate::primary::PrimaryError::Continuity {
        slot.record(Failure {
            generation: gate.snapshot().generation,
            kind: crate::socket::primary::Kind::Source(error),
            liveness: None,
        });
    }
    gate.snapshot()
        .failure
        .map(|e| Cause::Socket(Box::new(e)))
        .unwrap_or(Cause::Coordinator(CoordinatorError::Source(error)))
}
