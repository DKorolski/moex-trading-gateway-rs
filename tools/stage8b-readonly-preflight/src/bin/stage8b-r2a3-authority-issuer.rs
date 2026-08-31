//! Source-specific R2A3 receipt issuer entry.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args();
    let _program = arguments.next();
    let source = arguments.next().ok_or("missing fixed source name")?;
    if arguments.next().is_some() {
        return Err("unexpected argument".into());
    }
    stage8b_readonly_preflight::r2a3::issue_from_fixed_source(&source)?;
    Ok(())
}
