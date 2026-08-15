use broker_core::{OrderPreflightContext, PlaceOrder};
use finam_gateway::{
    Stage8ExecutionCapability, Stage8ExecutionPreflightError, Stage8a1CurrentOperationalSources,
    Stage8a1DurableRequestAuthority, Stage8a1OperationalAuthorityIssuer,
};

// Black-box compile witness: code outside the private capability module can
// enter the production path only with the owner-issued durable authority,
// typed current sources and file-backed production issuer. No individual
// opaque proof fields or constructors are needed or available here.
#[allow(dead_code)]
fn production_place_boundary<'a>(
    issuer: &mut Stage8a1OperationalAuthorityIssuer,
    durable: Stage8a1DurableRequestAuthority,
    order: &PlaceOrder,
    context: &OrderPreflightContext,
    sources: Stage8a1CurrentOperationalSources<'a>,
    logical_nonce: &str,
) -> Result<Stage8ExecutionCapability, Stage8ExecutionPreflightError> {
    issuer.authorize_place(durable, order, context, sources, logical_nonce)
}

#[test]
fn production_issuer_is_the_public_no_send_authority_boundary() {
    let _boundary = production_place_boundary;
}
