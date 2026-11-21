use finam_gateway::stage8b_p1f_o3_composition::{
    prepare_fixed_initial_schedule, run_fixed, O3Error,
};
use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    // The ordinary durable owner carries large typed lifecycle futures. Match
    // the Linux main-thread stack instead of Tokio's 2 MiB worker default;
    // keep two workers and a fixed, bounded allocation (no environment knob).
    match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(8 * 1024 * 1024)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(execute()),
        Err(_) => ExitCode::from(70),
    }
}

async fn execute() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = match args.as_slice() {
        [command, manifest] if command == "run" => run_fixed(manifest).await,
        [command, manifest, confirmation] if command == "prepare-initial-schedule" => {
            prepare_fixed_initial_schedule(manifest, confirmation).and_then(|bytes| {
                std::io::stdout()
                    .lock()
                    .write_all(&bytes)
                    .map_err(|_| O3Error::Io)
            })
        }
        _ => return ExitCode::from(64),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("stage8b-p1f-o3-composition: {error}");
            ExitCode::from(match error {
                O3Error::Runtime(inner) => inner.exit_code(),
                _ => 70,
            })
        }
    }
}
