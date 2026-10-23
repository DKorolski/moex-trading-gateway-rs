//! Isolated signal-shadow foundation, not a trading or deployment entry.
//! Read-only transports are connected only by the explicitly invoked runner.
//! Config validation does not establish warmup, reference parity or readiness.

mod config;
pub mod feedback;
pub mod input;
pub mod model;
pub mod observer;
pub mod prefix;
pub mod profile;
pub mod quote;
pub mod replay;
pub mod runner;
pub mod seed;
pub mod segment;
pub mod session;
pub mod socket;
pub mod timing;
pub mod transport;
pub mod ws;

pub use config::{ConfigError, Reference, RestartMode, ShadowConfig, ValidatedConfig};
