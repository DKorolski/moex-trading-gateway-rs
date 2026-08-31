//! Exact no-argument publisher for the first production R2B authority link.

use finam_gateway::run_stage8b_r2a8_upstream_current_authority_publisher;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("upstream current-authority publisher accepts no arguments".into());
    }
    let evidence = run_stage8b_r2a8_upstream_current_authority_publisher()?;
    println!("{}", serde_json::to_string(&evidence)?);
    Ok(())
}
