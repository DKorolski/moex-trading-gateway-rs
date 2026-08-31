//! Fixed-input, no-network Stage 8B-P production current-source writer.

use finam_gateway::run_stage8b_r2a8_production_current_source_writer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("production current-source writer accepts no arguments".into());
    }
    let evidence = run_stage8b_r2a8_production_current_source_writer()?;
    println!("{}", serde_json::to_string(&evidence)?);
    Ok(())
}
