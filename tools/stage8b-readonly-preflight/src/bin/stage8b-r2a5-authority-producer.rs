use stage8b_readonly_preflight::{r2a5, Operation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args();
    let _program = arguments.next();
    let first = arguments.next();
    let second = arguments.next();
    if arguments.next().is_some() {
        return Err("invalid producer arguments".into());
    }
    match (first.as_deref(), second.as_deref()) {
        (Some("--controlled-r2a8-place"), source) => {
            r2a5::produce_controlled_r2a8_for_effective_uid(Operation::Place, source)?;
        }
        (Some("--controlled-r2a8-cancel"), source) => {
            r2a5::produce_controlled_r2a8_for_effective_uid(Operation::Cancel, source)?;
        }
        (source, None) => r2a5::produce_for_effective_uid(source)?,
        _ => return Err("invalid producer arguments".into()),
    }
    Ok(())
}
