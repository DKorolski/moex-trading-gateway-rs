//! Local numerical ports and a bounded SQLite replay integration. No FINAM/ALOR
//! client, async runtime, Redis, credential, order or operational admission.
//! The current V6 runtime is not linked to this crate.
pub mod context;
pub mod fixture;
pub mod imoexf;
pub mod local;
pub mod ri;
pub mod ri_context;
mod ri_engine;
pub mod ri_fixture;
pub mod session;
pub mod usdrubf;

use serde::{Deserialize, Serialize};

/// Historical name retained for exact fixture compatibility: close_time_utc
/// is the M10 BEGIN label in seconds, NOT its availability or close time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BarEvent {
    pub symbol: String,
    pub close_time_utc: i64,
    pub close: f64,
    pub o: f64,
    pub h: f64,
    pub l: f64,
    pub v: f64,
    pub origin: DataOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataOrigin {
    History,
    HistoryGap,
    Live,
    Replay,
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod usd_tests;

#[cfg(test)]
mod ri_tests;

#[cfg(test)]
mod local_tests;
