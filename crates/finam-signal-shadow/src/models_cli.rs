//! Explicit model commands. Disabled builds fail closed, never baseline fallback.
use std::ffi::OsString;
pub(super) fn handles(args: &[OsString]) -> bool {
    args.first().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s,
            "validate-ws-primary-models"
                | "observe-ws-primary-models"
                | "verify-ws-primary-models"
                | "verify-usd-ri-source-component"
                | "verify-usd-ri-managed"
        )
    })
}
#[cfg(not(feature = "models-bridge"))]
pub(super) async fn run(_args: &[OsString]) -> Result<u8, &'static str> {
    Err("models_feature_required")
}
#[cfg(feature = "models-bridge")]
pub(super) async fn run(args: &[OsString]) -> Result<u8, &'static str> {
    use super::{credential_permissions, read_bounded};
    use finam_signal_shadow::{
        primary::{
            entry,
            models_entry::{self, ModelPlan},
            WsPrimaryConfig,
        },
        transport::StopHandle,
    };
    use std::{path::Path, sync::Arc};
    let command = args
        .first()
        .and_then(|s| s.to_str())
        .ok_or("models_command")?;
    if command == "verify-usd-ri-managed" {
        if args.len() != 2 {
            return Err("usage: verify-usd-ri-managed OUTPUT_DIRECTORY");
        }
        let report = finam_signal_shadow::primary::additional_models::managed::reader::inspect(
            Path::new(&args[1]),
        )
        .map_err(|_| "additional_managed_verification_refused")?;
        println!("{report}");
        return Ok(if report["status"] == "MANAGED_PREFIX_VERIFIED" {
            0
        } else {
            2
        });
    }
    if command == "verify-usd-ri-source-component" {
        if args.len() != 2 {
            return Err("usage: verify-usd-ri-source-component OUTPUT_DIRECTORY");
        }
        let result =
            finam_signal_shadow::primary::additional_models::reader::inspect(Path::new(&args[1]))
                .map_err(|_| "additional_component_verification_refused")?;
        println!("{result}");
        return Ok(if result["status"] == "COMPONENT_PREFIX_VERIFIED" {
            0
        } else {
            2
        });
    }
    if command == "verify-ws-primary-models" {
        if args.len() != 3 {
            return Err("usage: verify-ws-primary-models CONFIG OUTPUT_DIRECTORY");
        }
        let config = WsPrimaryConfig::parse(&read_bounded(Path::new(&args[1]), &mut 16384)?)
            .map_err(|_| "models_config_invalid")?;
        let result = models_entry::reader::verify_directory(&config, Path::new(&args[2]))
            .map_err(|_| "models_verification_refused")?;
        println!(
            "{}",
            serde_json::to_string(&result).map_err(|_| "models_report")?
        );
        return Ok(if result.status == "PARTIAL" { 2 } else { 0 });
    }
    let validate = command == "validate-ws-primary-models";
    if args.len() != if validate { 12 } else { 14 } {
        return Err("usage: {validate|observe}-ws-primary-models CONFIG REFERENCE MAPPING SEED_CALENDAR SEED MODEL_DAY RAW_CALENDAR ENTRY_PLAN MODELS_PLAN MODELS_HISTORY MODELS_CONTEXT_CALENDAR [READ_ONLY_SECRET_FILE NEW_OUTPUT_DIRECTORY]");
    }
    let config = WsPrimaryConfig::parse(&read_bounded(Path::new(&args[1]), &mut 16384)?)
        .map_err(|_| "models_config_invalid")?;
    let mut budget = config.base.max_input_bytes.min(16 * 1024 * 1024);
    let reference = read_bounded(Path::new(&args[2]), &mut budget)?;
    let mapping = read_bounded(Path::new(&args[3]), &mut budget)?;
    let seed_calendar = read_bounded(Path::new(&args[4]), &mut budget)?;
    let seed = read_bounded(Path::new(&args[5]), &mut budget)?;
    let model_day = read_bounded(Path::new(&args[6]), &mut budget)?;
    let raw_calendar = read_bounded(Path::new(&args[7]), &mut budget)?;
    let plan = entry::Plan::parse(&read_bounded(Path::new(&args[8]), &mut 16384)?)
        .map_err(|_| "models_entry_plan")?;
    let model_plan = ModelPlan::parse(&read_bounded(Path::new(&args[9]), &mut 16384)?)
        .map_err(|_| "models_plan_invalid")?;
    let history = read_bounded(Path::new(&args[10]), &mut budget)?;
    let calendar = read_bounded(Path::new(&args[11]), &mut budget)?;
    let prepared = models_entry::prepare(
        config,
        plan,
        entry::Inputs {
            reference: &reference,
            mapping: &mapping,
            seed_calendar: &seed_calendar,
            seed: &seed,
            model_day: &model_day,
            raw_calendar: &raw_calendar,
            prefix: None,
        },
        model_plan,
        &history,
        &calendar,
    )
    .map_err(|_| "models_input_admission")?;
    let receipt = prepared.receipt().map_err(|_| "models_admission_receipt")?;
    if validate {
        println!("{receipt}");
        return Ok(0);
    }
    if receipt["entry"]["entry_window_open_now"] != true {
        return Err("models_entry_window");
    }
    let secret_path = Path::new(&args[12]);
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
        broker_finam::Stage8bP1fO2GetOnlyClientV1::new(account).map_err(|_| "models_client")?,
    );
    let stop = StopHandle::default();
    #[cfg(unix)]
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|_| "models_signal")?;
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| "models_signal")?;
    let signal = async {
        #[cfg(unix)]
        tokio::select! {_=interrupt.recv()=>(),_=terminate.recv()=>()};
        #[cfg(not(unix))]
        let _ = tokio::signal::ctrl_c().await;
    };
    let work = models_entry::run(prepared, auth, client, Path::new(&args[13]), stop.clone());
    tokio::pin!(work);
    let result = tokio::select! {biased;_=signal=>{stop.stop();work.await},r=&mut work=>r}
        .map_err(|_| "models_entry_or_export")?;
    let success = matches!(
        result["status"].as_str(),
        Some("BOUNDED_COMPLETE" | "MODEL_EOD")
    );
    println!("{result}");
    Ok(if success { 0 } else { 2 })
}
