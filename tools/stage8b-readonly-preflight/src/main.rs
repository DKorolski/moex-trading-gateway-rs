//! R2A2 qualification binary.
//!
//! R2A2 intentionally has no credential-bearing production entry. The exact
//! binary is qualified and frozen here; an independently reviewed R2B slice
//! may add a launcher only after it verifies this frozen digest.

use sha2::{Digest, Sha256};

const QUALIFIED_R2A2_SOURCE: &str = include_str!("r2a2.rs");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Bind the exact qualification implementation into this fail-closed
    // executable even though R2B has not opened its production entry yet.
    std::hint::black_box(Sha256::digest(QUALIFIED_R2A2_SOURCE.as_bytes()));
    Err(std::io::Error::other(
        "Stage 8B-P R2A2 is qualification-only: credential and network entry remain closed",
    )
    .into())
}
