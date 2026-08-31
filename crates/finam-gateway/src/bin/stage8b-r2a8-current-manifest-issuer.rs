//! Fixed-interface Stage 8B-P R2A8 trusted current-manifest issuer.

use finam_gateway::{issue_stage8b_r2a8_reader_manifest, Stage8bR2a7RunMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args();
    let _program = args.next();
    let mode = match (args.next().as_deref(), args.next()) {
        (Some("--one-shot-production"), None) => Stage8bR2a7RunMode::Production,
        (Some("--one-shot-controlled-place"), None) => Stage8bR2a7RunMode::ControlledPlace,
        (Some("--one-shot-controlled-cancel"), None) => Stage8bR2a7RunMode::ControlledCancel,
        _ => {
            return Err("usage: stage8b-r2a8-current-manifest-issuer --one-shot-production|--one-shot-controlled-place|--one-shot-controlled-cancel".into())
        }
    };
    issue_stage8b_r2a8_reader_manifest(mode)?;
    println!("stage8b-r2a8-current-manifest-issuer: PASS");
    Ok(())
}
