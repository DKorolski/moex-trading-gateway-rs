use stage8b_readonly_preflight::{r2a5, Operation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [command, operation] if command == "seed" => {
            let operation = match operation.as_str() {
                "PLACE" => Operation::Place,
                "CANCEL" => Operation::Cancel,
                _ => return Err("operation must be PLACE or CANCEL".into()),
            };
            r2a5::seed_controlled_fixed_layout(operation)?;
        }
        [command, operation] if command == "seed-r2a6" => {
            let operation = match operation.as_str() {
                "PLACE" => Operation::Place,
                "CANCEL" => Operation::Cancel,
                _ => return Err("operation must be PLACE or CANCEL".into()),
            };
            r2a5::seed_controlled_r2a6_layout(operation)?;
        }
        [command] if command == "bind-r2a6" => {
            r2a5::bind_controlled_r2a6_manifest_to_operational_sources()?;
        }
        [command, helper_sha256] if command == "finalize" => {
            r2a5::finalize_controlled_fixed_layout(helper_sha256)?;
        }
        [command] if command == "authority-values" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&r2a5::controlled_authority_values()?)?
            );
        }
        _ => {
            return Err(
                "usage: seed|seed-r2a6 PLACE|CANCEL | bind-r2a6 | finalize HELPER_SHA256 | authority-values".into(),
            )
        }
    }
    Ok(())
}
