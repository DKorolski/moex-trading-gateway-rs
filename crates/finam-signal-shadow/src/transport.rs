//! Bounded GET-only collection; not an operational entry or live-readiness gate.
use broker_finam::{
    sparse_m10::{AdmittedClosedM1Snapshot, ClosedM1RequestPlan},
    AccessToken, Stage8bP1fO2GetOnlyClientV1,
};
use chrono::{DateTime, Utc};
use std::{future::Future, time::Duration};
use tokio::{sync::watch, time::Instant};

#[derive(Clone)]
pub struct StopHandle(watch::Sender<bool>);

impl Default for StopHandle {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}

impl StopHandle {
    pub(crate) fn same_run(&self, other: &Self) -> bool {
        self.0.same_channel(&other.0)
    }
    /// Owned by one managed connection. Its wrapper must relay the independent
    /// run STOP; stopping/replacing this scope never resets or stops the run.
    pub(crate) fn connection_scope() -> Self {
        Self::default()
    }
    pub(crate) async fn cancelled(&self) {
        let mut rx = self.0.subscribe();
        if !*rx.borrow() {
            let _ = rx.changed().await;
        }
    }
    pub fn is_stopped(&self) -> bool {
        *self.0.borrow()
    }
    /// One-way latch. No reset API.
    pub fn stop(&self) {
        self.0.send_replace(true);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum TransportError {
    Bounds,
    Stopped,
    Deadline,
    Held,
    Fetch,
    Socket(crate::socket::SocketFailure),
    Admission,
    Clock,
}

pub struct ReadOnlyCollector {
    stop: watch::Receiver<bool>,
    deadline: Instant,
    wall_deadline: DateTime<Utc>,
    last_wall: DateTime<Utc>,
    request_budget: Duration,
    held: bool,
}

impl ReadOnlyCollector {
    pub(crate) fn for_shared_stop(
        stop: &StopHandle,
        deadline: DateTime<Utc>,
        budget: Duration,
    ) -> Result<Self, TransportError> {
        let (_, mut value) = Self::new(deadline, budget)?;
        value.stop = stop.0.subscribe();
        Ok(value)
    }
    pub(crate) fn io_budget(&self) -> Duration {
        self.request_budget
    }
    pub(crate) fn sibling(&self) -> Self {
        Self {
            stop: self.stop.clone(),
            deadline: self.deadline,
            wall_deadline: self.wall_deadline,
            last_wall: self.last_wall,
            request_budget: self.request_budget,
            held: false,
        }
    }
    pub fn new(
        wall_deadline: DateTime<Utc>,
        request_budget: Duration,
    ) -> Result<(StopHandle, Self), TransportError> {
        let now = Utc::now();
        let remaining = (wall_deadline - now)
            .to_std()
            .map_err(|_| TransportError::Bounds)?;
        if remaining.is_zero()
            || remaining > Duration::from_secs(86_400)
            || request_budget.is_zero()
            || request_budget > Duration::from_secs(15)
        {
            return Err(TransportError::Bounds);
        }
        let (tx, rx) = watch::channel(false);
        Ok((
            StopHandle(tx),
            Self {
                stop: rx,
                deadline: Instant::now() + remaining,
                wall_deadline,
                last_wall: now,
                request_budget,
                held: false,
            },
        ))
    }

    /// Existing client owns the fixed HTTPS GET route and complete-body limits.
    /// Caller supplies credentials and independently planned closed ranges.
    /// No credentials are discovered, printed, persisted or renewed here.
    pub async fn collect(
        &mut self,
        client: &Stage8bP1fO2GetOnlyClientV1,
        token: &AccessToken,
        plan: &ClosedM1RequestPlan,
    ) -> Result<AdmittedClosedM1Snapshot, TransportError> {
        let evidence = self
            .bounded(async {
                client
                    .fetch_closed_m1_snapshot(token, plan)
                    .await
                    .map_err(|_| TransportError::Fetch)
            })
            .await?;
        // Fail closed if admission fails; there is no retry loop.
        self.held = true;
        let admitted = AdmittedClosedM1Snapshot::admit(evidence, plan, Utc::now())
            .map_err(|_| TransportError::Admission)?;
        self.check()?;
        self.held = false;
        Ok(admitted)
    }

    fn check(&mut self) -> Result<(), TransportError> {
        if *self.stop.borrow() || self.stop.has_changed().is_err() {
            return Err(TransportError::Stopped);
        }
        let now = Utc::now();
        if now < self.last_wall {
            return Err(TransportError::Clock);
        }
        self.last_wall = now;
        if Instant::now() >= self.deadline || now >= self.wall_deadline {
            return Err(TransportError::Deadline);
        }
        Ok(())
    }

    pub(crate) async fn bounded<T>(
        &mut self,
        work: impl Future<Output = Result<T, TransportError>>,
    ) -> Result<T, TransportError> {
        self.bounded_wait(self.request_budget, work).await
    }

    /// Socket data wait uses the remaining protocol silence budget, not the
    /// short REST/connect budget. Shared stop and absolute deadline still win.
    pub(crate) async fn bounded_wait<T>(
        &mut self,
        budget: Duration,
        work: impl Future<Output = Result<T, TransportError>>,
    ) -> Result<T, TransportError> {
        if self.held {
            return Err(TransportError::Held);
        }
        // If the caller drops this future mid-request, subsequent use stays held.
        self.held = true;
        if budget.is_zero() || budget > Duration::from_secs(3600) {
            return Err(TransportError::Bounds);
        }
        self.check()?;
        let end = self.deadline.min(Instant::now() + budget);
        let result = tokio::select! {
            biased;
            _ = self.stop.changed() => Err(TransportError::Stopped),
            _ = tokio::time::sleep_until(end) => Err(TransportError::Deadline),
            value = work => value,
        }?;
        if Instant::now() >= end {
            return Err(TransportError::Deadline);
        }
        self.check()?;
        self.held = false;
        Ok(result)
    }
}

/// Single configuration and stop owner for the three transport channels.
/// This is construction/configuration binding, not source/run authorization.
pub struct ConfiguredTransports {
    fingerprint: String,
    stop: StopHandle,
    rest: ReadOnlyCollector,
    bars: Option<ReadOnlyCollector>,
    quotes: Option<ReadOnlyCollector>,
    silence_ms: u64,
    reconnect_ms: u64,
    quote_age_ms: u64,
    quote_capacity: usize,
    max_input_bytes: u64,
}

impl ConfiguredTransports {
    pub fn new(config: &crate::ValidatedConfig) -> Result<Self, TransportError> {
        let c = config.config();
        if c.instrument != "IMOEXF@RTSX"
            || c.max_cache_bars > 4096
            || c.ws_silence_budget_ms > 3_600_000
            || c.reconnect_budget_ms > 3_600_000
            || c.max_quote_age_ms > 3_600_000
            || c.max_input_bytes > 16 * 1024 * 1024
        {
            return Err(TransportError::Bounds);
        }
        let deadline =
            DateTime::from_timestamp_millis(c.run_deadline_utc_ms).ok_or(TransportError::Bounds)?;
        // Explicit derived IO cap; do not borrow the model latency allowance.
        let io_ms = 15_000.min(c.stop_timeout_ms).min(c.ws_silence_budget_ms);
        let (stop, rest) = ReadOnlyCollector::new(deadline, Duration::from_millis(io_ms))?;
        Ok(Self {
            fingerprint: config.fingerprint().into(),
            stop,
            bars: Some(rest.sibling()),
            // No QUOTES transport capability exists in the candle-only profile.
            quotes: (!config.candle_only()).then(|| rest.sibling()),
            rest,
            silence_ms: c.ws_silence_budget_ms,
            reconnect_ms: c.reconnect_budget_ms,
            quote_age_ms: c.max_quote_age_ms,
            quote_capacity: crate::quote::QUOTE_DEDUP_CAPACITY,
            max_input_bytes: c.max_input_bytes,
        })
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn stop_handle(&self) -> StopHandle {
        self.stop.clone()
    }

    pub async fn connect_bars(
        &mut self,
        lease: broker_finam::AccessTokenLease,
    ) -> Result<crate::socket::BarsSocket, TransportError> {
        let budget = self.bars.take().ok_or(TransportError::Held)?;
        let mut guard = StopOnFailure::new(self.stop.clone());
        let socket =
            crate::socket::BarsSocket::connect(lease, budget, self.silence_ms, self.reconnect_ms)
                .await?;
        guard.success = true;
        Ok(socket)
    }

    pub async fn connect_quotes(
        &mut self,
        lease: broker_finam::AccessTokenLease,
    ) -> Result<crate::socket::QuotesSocket, TransportError> {
        let budget = self.quotes.take().ok_or(TransportError::Held)?;
        let mut guard = StopOnFailure::new(self.stop.clone());
        let socket = crate::socket::QuotesSocket::connect(
            lease,
            budget,
            self.silence_ms,
            self.reconnect_ms,
            self.quote_age_ms,
            self.quote_capacity,
        )
        .await?;
        guard.success = true;
        Ok(socket)
    }

    pub async fn collect(
        &mut self,
        client: &Stage8bP1fO2GetOnlyClientV1,
        token: &AccessToken,
        plan: &ClosedM1RequestPlan,
    ) -> Result<AdmittedClosedM1Snapshot, TransportError> {
        let mut guard = StopOnFailure::new(self.stop.clone());
        let snapshot = self.rest.collect(client, token, plan).await?;
        // Transport already has its fixed allocation cap. Enforce tighter run
        // input allowance before delivering anything to the model/input adapter.
        let bytes: usize = snapshot
            .evidence()
            .parts
            .iter()
            .map(|p| p.raw_body.len())
            .sum();
        if bytes as u64 > self.max_input_bytes {
            return Err(TransportError::Bounds);
        }
        guard.success = true;
        Ok(snapshot)
    }
}

impl Drop for ConfiguredTransports {
    fn drop(&mut self) {
        self.stop.stop();
    }
}

struct StopOnFailure {
    stop: StopHandle,
    success: bool,
}
impl StopOnFailure {
    fn new(stop: StopHandle) -> Self {
        Self {
            stop,
            success: false,
        }
    }
}
impl Drop for StopOnFailure {
    fn drop(&mut self) {
        if !self.success {
            self.stop.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    fn collector(ms: u64) -> (StopHandle, ReadOnlyCollector) {
        ReadOnlyCollector::new(
            Utc::now() + chrono::Duration::seconds(10),
            Duration::from_millis(ms),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn data_wait_is_not_short_rest_timeout_but_stop_still_interrupts() {
        let (stop, mut budget) = collector(1);
        assert_eq!(
            budget
                .bounded_wait(Duration::from_secs(1), async {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Ok(7)
                })
                .await,
            Ok(7)
        );
        stop.stop();
        assert_eq!(
            budget
                .bounded_wait(Duration::from_secs(1), async { Ok(8) })
                .await,
            Err(TransportError::Stopped)
        );
    }

    #[tokio::test]
    async fn stopped_before_request_never_polls_work() {
        let (stop, mut c) = collector(1000);
        stop.stop();
        let polled = AtomicBool::new(false);
        assert_eq!(
            c.bounded(async {
                polled.store(true, Ordering::SeqCst);
                Ok(())
            })
            .await,
            Err(TransportError::Stopped)
        );
        assert!(!polled.load(Ordering::SeqCst));
        assert_eq!(c.bounded(async { Ok(()) }).await, Err(TransportError::Held));
    }

    #[tokio::test]
    async fn request_deadline_holds_without_retry() {
        let (_stop, mut c) = collector(5);
        assert_eq!(
            c.bounded(std::future::pending::<Result<(), TransportError>>())
                .await,
            Err(TransportError::Deadline)
        );
        assert_eq!(c.bounded(async { Ok(()) }).await, Err(TransportError::Held));
    }

    struct OnDrop(Arc<AtomicBool>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn stop_cancels_inflight_future_and_releases_its_resources() {
        let (stop, mut c) = collector(1000);
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = OnDrop(dropped.clone());
        let (started, ready) = tokio::sync::oneshot::channel();
        let work = async move {
            let _guard = guard;
            let _ = started.send(());
            std::future::pending::<Result<(), TransportError>>().await
        };
        let task = tokio::spawn(async move { c.bounded(work).await });
        ready.await.unwrap();
        stop.stop();
        assert_eq!(task.await.unwrap(), Err(TransportError::Stopped));
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn sender_loss_fails_closed_and_success_can_collect_again() {
        let (stop, mut c) = collector(1000);
        assert_eq!(c.bounded(async { Ok(1) }).await, Ok(1));
        assert_eq!(c.bounded(async { Ok(2) }).await, Ok(2));
        drop(stop);
        assert_eq!(
            c.bounded(async { Ok(3) }).await,
            Err(TransportError::Stopped)
        );
    }

    #[tokio::test]
    async fn caller_cancellation_and_fetch_error_leave_collector_held() {
        let (_stop, mut c) = collector(1000);
        let outer = tokio::time::timeout(
            Duration::from_millis(5),
            c.bounded(std::future::pending::<Result<(), TransportError>>()),
        )
        .await;
        assert!(outer.is_err());
        assert_eq!(c.bounded(async { Ok(()) }).await, Err(TransportError::Held));
        let (_stop, mut c) = collector(1000);
        assert_eq!(
            c.bounded(async { Err::<(), _>(TransportError::Fetch) })
                .await,
            Err(TransportError::Fetch)
        );
        assert_eq!(c.bounded(async { Ok(()) }).await, Err(TransportError::Held));
    }

    #[tokio::test]
    async fn shared_stop_interrupts_all_three_channels() {
        let (stop, rest) = collector(1000);
        let bars = rest.sibling();
        let quotes = rest.sibling();
        assert_eq!(rest.deadline, bars.deadline);
        assert_eq!(rest.deadline, quotes.deadline);
        let mut tasks = Vec::new();
        let mut ready = Vec::new();
        for mut channel in [rest, bars, quotes] {
            let (tx, rx) = tokio::sync::oneshot::channel();
            ready.push(rx);
            tasks.push(tokio::spawn(async move {
                channel
                    .bounded(async {
                        let _ = tx.send(());
                        std::future::pending::<Result<(), TransportError>>().await
                    })
                    .await
            }));
        }
        for rx in ready {
            rx.await.unwrap();
        }
        stop.stop();
        for task in tasks {
            assert_eq!(task.await.unwrap(), Err(TransportError::Stopped));
        }
    }

    #[tokio::test]
    async fn failed_or_cancelled_composition_stops_siblings() {
        let (stop, mut sibling) = collector(1000);
        {
            let _guard = StopOnFailure::new(stop);
        }
        assert_eq!(
            sibling.bounded(async { Ok(()) }).await,
            Err(TransportError::Stopped)
        );
        let (stop, mut sibling) = collector(1000);
        let result = tokio::time::timeout(Duration::from_millis(5), async {
            let _guard = StopOnFailure::new(stop);
            std::future::pending::<()>().await
        })
        .await;
        assert!(result.is_err());
        assert_eq!(
            sibling.bounded(async { Ok(()) }).await,
            Err(TransportError::Stopped)
        );
    }
}
