//! Qualification-only durable-store seeder for the exact R2A7 reader binary.

use finam_gateway::{seed_stage8b_r2a7_controlled_reader, Stage8bR2a7RunMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args();
    let _program = args.next();
    let mode = match (args.next().as_deref(), args.next()) {
        (Some("--seed-controlled-place"), None) => Stage8bR2a7RunMode::ControlledPlace,
        (Some("--seed-controlled-cancel"), None) => Stage8bR2a7RunMode::ControlledCancel,
        _ => {
            return Err("usage: stage8b-r2a7-controlled-seeder --seed-controlled-place|--seed-controlled-cancel".into())
        }
    };
    seed_stage8b_r2a7_controlled_reader(mode)?;
    println!("stage8b-r2a7-controlled-seeder: PASS");
    Ok(())
}
