//! Replay is file-only. Explicit `observe` binds read-only REST/BARS and
//! QUOTES only for the original quote-feedback policy. Candle-close simulation
//! is explicitly selected by its separate config feedback-policy digest.
//! `restart` is file-only admitted prefix reconstruction, not an observe switch.
//! `observe-prefix` additionally requires a separately approved PrefixRunPlan.
use broker_core::HybridRuntimeBarEvent;
use finam_signal_shadow::{
    feedback::Quote, replay::OfflineReplay, segment::SeededSegment, ValidatedConfig,
};
use serde::Deserialize;
use std::{fs::File, io::Read, path::Path, process::ExitCode};
mod credential_permissions;
mod models_cli;
mod primary_cli;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Event {
    Bar {
        bar: HybridRuntimeBarEvent,
        raw_provenance_sha256: String,
        virtual_result_at_ms: i64,
    },
    Quote {
        quote: Quote,
        virtual_applied_at_ms: i64,
    },
}

fn read_bounded(path: &Path, budget: &mut u64) -> Result<Vec<u8>, &'static str> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "input_io")?;
    if !meta.is_file() || meta.len() > *budget {
        return Err("input_bound_or_type");
    }
    let file = File::open(path).map_err(|_| "input_io")?;
    let mut bytes = Vec::new();
    file.take(*budget + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "input_io")?;
    if bytes.len() as u64 > *budget {
        return Err("input_bound_or_type");
    }
    *budget -= bytes.len() as u64;
    Ok(bytes)
}

fn run(args: &[std::ffi::OsString]) -> Result<bool, &'static str> {
    if args.len() != 8 || args[0] != "replay" {
        return Err("usage: finam-signal-shadow replay CONFIG REFERENCE MAPPING CALENDAR SEED EVENTS NEW_OUTPUT");
    }
    // Caller supplies trusted local files/directories; not a filesystem sandbox.
    let config = read_bounded(Path::new(&args[1]), &mut 16_384)?;
    let config = ValidatedConfig::parse(&config).map_err(|_| "config_invalid")?;
    let mut budget = config.config().max_input_bytes.min(16 * 1024 * 1024);
    let reference = read_bounded(Path::new(&args[2]), &mut budget)?;
    let mapping = read_bounded(Path::new(&args[3]), &mut budget)?;
    let calendar = read_bounded(Path::new(&args[4]), &mut budget)?;
    let seed = read_bounded(Path::new(&args[5]), &mut budget)?;
    let input = read_bounded(Path::new(&args[6]), &mut budget)?;
    let events: Vec<Event> = serde_json::from_slice(&input).map_err(|_| "events_invalid")?;
    if events.is_empty() || events.len() > 4096 {
        return Err("event_count");
    }
    let segment = SeededSegment::from_inputs(config, &reference, &mapping, &calendar, &seed)
        .map_err(|_| "seed_invalid")?;
    let mut replay =
        OfflineReplay::create(segment, Path::new(&args[7])).map_err(|_| "output_create_failed")?;
    for event in events {
        match event {
            Event::Bar {
                bar,
                raw_provenance_sha256,
                virtual_result_at_ms,
            } => {
                replay
                    .step(bar, &raw_provenance_sha256, virtual_result_at_ms)
                    .map_err(|_| "replay_bar_failed")?;
            }
            Event::Quote {
                quote,
                virtual_applied_at_ms,
            } => {
                replay
                    .quote(quote, virtual_applied_at_ms)
                    .map_err(|_| "replay_quote_failed")?;
            }
        }
    }
    let flat = replay.complete_flat();
    replay.finish().map_err(|_| "output_finish_failed")?;
    // Flat completion is not coverage, parity, live readiness or EOD acceptance.
    Ok(flat)
}

async fn observe(args: &[std::ffi::OsString]) -> Result<(), &'static str> {
    let with_prefix = args.first().is_some_and(|arg| arg == "observe-prefix");
    let offset = if with_prefix { 2 } else { 0 };
    if args.len() != 9 + offset {
        return Err("usage: finam-signal-shadow observe[-prefix] CONFIG REFERENCE MAPPING CALENDAR SEED [PREFIX_BINDING PREFIX] PLAN READ_ONLY_SECRET_FILE NEW_OUTPUT");
    }
    let config = read_bounded(Path::new(&args[1]), &mut 16_384)?;
    let config = ValidatedConfig::parse(&config).map_err(|_| "config_invalid")?;
    let mut budget = config.config().max_input_bytes.min(16 * 1024 * 1024);
    let reference = read_bounded(Path::new(&args[2]), &mut budget)?;
    let mapping = read_bounded(Path::new(&args[3]), &mut budget)?;
    let calendar = read_bounded(Path::new(&args[4]), &mut budget)?;
    let seed = read_bounded(Path::new(&args[5]), &mut budget)?;
    let prefix_input = if with_prefix {
        let binding = read_bounded(Path::new(&args[6]), &mut 16_384)?;
        let binding = serde_json::from_slice(&binding).map_err(|_| "prefix_binding_invalid")?;
        let bytes = read_bounded(Path::new(&args[7]), &mut budget)?;
        Some((binding, bytes))
    } else {
        None
    };
    let plan = read_bounded(Path::new(&args[6 + offset]), &mut budget)?;
    let prefix_plan: Option<finam_signal_shadow::runner::PrefixRunPlan> = if with_prefix {
        Some(serde_json::from_slice(&plan).map_err(|_| "prefix_plan_invalid")?)
    } else {
        None
    };
    let fresh_plan: Option<finam_signal_shadow::runner::RunPlan> = if with_prefix {
        None
    } else {
        Some(serde_json::from_slice(&plan).map_err(|_| "plan_invalid")?)
    };
    let plan = if let Some(prefix) = &prefix_plan {
        &prefix.plan
    } else {
        fresh_plan.as_ref().ok_or("plan_invalid")?
    };
    if !with_prefix {
        plan.validate(&config)?;
    }
    // Explicit protected secret, not an access token or environment discovery.
    credential_permissions::validate(Path::new(&args[7 + offset]))?;
    let token = read_bounded(Path::new(&args[7 + offset]), &mut 16_384)?;
    let token = std::str::from_utf8(&token)
        .map_err(|_| "credential_invalid")?
        .trim();
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return Err("credential_invalid");
    }
    let auth = finam_signal_shadow::auth::AuthLifecycle::new(
        broker_finam::SecretToken::new(token),
        plan.account_id.clone(),
    )?;
    let segment = SeededSegment::from_inputs(
        ValidatedConfig::validate(config.config().clone()).map_err(|_| "config_invalid")?,
        &reference,
        &mapping,
        &calendar,
        &seed,
    )
    .map_err(|_| "seed_invalid")?;
    if let Some((binding, bytes)) = prefix_input {
        let prefix = finam_signal_shadow::prefix::reconstruct(
            segment,
            binding,
            &bytes,
            Path::new(&args[10]),
        )
        .map_err(|_| "prefix_reconstruction_failed")?;
        finam_signal_shadow::runner::run_prefix(
            prefix,
            &config,
            prefix_plan.ok_or("prefix_plan_invalid")?,
            auth,
        )
        .await
    } else {
        finam_signal_shadow::runner::run(
            segment,
            &config,
            fresh_plan.ok_or("plan_invalid")?,
            auth,
            Path::new(&args[8]),
        )
        .await
    }
}

async fn diagnostic(args: &[std::ffi::OsString]) -> Result<u8, &'static str> {
    if args.len() != 4 {
        return Err(
            "usage: finam-signal-shadow auth-ws-diagnostic PLAN READ_ONLY_SECRET_FILE NEW_OUTPUT",
        );
    }
    let bytes = read_bounded(Path::new(&args[1]), &mut 16384)?;
    let plan: finam_signal_shadow::diagnostic::Plan =
        serde_json::from_slice(&bytes).map_err(|_| "diagnostic_plan")?;
    plan.validate(chrono::Utc::now())?;
    credential_permissions::validate_diagnostic(Path::new(&args[2]))?;
    let secret = read_bounded(Path::new(&args[2]), &mut 16384)?;
    let secret = std::str::from_utf8(&secret)
        .map_err(|_| "credential_invalid")?
        .trim();
    if secret.is_empty() || secret.chars().any(char::is_whitespace) {
        return Err("credential_invalid");
    }
    let auth = finam_signal_shadow::auth::AuthLifecycle::new(
        broker_finam::SecretToken::new(secret),
        plan.account_id.clone(),
    )?;
    finam_signal_shadow::diagnostic::run(plan, auth, Path::new(&args[3]), tokio::signal::ctrl_c())
        .await
}

async fn observe_candle_intraday(args: &[std::ffi::OsString]) -> Result<(), &'static str> {
    if args.len() != 10 {
        return Err("usage: finam-signal-shadow observe-candle-intraday CONFIG REFERENCE MAPPING CALENDAR SEED HISTORY INTRADAY_PLAN READ_ONLY_SECRET_FILE NEW_OUTPUT");
    }
    let bytes = read_bounded(Path::new(&args[1]), &mut 16_384)?;
    let config = ValidatedConfig::parse(&bytes).map_err(|_| "config_invalid")?;
    let mut budget = config.config().max_input_bytes.min(16 * 1024 * 1024);
    let reference = read_bounded(Path::new(&args[2]), &mut budget)?;
    let mapping = read_bounded(Path::new(&args[3]), &mut budget)?;
    let calendar = read_bounded(Path::new(&args[4]), &mut budget)?;
    let seed = read_bounded(Path::new(&args[5]), &mut budget)?;
    let history = read_bounded(Path::new(&args[6]), &mut budget)?;
    let bytes = read_bounded(Path::new(&args[7]), &mut budget)?;
    let plan: finam_signal_shadow::intraday::IntradayRunPlan =
        serde_json::from_slice(&bytes).map_err(|_| "intraday_plan_invalid")?;
    plan.validate_now(&config, chrono::Utc::now().timestamp_millis())
        .map_err(|_| "intraday_window_or_binding")?;
    credential_permissions::validate(Path::new(&args[8]))?;
    let secret = read_bounded(Path::new(&args[8]), &mut 16_384)?;
    let secret = std::str::from_utf8(&secret)
        .map_err(|_| "credential_invalid")?
        .trim();
    if secret.is_empty() || secret.chars().any(char::is_whitespace) {
        return Err("credential_invalid");
    }
    let auth = finam_signal_shadow::auth::AuthLifecycle::new(
        broker_finam::SecretToken::new(secret),
        plan.plan.account_id.clone(),
    )?;
    let segment = SeededSegment::from_inputs(
        ValidatedConfig::validate(config.config().clone()).map_err(|_| "config_invalid")?,
        &reference,
        &mapping,
        &calendar,
        &seed,
    )
    .map_err(|_| "seed_invalid")?;
    let day =
        finam_signal_shadow::intraday::reconstruct(segment, &plan, &history, Path::new(&args[9]))
            .map_err(|_| "intraday_reconstruction_failed")?;
    finam_signal_shadow::runner::run_intraday(day, &config, plan, auth).await
}

fn restart(args: &[std::ffi::OsString]) -> Result<bool, &'static str> {
    if args.len() != 9 {
        return Err("usage: finam-signal-shadow restart CONFIG REFERENCE MAPPING CALENDAR SEED PREFIX_BINDING PREFIX NEW_OUTPUT");
    }
    let config = read_bounded(Path::new(&args[1]), &mut 16_384)?;
    let config = ValidatedConfig::parse(&config).map_err(|_| "config_invalid")?;
    let mut budget = config.config().max_input_bytes.min(16 * 1024 * 1024);
    let reference = read_bounded(Path::new(&args[2]), &mut budget)?;
    let mapping = read_bounded(Path::new(&args[3]), &mut budget)?;
    let calendar = read_bounded(Path::new(&args[4]), &mut budget)?;
    let seed = read_bounded(Path::new(&args[5]), &mut budget)?;
    let binding = read_bounded(Path::new(&args[6]), &mut 16_384)?;
    let binding = serde_json::from_slice(&binding).map_err(|_| "prefix_binding_invalid")?;
    let bytes = read_bounded(Path::new(&args[7]), &mut budget)?;
    let segment = SeededSegment::from_inputs(config, &reference, &mapping, &calendar, &seed)
        .map_err(|_| "seed_invalid")?;
    finam_signal_shadow::prefix::reconstruct(segment, binding, &bytes, Path::new(&args[8]))
        .map_err(|_| "prefix_reconstruction_failed")?
        .finish()
        .map_err(|_| "prefix_finish_failed")
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if models_cli::handles(&args) {
        return match models_cli::run(&args).await {
            Ok(code) => ExitCode::from(code),
            Err(reason) => {
                eprintln!("{reason}");
                ExitCode::from(1)
            }
        };
    }
    if primary_cli::handles(&args) {
        return match primary_cli::run(&args).await {
            Ok(code) => ExitCode::from(code),
            Err(code) => {
                eprintln!("{code}");
                ExitCode::from(1)
            }
        };
    }
    if args
        .first()
        .is_some_and(|arg| arg == "observe-candle-intraday")
    {
        return match observe_candle_intraday(&args).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(reason) => {
                eprintln!("{reason}");
                ExitCode::from(1)
            }
        };
    }
    if args.first().is_some_and(|arg| arg == "restart") {
        return match restart(&args) {
            Ok(flat) => {
                println!("PREFIX_RECONSTRUCTED_NOT_LIVE_READINESS");
                if flat {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(2)
                }
            }
            Err(code) => {
                eprintln!("{code}");
                ExitCode::from(1)
            }
        };
    }
    if args.first().is_some_and(|arg| arg == "auth-ws-diagnostic") {
        return match diagnostic(&args).await {
            Ok(code) => ExitCode::from(code),
            Err(reason) => {
                eprintln!("{reason}");
                ExitCode::from(1)
            }
        };
    }
    if args
        .first()
        .is_some_and(|arg| arg == "observe" || arg == "observe-prefix")
    {
        return match observe(&args).await {
            Ok(()) => {
                println!("OBSERVATION_ENDED_NOT_PARITY_ACCEPTANCE");
                ExitCode::SUCCESS
            }
            Err(code) => {
                eprintln!("{code}");
                ExitCode::from(1)
            }
        };
    }
    match run(&args) {
        Ok(true) => {
            println!("OFFLINE_REPLAY_END_FLAT");
            ExitCode::SUCCESS
        }
        Ok(false) => {
            eprintln!("OFFLINE_REPLAY_INCOMPLETE");
            ExitCode::from(2)
        }
        Err(code) => {
            eprintln!("{code}");
            ExitCode::from(1)
        }
    }
}
