use std::process::ExitCode;

use runtime_durable_service::{
    run_stage8b_p1f_local_supervisor_v1, Stage8bP1fLocalSupervisionErrorV1,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let mut args = std::env::args();
    let _binary = args.next();
    let manifest_sha256 = match (args.next().as_deref(), args.next(), args.next()) {
        (Some("run"), Some(manifest), None) => manifest,
        _ => {
            let error = Stage8bP1fLocalSupervisionErrorV1::Usage;
            eprintln!("stage8b-p1f-local-supervisor: {error}");
            return ExitCode::from(error.exit_code());
        }
    };
    match run_stage8b_p1f_local_supervisor_v1(&manifest_sha256).await {
        Ok(result) => match serde_json::to_string(&result) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(_) => ExitCode::from(70),
        },
        Err(error) => {
            eprintln!("stage8b-p1f-local-supervisor: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}
