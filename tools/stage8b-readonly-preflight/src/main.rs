use chrono::Utc;
use sha2::{Digest, Sha256};
use stage8b_readonly_preflight::execute_production;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

fn usage() -> &'static str {
    "usage: stage8b-readonly-preflight --manifest PATH --current-sources PATH --output PATH"
}

fn exact_args() -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let mut args = std::env::args_os().skip(1);
    let mut manifest = None;
    let mut current_sources = None;
    let mut output = None;
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value: {}", usage()))?;
        match flag.to_str() {
            Some("--manifest") if manifest.is_none() => manifest = Some(PathBuf::from(value)),
            Some("--current-sources") if current_sources.is_none() => {
                current_sources = Some(PathBuf::from(value))
            }
            Some("--output") if output.is_none() => output = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown or duplicate argument: {}", usage())),
        }
    }
    match (manifest, current_sources, output) {
        (Some(manifest), Some(current_sources), Some(output)) => {
            Ok((manifest, current_sources, output))
        }
        _ => Err(usage().to_owned()),
    }
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "input must be a regular non-symlink file: {}",
            path.display()
        ));
    }
    fs::read(path).map_err(|error| error.to_string())
}

fn current_executable_sha256() -> Result<String, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let bytes = read_regular_file(&executable)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (manifest_path, sources_path, output_path) = exact_args().map_err(std::io::Error::other)?;
    let manifest = read_regular_file(&manifest_path).map_err(std::io::Error::other)?;
    let current_sources = read_regular_file(&sources_path).map_err(std::io::Error::other)?;
    let secret = Zeroizing::new(
        std::env::var("FINAM_SECRET_TOKEN")
            .map_err(|_| std::io::Error::other("FINAM_SECRET_TOKEN is required"))?,
    );
    let account_id = Zeroizing::new(
        std::env::var("FINAM_ACCOUNT_ID")
            .map_err(|_| std::io::Error::other("FINAM_ACCOUNT_ID is required"))?,
    );
    let executable_sha256 = current_executable_sha256().map_err(std::io::Error::other)?;
    let evidence = execute_production(
        secret.as_str(),
        account_id.as_str(),
        &manifest,
        &current_sources,
        &executable_sha256,
        Utc::now(),
    )
    .await?;
    let encoded = serde_json::to_vec_pretty(&evidence)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output_path)?;
    output.write_all(&encoded)?;
    output.write_all(b"\n")?;
    output.sync_all()?;
    Ok(())
}
