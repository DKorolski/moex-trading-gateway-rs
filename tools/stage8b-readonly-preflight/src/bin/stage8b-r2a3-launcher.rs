//! Linux fd-bound launcher for the independently accepted R2A3 helper.

use std::ffi::CString;
use std::path::Path;

const HELPER: &str = "/opt/moex-trading/stage8b-r2a3/bin/stage8b-readonly-preflight";
const ACCEPTED_SHA256: &str =
    include_str!("../../../../docs/stage-8/stage8b-p-r2a3-accepted-helper-sha256.txt");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("launcher accepts no caller-selected arguments".into());
    }
    let arguments = [
        stage8b_readonly_preflight::r2a3::path_argument(Path::new(HELPER))?,
        CString::new("--r2b-one-shot")?,
    ];
    // The helper uses fixed absolute paths and receives no ambient environment.
    let environment: Vec<CString> = Vec::new();
    stage8b_readonly_preflight::r2a3::verified_exec(
        Path::new(HELPER),
        ACCEPTED_SHA256.trim(),
        &arguments,
        &environment,
    )?;
    Ok(())
}
