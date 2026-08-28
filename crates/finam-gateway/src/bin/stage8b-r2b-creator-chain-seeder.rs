//! Qualification-only predecessor seeder for the exact creator -> stager chain.

use finam_gateway::seed_stage8b_r2b_creator_chain_qualification;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("creator-chain qualification seeder accepts no arguments".into());
    }
    seed_stage8b_r2b_creator_chain_qualification()?;
    println!("stage8b-r2b-creator-chain-seeder: PASS");
    Ok(())
}
