//! Exact no-argument owner-mediated Stage 8B intake creator.

use finam_gateway::run_stage8b_r2a8_authoritative_intake_creator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("authoritative intake creator accepts no arguments".into());
    }
    let evidence = run_stage8b_r2a8_authoritative_intake_creator()?;
    println!("{}", serde_json::to_string(&evidence)?);
    Ok(())
}
