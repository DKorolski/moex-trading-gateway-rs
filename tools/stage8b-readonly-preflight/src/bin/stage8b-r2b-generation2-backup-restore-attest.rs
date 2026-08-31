use chrono::{DateTime, Utc};
use stage8b_readonly_preflight::r2a5::{self, TrustRebindBackupRestoreMetadata};
use std::path::Path;

fn environment(name: &str) -> Result<std::ffi::OsString, Box<dyn std::error::Error>> {
    std::env::var_os(name).ok_or_else(|| format!("missing local environment: {name}").into())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("usage: custody paths are accepted only through local environment".into());
    }
    let primary = environment("STAGE8B_R2B_G2_PRIMARY_CEREMONY_DIR")?;
    let restored = environment("STAGE8B_R2B_G2_RESTORED_CEREMONY_DIR")?;
    let restore_parent = environment("STAGE8B_R2B_G2_RESTORE_PARENT_DIR")?;
    let metadata_path = environment("STAGE8B_R2B_G2_BACKUP_METADATA_FILE")?;
    let source_ref = std::env::var("STAGE8B_R2B_G2_SOURCE_REF")?;
    let verified_at: DateTime<Utc> =
        DateTime::parse_from_rfc3339(&std::env::var("STAGE8B_R2B_G2_VERIFIED_AT_UTC")?)?
            .with_timezone(&Utc);
    let metadata: TrustRebindBackupRestoreMetadata =
        serde_json::from_slice(&std::fs::read(metadata_path)?)?;
    let receipt = r2a5::create_trust_rebind_backup_restore_receipt(
        Path::new(&primary),
        Path::new(&restored),
        Path::new(&restore_parent),
        &source_ref,
        verified_at,
        &metadata,
    )?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
