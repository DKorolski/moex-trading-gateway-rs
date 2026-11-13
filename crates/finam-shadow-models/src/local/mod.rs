//! Session-bounded, local replay integration. JSON is NOT a FINAM admission
//! capability. No observed-session, order, deployment or delivery claim exists.
mod arms;
pub mod contract;
mod journal;

use anyhow::Result;
pub use journal::{Commit, Journal};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const VERSION: &str = "finam.shadow.local-six-arm-session.v1";
pub(crate) fn hash<T: Serialize>(v: &T) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(v)?)))
}
pub fn implementation_sha256() -> Result<String> {
    let files: &[&[u8]] = &[
        include_bytes!("../../Cargo.lock"),
        include_bytes!("../../Cargo.toml"),
        include_bytes!("../local_main.rs"),
        include_bytes!("../../profiles/imoexf.json"),
        include_bytes!("../../profiles/usdrubf.json"),
        include_bytes!("../../profiles/ri.json"),
        include_bytes!("../lib.rs"),
        include_bytes!("../context.rs"),
        include_bytes!("../session.rs"),
        include_bytes!("../imoexf.rs"),
        include_bytes!("../usdrubf.rs"),
        include_bytes!("../ri.rs"),
        include_bytes!("../ri_engine.rs"),
        include_bytes!("../ri_context.rs"),
        include_bytes!("mod.rs"),
        include_bytes!("contract.rs"),
        include_bytes!("arms.rs"),
        include_bytes!("journal.rs"),
    ];
    hash(&(
        VERSION,
        files
            .iter()
            .map(|b| format!("{:x}", Sha256::digest(b)))
            .collect::<Vec<_>>(),
    ))
}
