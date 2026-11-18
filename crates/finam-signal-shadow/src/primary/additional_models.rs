//! Native USD/RI source-to-model composition. Not a deployment permission.
//! Legacy IMOEXF entry/config/reader stay closed to these instruments.
mod cohort;
mod plan;
pub mod reader;
pub use cohort::{Cohort, Finish, Progress};
pub use plan::{IdentityReceipt, InstrumentEvidence, Plan, Prepared};
#[cfg(test)]
mod tests;

/// Exact component contract; includes the shared adapters it relies on.
pub fn implementation() -> String {
    crate::seed::digest(
        &[
            include_bytes!("additional_models.rs").as_slice(),
            include_bytes!("additional_models/plan.rs").as_slice(),
            include_bytes!("additional_models/cohort.rs").as_slice(),
            include_bytes!("additional_models/reader.rs").as_slice(),
            include_bytes!("calendar/additional.rs").as_slice(),
            include_bytes!("calendar.rs").as_slice(),
            include_bytes!("wire.rs").as_slice(),
            include_bytes!("../../src/socket/primary.rs").as_slice(),
            include_bytes!("../../src/runner/primary/http.rs").as_slice(),
            include_bytes!("../../../broker-finam/src/shadow_instrument.rs").as_slice(),
            include_bytes!("../../../broker-finam/src/sparse_m10.rs").as_slice(),
            include_bytes!("../../../broker-finam/src/o2_readonly.rs").as_slice(),
            include_bytes!("../../../broker-finam/src/o2_readonly/observed.rs").as_slice(),
            include_bytes!("../../../broker-finam/src/o2_readonly/observed/primary.rs").as_slice(),
            super::models_bridge::implementation().as_bytes(),
        ]
        .concat(),
    )
}
