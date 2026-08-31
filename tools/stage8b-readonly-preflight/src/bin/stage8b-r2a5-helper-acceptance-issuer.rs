use stage8b_readonly_preflight::r2a5;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let [helper_sha256, effect_sha256] = arguments.as_slice() else {
        return Err(
            "usage: stage8b-r2a5-helper-acceptance-issuer HELPER_SHA256 EFFECT_SHA256".into(),
        );
    };
    r2a5::accept_helper_from_fixed_authority(helper_sha256, effect_sha256)?;
    println!("stage8b-r2a5-helper-acceptance: PASS");
    Ok(())
}
