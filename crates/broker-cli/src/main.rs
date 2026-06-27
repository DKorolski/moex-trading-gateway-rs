use anyhow::Result;
use broker_finam::{FinamCapabilities, FinamConfig};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about = "MOEX broker gateway operator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print compiled-in Finam endpoint defaults and capability assumptions.
    FinamInfo,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::FinamInfo => {
            let payload = serde_json::json!({
                "config": FinamConfig::default(),
                "capabilities": FinamCapabilities::default(),
                "live_trading_enabled": false,
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
    }
    Ok(())
}
