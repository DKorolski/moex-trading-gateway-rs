//! Fixed, finite O3 composition. No bootstrap, source discovery, clock override,
//! namespace initialization or FINAM transport is available on this path.
use std::{fs, path::Path, time::Duration as StdDuration};

use chrono::{DateTime, Utc};
use runtime_durable_service as durable;
use tokio::sync::watch;

pub(crate) mod authority;
mod inputs;
pub(crate) mod producers;
mod provisioning;
pub(crate) use inputs::AdmittedInputs;
pub use inputs::{O3CompositionPlan, O3ObservedInput, O3ScheduleTemplate};
pub use provisioning::provision_fixed_o3_redis;

pub const O3_CONFIG_ROOT: &str = "/etc/moex-finam-p1-paper/o3";
pub const O3_PRODUCER_ROOT: &str = "/var/lib/moex-finam-p1-paper-o3-producer";
pub const O3_PHASE_PATH: &str = "/etc/moex-finam-p1-paper/o3/phase-manifest.signed.json";
const HIGH_WATER: &str = "/var/lib/moex-finam-p1-paper-o3-producer/m10-high-water.json";
const PREDECESSOR: &str = "/var/lib/moex-finam-p1-paper-o3-producer/m10-predecessor.json";
const SCHEDULE_STATE: &str = "/var/lib/moex-finam-p1-paper-o3-producer/schedule-high-water.json";

#[derive(Debug, thiserror::Error)]
pub enum O3Error {
    #[error("O3 fixed input, custody or phase binding is invalid")]
    Input,
    #[error("O3 preparation window elapsed")]
    Deadline,
    #[error("O3 fixed producer failed")]
    Producer,
    #[error("O3 fixed producer preparation/publication failed: {0}")]
    ProducerDetail(crate::Stage8bP1fProducerErrorV1),
    #[error("O3 ordinary runtime failed: {0}")]
    Runtime(durable::Stage8bP1eProcessErrorV1),
    #[error("O3 fixed filesystem operation failed")]
    Io,
}

/// Explicit, separate first-publication preparation. No Redis write or runtime
/// call is reachable here. The returned public envelope is installed for the
/// initializer only AFTER its exact Prepared checkpoint is durable.
pub fn prepare_fixed_initial_schedule(
    manifest_sha256: &str,
    confirmation: &str,
) -> Result<Vec<u8>, O3Error> {
    if !cfg!(target_os = "linux")
        || unsafe { libc::geteuid() } == 0
        || confirmation != "PREPARE-FRESH-O3-SCHEDULE"
    {
        return Err(O3Error::Input);
    }
    let inputs = AdmittedInputs::load_fixed(manifest_sha256)?;
    producers::validate_private_root(Path::new(O3_PRODUCER_ROOT))?;
    let _lease = producers::producer_lease()?;
    producers::prepare_initial_schedule(&inputs)
}

/// Called only by the fixed guardian child. The parent keeps the existing
/// execution lock, deadline and restart budget. The typed Published window is
/// passed directly, never serialized across exec. The runtime still performs
/// its own root/seal admission and exact Redis reread before callbacks.
pub async fn run_fixed(manifest_sha256: &str) -> Result<(), O3Error> {
    if unsafe { libc::geteuid() } == 0 {
        return Err(O3Error::Input);
    }
    let inputs = AdmittedInputs::load_fixed(manifest_sha256)?;
    producers::validate_private_root(Path::new(O3_PRODUCER_ROOT))?;
    // Exclude a second producer even if someone invokes the child outside the
    // fixed supervisor. This never creates/replaces the accepted durable root.
    let _lease = producers::producer_lease()?;
    let (stop, stopped) = watch::channel(false);
    // Observe the same external signals as the ordinary runtime. On a parent
    // stop, cease signing/publication first: the guardian has already entered
    // Stopping and the signer must no longer grant fresh authority.
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| O3Error::Producer)?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|_| O3Error::Producer)?;
    let (schedule, current_schedule) = watch::channel(None);
    let schedule_inputs = inputs.clone();
    let mut publisher =
        tokio::spawn(
            async move { producers::schedule_loop(schedule_inputs, schedule, stopped).await },
        );
    let prepared = tokio::select! {
        biased;
        _ = terminate.recv() => {
            let _ = stop.send(true);
            let _ = publisher.await;
            return Ok(());
        },
        _ = interrupt.recv() => {
            let _ = stop.send(true);
            let _ = publisher.await;
            return Ok(());
        },
        result = producers::published_window(&inputs, current_schedule) => result,
        _ = &mut publisher => Err(O3Error::Producer),
    };
    let window = match prepared {
        Ok(window) => window,
        Err(error) => {
            let _ = stop.send(true);
            if !publisher.is_finished() {
                let _ = publisher.await;
            }
            return Err(error);
        }
    };
    // Keep the ordinary future alive during publisher failure: SIGTERM asks
    // the existing runtime to drain. Do not cancel/drop a pending durable step.
    let runtime = durable::execute_stage8b_p1e_observed_run_v1(window);
    tokio::pin!(runtime);
    let result = tokio::select! {
        biased;
        _ = terminate.recv() => {
            let _ = stop.send(true);
            runtime.await.map(|_| ()).map_err(O3Error::Runtime)
        },
        _ = interrupt.recv() => {
            let _ = stop.send(true);
            runtime.await.map(|_| ()).map_err(O3Error::Runtime)
        },
        result = &mut runtime => result.map(|_| ()).map_err(O3Error::Runtime),
        _ = &mut publisher => {
            unsafe { libc::raise(libc::SIGTERM); }
            let _ = runtime.await;
            Err(O3Error::Producer)
        }
    };
    let _ = stop.send(true);
    // The selected runtime/signal result owns shutdown. The publisher may
    // have observed revoked signing authority concurrently with that stop.
    // Its unprompted termination above still remains an explicit fatal error.
    if !publisher.is_finished() {
        let _ = publisher.await;
    }
    result
}

pub(crate) fn now_before(deadline: DateTime<Utc>) -> Result<DateTime<Utc>, O3Error> {
    let now = Utc::now();
    if now >= deadline {
        return Err(O3Error::Deadline);
    }
    Ok(now)
}
