//! Explicit two-instrument source executable. No baseline fallback or resume.
use finam_signal_shadow::{primary::additional_models::executable, transport::StopHandle};
use std::{ffi::OsString, path::Path};

pub(super) async fn run(args: &[OsString]) -> Result<u8, &'static str> {
    let command = args
        .first()
        .and_then(|s| s.to_str())
        .ok_or("usd_ri_command")?;
    match command {
        "prepare-usd-ri-managed" => {
            if args.len() != 3 {
                return Err("usage: prepare-usd-ri-managed DRAFT NEW_FROZEN_INPUT");
            }
            let report = executable::freeze_file(Path::new(&args[1]), Path::new(&args[2]))
                .map_err(|_| "usd_ri_input_preparation_refused")?;
            println!("{report}");
            Ok(0)
        }
        "verify-usd-ri-executable" => {
            if args.len() != 2 {
                return Err("usage: verify-usd-ri-executable OUTPUT_DIRECTORY");
            }
            let report = executable::inspect(Path::new(&args[1]))
                .map_err(|_| "usd_ri_executable_verification_refused")?;
            let success = report["status"] == "MANAGED_PREFIX_VERIFIED";
            println!("{report}");
            Ok(if success { 0 } else { 2 })
        }
        "validate-usd-ri-managed" | "observe-usd-ri-managed" => {
            let validate = command == "validate-usd-ri-managed";
            if args.len() != if validate { 3 } else { 5 } {
                return Err("usage: {validate|observe}-usd-ri-managed FROZEN_INPUT EXPECTED_SHA256 [READ_ONLY_SECRET_FILE NEW_OUTPUT_DIRECTORY]");
            }
            let pin = args[2].to_str().ok_or("usd_ri_expected_hash")?;
            let admitted = executable::admit_file(Path::new(&args[1]), pin)
                .map_err(|_| "usd_ri_input_admission_refused")?;
            let receipt = admitted.receipt().map_err(|_| "usd_ri_admission_receipt")?;
            if validate {
                println!("{receipt}");
                return Ok(0);
            }
            if receipt["entry_window_open_now"] != true {
                return Err("usd_ri_entry_window");
            }
            let path = Path::new(&args[3]);
            super::super::credential_permissions::validate_usd_ri(path)?;
            let secret = super::super::read_bounded(path, &mut 16384)?;
            let secret = std::str::from_utf8(&secret)
                .map_err(|_| "credential_invalid")?
                .trim();
            if secret.is_empty() || secret.chars().any(char::is_whitespace) {
                return Err("credential_invalid");
            }
            let auth = finam_signal_shadow::auth::AuthLifecycle::new(
                broker_finam::SecretToken::new(secret),
                admitted.account_id().to_owned(),
            )?;
            let stop = StopHandle::default();
            #[cfg(unix)]
            let mut interrupt =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                    .map_err(|_| "usd_ri_signal")?;
            #[cfg(unix)]
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .map_err(|_| "usd_ri_signal")?;
            let signal = async {
                #[cfg(unix)]
                tokio::select! { _=interrupt.recv()=>(), _=terminate.recv()=>() };
                #[cfg(not(unix))]
                let _ = tokio::signal::ctrl_c().await;
            };
            let work = executable::run(admitted, auth, Path::new(&args[4]), stop.clone());
            tokio::pin!(work);
            let report =
                tokio::select! { biased; _=signal=>{stop.stop();work.await}, r=&mut work=>r }
                    .map_err(|_| "usd_ri_entry_or_export_refused")?;
            let code = match report["status"].as_str() {
                Some("MANAGED_PREFIX_VERIFIED") => 0,
                Some("HELD") => 2,
                _ => 1,
            };
            println!("{report}");
            Ok(code)
        }
        _ => Err("usd_ri_command"),
    }
}
