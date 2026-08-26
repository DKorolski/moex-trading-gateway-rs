//! Exact one-shot R2A3 helper entry. No scheduler or background loop exists.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().collect::<Vec<_>>();
    match arguments.as_slice() {
        [_program, mode] if mode == "--r2b-one-shot" => {
            let evidence = stage8b_readonly_preflight::r2a5::run_r2b_one_shot().await?;
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
            "usage: stage8b-readonly-preflight (--qualify-controlled|--r2a4-qualify-fixed-layout|--r2a5-qualify-fixed-layout|--r2b-one-shot)",
        )
        .into()),
    }
}
