//! Fixed Stage 8B-P1-f O2 bootstrap-unit supervision.

use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::future::Future;
use std::io::{ErrorKind, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};
use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::signal::unix::{signal, SignalKind};

use crate::{
    Stage8bP1fAuthorityErrorV1, Stage8bP1fAuthorityStoreV1, Stage8bP1fDeadlineDecisionV1,
    Stage8bP1fOperatorStopCauseV1, Stage8bP1fPhaseStateV1, Stage8bP1fRunPermitV1,
    Stage8bP1fTerminalReceiptV1, STAGE8B_P1F_SERVICE_USER,
};

pub const STAGE8B_P1F_O2_RUNNER_BINARY_PATH: &str =
    "/usr/local/libexec/moex/stage8b-p1f-o2-operator";
pub const STAGE8B_P1F_O2_RUNNER_UNIT: &str = "moex-finam-p1-paper-o2-bootstrap-runner.service";
pub const STAGE8B_P1F_O2_ACTIVE_MANIFEST_PATH: &str =
    "/etc/moex-finam-p1-paper/o2/active-manifest.sha256";
const BOOTSTRAP_UNIT: &str = "moex-finam-p1-paper-bootstrap.service";
const MATERIALIZER_UNIT: &str = "moex-finam-p1f-o2-materializer.service";
const INSTALLATION_PATH: &str = "/usr/local/share/moex/stage8b-p1e/installation-o2-v1.json";
const CONFIG_ROOT: &str = "/etc/moex-finam-p1-paper";
const SYSTEMCTL: &str = "/usr/bin/systemctl";
const POLL_INTERVAL: StdDuration = StdDuration::from_millis(250);
const COMMAND_TIMEOUT: StdDuration = StdDuration::from_secs(5);
const FORCE_KILL_PROOF_TIMEOUT: StdDuration = StdDuration::from_secs(45);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fO2UnitEvidenceV1 {
    pub schema_version: u16,
    pub domain: String,
    pub unit: String,
    pub active_state: String,
    pub sub_state: String,
    pub result: String,
    pub exec_main_status: i32,
    pub main_pid: u32,
    pub control_pid: u32,
    pub job: String,
    pub control_group: String,
    pub cgroup_procs_empty: bool,
    pub stopped_proven: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fO2RunnerResultV1 {
    pub schema_version: u16,
    pub domain: String,
    pub manifest_sha256: String,
    pub disposition: String,
    pub start_client_exit_code: Option<i32>,
    pub force_kill_used: bool,
    pub unit_evidence: Stage8bP1fO2UnitEvidenceV1,
    pub terminal_receipt: Stage8bP1fTerminalReceiptV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materializer_unit_evidence: Option<Stage8bP1fO2UnitEvidenceV1>,
}

/// Serialization only: grants no bootstrap capability. The materializer holds
/// it from before GET collection through staged publication.
pub struct Stage8bP1fO2CollectionLockV1 {
    store: Stage8bP1fAuthorityStoreV1,
    manifest: String,
    _lock: File,
}

impl Stage8bP1fO2CollectionLockV1 {
    pub fn validate_before_publication(&self) -> Result<(), Stage8bP1fO2RunnerErrorV1> {
        let head = self.store.inspect()?;
        if head.state != Stage8bP1fPhaseStateV1::Active
            || head.active_manifest_sha256.as_deref() != Some(&self.manifest)
            || head
                .deadline_utc
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map_or(true, |deadline| Utc::now() >= deadline)
        {
            return Err(Stage8bP1fAuthorityErrorV1::ActiveConflict.into());
        }
        Ok(())
    }
}

pub fn lock_stage8b_p1f_o2_collection_v1(
    expected_manifest: &str,
) -> Result<Stage8bP1fO2CollectionLockV1, Stage8bP1fO2RunnerErrorV1> {
    require_root()?;
    let manifest = read_fixed_manifest_selector()?;
    if manifest != expected_manifest {
        return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict.into());
    }
    let store = Stage8bP1fAuthorityStoreV1::open_production(service_group_gid()?)?;
    let lock = store.lock_o2_effects()?;
    store.validate_o2_cleanup_binding(&manifest, &read_installation()?)?;
    let guard = Stage8bP1fO2CollectionLockV1 {
        store,
        manifest,
        _lock: lock,
    };
    guard.validate_before_publication()?;
    Ok(guard)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fO2ReadOnlyEvidenceV1 {
    pub schema_version: u16,
    pub domain: String,
    pub manifest_sha256: String,
    pub authority: crate::Stage8bP1fAuthorityInspectionV1,
    pub bootstrap_unit: Stage8bP1fO2UnitEvidenceV1,
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1fO2RunnerErrorV1 {
    #[error("O2 runner requires the fixed root identity")]
    RootRequired,
    #[error("O2 runner service group is unavailable")]
    ServiceIdentity,
    #[error("O2 runner signal setup failed")]
    Signal,
    #[error("O2 bounded systemctl command failed: {0:?}")]
    Command(ErrorKind),
    #[error("O2 bounded systemctl command timed out")]
    CommandTimeout,
    #[error("O2 systemd unit evidence is invalid")]
    InvalidUnitEvidence,
    #[error("O2 bootstrap unit stop cannot be proved")]
    StopNotProven,
    #[error(transparent)]
    Authority(#[from] Stage8bP1fAuthorityErrorV1),
}

impl Stage8bP1fO2RunnerErrorV1 {
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::RootRequired | Self::ServiceIdentity => 64,
            Self::Authority(_) => 66,
            Self::StopNotProven | Self::InvalidUnitEvidence => 72,
            Self::Signal => 73,
            Self::Command(_) | Self::CommandTimeout => 74,
        }
    }
}

pub async fn run_stage8b_p1f_o2_systemd_runner_v1(
    manifest_sha256: &str,
) -> Result<Stage8bP1fO2RunnerResultV1, Stage8bP1fO2RunnerErrorV1> {
    require_root()?;
    let pre_spawn_signals = register_pre_spawn_signals()?;
    let mut terminate =
        signal(SignalKind::terminate()).map_err(|_| Stage8bP1fO2RunnerErrorV1::Signal)?;
    let mut interrupt =
        signal(SignalKind::interrupt()).map_err(|_| Stage8bP1fO2RunnerErrorV1::Signal)?;
    let store = Stage8bP1fAuthorityStoreV1::open_production(service_group_gid()?)?;
    let initial_evidence = collect_stage8b_p1f_o2_unit_evidence_v1().await?;
    if initial_evidence.stopped_proven && store.pending_terminal_present()? {
        return recover_and_finish(&store, manifest_sha256).await;
    }
    let mut permit = match store.admit_active_phase(manifest_sha256, Utc::now()) {
        Ok(permit) => permit,
        Err(Stage8bP1fAuthorityErrorV1::DeadlineExpired) => {
            return recover_and_finish(&store, manifest_sha256).await;
        }
        Err(error) => return Err(error.into()),
    };
    if let Some(cause) = pre_spawn_signals.cause() {
        permit.request_operator_stop(cause, Utc::now())?;
        let evidence = stop_kill_and_prove().await?;
        return finish_new_terminal(
            &store,
            manifest_sha256,
            &permit,
            failed_exit_status(),
            true,
            true,
            evidence,
        );
    }
    let mut start = spawn_systemctl(&["start", "--wait", BOOTSTRAP_UNIT])?;
    let mut start_status = None;
    let mut requested_stop = false;
    let mut stop_command_sent = false;
    let mut force_kill_used = false;
    loop {
        if start_status.is_none() {
            start_status = start
                .try_wait()
                .map_err(|error| Stage8bP1fO2RunnerErrorV1::Command(error.kind()))?;
        }
        if start_status.is_some_and(|status| !status.success()) && !requested_stop {
            permit.request_operator_stop(
                Stage8bP1fOperatorStopCauseV1::SupervisionFailure,
                Utc::now(),
            )?;
            requested_stop = true;
            stop_bootstrap_unit().await?;
            stop_command_sent = true;
        }
        let now = Utc::now();
        let decision = permit.poll_deadline(now)?;
        if decision != Stage8bP1fDeadlineDecisionV1::Continue && !stop_command_sent {
            requested_stop = true;
            stop_bootstrap_unit().await?;
            stop_command_sent = true;
        }
        if decision == Stage8bP1fDeadlineDecisionV1::ForceKill && !force_kill_used {
            force_kill_used = true;
            kill_bootstrap_unit().await?;
            let evidence = wait_for_stopped_proof(
                collect_stage8b_p1f_o2_unit_evidence_v1,
                FORCE_KILL_PROOF_TIMEOUT,
                POLL_INTERVAL,
            )
            .await?;
            let status = start_status.unwrap_or_else(failed_exit_status);
            return finish_new_terminal(
                &store,
                manifest_sha256,
                &permit,
                status,
                true,
                force_kill_used,
                evidence,
            );
        }
        if let Some(status) = start_status {
            let evidence = collect_stage8b_p1f_o2_unit_evidence_v1().await?;
            if evidence.stopped_proven {
                return finish_new_terminal(
                    &store,
                    manifest_sha256,
                    &permit,
                    status,
                    requested_stop,
                    force_kill_used,
                    evidence,
                );
            }
        }
        tokio::select! {
            _ = terminate.recv() => {
                if !requested_stop {
                    permit.request_operator_stop(Stage8bP1fOperatorStopCauseV1::Sigterm, Utc::now())?;
                    requested_stop = true;
                }
                if !stop_command_sent {
                    stop_bootstrap_unit().await?;
                    stop_command_sent = true;
                }
            }
            _ = interrupt.recv() => {
                if !requested_stop {
                    permit.request_operator_stop(Stage8bP1fOperatorStopCauseV1::Sigint, Utc::now())?;
                    requested_stop = true;
                }
                if !stop_command_sent {
                    stop_bootstrap_unit().await?;
                    stop_command_sent = true;
                }
            }
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
        }
    }
}

/// Fixed production entry point used by the reviewed systemd unit. The unit
/// cannot supply or substitute a manifest on its command line.
pub async fn run_stage8b_p1f_o2_fixed_systemd_runner_v1(
) -> Result<Stage8bP1fO2RunnerResultV1, Stage8bP1fO2RunnerErrorV1> {
    let manifest = read_fixed_manifest_selector()?;
    run_stage8b_p1f_o2_systemd_runner_v1(&manifest).await
}

/// Reads the root-owned active-manifest selector through the same fixed-path
/// boundary used by the runner and cleanup entry points.
pub fn stage8b_p1f_o2_active_manifest_sha256_v1() -> Result<String, Stage8bP1fO2RunnerErrorV1> {
    read_fixed_manifest_selector()
}

pub async fn run_stage8b_p1f_o2_cleanup_v1(
    manifest_sha256: &str,
) -> Result<Stage8bP1fO2RunnerResultV1, Stage8bP1fO2RunnerErrorV1> {
    require_root()?;
    if read_fixed_manifest_selector()? != manifest_sha256 {
        return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict.into());
    }
    let store = Stage8bP1fAuthorityStoreV1::open_production(service_group_gid()?)?;
    cleanup_with_store(
        &store,
        manifest_sha256,
        Path::new(CONFIG_ROOT),
        &read_installation()?,
        stop_o2_units,
        Utc::now,
    )
    .await
}

// Shared by the public fixed-path cleanup and real-store tests. Only the OS
// stop/probe and clock are substituted in tests; no guardian state is mocked.
pub(crate) async fn cleanup_with_store<F, Fut, C>(
    store: &Stage8bP1fAuthorityStoreV1,
    manifest_sha256: &str,
    config_root: &Path,
    installation: &[u8],
    stop_and_collect: F,
    now: C,
) -> Result<Stage8bP1fO2RunnerResultV1, Stage8bP1fO2RunnerErrorV1>
where
    F: FnOnce() -> Fut,
    Fut: Future<
        Output = Result<
            (Stage8bP1fO2UnitEvidenceV1, Stage8bP1fO2UnitEvidenceV1, bool),
            Stage8bP1fO2RunnerErrorV1,
        >,
    >,
    C: FnOnce() -> DateTime<Utc>,
{
    let _effects = store.lock_o2_effects()?;
    let manifest = store.validate_o2_cleanup_binding(manifest_sha256, installation)?;
    let (evidence, materializer, force_kill_used) = stop_and_collect().await?;
    require_stopped(&evidence, BOOTSTRAP_UNIT)?;
    require_stopped(&materializer, MATERIALIZER_UNIT)?;
    let finish_result = |disposition, receipt| {
        let mut result = result(
            manifest_sha256,
            disposition,
            None,
            force_kill_used,
            evidence.clone(),
            receipt,
        );
        result.materializer_unit_evidence = Some(materializer.clone());
        result
    };
    if let Some(receipt) = store.resume_pending_terminal(manifest_sha256)? {
        return Ok(finish_result("EXACT_PENDING_TERMINAL_REPLAY", receipt));
    }
    store.resume_o2_cleanup_stopping(manifest_sha256)?;
    if let Some(receipt) = store.terminal_receipt(manifest_sha256)? {
        return Ok(finish_result("ALREADY_TERMINAL", receipt));
    }
    let reason = store.validate_o2_cleanup_materialization(manifest_sha256, config_root)?;
    let at = now();
    let expired = at
        >= DateTime::parse_from_rfc3339(&manifest.deadline_utc)
            .map_err(|_| Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)?;
    let state = if expired {
        Stage8bP1fPhaseStateV1::Expired
    } else {
        Stage8bP1fPhaseStateV1::Failed
    };
    // Preserve failure cause independently from deadline classification.
    let receipt = store.finish_phase(manifest_sha256, state, &reason, at)?;
    if store.terminal_receipt(manifest_sha256)?.as_ref() != Some(&receipt) {
        return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict.into());
    }
    Ok(finish_result(
        if expired { "EXPIRED" } else { "FAILED" },
        receipt,
    ))
}

fn require_stopped(
    e: &Stage8bP1fO2UnitEvidenceV1,
    unit: &str,
) -> Result<(), Stage8bP1fO2RunnerErrorV1> {
    if e.schema_version != 1
        || e.domain != "stage8b-p1f-o2-unit-evidence-v1"
        || e.unit != unit
        || !e.stopped_proven
        || !matches!(e.active_state.as_str(), "inactive" | "failed")
        || e.main_pid != 0
        || e.control_pid != 0
        || !matches!(e.job.as_str(), "" | "0")
        || !e.cgroup_procs_empty
    {
        return Err(Stage8bP1fO2RunnerErrorV1::StopNotProven);
    }
    Ok(())
}

/// Fixed `ExecStopPost` entry point. It uses the same root-owned selector as
/// `ExecStart`, so cleanup cannot be redirected to another authority record.
pub async fn run_stage8b_p1f_o2_fixed_cleanup_v1(
) -> Result<Stage8bP1fO2RunnerResultV1, Stage8bP1fO2RunnerErrorV1> {
    let manifest = read_fixed_manifest_selector()?;
    run_stage8b_p1f_o2_cleanup_v1(&manifest).await
}

/// Collects only redacted authority projection plus bootstrap-unit state. It
/// cannot admit, stop, finish or otherwise mutate either state machine.
pub async fn collect_stage8b_p1f_o2_readonly_evidence_v1(
) -> Result<Stage8bP1fO2ReadOnlyEvidenceV1, Stage8bP1fO2RunnerErrorV1> {
    require_root()?;
    let manifest_sha256 = read_fixed_manifest_selector()?;
    let store = Stage8bP1fAuthorityStoreV1::open_production(service_group_gid()?)?;
    Ok(Stage8bP1fO2ReadOnlyEvidenceV1 {
        schema_version: 1,
        domain: "stage8b-p1f-o2-readonly-evidence-v1".into(),
        manifest_sha256,
        authority: store.inspect()?,
        bootstrap_unit: collect_stage8b_p1f_o2_unit_evidence_v1().await?,
    })
}

pub async fn collect_stage8b_p1f_o2_unit_evidence_v1(
) -> Result<Stage8bP1fO2UnitEvidenceV1, Stage8bP1fO2RunnerErrorV1> {
    collect_unit_evidence(BOOTSTRAP_UNIT).await
}

async fn collect_unit_evidence(
    unit: &str,
) -> Result<Stage8bP1fO2UnitEvidenceV1, Stage8bP1fO2RunnerErrorV1> {
    let output = bounded_systemctl(&[
        "show",
        unit,
        "--property=ActiveState,SubState,Result,ExecMainStatus,MainPID,ControlPID,Job,ControlGroup",
    ])
    .await?;
    if !output.status.success() {
        return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
    }
    let mut evidence = parse_unit_evidence(&output.stdout)?;
    evidence.unit = unit.into();
    Ok(evidence)
}

fn finish_new_terminal(
    store: &Stage8bP1fAuthorityStoreV1,
    manifest_sha256: &str,
    permit: &Stage8bP1fRunPermitV1,
    start_status: ExitStatus,
    requested_stop: bool,
    force_kill_used: bool,
    evidence: Stage8bP1fO2UnitEvidenceV1,
) -> Result<Stage8bP1fO2RunnerResultV1, Stage8bP1fO2RunnerErrorV1> {
    if !evidence.stopped_proven {
        return Err(Stage8bP1fO2RunnerErrorV1::StopNotProven);
    }
    if let Some(receipt) = store.resume_pending_terminal(manifest_sha256)? {
        return Ok(result(
            manifest_sha256,
            "EXACT_PENDING_TERMINAL_REPLAY",
            start_status.code(),
            force_kill_used,
            evidence,
            receipt,
        ));
    }
    let now = Utc::now();
    let expired = now
        >= DateTime::parse_from_rfc3339(&permit.deadline_utc())
            .map_err(|_| Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)?
            .with_timezone(&Utc);
    let decision = classify_terminal(
        expired,
        start_status.success(),
        &evidence.result,
        requested_stop,
        permit.stopping_reason_code(),
    );
    let receipt = store.finish_phase(manifest_sha256, decision.state, &decision.reason, now)?;
    Ok(result(
        manifest_sha256,
        decision.disposition,
        start_status.code(),
        force_kill_used,
        evidence,
        receipt,
    ))
}

struct TerminalDecisionV1 {
    state: Stage8bP1fPhaseStateV1,
    reason: String,
    disposition: &'static str,
}

fn classify_terminal(
    expired: bool,
    start_client_success: bool,
    unit_result: &str,
    requested_stop: bool,
    stopping_reason: Option<&str>,
) -> TerminalDecisionV1 {
    if expired {
        TerminalDecisionV1 {
            state: Stage8bP1fPhaseStateV1::Expired,
            reason: "o2-deadline-expired".into(),
            disposition: "EXPIRED",
        }
    } else if start_client_success && unit_result == "success" && !requested_stop {
        TerminalDecisionV1 {
            state: Stage8bP1fPhaseStateV1::Completed,
            reason: "o2-bootstrap-completed".into(),
            disposition: "COMPLETED",
        }
    } else {
        TerminalDecisionV1 {
            state: Stage8bP1fPhaseStateV1::Failed,
            reason: stopping_reason.unwrap_or("o2-bootstrap-failed").into(),
            disposition: "FAILED",
        }
    }
}

async fn recover_and_finish(
    store: &Stage8bP1fAuthorityStoreV1,
    manifest_sha256: &str,
) -> Result<Stage8bP1fO2RunnerResultV1, Stage8bP1fO2RunnerErrorV1> {
    if read_fixed_manifest_selector()? != manifest_sha256 {
        return Err(Stage8bP1fAuthorityErrorV1::HistoryConflict.into());
    }
    cleanup_with_store(
        store,
        manifest_sha256,
        Path::new(CONFIG_ROOT),
        &read_installation()?,
        stop_o2_units,
        Utc::now,
    )
    .await
}

fn result(
    manifest_sha256: &str,
    disposition: &str,
    start_client_exit_code: Option<i32>,
    force_kill_used: bool,
    unit_evidence: Stage8bP1fO2UnitEvidenceV1,
    terminal_receipt: Stage8bP1fTerminalReceiptV1,
) -> Stage8bP1fO2RunnerResultV1 {
    Stage8bP1fO2RunnerResultV1 {
        schema_version: 1,
        domain: "stage8b-p1f-o2-runner-result-v1".into(),
        manifest_sha256: manifest_sha256.into(),
        disposition: disposition.into(),
        start_client_exit_code,
        force_kill_used,
        unit_evidence,
        terminal_receipt,
        materializer_unit_evidence: None,
    }
}

async fn stop_kill_and_prove() -> Result<Stage8bP1fO2UnitEvidenceV1, Stage8bP1fO2RunnerErrorV1> {
    Ok(stop_kill_and_prove_unit(BOOTSTRAP_UNIT).await?.0)
}

async fn stop_o2_units(
) -> Result<(Stage8bP1fO2UnitEvidenceV1, Stage8bP1fO2UnitEvidenceV1, bool), Stage8bP1fO2RunnerErrorV1>
{
    let (bootstrap, b_killed) = stop_kill_and_prove_unit(BOOTSTRAP_UNIT).await?;
    let (materializer, m_killed) = stop_kill_and_prove_unit(MATERIALIZER_UNIT).await?;
    Ok((bootstrap, materializer, b_killed || m_killed))
}

async fn stop_kill_and_prove_unit(
    unit: &str,
) -> Result<(Stage8bP1fO2UnitEvidenceV1, bool), Stage8bP1fO2RunnerErrorV1> {
    // Never stop/wait on the runner itself: this is also its ExecStopPost.
    if !matches!(unit, BOOTSTRAP_UNIT | MATERIALIZER_UNIT) {
        return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
    }
    let stopped = bounded_systemctl(&["stop", "--no-block", unit]).await?;
    if !stopped.status.success() {
        return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
    }
    let evidence = collect_unit_evidence(unit).await?;
    if evidence.stopped_proven {
        return Ok((evidence, false));
    }
    let killed = bounded_systemctl(&["kill", "--kill-who=all", "--signal=SIGKILL", unit]).await?;
    if !killed.status.success() {
        return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
    }
    let evidence = wait_for_stopped_proof(
        || collect_unit_evidence(unit),
        FORCE_KILL_PROOF_TIMEOUT,
        POLL_INTERVAL,
    )
    .await?;
    Ok((evidence, true))
}

async fn wait_for_stopped_proof<F, Fut>(
    mut collect: F,
    timeout: StdDuration,
    poll_interval: StdDuration,
) -> Result<Stage8bP1fO2UnitEvidenceV1, Stage8bP1fO2RunnerErrorV1>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Stage8bP1fO2UnitEvidenceV1, Stage8bP1fO2RunnerErrorV1>>,
{
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Stage8bP1fO2RunnerErrorV1::StopNotProven);
        }
        let evidence = tokio::time::timeout(remaining, collect())
            .await
            .map_err(|_| Stage8bP1fO2RunnerErrorV1::StopNotProven)??;
        if evidence.stopped_proven {
            return Ok(evidence);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Stage8bP1fO2RunnerErrorV1::StopNotProven);
        }
        tokio::time::sleep(poll_interval.min(remaining)).await;
    }
}

async fn stop_bootstrap_unit() -> Result<(), Stage8bP1fO2RunnerErrorV1> {
    let output = bounded_systemctl(&["stop", "--no-block", BOOTSTRAP_UNIT]).await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)
    }
}

async fn kill_bootstrap_unit() -> Result<(), Stage8bP1fO2RunnerErrorV1> {
    let output =
        bounded_systemctl(&["kill", "--kill-who=all", "--signal=SIGKILL", BOOTSTRAP_UNIT]).await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)
    }
}

struct CommandOutputV1 {
    status: ExitStatus,
    stdout: Vec<u8>,
}

async fn bounded_systemctl(args: &[&str]) -> Result<CommandOutputV1, Stage8bP1fO2RunnerErrorV1> {
    let mut child = spawn_systemctl(args)?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| Stage8bP1fO2RunnerErrorV1::Command(error.kind()))?
        {
            let output = child
                .wait_with_output()
                .map_err(|error| Stage8bP1fO2RunnerErrorV1::Command(error.kind()))?;
            return Ok(CommandOutputV1 {
                status,
                stdout: output.stdout,
            });
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Stage8bP1fO2RunnerErrorV1::CommandTimeout);
        }
        tokio::time::sleep(StdDuration::from_millis(25)).await;
    }
}

fn spawn_systemctl(args: &[&str]) -> Result<Child, Stage8bP1fO2RunnerErrorV1> {
    Command::new(SYSTEMCTL)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| Stage8bP1fO2RunnerErrorV1::Command(error.kind()))
}

pub(crate) fn parse_unit_evidence(
    bytes: &[u8],
) -> Result<Stage8bP1fO2UnitEvidenceV1, Stage8bP1fO2RunnerErrorV1> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)?;
    let mut fields = BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)?;
        if fields.insert(key, value).is_some() {
            return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
        }
    }
    let active_state = field(&fields, "ActiveState")?.to_string();
    let sub_state = field(&fields, "SubState")?.to_string();
    let result = field(&fields, "Result")?.to_string();
    let exec_main_status = field(&fields, "ExecMainStatus")?
        .parse()
        .map_err(|_| Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)?;
    let main_pid = field(&fields, "MainPID")?
        .parse()
        .map_err(|_| Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)?;
    let control_pid = field(&fields, "ControlPID")?
        .parse()
        .map_err(|_| Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)?;
    let job = field(&fields, "Job")?.to_string();
    let control_group = field(&fields, "ControlGroup")?.to_string();
    let cgroup_procs_empty = cgroup_is_empty(&control_group)?;
    let stopped_proven = matches!(active_state.as_str(), "inactive" | "failed")
        && main_pid == 0
        && control_pid == 0
        && matches!(job.as_str(), "" | "0")
        && cgroup_procs_empty;
    Ok(Stage8bP1fO2UnitEvidenceV1 {
        schema_version: 1,
        domain: "stage8b-p1f-o2-unit-evidence-v1".into(),
        unit: BOOTSTRAP_UNIT.into(),
        active_state,
        sub_state,
        result,
        exec_main_status,
        main_pid,
        control_pid,
        job,
        control_group,
        cgroup_procs_empty,
        stopped_proven,
    })
}

fn field<'a>(
    fields: &'a BTreeMap<&str, &str>,
    key: &str,
) -> Result<&'a str, Stage8bP1fO2RunnerErrorV1> {
    fields
        .get(key)
        .copied()
        .ok_or(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)
}

fn cgroup_is_empty(control_group: &str) -> Result<bool, Stage8bP1fO2RunnerErrorV1> {
    if control_group.is_empty() {
        return Ok(true);
    }
    if control_group == "/" || !control_group.starts_with('/') || control_group.contains("..") {
        return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
    }
    let path = Path::new("/sys/fs/cgroup").join(control_group.trim_start_matches('/'));
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(Stage8bP1fO2RunnerErrorV1::Command(error.kind())),
        Ok(meta) if !meta.is_dir() => return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence),
        Ok(_) => {}
    }
    let procs = std::fs::read_to_string(path.join("cgroup.procs"))
        .map_err(|e| Stage8bP1fO2RunnerErrorV1::Command(e.kind()))?;
    // cgroup v2 populated covers descendants, unlike this group's procs alone.
    let events = std::fs::read_to_string(path.join("cgroup.events"))
        .map_err(|e| Stage8bP1fO2RunnerErrorV1::Command(e.kind()))?;
    Ok(procs.trim().is_empty() && cgroup_unpopulated(&events)?)
}

fn cgroup_unpopulated(events: &str) -> Result<bool, Stage8bP1fO2RunnerErrorV1> {
    let mut populated = None;
    for line in events.lines() {
        let mut fields = line.split_whitespace();
        if fields.next() == Some("populated") {
            if populated.is_some() {
                return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
            }
            populated = match (fields.next(), fields.next()) {
                (Some("0"), None) => Some(false),
                (Some("1"), None) => Some(true),
                _ => return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence),
            };
        }
    }
    populated
        .map(|value| !value)
        .ok_or(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)
}

fn read_fixed_manifest_selector() -> Result<String, Stage8bP1fO2RunnerErrorV1> {
    require_root()?;
    let path = Path::new(STAGE8B_P1F_O2_ACTIVE_MANIFEST_PATH);
    let expected_gid = service_group_gid()?;
    let before = path
        .symlink_metadata()
        .map_err(|error| Stage8bP1fO2RunnerErrorV1::Command(error.kind()))?;
    if !before.is_file()
        || before.file_type().is_symlink()
        || before.nlink() != 1
        || before.uid() != 0
        || before.gid() != expected_gid
        || before.permissions().mode() & 0o777 != 0o440
        || before.len() != 65
    {
        return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| Stage8bP1fO2RunnerErrorV1::Command(error.kind()))?;
    let opened = file
        .metadata()
        .map_err(|error| Stage8bP1fO2RunnerErrorV1::Command(error.kind()))?;
    if opened.dev() != before.dev()
        || opened.ino() != before.ino()
        || opened.len() != before.len()
        || opened.mtime() != before.mtime()
        || opened.mtime_nsec() != before.mtime_nsec()
    {
        return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
    }
    let mut bytes = Vec::with_capacity(65);
    file.read_to_end(&mut bytes)
        .map_err(|error| Stage8bP1fO2RunnerErrorV1::Command(error.kind()))?;
    let after = file
        .metadata()
        .map_err(|error| Stage8bP1fO2RunnerErrorV1::Command(error.kind()))?;
    if after.dev() != opened.dev()
        || after.ino() != opened.ino()
        || after.len() != opened.len()
        || after.mtime() != opened.mtime()
        || after.mtime_nsec() != opened.mtime_nsec()
        || bytes.len() != 65
        || bytes.last() != Some(&b'\n')
    {
        return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
    }
    bytes.pop();
    let manifest =
        std::str::from_utf8(&bytes).map_err(|_| Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence)?;
    if manifest.len() != 64
        || !manifest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
    }
    Ok(manifest.to_string())
}

fn read_installation() -> Result<Vec<u8>, Stage8bP1fO2RunnerErrorV1> {
    let path = Path::new(INSTALLATION_PATH);
    let before = path
        .symlink_metadata()
        .map_err(|e| Stage8bP1fO2RunnerErrorV1::Command(e.kind()))?;
    let valid = |m: &std::fs::Metadata| {
        m.is_file()
            && m.nlink() == 1
            && m.uid() == 0
            && m.gid() == 0
            && m.permissions().mode() & 0o777 == 0o644
            && m.len() > 0
            && m.len() <= 65536
    };
    if !valid(&before) {
        return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
    }
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| Stage8bP1fO2RunnerErrorV1::Command(e.kind()))?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut f)
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| Stage8bP1fO2RunnerErrorV1::Command(e.kind()))?;
    let after = f
        .metadata()
        .map_err(|e| Stage8bP1fO2RunnerErrorV1::Command(e.kind()))?;
    if !valid(&after)
        || after.dev() != before.dev()
        || after.ino() != before.ino()
        || after.len() != before.len()
        || after.len() as usize != bytes.len()
        || after.mtime() != before.mtime()
        || after.mtime_nsec() != before.mtime_nsec()
    {
        return Err(Stage8bP1fO2RunnerErrorV1::InvalidUnitEvidence);
    }
    Ok(bytes)
}

fn require_root() -> Result<(), Stage8bP1fO2RunnerErrorV1> {
    if unsafe { libc::geteuid() } == 0 {
        Ok(())
    } else {
        Err(Stage8bP1fO2RunnerErrorV1::RootRequired)
    }
}

struct PreSpawnSignalsV1 {
    observed: Arc<AtomicI32>,
    terminate: signal_hook_registry::SigId,
    interrupt: signal_hook_registry::SigId,
}

impl PreSpawnSignalsV1 {
    fn cause(&self) -> Option<Stage8bP1fOperatorStopCauseV1> {
        match self.observed.load(Ordering::SeqCst) {
            libc::SIGTERM => Some(Stage8bP1fOperatorStopCauseV1::Sigterm),
            libc::SIGINT => Some(Stage8bP1fOperatorStopCauseV1::Sigint),
            _ => None,
        }
    }
}

impl Drop for PreSpawnSignalsV1 {
    fn drop(&mut self) {
        let _ = signal_hook_registry::unregister(self.terminate);
        let _ = signal_hook_registry::unregister(self.interrupt);
    }
}

fn register_pre_spawn_signals() -> Result<PreSpawnSignalsV1, Stage8bP1fO2RunnerErrorV1> {
    let observed = Arc::new(AtomicI32::new(0));
    let terminate_observed = Arc::clone(&observed);
    let terminate = unsafe {
        signal_hook_registry::register(libc::SIGTERM, move || {
            let _ = terminate_observed.compare_exchange(
                0,
                libc::SIGTERM,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        })
    }
    .map_err(|_| Stage8bP1fO2RunnerErrorV1::Signal)?;
    let interrupt_observed = Arc::clone(&observed);
    let interrupt = match unsafe {
        signal_hook_registry::register(libc::SIGINT, move || {
            let _ = interrupt_observed.compare_exchange(
                0,
                libc::SIGINT,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        })
    } {
        Ok(registration) => registration,
        Err(_) => {
            let _ = signal_hook_registry::unregister(terminate);
            return Err(Stage8bP1fO2RunnerErrorV1::Signal);
        }
    };
    Ok(PreSpawnSignalsV1 {
        observed,
        terminate,
        interrupt,
    })
}

fn service_group_gid() -> Result<u32, Stage8bP1fO2RunnerErrorV1> {
    let name = CString::new(STAGE8B_P1F_SERVICE_USER)
        .map_err(|_| Stage8bP1fO2RunnerErrorV1::ServiceIdentity)?;
    let group = unsafe { libc::getgrnam(name.as_ptr()) };
    if group.is_null() {
        return Err(Stage8bP1fO2RunnerErrorV1::ServiceIdentity);
    }
    Ok(unsafe { (*group).gr_gid })
}

#[cfg(unix)]
fn failed_exit_status() -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(1 << 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_proof_requires_job_pid_state_and_empty_cgroup() {
        let evidence = parse_unit_evidence(
            b"ActiveState=inactive\nSubState=dead\nResult=success\nExecMainStatus=0\nMainPID=0\nControlPID=0\nJob=\nControlGroup=\n",
        )
        .unwrap();
        assert!(evidence.stopped_proven);
        for mutation in [
            b"ActiveState=active\nSubState=running\nResult=success\nExecMainStatus=0\nMainPID=0\nControlPID=0\nJob=\nControlGroup=\n".as_slice(),
            b"ActiveState=inactive\nSubState=dead\nResult=success\nExecMainStatus=0\nMainPID=42\nControlPID=0\nJob=\nControlGroup=\n".as_slice(),
            b"ActiveState=failed\nSubState=failed\nResult=exit-code\nExecMainStatus=1\nMainPID=0\nControlPID=0\nJob=99\nControlGroup=\n".as_slice(),
        ] {
            assert!(!parse_unit_evidence(mutation).unwrap().stopped_proven);
        }
    }

    #[test]
    fn malformed_or_duplicate_systemd_properties_fail_closed() {
        assert!(cgroup_is_empty("/").is_err());
        assert!(cgroup_is_empty("/../foreign").is_err());
        assert!(cgroup_unpopulated("populated 0\nfrozen 0\n").unwrap());
        assert!(!cgroup_unpopulated("populated 1\nfrozen 0\n").unwrap());
        for invalid in ["", "populated 2\n", "populated 0\npopulated 0\n"] {
            assert!(cgroup_unpopulated(invalid).is_err());
        }
        assert!(parse_unit_evidence(b"ActiveState=inactive\n").is_err());
        assert!(parse_unit_evidence(
            b"ActiveState=inactive\nActiveState=failed\nSubState=dead\nResult=success\nExecMainStatus=0\nMainPID=0\nControlPID=0\nJob=\nControlGroup=\n"
        )
        .is_err());
    }

    #[test]
    fn terminal_classifier_requires_verified_success_before_deadline() {
        let completed = classify_terminal(false, true, "success", false, None);
        assert_eq!(completed.state, Stage8bP1fPhaseStateV1::Completed);
        assert_eq!(completed.disposition, "COMPLETED");

        for failed in [
            classify_terminal(false, false, "exit-code", false, None),
            classify_terminal(false, true, "exit-code", false, None),
            classify_terminal(false, true, "success", true, Some("external-sigterm")),
            classify_terminal(false, true, "success", true, Some("external-sigint")),
            classify_terminal(false, true, "success", true, Some("supervision-failure")),
        ] {
            assert_eq!(failed.state, Stage8bP1fPhaseStateV1::Failed);
            assert_eq!(failed.disposition, "FAILED");
        }

        let expired = classify_terminal(true, true, "success", false, None);
        assert_eq!(expired.state, Stage8bP1fPhaseStateV1::Expired);
        assert_eq!(expired.disposition, "EXPIRED");
    }

    fn evidence(stopped_proven: bool) -> Stage8bP1fO2UnitEvidenceV1 {
        Stage8bP1fO2UnitEvidenceV1 {
            schema_version: 1,
            domain: "stage8b-p1f-o2-unit-evidence-v1".into(),
            unit: BOOTSTRAP_UNIT.into(),
            active_state: if stopped_proven {
                "inactive"
            } else {
                "deactivating"
            }
            .into(),
            sub_state: if stopped_proven {
                "dead"
            } else {
                "stop-sigkill"
            }
            .into(),
            result: "success".into(),
            exec_main_status: 0,
            main_pid: u32::from(!stopped_proven),
            control_pid: 0,
            job: if stopped_proven { "" } else { "17" }.into(),
            control_group: String::new(),
            cgroup_procs_empty: stopped_proven,
            stopped_proven,
        }
    }

    #[tokio::test]
    async fn controlled_stop_proof_adapter_covers_proof_kill_then_proof_and_timeout() {
        let stopped = wait_for_stopped_proof(
            || async { Ok(evidence(true)) },
            StdDuration::from_secs(1),
            StdDuration::ZERO,
        )
        .await
        .unwrap();
        assert!(stopped.stopped_proven);

        let observations = Arc::new(std::sync::Mutex::new(vec![false, true].into_iter()));
        let after_kill = wait_for_stopped_proof(
            {
                let observations = Arc::clone(&observations);
                move || {
                    let observations = Arc::clone(&observations);
                    async move {
                        Ok(evidence(
                            observations.lock().unwrap().next().unwrap_or(true),
                        ))
                    }
                }
            },
            StdDuration::from_secs(1),
            StdDuration::ZERO,
        )
        .await
        .unwrap();
        assert!(after_kill.stopped_proven);

        let failure = wait_for_stopped_proof(
            || async { Ok(evidence(false)) },
            StdDuration::from_millis(1),
            StdDuration::ZERO,
        )
        .await
        .unwrap_err();
        assert_eq!(failure.exit_code(), 72);
    }
}
