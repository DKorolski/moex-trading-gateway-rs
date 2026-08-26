//! Linux fd-bound launcher for the independently reviewed R2A4 helper.

use std::ffi::CString;
use std::path::Path;

const HELPER: &str = "/opt/moex-trading/stage8b-r2a4/bin/stage8b-readonly-preflight";
const ACCEPTED_SHA256: &str =
    include_str!("../../../../docs/stage-8/stage8b-p-r2a4-accepted-helper-sha256.txt");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    let mode = match arguments.as_slice() {
        [_program] => "--r2b-one-shot",
        [_program, controlled] if controlled == "--controlled-fixed-layout" => {
            "--r2a4-qualify-fixed-layout"
        }
        _ => return Err("launcher accepts no arguments or --controlled-fixed-layout".into()),
    };
    let helper_arguments = [
        stage8b_readonly_preflight::r2a3::path_argument(Path::new(HELPER))?,
        CString::new(mode)?,
    ];
    let environment: Vec<CString> = Vec::new();
    stage8b_readonly_preflight::r2a3::verified_exec(
        Path::new(HELPER),
        ACCEPTED_SHA256.trim(),
        &helper_arguments,
        &environment,
    )?;
    Ok(())
}
