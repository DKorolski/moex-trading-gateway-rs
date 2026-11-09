//! Separate v2 commands; no legacy mode guessing, environment secret discovery,
//! alternate endpoint or order path. Validation/verification never open auth.
use super::{credential_permissions, read_bounded};
use finam_signal_shadow::{
    primary::{
        entry::{self, Plan, PreparedEntry, Startup},
        verify, WsPrimaryConfig,
    },
    transport::StopHandle,
};
use std::{ffi::OsString, path::Path, sync::Arc};

const OBSERVE:&str="usage: finam-signal-shadow observe-ws-primary[-restart] CONFIG REFERENCE MAPPING SEED_CALENDAR SEED MODEL_DAY RAW_CALENDAR PLAN [PREFIX] READ_ONLY_SECRET_FILE NEW_OUTPUT_DIRECTORY";
const VALIDATE:&str="usage: finam-signal-shadow validate-ws-primary[-restart] CONFIG REFERENCE MAPPING SEED_CALENDAR SEED MODEL_DAY RAW_CALENDAR PLAN [PREFIX]";
pub(super) fn handles(args: &[OsString]) -> bool {
    args.first().is_some_and(|s| {
        matches!(
            s.to_str(),
            Some(
                "observe-ws-primary"
                    | "observe-ws-primary-restart"
                    | "validate-ws-primary"
                    | "validate-ws-primary-restart"
                    | "verify-ws-primary"
            )
        )
    })
}
fn load(args: &[OsString], restart: bool) -> Result<PreparedEntry, &'static str> {
    let config = read_bounded(Path::new(&args[1]), &mut 16384)?;
    let config = WsPrimaryConfig::parse(&config).map_err(|_| "primary_config_invalid")?;
    let mut budget = config.base.max_input_bytes.min(16 * 1024 * 1024);
    let reference = read_bounded(Path::new(&args[2]), &mut budget)?;
    let mapping = read_bounded(Path::new(&args[3]), &mut budget)?;
    let seed_calendar = read_bounded(Path::new(&args[4]), &mut budget)?;
    let seed = read_bounded(Path::new(&args[5]), &mut budget)?;
    let model_day = read_bounded(Path::new(&args[6]), &mut budget)?;
    let raw_calendar = read_bounded(Path::new(&args[7]), &mut budget)?;
    let plan = read_bounded(Path::new(&args[8]), &mut 16384)?;
    let plan = Plan::parse(&plan).map_err(|_| "primary_plan_invalid")?;
    if restart != matches!(plan.startup, Startup::CommittedPrefix { .. }) {
        return Err("primary_startup_mode");
    }
    let prefix = if restart {
        Some(read_bounded(Path::new(&args[9]), &mut budget)?)
    } else {
        None
    };
    entry::prepare(
        config,
        plan,
        entry::Inputs {
            reference: &reference,
            mapping: &mapping,
            seed_calendar: &seed_calendar,
            seed: &seed,
            model_day: &model_day,
            raw_calendar: &raw_calendar,
            prefix: prefix.as_deref(),
        },
    )
    .map_err(|_| "primary_input_admission")
}
pub(super) async fn run(args: &[OsString]) -> Result<u8, &'static str> {
    let command = args.first().and_then(|s| s.to_str()).ok_or(VALIDATE)?;
    if command == "verify-ws-primary" {
        return verify_files(args);
    }
    let restart = command.ends_with("-restart");
    let extra = usize::from(restart);
    if command.starts_with("validate-") {
        if args.len() != 9 + extra {
            return Err(VALIDATE);
        }
        let prepared = load(args, restart)?;
        println!(
            "{}",
            serde_json::to_string(&prepared.receipt().map_err(|_| "primary_receipt")?)
                .map_err(|_| "primary_receipt")?
        );
        return Ok(0);
    }
    if args.len() != 11 + extra {
        return Err(OBSERVE);
    }
    let prepared = load(args, restart)?;
    if !prepared
        .receipt()
        .map_err(|_| "primary_receipt")?
        .entry_window_open_now
    {
        return Err("primary_entry_window");
    }
    let secret_path = Path::new(&args[9 + extra]);
    credential_permissions::validate(secret_path)?;
    let secret = read_bounded(secret_path, &mut 16384)?;
    let secret = std::str::from_utf8(&secret)
        .map_err(|_| "credential_invalid")?
        .trim();
    if secret.is_empty() || secret.chars().any(char::is_whitespace) {
        return Err("credential_invalid");
    }
    let account = prepared.account_id().to_owned();
    let auth = finam_signal_shadow::auth::AuthLifecycle::new(
        broker_finam::SecretToken::new(secret),
        account.clone(),
    )?;
    let client = Arc::new(
        broker_finam::Stage8bP1fO2GetOnlyClientV1::new(account).map_err(|_| "primary_client")?,
    );
    let stop = StopHandle::default();
    // Install Unix handlers before entering the runner. Do not drop its future
    // on SIGINT/TERM: root STOP must finish bounded IO/writer drain and export.
    #[cfg(unix)]
    let mut interrupts = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|_| "primary_signal")?;
    #[cfg(unix)]
    let mut terminates = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| "primary_signal")?;
    let interrupted = async {
        #[cfg(unix)]
        tokio::select! {_=interrupts.recv()=>(),_=terminates.recv()=>()};
        #[cfg(not(unix))]
        let _ = tokio::signal::ctrl_c().await;
    };
    let work = entry::run(
        prepared,
        auth,
        client,
        Path::new(&args[10 + extra]),
        stop.clone(),
    );
    tokio::pin!(work);
    let result = tokio::select! {biased;
        _=interrupted=>{stop.stop();work.await},
        result=&mut work=>result,
    }
    .map_err(|_| "primary_entry_or_export")?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|_| "primary_report")?
    );
    Ok(if result.semantic_eod_passed { 0 } else { 2 })
}
fn verify_files(args: &[OsString]) -> Result<u8, &'static str> {
    if args.len() != 5 {
        return Err("usage: finam-signal-shadow verify-ws-primary CONFIG EVIDENCE_BINDING JOURNAL COMPLETION");
    }
    let bytes = read_bounded(Path::new(&args[1]), &mut 16384)?;
    let config = WsPrimaryConfig::parse(&bytes).map_err(|_| "primary_config_invalid")?;
    let bytes = read_bounded(Path::new(&args[2]), &mut 16384)?;
    let binding: verify::EvidenceBinding =
        serde_json::from_slice(&bytes).map_err(|_| "primary_evidence_binding")?;
    let journal = read_bounded(
        Path::new(&args[3]),
        &mut config.base.max_output_bytes.min(64 * 1024 * 1024),
    )?;
    let completion = read_bounded(Path::new(&args[4]), &mut 16384)?;
    let verified = verify::verify(
        &config,
        &binding,
        &journal,
        &completion,
        chrono::Utc::now().timestamp_millis(),
    )
    .map_err(|_| "primary_verification_refused")?;
    println!(
        "{}",
        serde_json::to_string(&verified).map_err(|_| "primary_report")?
    );
    Ok(0)
}
