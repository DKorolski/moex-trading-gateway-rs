use anyhow::{bail, ensure, Context, Result};
use finam_shadow_models::fixture::{run, MAX_INPUT_BYTES};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        bail!("usage: finam-shadow-reaction-fixture INPUT_JSON NEW_OUTPUT_JSON IMPLEMENTATION_ID");
    }
    let input = Path::new(&args[0]);
    ensure!(
        fs::symlink_metadata(input)?.file_type().is_file(),
        "input must be a regular file"
    );
    let f = fs::File::open(input)?;
    ensure!(
        f.metadata()?.is_file() && f.metadata()?.len() <= MAX_INPUT_BYTES as u64,
        "regular bounded input required"
    );
    let mut bytes = Vec::new();
    f.take(MAX_INPUT_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    let result = run(
        &bytes,
        args[2].to_str().context("invalid implementation ID")?,
    )?;
    // Write only after the entire replay succeeded, never overwrite an artifact.
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    serde_json::to_writer(&mut out, &result)?;
    writeln!(&mut out)?;
    out.sync_all()?;
    Ok(())
}
