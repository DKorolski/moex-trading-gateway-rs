//! Fixed local Stage 8B-P1-f guardian/I1 composition.
//!
//! This is deliberately not a general process orchestrator. Production can
//! launch only the accepted I1 `run` command. Tests replace that one command
//! with small real Unix children to exercise process-group lifecycle behavior.

use std::{
    collections::VecDeque,
    ffi::{CStr, CString, OsString},
    io::ErrorKind,
    os::unix::process::{CommandExt, ExitStatusExt},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicU8, Ordering as AtomicOrdering},
        Arc,
    },
    time::{Duration as StdDuration, Instant},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::{
    signal::unix::{signal as unix_signal, Signal, SignalKind},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    stage8b_p1f_guardian::Stage8bP1fLocalStopCauseV1, Stage8bP1fAuthorityErrorV1,
    Stage8bP1fAuthorityStoreV1, Stage8bP1fDeadlineDecisionV1, Stage8bP1fPhaseStateV1,
    Stage8bP1fPhaseV1, Stage8bP1fRunPermitV1, Stage8bP1fTerminalReceiptV1,
    STAGE8B_P1E_SUPERVISOR_CONFIG_PATH, STAGE8B_P1F_SERVICE_USER,
};

pub const STAGE8B_P1F_I1_BINARY_PATH: &str = "/usr/local/libexec/moex/stage8b-p1-paper-supervisor";
const PROCESS_POLL_INTERVAL: StdDuration = StdDuration::from_millis(25);
const RESTART_WINDOW: StdDuration = StdDuration::from_secs(600);
const RESTART_DELAY: StdDuration = StdDuration::from_secs(5);
const MAX_STARTS_PER_WINDOW: usize = 5;
const FAIL_CLOSED_GRACE: StdDuration = StdDuration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalSignalV1 {
    Sigterm,
    Sigint,
    SignalTaskFailed,
}

impl LocalSignalV1 {
    const fn stop_cause(self) -> Stage8bP1fLocalStopCauseV1 {
        match self {
            Self::Sigterm => Stage8bP1fLocalStopCauseV1::Sigterm,
            Self::Sigint => Stage8bP1fLocalStopCauseV1::Sigint,
            Self::SignalTaskFailed => Stage8bP1fLocalStopCauseV1::SupervisionFailure,
        }
    }

    const fn unix_signal(self) -> i32 {
        match self {
            Self::Sigint => libc::SIGINT,
            Self::Sigterm | Self::SignalTaskFailed => libc::SIGTERM,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LocalSupervisionPolicyV1 {
    poll_interval: StdDuration,
    restart_window: StdDuration,
    restart_delay: StdDuration,
    max_starts_per_window: usize,
    fail_closed_grace: StdDuration,
    #[cfg(any(test, feature = "stage8b-p1-test-fixtures"))]
    stop_grace_override: Option<StdDuration>,
}

impl LocalSupervisionPolicyV1 {
    const fn production() -> Self {
        Self {
            poll_interval: PROCESS_POLL_INTERVAL,
            restart_window: RESTART_WINDOW,
            restart_delay: RESTART_DELAY,
            max_starts_per_window: MAX_STARTS_PER_WINDOW,
            fail_closed_grace: FAIL_CLOSED_GRACE,
            #[cfg(any(test, feature = "stage8b-p1-test-fixtures"))]
            stop_grace_override: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct FixedChildCommandV1 {
    executable: PathBuf,
    args: Vec<OsString>,
    identity: Option<(u32, u32)>,
}

impl FixedChildCommandV1 {
    fn production(uid: u32, gid: u32) -> Self {
        Self {
            executable: PathBuf::from(STAGE8B_P1F_I1_BINARY_PATH),
            args: vec![
                OsString::from("run"),
                OsString::from(STAGE8B_P1E_SUPERVISOR_CONFIG_PATH),
            ],
            identity: Some((uid, gid)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Stage8bP1fLocalSupervisionDispositionV1 {
    GracefulStop,
    DeadlineStop,
    ForceKilled,
    RecoveryTerminated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fLocalSupervisionResultV1 {
    pub schema_version: u16,
    pub domain: String,
    pub manifest_sha256: String,
    pub child_starts: u32,
    pub last_exit_code: Option<i32>,
    pub last_signal: Option<i32>,
    pub disposition: Stage8bP1fLocalSupervisionDispositionV1,
    pub force_killed: bool,
    pub terminal_receipt: Stage8bP1fTerminalReceiptV1,
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1fLocalSupervisionErrorV1 {
    #[error("invalid Stage 8B-P1-f local-supervision command line")]
    Usage,
    #[error("P1-f service identity is invalid")]
    ServiceIdentity,
    #[error("P1-f signal supervision failed")]
    SignalTask,
    #[error("P1-f child process operation failed: {0:?}")]
    ChildProcess(ErrorKind),
    #[error("P1-f child terminated with accepted fatal exit class {0}")]
    ChildExit(u8),
    #[error("P1-f child restart budget was exhausted")]
    RestartExhausted,
    #[error("P1-f child exited successfully without a stop decision")]
    UnexpectedCleanExit,
    #[error("P1-f child exceeded the retained stopping grace")]
    ForceKilled,
    #[error("P1-f recovered execution was conservatively terminated")]
    RecoveryTerminated,
    #[error(transparent)]
    Authority(#[from] Stage8bP1fAuthorityErrorV1),
}

impl Stage8bP1fLocalSupervisionErrorV1 {
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage | Self::ServiceIdentity => 64,
            Self::Authority(_) | Self::RecoveryTerminated => 66,
            Self::RestartExhausted | Self::UnexpectedCleanExit => 67,
            Self::ChildProcess(_) => 70,
            Self::ChildExit(code) => *code,
            Self::ForceKilled => 72,
            Self::SignalTask => 73,
        }
    }
}

type SignalBarrierSenderV1 = mpsc::UnboundedSender<oneshot::Sender<()>>;

const NO_PRE_SPAWN_SIGNAL: u8 = 0;
const PRE_SPAWN_SIGTERM: u8 = 1;
const PRE_SPAWN_SIGINT: u8 = 2;

#[derive(Clone)]
struct PreSpawnSignalWitnessV1 {
    observed: Arc<AtomicU8>,
}

impl PreSpawnSignalWitnessV1 {
    fn signal(&self) -> Option<LocalSignalV1> {
        match self.observed.load(AtomicOrdering::SeqCst) {
            PRE_SPAWN_SIGTERM => Some(LocalSignalV1::Sigterm),
            PRE_SPAWN_SIGINT => Some(LocalSignalV1::Sigint),
            _ => None,
        }
    }
}

struct SignalHandlerRegistrationsV1 {
    terminate: signal_hook_registry::SigId,
    interrupt: signal_hook_registry::SigId,
}

impl Drop for SignalHandlerRegistrationsV1 {
    fn drop(&mut self) {
        let _ = signal_hook_registry::unregister(self.terminate);
        let _ = signal_hook_registry::unregister(self.interrupt);
    }
}

struct RegisteredUnixSignalsV1 {
    signal_receiver: mpsc::UnboundedReceiver<LocalSignalV1>,
    barrier_sender: SignalBarrierSenderV1,
    pre_spawn_witness: PreSpawnSignalWitnessV1,
    registrations: SignalHandlerRegistrationsV1,
    task: JoinHandle<()>,
}

async fn register_unix_signal_supervision(
) -> Result<RegisteredUnixSignalsV1, Stage8bP1fLocalSupervisionErrorV1> {
    let observed = Arc::new(AtomicU8::new(NO_PRE_SPAWN_SIGNAL));
    let term_observed = Arc::clone(&observed);
    // SAFETY: the callback performs only a lock-free atomic compare/exchange;
    // signal-hook-registry owns handler chaining and callback lifetime.
    let terminate_registration = unsafe {
        signal_hook_registry::register(libc::SIGTERM, move || {
            let _ = term_observed.compare_exchange(
                NO_PRE_SPAWN_SIGNAL,
                PRE_SPAWN_SIGTERM,
                AtomicOrdering::SeqCst,
                AtomicOrdering::SeqCst,
            );
        })
    }
    .map_err(|_| Stage8bP1fLocalSupervisionErrorV1::SignalTask)?;
    let int_observed = Arc::clone(&observed);
    // SAFETY: identical to the SIGTERM callback above; the first observed
    // operator stop wins and is visible without waiting for Tokio scheduling.
    let interrupt_registration = match unsafe {
        signal_hook_registry::register(libc::SIGINT, move || {
            let _ = int_observed.compare_exchange(
                NO_PRE_SPAWN_SIGNAL,
                PRE_SPAWN_SIGINT,
                AtomicOrdering::SeqCst,
                AtomicOrdering::SeqCst,
            );
        })
    } {
        Ok(registration) => registration,
        Err(_) => {
            let _ = signal_hook_registry::unregister(terminate_registration);
            return Err(Stage8bP1fLocalSupervisionErrorV1::SignalTask);
        }
    };
    let registrations = SignalHandlerRegistrationsV1 {
        terminate: terminate_registration,
        interrupt: interrupt_registration,
    };
    let pre_spawn_witness = PreSpawnSignalWitnessV1 { observed };

    // Tokio streams retain ordinary post-spawn supervision. They are also
    // installed synchronously before identity/open/admission; pre-spawn safety
    // does not depend on when Tokio's signal driver broadcasts to these streams.
    let terminate = unix_signal(SignalKind::terminate())
        .map_err(|_| Stage8bP1fLocalSupervisionErrorV1::SignalTask)?;
    let interrupt = unix_signal(SignalKind::interrupt())
        .map_err(|_| Stage8bP1fLocalSupervisionErrorV1::SignalTask)?;
    let (signal_sender, signal_receiver) = mpsc::unbounded_channel();
    let (barrier_sender, barrier_receiver) = mpsc::unbounded_channel();
    let (ready_sender, ready_receiver) = oneshot::channel();
    let task = tokio::spawn(forward_unix_signals(
        terminate,
        interrupt,
        signal_sender,
        barrier_receiver,
        ready_sender,
    ));
    ready_receiver
        .await
        .map_err(|_| Stage8bP1fLocalSupervisionErrorV1::SignalTask)?;
    Ok(RegisteredUnixSignalsV1 {
        signal_receiver,
        barrier_sender,
        pre_spawn_witness,
        registrations,
        task,
    })
}

struct ChildGroupV1 {
    child: Option<Child>,
    process_group: i32,
}

impl ChildGroupV1 {
    fn spawn(spec: &FixedChildCommandV1) -> Result<Self, Stage8bP1fLocalSupervisionErrorV1> {
        let mut command = Command::new(&spec.executable);
        command
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .process_group(0);
        #[cfg(target_os = "linux")]
        let parent_pid = std::process::id();
        let identity = spec.identity;
        // SAFETY: this closure invokes only async-signal-safe libc operations
        // between fork and exec. No allocation, locking or Rust I/O occurs.
        unsafe {
            command.pre_exec(move || {
                if let Some((uid, gid)) = identity {
                    if libc::setgroups(0, std::ptr::null()) != 0
                        || libc::setgid(gid) != 0
                        || libc::setuid(uid) != 0
                    {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                #[cfg(target_os = "linux")]
                {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::getppid() as u32 != parent_pid {
                        return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
                    }
                }
                Ok(())
            });
        }
        let child = command
            .spawn()
            .map_err(|error| Stage8bP1fLocalSupervisionErrorV1::ChildProcess(error.kind()))?;
        let process_group = i32::try_from(child.id())
            .map_err(|_| Stage8bP1fLocalSupervisionErrorV1::ChildProcess(ErrorKind::InvalidData))?;
        Ok(Self {
            child: Some(child),
            process_group,
        })
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>, Stage8bP1fLocalSupervisionErrorV1> {
        self.child
            .as_mut()
            .ok_or(Stage8bP1fLocalSupervisionErrorV1::ChildProcess(
                ErrorKind::NotFound,
            ))?
            .try_wait()
            .map_err(|error| Stage8bP1fLocalSupervisionErrorV1::ChildProcess(error.kind()))
    }

    fn signal(&self, signal: i32) -> Result<(), Stage8bP1fLocalSupervisionErrorV1> {
        let result = unsafe { libc::kill(-self.process_group, signal) };
        if result == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(Stage8bP1fLocalSupervisionErrorV1::ChildProcess(
                error.kind(),
            ))
        }
    }

    fn wait(&mut self) -> Result<ExitStatus, Stage8bP1fLocalSupervisionErrorV1> {
        self.child
            .as_mut()
            .ok_or(Stage8bP1fLocalSupervisionErrorV1::ChildProcess(
                ErrorKind::NotFound,
            ))?
            .wait()
            .map_err(|error| Stage8bP1fLocalSupervisionErrorV1::ChildProcess(error.kind()))
    }

    fn disarm_after_reap(&mut self) {
        self.child.take();
    }

    fn force_kill_and_reap(&mut self) -> Result<ExitStatus, Stage8bP1fLocalSupervisionErrorV1> {
        self.signal(libc::SIGKILL)?;
        let status = self.wait()?;
        self.disarm_after_reap();
        Ok(status)
    }

    fn reap_exited(
        &mut self,
        status: ExitStatus,
    ) -> Result<ExitStatus, Stage8bP1fLocalSupervisionErrorV1> {
        // A failed group leader must not leave descendants alive before a
        // restart. ESRCH is the expected result for an already empty group.
        self.signal(libc::SIGKILL)?;
        self.disarm_after_reap();
        Ok(status)
    }
}

impl Drop for ChildGroupV1 {
    fn drop(&mut self) {
        if self.child.is_some() {
            let _ = unsafe { libc::kill(-self.process_group, libc::SIGKILL) };
            if let Some(child) = self.child.as_mut() {
                let _ = child.wait();
            }
        }
    }
}

pub async fn run_stage8b_p1f_local_supervisor_v1(
    manifest_sha256: &str,
) -> Result<Stage8bP1fLocalSupervisionResultV1, Stage8bP1fLocalSupervisionErrorV1> {
    let RegisteredUnixSignalsV1 {
        signal_receiver,
        barrier_sender,
        pre_spawn_witness,
        registrations: _signal_registrations,
        task: signal_task,
    } = register_unix_signal_supervision().await?;
    let result = async {
        let (service_uid, service_gid) = resolve_service_identity()?;
        let store = Stage8bP1fAuthorityStoreV1::open_production(service_gid)?;
        match prepare_permit_or_expire(&store, manifest_sha256, Utc::now())? {
            PreparedRunV1::Permit(permit) => {
                supervise_admitted_child(
                    &store,
                    permit,
                    FixedChildCommandV1::production(service_uid, service_gid),
                    signal_receiver,
                    Some(barrier_sender),
                    Some(pre_spawn_witness),
                    LocalSupervisionPolicyV1::production(),
                )
                .await
            }
            PreparedRunV1::Expired(result) => Ok(result),
        }
    }
    .await;
    signal_task.abort();
    result
}

#[derive(Debug)]
pub(crate) enum PreparedRunV1 {
    Permit(Stage8bP1fRunPermitV1),
    Expired(Stage8bP1fLocalSupervisionResultV1),
}

fn prepare_permit_or_expire(
    store: &Stage8bP1fAuthorityStoreV1,
    manifest_sha256: &str,
    trusted_now: DateTime<Utc>,
) -> Result<PreparedRunV1, Stage8bP1fLocalSupervisionErrorV1> {
    let inspection = match store.inspect() {
        Ok(inspection) => inspection,
        Err(Stage8bP1fAuthorityErrorV1::PendingRecoveryRequired) => {
            return Ok(PreparedRunV1::Permit(
                store.resume_stopping_phase(manifest_sha256, trusted_now)?,
            ));
        }
        Err(error) => return Err(error.into()),
    };
    if inspection.active_manifest_sha256.as_deref() != Some(manifest_sha256) {
        return Err(Stage8bP1fAuthorityErrorV1::ActiveConflict.into());
    }
    if inspection.state == Stage8bP1fPhaseStateV1::Stopping {
        return Ok(PreparedRunV1::Permit(
            store.resume_stopping_phase(manifest_sha256, trusted_now)?,
        ));
    }
    if inspection.state != Stage8bP1fPhaseStateV1::Active {
        return Err(Stage8bP1fAuthorityErrorV1::ActiveConflict.into());
    }
    match store.admit_active_phase(manifest_sha256, trusted_now) {
        Ok(permit) => Ok(PreparedRunV1::Permit(permit)),
        Err(Stage8bP1fAuthorityErrorV1::DeadlineExpired) => {
            let after = store.inspect()?;
            if after.state == Stage8bP1fPhaseStateV1::Stopping {
                return Ok(PreparedRunV1::Permit(
                    store.resume_stopping_phase(manifest_sha256, trusted_now)?,
                ));
            }
            let receipt = store.finish_phase(
                manifest_sha256,
                Stage8bP1fPhaseStateV1::Expired,
                "deadline-before-child",
                trusted_now,
            )?;
            Ok(PreparedRunV1::Expired(supervision_result(
                manifest_sha256,
                0,
                None,
                Stage8bP1fLocalSupervisionDispositionV1::DeadlineStop,
                false,
                receipt,
            )))
        }
        Err(error) => Err(error.into()),
    }
}

async fn forward_unix_signals(
    mut terminate: Signal,
    mut interrupt: Signal,
    sender: mpsc::UnboundedSender<LocalSignalV1>,
    mut barrier_receiver: mpsc::UnboundedReceiver<oneshot::Sender<()>>,
    ready_sender: oneshot::Sender<()>,
) {
    if ready_sender.send(()).is_err() {
        return;
    }
    loop {
        tokio::select! {
            biased;
            value = terminate.recv() => {
                let signal = value.map_or(LocalSignalV1::SignalTaskFailed, |_| LocalSignalV1::Sigterm);
                if sender.send(signal).is_err() || signal == LocalSignalV1::SignalTaskFailed {
                    return;
                }
            }
            value = interrupt.recv() => {
                let signal = value.map_or(LocalSignalV1::SignalTaskFailed, |_| LocalSignalV1::Sigint);
                if sender.send(signal).is_err() || signal == LocalSignalV1::SignalTaskFailed {
                    return;
                }
            }
            barrier = barrier_receiver.recv() => {
                match barrier {
                    Some(barrier) => {
                        let _ = barrier.send(());
                    }
                    None => return,
                }
            }
        }
    }
}

async fn cross_signal_barrier(
    sender: Option<&SignalBarrierSenderV1>,
    pre_spawn_witness: Option<&PreSpawnSignalWitnessV1>,
) -> Result<Option<LocalSignalV1>, Stage8bP1fLocalSupervisionErrorV1> {
    if let Some(sender) = sender {
        let (completed_sender, completed_receiver) = oneshot::channel();
        sender
            .send(completed_sender)
            .map_err(|_| Stage8bP1fLocalSupervisionErrorV1::SignalTask)?;
        completed_receiver
            .await
            .map_err(|_| Stage8bP1fLocalSupervisionErrorV1::SignalTask)?;
    } else {
        tokio::task::yield_now().await;
    }
    Ok(pre_spawn_witness.and_then(PreSpawnSignalWitnessV1::signal))
}

async fn supervise_admitted_child(
    store: &Stage8bP1fAuthorityStoreV1,
    mut permit: Stage8bP1fRunPermitV1,
    child_spec: FixedChildCommandV1,
    mut signal_receiver: mpsc::UnboundedReceiver<LocalSignalV1>,
    signal_barrier: Option<SignalBarrierSenderV1>,
    pre_spawn_witness: Option<PreSpawnSignalWitnessV1>,
    policy: LocalSupervisionPolicyV1,
) -> Result<Stage8bP1fLocalSupervisionResultV1, Stage8bP1fLocalSupervisionErrorV1> {
    let manifest_sha256 = permit.manifest_sha256().to_string();
    if !matches!(
        permit.phase(),
        Stage8bP1fPhaseV1::O3SyntheticPaper | Stage8bP1fPhaseV1::O4FinamReadOnly
    ) {
        let _receipt = store.finish_phase(
            &manifest_sha256,
            Stage8bP1fPhaseStateV1::Failed,
            "local-supervision-phase-invalid",
            Utc::now(),
        )?;
        return Err(Stage8bP1fLocalSupervisionErrorV1::RecoveryTerminated);
    }

    let mut start_times = VecDeque::new();
    let mut child_starts = 0u32;
    let mut signal_channel_open = true;
    loop {
        // The direct handler witness is updated in signal context and does not
        // depend on Tokio's driver or forwarding-task schedule. The actor
        // barrier still proves that asynchronous supervision remains alive.
        let mut queued =
            cross_signal_barrier(signal_barrier.as_ref(), pre_spawn_witness.as_ref()).await?;
        if queued.is_none() && signal_channel_open {
            queued = match signal_receiver.try_recv() {
                Ok(signal) => Some(signal),
                Err(mpsc::error::TryRecvError::Empty) => None,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    signal_channel_open = false;
                    Some(LocalSignalV1::SignalTaskFailed)
                }
            };
        }
        if let Some(signal) = queued {
            let decision = permit.request_local_stop(signal.stop_cause(), Utc::now())?;
            #[cfg(any(test, feature = "stage8b-p1-test-fixtures"))]
            if let Some(grace) = policy.stop_grace_override {
                permit.shorten_stop_grace_for_test(grace);
            }
            let result = finish_without_child(store, &permit, child_starts, decision, Utc::now())?;
            return settle_terminal_result(&permit, result);
        }
        match poll_guardian(&mut permit, Utc::now())? {
            Stage8bP1fDeadlineDecisionV1::Continue => {}
            decision => {
                let result =
                    finish_without_child(store, &permit, child_starts, decision, Utc::now())?;
                return settle_terminal_result(&permit, result);
            }
        }

        prune_start_window(&mut start_times, policy.restart_window);
        if start_times.len() >= policy.max_starts_per_window {
            let receipt = store.finish_phase(
                &manifest_sha256,
                Stage8bP1fPhaseStateV1::Failed,
                "child-restart-exhausted",
                Utc::now(),
            )?;
            let _ = receipt;
            return Err(Stage8bP1fLocalSupervisionErrorV1::RestartExhausted);
        }
        let mut child = match ChildGroupV1::spawn(&child_spec) {
            Ok(child) => child,
            Err(error) => {
                let _ = store.finish_phase(
                    &manifest_sha256,
                    Stage8bP1fPhaseStateV1::Failed,
                    "child-spawn-failed",
                    Utc::now(),
                );
                return Err(error);
            }
        };
        start_times.push_back(Instant::now());
        child_starts = child_starts.saturating_add(1);
        let mut stopping_signal_sent = false;

        loop {
            if let Some(status) = child.try_wait()? {
                let status = child.reap_exited(status)?;
                if permit.stopping_reason_code().is_some() {
                    let result = finish_after_child_stop(
                        store,
                        &permit,
                        child_starts,
                        status,
                        false,
                        Utc::now(),
                    )?;
                    return settle_terminal_result(&permit, result);
                }
                if status.success() {
                    let _ = store.finish_phase(
                        &manifest_sha256,
                        Stage8bP1fPhaseStateV1::Failed,
                        "child-exited-clean",
                        Utc::now(),
                    );
                    return Err(Stage8bP1fLocalSupervisionErrorV1::UnexpectedCleanExit);
                }
                break;
            }

            match poll_guardian(&mut permit, Utc::now()) {
                Ok(Stage8bP1fDeadlineDecisionV1::Continue) => {}
                Ok(Stage8bP1fDeadlineDecisionV1::BeginStopping) => {
                    #[cfg(any(test, feature = "stage8b-p1-test-fixtures"))]
                    if let Some(grace) = policy.stop_grace_override {
                        permit.shorten_stop_grace_for_test(grace);
                    }
                    if !stopping_signal_sent {
                        child.signal(libc::SIGTERM)?;
                        stopping_signal_sent = true;
                    }
                }
                Ok(Stage8bP1fDeadlineDecisionV1::ForceKill) => {
                    let status = child.force_kill_and_reap()?;
                    let result = finish_after_child_stop(
                        store,
                        &permit,
                        child_starts,
                        status,
                        true,
                        Utc::now(),
                    )?;
                    return if result.terminal_receipt.terminal_state
                        == Stage8bP1fPhaseStateV1::Expired
                    {
                        Ok(result)
                    } else {
                        Err(Stage8bP1fLocalSupervisionErrorV1::ForceKilled)
                    };
                }
                Err(error) => {
                    fail_closed_child(&mut child, policy.fail_closed_grace).await;
                    let _ = store.finish_phase(
                        &manifest_sha256,
                        Stage8bP1fPhaseStateV1::Failed,
                        "guardian-error",
                        Utc::now(),
                    );
                    return Err(error);
                }
            }

            tokio::select! {
                () = tokio::time::sleep(policy.poll_interval) => {}
                signal = signal_receiver.recv(), if signal_channel_open => {
                    match signal {
                        Some(signal) => {
                            let decision = match permit.request_local_stop(signal.stop_cause(), Utc::now()) {
                                Ok(decision) => decision,
                                Err(error) => {
                                    fail_closed_child(&mut child, policy.fail_closed_grace).await;
                                    let _ = store.finish_phase(
                                        &manifest_sha256,
                                        Stage8bP1fPhaseStateV1::Failed,
                                        "guardian-error",
                                        Utc::now(),
                                    );
                                    return Err(error.into());
                                }
                            };
                            #[cfg(any(test, feature = "stage8b-p1-test-fixtures"))]
                            if let Some(grace) = policy.stop_grace_override {
                                permit.shorten_stop_grace_for_test(grace);
                            }
                            if decision == Stage8bP1fDeadlineDecisionV1::ForceKill {
                                let status = child.force_kill_and_reap()?;
                                let _ = finish_after_child_stop(
                                    store,
                                    &permit,
                                    child_starts,
                                    status,
                                    true,
                                    Utc::now(),
                                )?;
                                return Err(Stage8bP1fLocalSupervisionErrorV1::ForceKilled);
                            }
                            if !stopping_signal_sent {
                                child.signal(signal.unix_signal())?;
                                stopping_signal_sent = true;
                            }
                            if signal == LocalSignalV1::SignalTaskFailed {
                                signal_channel_open = false;
                            }
                        }
                        None => {
                            signal_channel_open = false;
                            let decision = permit.request_local_stop(
                                Stage8bP1fLocalStopCauseV1::SupervisionFailure,
                                Utc::now(),
                            )?;
                            if decision == Stage8bP1fDeadlineDecisionV1::ForceKill {
                                let status = child.force_kill_and_reap()?;
                                let _ = finish_after_child_stop(
                                    store,
                                    &permit,
                                    child_starts,
                                    status,
                                    true,
                                    Utc::now(),
                                )?;
                                return Err(Stage8bP1fLocalSupervisionErrorV1::ForceKilled);
                            }
                            child.signal(libc::SIGTERM)?;
                            stopping_signal_sent = true;
                        }
                    }
                }
            }
        }

        match wait_restart_delay(
            store,
            &mut permit,
            child_starts,
            &mut signal_receiver,
            &mut signal_channel_open,
            policy,
        )
        .await?
        {
            RestartDelayOutcomeV1::Restart => continue,
            RestartDelayOutcomeV1::Terminal(result) => {
                return settle_terminal_result(&permit, *result)
            }
        }
    }
}

enum RestartDelayOutcomeV1 {
    Restart,
    Terminal(Box<Stage8bP1fLocalSupervisionResultV1>),
}

async fn wait_restart_delay(
    store: &Stage8bP1fAuthorityStoreV1,
    permit: &mut Stage8bP1fRunPermitV1,
    child_starts: u32,
    signal_receiver: &mut mpsc::UnboundedReceiver<LocalSignalV1>,
    signal_channel_open: &mut bool,
    policy: LocalSupervisionPolicyV1,
) -> Result<RestartDelayOutcomeV1, Stage8bP1fLocalSupervisionErrorV1> {
    let deadline = Instant::now() + policy.restart_delay;
    while Instant::now() < deadline {
        match poll_guardian(permit, Utc::now())? {
            Stage8bP1fDeadlineDecisionV1::Continue => {}
            decision => {
                return Ok(RestartDelayOutcomeV1::Terminal(Box::new(
                    finish_without_child(store, permit, child_starts, decision, Utc::now())?,
                )));
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        tokio::select! {
            () = tokio::time::sleep(remaining.min(policy.poll_interval)) => {}
            signal = signal_receiver.recv(), if *signal_channel_open => {
                let signal = match signal {
                    Some(signal) => signal,
                    None => {
                        *signal_channel_open = false;
                        LocalSignalV1::SignalTaskFailed
                    }
                };
                let decision = permit.request_local_stop(signal.stop_cause(), Utc::now())?;
                #[cfg(any(test, feature = "stage8b-p1-test-fixtures"))]
                if let Some(grace) = policy.stop_grace_override {
                    permit.shorten_stop_grace_for_test(grace);
                }
                return Ok(RestartDelayOutcomeV1::Terminal(Box::new(
                    finish_without_child(store, permit, child_starts, decision, Utc::now())?,
                )));
            }
        }
    }
    Ok(RestartDelayOutcomeV1::Restart)
}

fn poll_guardian(
    permit: &mut Stage8bP1fRunPermitV1,
    trusted_now: DateTime<Utc>,
) -> Result<Stage8bP1fDeadlineDecisionV1, Stage8bP1fLocalSupervisionErrorV1> {
    permit.poll_deadline(trusted_now).map_err(Into::into)
}

fn finish_without_child(
    store: &Stage8bP1fAuthorityStoreV1,
    permit: &Stage8bP1fRunPermitV1,
    child_starts: u32,
    decision: Stage8bP1fDeadlineDecisionV1,
    trusted_now: DateTime<Utc>,
) -> Result<Stage8bP1fLocalSupervisionResultV1, Stage8bP1fLocalSupervisionErrorV1> {
    let reason = permit.stopping_reason_code().unwrap_or("recovery-stop");
    let (state, terminal_reason, disposition) =
        terminal_classification(reason, false, decision, None);
    let receipt = store.finish_phase(
        permit.manifest_sha256(),
        state,
        terminal_reason,
        trusted_now,
    )?;
    Ok(supervision_result(
        permit.manifest_sha256(),
        child_starts,
        None,
        disposition,
        false,
        receipt,
    ))
}

fn finish_after_child_stop(
    store: &Stage8bP1fAuthorityStoreV1,
    permit: &Stage8bP1fRunPermitV1,
    child_starts: u32,
    status: ExitStatus,
    forced: bool,
    trusted_now: DateTime<Utc>,
) -> Result<Stage8bP1fLocalSupervisionResultV1, Stage8bP1fLocalSupervisionErrorV1> {
    let reason = permit
        .stopping_reason_code()
        .unwrap_or("supervision-failure");
    let decision = if forced {
        Stage8bP1fDeadlineDecisionV1::ForceKill
    } else {
        Stage8bP1fDeadlineDecisionV1::BeginStopping
    };
    let (state, terminal_reason, disposition) =
        terminal_classification(reason, forced, decision, Some(&status));
    let receipt = store.finish_phase(
        permit.manifest_sha256(),
        state,
        terminal_reason,
        trusted_now,
    )?;
    Ok(supervision_result(
        permit.manifest_sha256(),
        child_starts,
        Some(status),
        disposition,
        forced,
        receipt,
    ))
}

fn terminal_classification(
    stopping_reason: &str,
    forced: bool,
    decision: Stage8bP1fDeadlineDecisionV1,
    child_status: Option<&ExitStatus>,
) -> (
    Stage8bP1fPhaseStateV1,
    &'static str,
    Stage8bP1fLocalSupervisionDispositionV1,
) {
    if !forced && child_status.is_some_and(|status| !status.success()) {
        return (
            Stage8bP1fPhaseStateV1::Failed,
            "child-stop-failed",
            Stage8bP1fLocalSupervisionDispositionV1::RecoveryTerminated,
        );
    }
    if !forced && decision == Stage8bP1fDeadlineDecisionV1::ForceKill {
        return (
            Stage8bP1fPhaseStateV1::Failed,
            "recovered-stop-complete",
            Stage8bP1fLocalSupervisionDispositionV1::RecoveryTerminated,
        );
    }
    if stopping_reason == "deadline-reached" {
        return (
            Stage8bP1fPhaseStateV1::Expired,
            if forced {
                "phase-deadline-force-killed"
            } else {
                "phase-deadline"
            },
            if forced {
                Stage8bP1fLocalSupervisionDispositionV1::ForceKilled
            } else {
                Stage8bP1fLocalSupervisionDispositionV1::DeadlineStop
            },
        );
    }
    if matches!(stopping_reason, "external-sigterm" | "external-sigint") && !forced {
        return (
            Stage8bP1fPhaseStateV1::Completed,
            "external-stop-complete",
            Stage8bP1fLocalSupervisionDispositionV1::GracefulStop,
        );
    }
    (
        Stage8bP1fPhaseStateV1::Failed,
        if forced {
            "child-force-killed"
        } else if decision == Stage8bP1fDeadlineDecisionV1::ForceKill {
            "recovered-stop-complete"
        } else {
            "conservative-stop-complete"
        },
        if forced {
            Stage8bP1fLocalSupervisionDispositionV1::ForceKilled
        } else {
            Stage8bP1fLocalSupervisionDispositionV1::RecoveryTerminated
        },
    )
}

fn settle_terminal_result(
    permit: &Stage8bP1fRunPermitV1,
    result: Stage8bP1fLocalSupervisionResultV1,
) -> Result<Stage8bP1fLocalSupervisionResultV1, Stage8bP1fLocalSupervisionErrorV1> {
    if result
        .last_exit_code
        .is_some_and(|code| code != libc::EXIT_SUCCESS)
        || result.last_signal.is_some() && !result.force_killed
    {
        let code = match result.last_exit_code {
            Some(code @ (70..=72)) => code as u8,
            _ => 70,
        };
        return Err(Stage8bP1fLocalSupervisionErrorV1::ChildExit(code));
    }
    if result.terminal_receipt.terminal_state == Stage8bP1fPhaseStateV1::Failed {
        return match permit.stopping_reason_code() {
            Some("supervision-failure") => Err(Stage8bP1fLocalSupervisionErrorV1::SignalTask),
            _ => Err(Stage8bP1fLocalSupervisionErrorV1::RecoveryTerminated),
        };
    }
    match permit.stopping_reason_code() {
        Some("supervision-failure") => Err(Stage8bP1fLocalSupervisionErrorV1::SignalTask),
        Some("external-sigterm" | "external-sigint" | "deadline-reached") => Ok(result),
        Some(_) | None => Err(Stage8bP1fLocalSupervisionErrorV1::RecoveryTerminated),
    }
}

fn supervision_result(
    manifest_sha256: &str,
    child_starts: u32,
    status: Option<ExitStatus>,
    disposition: Stage8bP1fLocalSupervisionDispositionV1,
    force_killed: bool,
    terminal_receipt: Stage8bP1fTerminalReceiptV1,
) -> Stage8bP1fLocalSupervisionResultV1 {
    Stage8bP1fLocalSupervisionResultV1 {
        schema_version: 1,
        domain: "stage8b-p1f-local-supervision-result-v1".to_string(),
        manifest_sha256: manifest_sha256.to_string(),
        child_starts,
        last_exit_code: status.as_ref().and_then(ExitStatus::code),
        last_signal: status.as_ref().and_then(ExitStatusExt::signal),
        disposition,
        force_killed,
        terminal_receipt,
    }
}

async fn fail_closed_child(child: &mut ChildGroupV1, grace: StdDuration) {
    let _ = child.signal(libc::SIGTERM);
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            let _ = child.reap_exited(status);
            return;
        }
        tokio::time::sleep(PROCESS_POLL_INTERVAL.min(grace)).await;
    }
    let _ = child.force_kill_and_reap();
}

fn prune_start_window(starts: &mut VecDeque<Instant>, window: StdDuration) {
    let now = Instant::now();
    while starts
        .front()
        .is_some_and(|started| now.duration_since(*started) >= window)
    {
        starts.pop_front();
    }
}

fn resolve_service_identity() -> Result<(u32, u32), Stage8bP1fLocalSupervisionErrorV1> {
    let name = CString::new(STAGE8B_P1F_SERVICE_USER)
        .map_err(|_| Stage8bP1fLocalSupervisionErrorV1::ServiceIdentity)?;
    let passwd = unsafe { libc::getpwnam(name.as_ptr()) };
    let group = unsafe { libc::getgrnam(name.as_ptr()) };
    if passwd.is_null() || group.is_null() {
        return Err(Stage8bP1fLocalSupervisionErrorV1::ServiceIdentity);
    }
    let passwd = unsafe { &*passwd };
    let group = unsafe { &*group };
    let passwd_name = unsafe { CStr::from_ptr(passwd.pw_name) };
    let group_name = unsafe { CStr::from_ptr(group.gr_name) };
    if passwd_name.to_bytes() != STAGE8B_P1F_SERVICE_USER.as_bytes()
        || group_name.to_bytes() != STAGE8B_P1F_SERVICE_USER.as_bytes()
        || passwd.pw_uid == 0
        || group.gr_gid == 0
        || passwd.pw_gid != group.gr_gid
    {
        return Err(Stage8bP1fLocalSupervisionErrorV1::ServiceIdentity);
    }
    Ok((passwd.pw_uid, group.gr_gid))
}

#[cfg(any(test, feature = "stage8b-p1-test-fixtures"))]
#[allow(
    dead_code,
    reason = "fixture-only supervision controls are selectively composed"
)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn shell(command: String) -> FixedChildCommandV1 {
        FixedChildCommandV1 {
            executable: PathBuf::from("/bin/sh"),
            args: vec![OsString::from("-c"), OsString::from(command)],
            identity: None,
        }
    }

    pub(crate) fn policy(
        restart_delay: StdDuration,
        fail_closed_grace: StdDuration,
    ) -> LocalSupervisionPolicyV1 {
        LocalSupervisionPolicyV1 {
            poll_interval: StdDuration::from_millis(5),
            restart_window: StdDuration::from_secs(2),
            restart_delay,
            max_starts_per_window: 5,
            fail_closed_grace,
            stop_grace_override: None,
        }
    }

    pub(crate) fn force_policy() -> LocalSupervisionPolicyV1 {
        LocalSupervisionPolicyV1 {
            poll_interval: StdDuration::from_millis(5),
            restart_window: StdDuration::from_secs(2),
            restart_delay: StdDuration::from_millis(5),
            max_starts_per_window: 5,
            fail_closed_grace: StdDuration::from_millis(50),
            stop_grace_override: Some(StdDuration::ZERO),
        }
    }

    pub(crate) fn signals() -> (
        mpsc::UnboundedSender<LocalSignalV1>,
        mpsc::UnboundedReceiver<LocalSignalV1>,
    ) {
        mpsc::unbounded_channel()
    }

    pub(crate) fn sigterm() -> LocalSignalV1 {
        LocalSignalV1::Sigterm
    }

    pub(crate) fn sigint() -> LocalSignalV1 {
        LocalSignalV1::Sigint
    }

    pub(crate) async fn run(
        store: &Stage8bP1fAuthorityStoreV1,
        permit: Stage8bP1fRunPermitV1,
        child: FixedChildCommandV1,
        signals: mpsc::UnboundedReceiver<LocalSignalV1>,
        policy: LocalSupervisionPolicyV1,
    ) -> Result<Stage8bP1fLocalSupervisionResultV1, Stage8bP1fLocalSupervisionErrorV1> {
        supervise_admitted_child(store, permit, child, signals, None, None, policy).await
    }

    pub(crate) async fn run_after_synchronous_startup(
        store: &Stage8bP1fAuthorityStoreV1,
        permit: Stage8bP1fRunPermitV1,
        child: FixedChildCommandV1,
        policy: LocalSupervisionPolicyV1,
        registration_marker: &std::path::Path,
        block_for: StdDuration,
    ) -> Result<Stage8bP1fLocalSupervisionResultV1, Stage8bP1fLocalSupervisionErrorV1> {
        let RegisteredUnixSignalsV1 {
            signal_receiver,
            barrier_sender,
            pre_spawn_witness,
            registrations: _signal_registrations,
            task,
        } = register_unix_signal_supervision().await?;
        std::fs::write(registration_marker, b"registered")
            .map_err(|error| Stage8bP1fLocalSupervisionErrorV1::ChildProcess(error.kind()))?;
        // Model the synchronous identity/open/admission section of the real
        // entry while the already registered Tokio OS handlers retain signal.
        std::thread::sleep(block_for);
        let result = supervise_admitted_child(
            store,
            permit,
            child,
            signal_receiver,
            Some(barrier_sender),
            Some(pre_spawn_witness),
            policy,
        )
        .await;
        task.abort();
        result
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn hold_pdeath_child(marker: &std::path::Path, pid_path: &std::path::Path) -> ! {
        let child = ChildGroupV1::spawn(&shell(
            "trap '' TERM INT; while :; do sleep 1; done".to_string(),
        ))
        .expect("spawn parent-death child");
        std::fs::write(pid_path, child.process_group.to_string()).expect("write child pid");
        std::fs::write(marker, b"ready").expect("write helper marker");
        std::mem::forget(child);
        loop {
            std::thread::park();
        }
    }

    pub(crate) fn prepare(
        store: &Stage8bP1fAuthorityStoreV1,
        manifest_sha256: &str,
        trusted_now: DateTime<Utc>,
    ) -> Result<PreparedRunV1, Stage8bP1fLocalSupervisionErrorV1> {
        prepare_permit_or_expire(store, manifest_sha256, trusted_now)
    }
}
