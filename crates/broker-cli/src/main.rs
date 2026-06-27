use anyhow::Result;
use broker_finam::{
    AllAssetsQuery, BarsQuery, FinamApiCapabilities, FinamConfig, FinamRestClient,
    GatewayEnabledFeatures, HistoryQuery,
};
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
    #[command(name = "finam-info")]
    Info,
    /// Check Finam secret-token auth without printing the resulting JWT.
    #[command(name = "finam-auth-check")]
    AuthCheck {
        /// Environment variable that contains the Finam secret token.
        #[arg(long, default_value = "FINAM_SECRET_TOKEN")]
        secret_env: String,
    },
    /// Run a redacted read-only Finam probe. Does not place or cancel orders.
    #[command(name = "finam-readonly-check")]
    ReadonlyCheck {
        /// Environment variable that contains the Finam secret token.
        #[arg(long, default_value = "FINAM_SECRET_TOKEN")]
        secret_env: String,
        /// Optional Finam account id for account/orders/trades/transactions checks.
        #[arg(long, env = "FINAM_ACCOUNT_ID")]
        account_id: Option<String>,
        /// Optional venue symbol, for example TICKER@MIC, for asset/bars checks.
        #[arg(long, env = "FINAM_SYMBOL")]
        symbol: Option<String>,
        /// Bars timeframe value accepted by Finam REST, for example TIME_FRAME_M1.
        #[arg(long, default_value = "TIME_FRAME_M1")]
        timeframe: String,
        /// Optional inclusive start time, RFC3339, for history/bars checks.
        #[arg(long)]
        start_time: Option<String>,
        /// Optional exclusive end time, RFC3339, for history/bars checks.
        #[arg(long)]
        end_time: Option<String>,
        /// Query limit for account trades/transactions probes.
        #[arg(long, default_value_t = 10)]
        limit: u32,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Info => {
            let payload = serde_json::json!({
                "config": FinamConfig::default(),
                "api_capabilities": FinamApiCapabilities::default(),
                "enabled_features": GatewayEnabledFeatures::default(),
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        Command::AuthCheck { secret_env } => {
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
        Command::ReadonlyCheck {
            secret_env,
            account_id,
            symbol,
            timeframe,
            start_time,
            end_time,
            limit,
        } => {
            let secret = std::env::var(&secret_env)?;
            let client = FinamRestClient::new(FinamConfig::default());
            match client.auth(&secret).await {
                Ok(auth) => {
                    print_json(serde_json::json!({
                        "auth_http": 200,
                        "jwt_present": !auth.token.is_empty(),
                        "jwt_len": auth.token.len(),
                        "live_trading_enabled": false,
                    }))?;

                    print_probe_result(
                        "token_details",
                        client.token_details(&auth.token).await.as_ref(),
                    )?;
                    print_probe_result("clock", client.clock(&auth.token).await.as_ref())?;
                    print_probe_result("exchanges", client.exchanges(&auth.token).await.as_ref())?;
                    print_probe_result("assets", client.assets(&auth.token).await.as_ref())?;
                    print_probe_result(
                        "all_assets_active_first_page",
                        client
                            .all_assets(
                                &auth.token,
                                AllAssetsQuery {
                                    only_active: Some(true),
                                    ..AllAssetsQuery::default()
                                },
                            )
                            .await
                            .as_ref(),
                    )?;

                    if let Some(account_id) = account_id.as_deref() {
                        let history_query = HistoryQuery {
                            limit: Some(limit),
                            start_time: start_time.as_deref(),
                            end_time: end_time.as_deref(),
                        };
                        print_probe_result(
                            "account",
                            client.account(&auth.token, account_id).await.as_ref(),
                        )?;
                        print_probe_result(
                            "account_orders",
                            client
                                .account_orders(&auth.token, account_id)
                                .await
                                .as_ref(),
                        )?;
                        print_probe_result(
                            "account_trades",
                            client
                                .account_trades(&auth.token, account_id, history_query)
                                .await
                                .as_ref(),
                        )?;
                        print_probe_result(
                            "account_transactions",
                            client
                                .account_transactions(&auth.token, account_id, history_query)
                                .await
                                .as_ref(),
                        )?;
                    }

                    if let Some(symbol) = symbol.as_deref() {
                        let bars_query = BarsQuery {
                            timeframe: &timeframe,
                            start_time: start_time.as_deref(),
                            end_time: end_time.as_deref(),
                        };
                        print_probe_result(
                            "asset",
                            client
                                .asset(&auth.token, symbol, account_id.as_deref())
                                .await
                                .as_ref(),
                        )?;
                        print_probe_result(
                            "asset_params",
                            client
                                .asset_params(&auth.token, symbol, account_id.as_deref())
                                .await
                                .as_ref(),
                        )?;
                        print_probe_result(
                            "asset_schedule",
                            client.asset_schedule(&auth.token, symbol).await.as_ref(),
                        )?;
                        print_probe_result(
                            "last_quote",
                            client.last_quote(&auth.token, symbol).await.as_ref(),
                        )?;
                        print_probe_result(
                            "latest_trades",
                            client.latest_trades(&auth.token, symbol).await.as_ref(),
                        )?;
                        print_probe_result(
                            "bars",
                            client.bars(&auth.token, symbol, bars_query).await.as_ref(),
                        )?;
                    }
                }
                Err(error) => {
                    print_json(serde_json::json!({
                        "auth_error": error.to_string(),
                        "live_trading_enabled": false,
                    }))?;
                }
            }
        }
    }
    Ok(())
}

fn print_probe_result(
    name: &str,
    result: Result<&serde_json::Value, &broker_finam::FinamError>,
) -> Result<()> {
    let payload = match result {
        Ok(value) => serde_json::json!({
            "probe": name,
            "ok": true,
            "shape": json_shape(value),
        }),
        Err(error) => serde_json::json!({
            "probe": name,
            "ok": false,
            "error": error.to_string(),
        }),
    };
    print_json(payload)
}

fn json_shape(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => serde_json::json!({
            "kind": "object",
            "keys": object.keys().cloned().collect::<Vec<_>>(),
        }),
        serde_json::Value::Array(items) => serde_json::json!({
            "kind": "array",
            "len": items.len(),
        }),
        serde_json::Value::String(_) => serde_json::json!({ "kind": "string" }),
        serde_json::Value::Number(_) => serde_json::json!({ "kind": "number" }),
        serde_json::Value::Bool(_) => serde_json::json!({ "kind": "bool" }),
        serde_json::Value::Null => serde_json::json!({ "kind": "null" }),
    }
}

fn print_json(value: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
