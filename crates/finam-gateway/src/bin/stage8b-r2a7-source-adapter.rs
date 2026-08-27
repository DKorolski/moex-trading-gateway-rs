//! Fixed-interface Stage 8B-P R2A7 source adapter.

use finam_gateway::{run_stage8b_r2a7_source_adapter, Stage8bR2a7RunMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args();
    let _program = args.next();
    let mode = match (args.next().as_deref(), args.next(), args.next()) {
        (Some("--one-shot-production"), None, None) => Stage8bR2a7RunMode::Production,
        (Some("--one-shot-controlled-place"), None, None) => {
            Stage8bR2a7RunMode::ControlledPlace
        }
        (Some("--one-shot-controlled-cancel"), None, None) => {
            Stage8bR2a7RunMode::ControlledCancel
        }
        _ => {
            return Err("usage: stage8b-r2a7-source-adapter --one-shot-production|--one-shot-controlled-place|--one-shot-controlled-cancel".into())
        }
    };
    let evidence = run_stage8b_r2a7_source_adapter(mode)?;
    if evidence.execution_authority_granted
        || evidence.network_accessed
        || evidence.finam_credential_accessed
        || evidence.source_count != 10
    {
        return Err("R2A7 source publication invariant failed".into());
    }
    println!("{}", serde_json::to_string(&evidence)?);
    Ok(())
}
