use stage8b_readonly_preflight::r2a4;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().len() != 1 {
        return Err("no arguments are accepted".into());
    }
    r2a4::issue_run_package_from_fixed_draft()?;
    Ok(())
}
