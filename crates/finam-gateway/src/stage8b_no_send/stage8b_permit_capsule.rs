//! Opaque Stage 8B authority capsules shared by the K4 bridge and adapter.
//!
//! This module is a sibling of the adapter. Its private fields prevent the
//! adapter from fabricating either the Stage 8A-2 extraction proof or its own
//! request input.

use super::{
    digest_parts, Stage8bK4ControlApproved, Stage8bNoSendCompositionError,
    Stage8bSealedAttemptCommitted,
};
use crate::stage8a1_execution_capability::{
    Stage8a1Stage8bBoundContinuation, Stage8a2Stage8bRequestCapsule,
};
use crate::{
    Stage8a2BuilderCompositionDiagnostic, Stage8a2InMemoryNoSendSink, Stage8a3EndpointContext,
};

pub(crate) struct Stage8bA2PermitProof {
    permit_binding_sha256: String,
    durable_binding_sha256: String,
    continuation_binding_sha256: String,
    exact_attempt_sha256: String,
    covering_seal_sha256: String,
}

impl Stage8bA2PermitProof {
    fn mint_at_k4(
        permit_binding_sha256: String,
        durable_binding_sha256: String,
        continuation_binding_sha256: String,
        exact_attempt_sha256: String,
        covering_seal_sha256: String,
    ) -> Self {
        Self {
            permit_binding_sha256,
            durable_binding_sha256,
            continuation_binding_sha256,
            exact_attempt_sha256,
            covering_seal_sha256,
        }
    }

    pub(crate) fn authorizes_stage8a2_extraction(
        &self,
        durable_binding_sha256: &str,
        continuation_binding_sha256: &str,
    ) -> bool {
        self.durable_binding_sha256 == durable_binding_sha256
            && self.continuation_binding_sha256 == continuation_binding_sha256
            && is_lower_sha256(&self.permit_binding_sha256)
            && is_lower_sha256(&self.exact_attempt_sha256)
            && is_lower_sha256(&self.covering_seal_sha256)
    }
}

pub(super) struct Stage8bExactTransportPermit {
    proof: Stage8bA2PermitProof,
    continuation: Stage8a1Stage8bBoundContinuation,
}

impl Stage8bExactTransportPermit {
    pub(super) fn authorize_at_k4(
        sealed: Stage8bSealedAttemptCommitted,
        k4: Stage8bK4ControlApproved,
    ) -> Result<Self, Stage8bNoSendCompositionError> {
        if k4.rechecked_attempt_sha256 != sealed.attempt_sha256
            || !is_lower_sha256(&k4.control_sha256)
        {
            return Err(Stage8bNoSendCompositionError::InvalidCrossBinding);
        }
        let durable_binding_sha256 = sealed.continuation.durable_binding_sha256().to_string();
        let continuation_binding_sha256 = sealed
            .continuation
            .continuation_binding_sha256()
            .to_string();
        let permit_binding_sha256 = digest_parts(
            b"stage8b-it-r3-exact-transport-permit-v2",
            &[
                sealed.attempt_sha256.as_bytes(),
                sealed.covering_seal_sha256.as_bytes(),
                k4.control_sha256.as_bytes(),
                durable_binding_sha256.as_bytes(),
                continuation_binding_sha256.as_bytes(),
            ],
        );
        let proof = Stage8bA2PermitProof::mint_at_k4(
            permit_binding_sha256,
            durable_binding_sha256,
            continuation_binding_sha256,
            sealed.attempt_sha256,
            sealed.covering_seal_sha256,
        );
        Ok(Self {
            proof,
            continuation: sealed.continuation,
        })
    }

    pub(super) fn consume_stage8a2_request_capsule(
        self,
        sink: &mut Stage8a2InMemoryNoSendSink,
    ) -> Result<Stage8a2Stage8bRequestCapsule, Stage8bNoSendCompositionError> {
        Ok(self
            .continuation
            .consume_stage8a2_request_capsule(self.proof, sink)?)
    }
}

pub(super) enum Stage8bPrivateRequestSpec {
    Place {
        spec: broker_finam::FinamPlaceOrderRequestSpec,
        context: Stage8a3EndpointContext,
    },
    Cancel {
        spec: broker_finam::FinamCancelOrderRequestSpec,
        context: Stage8a3EndpointContext,
    },
}

pub(super) struct Stage8bApprovedRequestParts {
    proof: Stage8bA2PermitProof,
    diagnostic: Stage8a2BuilderCompositionDiagnostic,
    request: Stage8bPrivateRequestSpec,
}

impl Stage8bApprovedRequestParts {
    pub(super) fn from_permit_bound_capsule(
        proof: Stage8bA2PermitProof,
        diagnostic: Stage8a2BuilderCompositionDiagnostic,
        request: Stage8bPrivateRequestSpec,
    ) -> Self {
        Self {
            proof,
            diagnostic,
            request,
        }
    }

    pub(super) fn into_adapter_payload(
        self,
    ) -> (
        Stage8bPrivateRequestSpec,
        Stage8a2BuilderCompositionDiagnostic,
        String,
    ) {
        (
            self.request,
            self.diagnostic,
            self.proof.permit_binding_sha256,
        )
    }
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
