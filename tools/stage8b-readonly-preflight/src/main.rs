//! Exact one-shot R2A3 helper entry. No scheduler or background loop exists.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().collect::<Vec<_>>();
    match arguments.as_slice() {
        [_program, mode] if mode == "--r2b-one-shot" => {
            let evidence = stage8b_readonly_preflight::r2a5::run_r2b_one_shot().await?;
            println!("{}", serde_json::to_string(&evidence)?);
            Ok(())
        }
        [_program, mode] if mode == "--r2b-controlled-custody-one-shot" => {
            let evidence =
                stage8b_readonly_preflight::r2a5::run_r2b_controlled_custody_one_shot().await?;
            println!("{}", serde_json::to_string(&evidence)?);
            Ok(())
        }
        [_program, mode] if mode == "--qualify-controlled" => {
            stage8b_readonly_preflight::r2a3::run_controlled_qualification().await?;
            println!("stage8b-r2a3-controlled-qualification: PASS");
            Ok(())
        }
        [_program, mode] if mode == "--r2a4-qualify-fixed-layout" => {
            let evidence =
                stage8b_readonly_preflight::r2a4::run_controlled_fixed_layout().await?;
            println!("{}", serde_json::to_string(&evidence)?);
            Ok(())
        }
        [_program, mode] if mode == "--r2a5-qualify-fixed-layout" => {
            let evidence =
                stage8b_readonly_preflight::r2a5::run_controlled_fixed_layout().await?;
            println!("{}", serde_json::to_string(&evidence)?);
            Ok(())
        }
        _ => Err(std::io::Error::other(
            "usage: stage8b-readonly-preflight (--qualify-controlled|--r2a4-qualify-fixed-layout|--r2a5-qualify-fixed-layout|--r2b-one-shot|--r2b-controlled-custody-one-shot)",
        )
        .into()),
    }
}
