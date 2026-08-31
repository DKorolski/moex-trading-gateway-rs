//! Fixed-input Stage 8B-P intake verifier/stager; no network or credentials.

use finam_gateway::run_stage8b_r2a8_production_intake_stager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("production intake stager accepts no arguments".into());
    }
    let evidence = run_stage8b_r2a8_production_intake_stager()?;
    println!("{}", serde_json::to_string(&evidence)?);
    Ok(())
}
