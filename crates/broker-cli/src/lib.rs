//! Narrow cross-crate Stage 8B-I no-send caller facade.

pub fn invoke_stage8b_no_send_from_cli(
    request: finam_gateway::Stage8bOperatorInvocationRequest,
) -> Result<finam_gateway::Stage8bOperatorDiagnostic, finam_gateway::Stage8bOperatorFacadeError> {
    finam_gateway::invoke_stage8b_operator_once(request)
}
