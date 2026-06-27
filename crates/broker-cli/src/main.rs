use anyhow::Result;
use broker_finam::{FinamCapabilities, FinamConfig, FinamRestClient};
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
    /// Check Finam secret-token auth without printing the resulting JWT.
    FinamAuthCheck {
        /// Environment variable that contains the Finam secret token.
        #[arg(long, default_value = "FINAM_SECRET_TOKEN")]
        secret_env: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
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
        Command::FinamAuthCheck { secret_env } => {
            let secret = std::env::var(&secret_env)?;
            let client = FinamRestClient::new(FinamConfig::default());
            match client.auth(&secret).await {
                Ok(auth) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "auth_http": 200,
                            "jwt_present": !auth.token.is_empty(),
                            "jwt_len": auth.token.len(),
                        }))?
                    );
                    match client.token_details(&auth.token).await {
                        Ok(details) => {
                            let detail_keys = details
                                .as_object()
                                .map(|object| object.keys().cloned().collect::<Vec<_>>())
                                .unwrap_or_default();
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&serde_json::json!({
                                    "details_http": 200,
                                    "details_keys": detail_keys,
                                }))?
                            );
                        }
                        Err(error) => {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&serde_json::json!({
                                    "details_error": error.to_string(),
                                }))?
                            );
                        }
                    }
                }
                Err(error) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "auth_error": error.to_string(),
                        }))?
                    );
                }
            }
        }
    }
    Ok(())
}
