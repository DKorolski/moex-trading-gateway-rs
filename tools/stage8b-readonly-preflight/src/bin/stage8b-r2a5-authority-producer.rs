use stage8b_readonly_preflight::r2a5;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args();
    let _program = arguments.next();
    let source = arguments.next();
    if arguments.next().is_some() {
        return Err("at most one source assertion is allowed".into());
    }
    r2a5::produce_for_effective_uid(source.as_deref())?;
    Ok(())
}
