use finam_gateway::stage8b_p1f_o3_signer::run_fixed_signer;
use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [command, manifest] = args.as_slice() else {
        return ExitCode::from(64);
    };
    if command != "run" {
        return ExitCode::from(64);
    }
    match run_fixed_signer(manifest).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("stage8b-p1f-o3-signer: {error}");
            ExitCode::from(70)
        }
    }
}
