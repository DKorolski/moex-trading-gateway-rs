use stage8b_readonly_preflight::r2a5;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("usage: all inputs are accepted only through the local environment".into());
    }
    let ceremony = std::env::var_os("STAGE8B_R2B_TRUST_REBIND_CEREMONY_DIR")
        .ok_or("missing local ceremony environment")?;
    let helper = std::env::var("STAGE8B_R2B_GENERATION2_HELPER_SHA256")
        .map_err(|_| "missing helper SHA-256 environment")?;
    let effect = std::env::var("STAGE8B_R2B_GENERATION2_EFFECT_SHA256")
        .map_err(|_| "missing effect SHA-256 environment")?;
    let authority = r2a5::create_generation2_helper_acceptance_authority(
        Path::new(&ceremony),
        &helper,
        &effect,
    )?;
    println!("{}", serde_json::to_string_pretty(&authority)?);
    Ok(())
}
