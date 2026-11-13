//! Local files only. No credentials, network or installed-runtime access.
use anyhow::{ensure, Context, Result};
use finam_shadow_models::local::{
    contract::{Candle, Manifest, MAX_EVENT_BYTES, MAX_MANIFEST_BYTES},
    implementation_sha256, Journal,
};
use std::{io::Read, path::Path};
fn read(path: &str, bound: usize) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    std::fs::File::open(path)?
        .take(bound as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= bound, "input file bound");
    Ok(bytes)
}
fn run() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() == 2 && a[1] == "implementation" {
        println!("{}", implementation_sha256()?);
        return Ok(());
    }
    if a.len() == 5 && a[1] == "replay" {
        let manifest: Manifest = serde_json::from_slice(&read(&a[2], MAX_MANIFEST_BYTES)?)?;
        let candles: Vec<Candle> = serde_json::from_slice(&read(&a[4], MAX_EVENT_BYTES * 303)?)?;
        ensure!(candles.len() <= 303, "one session input bound");
        let mut j = Journal::create(Path::new(&a[3]), manifest)?;
        for c in candles {
            j.apply(c)?;
        }
        println!("{}", serde_json::to_string(&j.export()?)?);
        return Ok(());
    }
    ensure!(
        (a.len() == 4 && ["create", "export"].contains(&a[1].as_str()))
            || (a.len() == 5 && a[1] == "apply"),
        "usage: implementation | create/export MANIFEST DB | apply MANIFEST DB CANDLE | replay MANIFEST NEW_DB CANDLES"
    );
    let manifest: Manifest = serde_json::from_slice(&read(&a[2], MAX_MANIFEST_BYTES)?)?;
    let path = Path::new(&a[3]);
    match a[1].as_str() {
        "create" => {
            Journal::create(path, manifest)?;
            println!("{{\"status\":\"created_replay_only\"}}");
        }
        "apply" => {
            let c: Candle = serde_json::from_slice(&read(
                a.get(4).context("candle missing")?,
                MAX_EVENT_BYTES,
            )?)?;
            println!(
                "{}",
                serde_json::to_string(&Journal::open(path, manifest)?.apply(c)?)?
            );
        }
        "export" => println!(
            "{}",
            serde_json::to_string(&Journal::open(path, manifest)?.export()?)?
        ),
        _ => unreachable!(),
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("local_session_error: {e:#}");
        std::process::exit(1);
    }
}
