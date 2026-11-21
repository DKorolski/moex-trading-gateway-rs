//! Separate administrative executable only; run_fixed never calls this path.
use super::*;

pub async fn provision_fixed_o3_redis(manifest_sha256: &str) -> Result<(), O3Error> {
    if !cfg!(target_os = "linux") || unsafe { libc::geteuid() } != 0 {
        return Err(O3Error::Input);
    }
    let inputs = AdmittedInputs::load_fixed(manifest_sha256)?;
    let name =
        std::ffi::CString::new(durable::STAGE8B_P1F_SERVICE_USER).map_err(|_| O3Error::Input)?;
    let account = unsafe { libc::getpwnam(name.as_ptr()) };
    if account.is_null() || unsafe { (*account).pw_uid == 0 || (*account).pw_gid == 0 } {
        return Err(O3Error::Input);
    }
    let (uid, gid) = unsafe { ((*account).pw_uid, (*account).pw_gid) };
    let store =
        durable::Stage8bP1fAuthorityStoreV1::open_production(gid).map_err(|_| O3Error::Input)?;
    authority::require_active(&store, manifest_sha256, inputs.deadline).await?;
    let bytes = inputs::read_root_file(
        Path::new("/etc/moex-finam-p1-paper/o3/initial-schedule.json"),
        128 * 1024,
    )?;
    // The first Redis row must be replayable after a crash. Reject a missing,
    // foreign, advanced or differently signed checkpoint BEFORE connecting.
    producers::initial_schedule_checkpoint(Path::new(SCHEDULE_STATE), uid, &inputs.plan, &bytes)?;
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(|_| O3Error::Io)?;
    let boot = uuid::Uuid::parse_str(boot.trim()).map_err(|_| O3Error::Input)?;
    let config = durable::parse_stage8b_p1e_supervisor_config_v1(&inputs.supervisor_bytes)
        .map_err(|_| O3Error::Input)?;
    let config = durable::validate_stage8b_p1e_supervisor_config_v1(config, *boot.as_bytes())
        .map_err(|_| O3Error::Input)?;
    tokio::time::timeout(
        StdDuration::from_secs(10).min(
            (inputs.deadline - Utc::now())
                .to_std()
                .map_err(|_| O3Error::Deadline)?,
        ),
        durable::provision_stage8b_p1f_o3_redis_v1(
            &config,
            &bytes,
            &inputs.plan.schedule_semantic_sha256,
        ),
    )
    .await
    .map_err(|_| O3Error::Deadline)?
    .map_err(|_| O3Error::Producer)
}
