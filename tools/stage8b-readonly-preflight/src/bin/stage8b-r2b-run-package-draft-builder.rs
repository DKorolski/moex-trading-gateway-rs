use stage8b_readonly_preflight::r2a5;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    r2a5::build_run_package_draft_from_fixed_inputs()?;
    println!("stage8b-r2b-run-package-draft-builder: PASS");
    Ok(())
}
