use finam_gateway::{
    Stage8ExecutionPreflightError, Stage8a1AuthorityRoot, Stage8a1DurableRequestAuthority,
    Stage8a1OperationalAuthorityIssuer, Stage8a1TrustedCurrentSources,
};
use std::path::Path;

// Black-box compile witness: the trusted root and current-source types are
// public only as opaque authorities. External code cannot open a caller-chosen
// directory or construct raw current snapshots into minting authority.
#[allow(dead_code)]
fn opaque_authority_boundary(
    issuer: Stage8a1OperationalAuthorityIssuer,
    root: Stage8a1AuthorityRoot,
    sources: Stage8a1TrustedCurrentSources,
) {
    drop((issuer, root, sources));
}

#[allow(dead_code, clippy::too_many_arguments)]
fn durable_authority_constructor_boundary(
    durable: &Stage8a1DurableRequestAuthority,
    root: &Path,
    accepted_config_sha256: &str,
) -> Result<Stage8a1OperationalAuthorityIssuer, Stage8ExecutionPreflightError> {
    Stage8a1OperationalAuthorityIssuer::from_durable_authority(
        durable,
        root,
        accepted_config_sha256,
    )
}

#[test]
fn trusted_issuer_is_the_public_no_send_authority_boundary() {
    let _boundary = opaque_authority_boundary;
    let _constructor = durable_authority_constructor_boundary;
}
