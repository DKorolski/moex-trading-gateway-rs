use finam_gateway::{
    Stage8ExecutionPreflightError, Stage8a1AuthorityRoot, Stage8a1OperationalAuthorityIssuer,
    Stage8a1TrustedCurrentSources,
};
use runtime_durable_service::Stage7bRecoveryReadyOwner;
use std::path::Path;
use strategy_runtime_core::{
    Stage5gLifecycleCommitmentKey, Stage6DurableCommandSnapshotV1, Stage6DurableRequestIdentityV1,
};

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
fn owner_mediated_constructor_boundary(
    owner: &mut Stage7bRecoveryReadyOwner,
    commitment_key: &Stage5gLifecycleCommitmentKey,
    identity: &Stage6DurableRequestIdentityV1,
    command: &Stage6DurableCommandSnapshotV1,
    root: &Path,
    accepted_config_sha256: &str,
) -> Result<Stage8a1OperationalAuthorityIssuer, Stage8ExecutionPreflightError> {
    Stage8a1OperationalAuthorityIssuer::from_stage7b_owner(
        owner,
        commitment_key,
        identity,
        command,
        root,
        accepted_config_sha256,
    )
}

#[test]
fn trusted_issuer_is_the_public_no_send_authority_boundary() {
    let _boundary = opaque_authority_boundary;
    let _constructor = owner_mediated_constructor_boundary;
}
