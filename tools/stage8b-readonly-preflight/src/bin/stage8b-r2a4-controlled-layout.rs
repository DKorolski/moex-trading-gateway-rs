use stage8b_readonly_preflight::{r2a4, Operation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [command, operation] if command == "seed" => {
            let operation = match operation.as_str() {
                "PLACE" => Operation::Place,
                "CANCEL" => Operation::Cancel,
                _ => return Err("operation must be PLACE or CANCEL".into()),
            };
            r2a4::seed_controlled_fixed_layout(operation)?;
        }
        [command, helper_sha256] if command == "finalize" => {
            r2a4::finalize_controlled_fixed_layout(helper_sha256)?;
        }
        [command] if command == "authority-values" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&r2a4::controlled_authority_values()?)?
            );
        }
        _ => {
            return Err(
                "usage: seed PLACE|CANCEL | finalize HELPER_SHA256 | authority-values".into(),
            )
        }
    }
    Ok(())
}
