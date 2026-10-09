//! Explicit fresh-only administrative step; no runtime start or DB reset.
use finam_gateway::stage8b_p1f_o3_composition::provision_fixed_o3_redis;
use std::process::ExitCode;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [command, manifest, confirmation] = args.as_slice() else {
        return ExitCode::from(64);
    };
    if command != "initialize-fixed" || confirmation != "INITIALIZE-FRESH-O3-DB15" {
        return ExitCode::from(64);
    }
    match provision_fixed_o3_redis(manifest).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("stage8b-p1f-o3-provisioner: {error}");
            ExitCode::from(70)
        }
    }
}
