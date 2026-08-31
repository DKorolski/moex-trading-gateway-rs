use stage8b_readonly_preflight::r2a5;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("usage: ceremony path is accepted only through the local environment".into());
    }
    let ceremony = std::env::var_os("STAGE8B_R2B_TRUST_REBIND_CEREMONY_DIR")
        .ok_or("missing local ceremony environment")?;
    let source_ref = std::env::var("STAGE8B_R2B_TRUST_REBIND_SOURCE_REF")
        .map_err(|_| "missing source-ref environment")?;
    let verified_at = std::env::var("STAGE8B_R2B_TRUST_REBIND_VERIFIED_AT_UTC")
        .map_err(|_| "missing verification-time environment")?;
    let verifier_source_sha256 = std::env::var("STAGE8B_R2B_TRUST_REBIND_VERIFIER_SOURCE_SHA256")
        .map_err(|_| "missing verifier-source environment")?;
    let verified_at =
        chrono::DateTime::parse_from_rfc3339(&verified_at)?.with_timezone(&chrono::Utc);
    let receipt = r2a5::create_trust_rebind_verification_receipt(
        Path::new(&ceremony),
        &source_ref,
        verified_at,
        &verifier_source_sha256,
    )?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
