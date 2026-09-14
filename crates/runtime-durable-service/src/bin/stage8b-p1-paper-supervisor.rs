use std::process::ExitCode;

use runtime_durable_service::{
    execute_stage8b_p1e_process_command_v1, parse_stage8b_p1e_process_command_v1,
    Stage8bP1eProcessSuccessV1,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let command = match parse_stage8b_p1e_process_command_v1(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("stage8b-p1-paper-supervisor: {error}");
            return ExitCode::from(error.exit_code());
        }
    };
    match execute_stage8b_p1e_process_command_v1(command).await {
        Ok(success) => {
            let status = match success {
                Stage8bP1eProcessSuccessV1::ConfigValid => "config-valid",
                Stage8bP1eProcessSuccessV1::BootstrapAdopted => "bootstrap-adopted",
                Stage8bP1eProcessSuccessV1::RecoveryApplied => "recovery-applied",
            };
            println!("stage8b-p1-paper-supervisor: {status}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("stage8b-p1-paper-supervisor: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}
