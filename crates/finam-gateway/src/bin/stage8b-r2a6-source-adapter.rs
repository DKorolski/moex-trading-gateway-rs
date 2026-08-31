//! Dedicated Stage 8B-P R2A6 read-only source-adapter executable.
//!
//! This binary has no credential, FINAM transport, operator-arm, dispatch or
//! order endpoint entry point. The controlled mode exists only to qualify the
//! same production composition against source-authenticated durable fixtures.

use finam_gateway::run_stage8b_r2a6_controlled_source_adapter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args();
    let _program = args.next();
    let operation = args
        .next()
        .ok_or("usage: stage8b-r2a6-source-adapter --controlled-rehearsal PLACE|CANCEL")?;
    let value = args
        .next()
        .ok_or("usage: stage8b-r2a6-source-adapter --controlled-rehearsal PLACE|CANCEL")?;
    if operation != "--controlled-rehearsal" || args.next().is_some() {
        return Err(
            "usage: stage8b-r2a6-source-adapter --controlled-rehearsal PLACE|CANCEL".into(),
        );
    }
    let evidence = run_stage8b_r2a6_controlled_source_adapter(&value)?;
    if evidence.execution_authority_granted || evidence.source_count != 10 {
        return Err("R2A6 adapter publication invariant failed".into());
    }
    println!("{}", serde_json::to_string(&evidence)?);
    Ok(())
}
