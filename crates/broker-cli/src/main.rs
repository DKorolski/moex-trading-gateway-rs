use anyhow::{Context, Result};
use broker_core::MarketDataEvent;
use broker_finam::{
    active_orders, has_blocking_unknown_order_statuses, map_account_trade, map_bar,
    map_latest_market_trade, map_order_state, map_portfolio_snapshot, map_quote,
    redact_json_key_for_diagnostics, terminal_orders, AllAssetsQuery, BarsQuery,
    FinamApiCapabilities, FinamAuthManager, FinamConfig, FinamMapperError, FinamRestClient,
    GatewayEnabledFeatures, HistoryQuery, SecretToken,
};
use chrono::{Duration, Utc};
use clap::{Parser, Subcommand};
use finam_gateway::{
    default_readonly_health, FinamGateway, GatewayConfig, GatewayFeatureSet,
    RedisConnectionStreamSink, RedisStreamConfig,
};
use redis::streams::StreamRangeReply;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

const JSON_SHAPE_MAX_DEPTH: usize = 4;

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
        /// Optional file path for saving redacted probe records as JSON.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Run typed DTO and mapper smoke checks. Does not place or cancel orders.
    #[command(name = "finam-typed-readonly-check")]
    TypedReadonlyCheck {
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
        /// Optional file path for saving typed redacted probe records as JSON.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Run one FINAM read-only shadow gateway pass and publish broker-truth events to Redis.
    #[command(name = "finam-gateway-shadow-once")]
    GatewayShadowOnce {
        /// Optional JSON config file with Redis streams and read-only FINAM inputs.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Environment variable that contains the Finam secret token.
        #[arg(long, default_value = "FINAM_SECRET_TOKEN")]
        secret_env: String,
        /// Redis connection URL. Overrides config file.
        #[arg(long, env = "FINAM_GATEWAY_REDIS_URL")]
        redis_url: Option<String>,
        /// Finam account id. Overrides config file.
        #[arg(long, env = "FINAM_ACCOUNT_ID")]
        account_id: Option<String>,
        /// Finam venue symbol, for example IMOEXF@RTSX. Overrides config file.
        #[arg(long, env = "FINAM_SYMBOL")]
        symbol: Option<String>,
        /// Bars timeframe value accepted by Finam REST, for example TIME_FRAME_M1.
        #[arg(long, env = "FINAM_TIMEFRAME")]
        timeframe: Option<String>,
        /// Optional inclusive start time, RFC3339, for bars publication.
        #[arg(long)]
        start_time: Option<String>,
        /// Optional exclusive end time, RFC3339, for bars publication.
        #[arg(long)]
        end_time: Option<String>,
        /// Default bars lookback if start/end are not supplied.
        #[arg(long, default_value_t = 60)]
        bars_lookback_minutes: i64,
    },
    /// Publish a synthetic gateway envelope to Redis and read it back with XRANGE.
    #[command(name = "finam-gateway-redis-smoke")]
    GatewayRedisSmoke {
        /// Redis connection URL.
        #[arg(
            long,
            env = "FINAM_GATEWAY_REDIS_URL",
            default_value = "redis://127.0.0.1:6379/"
        )]
        redis_url: String,
        /// Redis stream used for the synthetic smoke event.
        #[arg(long, default_value = "finam:smoke")]
        stream: String,
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
            let secret = SecretToken::new(std::env::var(&secret_env)?);
            let client = FinamRestClient::try_new(FinamConfig::default())?;
            let auth_manager = FinamAuthManager::new(client.clone(), secret);
            match auth_manager.access_token().await {
                Ok(token) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "auth_http": 200,
                            "jwt_present": !token.is_empty(),
                            "jwt_len": token.len(),
                        }))?
                    );
                    match client.token_details(&token).await {
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
                                    "details_error_kind": error.kind(),
                                    "details_error": error.to_redacted_string(),
                                }))?
                            );
                        }
                    }
                }
                Err(error) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "auth_error_kind": error.kind(),
                            "auth_error": error.to_redacted_string(),
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
            output,
        } => {
            let mut records = Vec::new();
            let secret = SecretToken::new(std::env::var(&secret_env)?);
            let client = FinamRestClient::try_new(FinamConfig::default())?;
            let auth_manager = FinamAuthManager::new(client.clone(), secret);
            match auth_manager.access_token().await {
                Ok(token) => {
                    emit_record(
                        &mut records,
                        serde_json::json!({
                            "auth_http": 200,
                            "jwt_present": !token.is_empty(),
                            "jwt_len": token.len(),
                            "live_trading_enabled": false,
                        }),
                    )?;

                    emit_probe_result(
                        &mut records,
                        "token_details",
                        client.token_details(&token).await.as_ref(),
                    )?;
                    emit_probe_result(&mut records, "clock", client.clock(&token).await.as_ref())?;
                    emit_probe_result(
                        &mut records,
                        "exchanges",
                        client.exchanges(&token).await.as_ref(),
                    )?;
                    emit_probe_result(
                        &mut records,
                        "assets",
                        client.assets(&token).await.as_ref(),
                    )?;
                    emit_probe_result(
                        &mut records,
                        "all_assets_active_first_page",
                        client
                            .all_assets(
                                &token,
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
                        emit_probe_result(
                            &mut records,
                            "account",
                            client.account(&token, account_id).await.as_ref(),
                        )?;
                        emit_probe_result(
                            &mut records,
                            "account_orders",
                            client.account_orders(&token, account_id).await.as_ref(),
                        )?;
                        emit_probe_result(
                            &mut records,
                            "account_trades",
                            client
                                .account_trades(&token, account_id, history_query)
                                .await
                                .as_ref(),
                        )?;
                        emit_probe_result(
                            &mut records,
                            "account_transactions",
                            client
                                .account_transactions(&token, account_id, history_query)
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
                        emit_probe_result(
                            &mut records,
                            "asset",
                            client
                                .asset(&token, symbol, account_id.as_deref())
                                .await
                                .as_ref(),
                        )?;
                        emit_probe_result(
                            &mut records,
                            "asset_params",
                            client
                                .asset_params(&token, symbol, account_id.as_deref())
                                .await
                                .as_ref(),
                        )?;
                        emit_probe_result(
                            &mut records,
                            "asset_schedule",
                            client.asset_schedule(&token, symbol).await.as_ref(),
                        )?;
                        emit_probe_result(
                            &mut records,
                            "last_quote",
                            client.last_quote(&token, symbol).await.as_ref(),
                        )?;
                        emit_probe_result(
                            &mut records,
                            "latest_trades",
                            client.latest_trades(&token, symbol).await.as_ref(),
                        )?;
                        emit_probe_result(
                            &mut records,
                            "bars",
                            client.bars(&token, symbol, bars_query).await.as_ref(),
                        )?;
                    }
                }
                Err(error) => {
                    emit_record(
                        &mut records,
                        serde_json::json!({
                            "auth_error_kind": error.kind(),
                            "auth_error": error.to_redacted_string(),
                            "live_trading_enabled": false,
                        }),
                    )?;
                }
            }
            if let Some(output) = output {
                write_redacted_fixture(output, &records)?;
            }
        }
        Command::TypedReadonlyCheck {
            secret_env,
            account_id,
            symbol,
            timeframe,
            start_time,
            end_time,
            limit,
            output,
        } => {
            let mut records = Vec::new();
            let secret = SecretToken::new(std::env::var(&secret_env)?);
            let client = FinamRestClient::try_new(FinamConfig::default())?;
            let auth_manager = FinamAuthManager::new(client.clone(), secret);
            match auth_manager.access_token().await {
                Ok(token) => {
                    emit_record(
                        &mut records,
                        serde_json::json!({
                            "auth_http": 200,
                            "jwt_present": !token.is_empty(),
                            "jwt_len": token.len(),
                            "live_trading_enabled": false,
                            "typed_probe": true,
                        }),
                    )?;

                    emit_typed_result(
                        &mut records,
                        "token_details_typed",
                        client.token_details_typed(&token).await,
                        |details| {
                            Ok(serde_json::json!({
                                "accounts_count": details.account_ids.len(),
                                "md_permissions_count": details.md_permissions.len(),
                                "readonly_present": details.readonly.is_some(),
                            }))
                        },
                    )?;
                    emit_typed_result(
                        &mut records,
                        "exchanges_typed",
                        client.exchanges_typed(&token).await,
                        |exchanges| {
                            Ok(serde_json::json!({
                                "exchanges_count": exchanges.exchanges.len(),
                            }))
                        },
                    )?;
                    emit_typed_result(
                        &mut records,
                        "assets_typed",
                        client.assets_typed(&token).await,
                        |assets| {
                            Ok(serde_json::json!({
                                "assets_count": assets.assets.len(),
                            }))
                        },
                    )?;
                    emit_typed_result(
                        &mut records,
                        "all_assets_typed",
                        client
                            .all_assets_typed(
                                &token,
                                AllAssetsQuery {
                                    only_active: Some(true),
                                    ..AllAssetsQuery::default()
                                },
                            )
                            .await,
                        |assets| {
                            Ok(serde_json::json!({
                                "assets_count": assets.assets.len(),
                                "next_cursor_present": assets.next_cursor.is_some(),
                            }))
                        },
                    )?;

                    if let Some(account_id) = account_id.as_deref() {
                        let history_query = HistoryQuery {
                            limit: Some(limit),
                            start_time: start_time.as_deref(),
                            end_time: end_time.as_deref(),
                        };
                        emit_typed_result(
                            &mut records,
                            "account_typed",
                            client.account_typed(&token, account_id).await,
                            |account| {
                                let snapshot = map_portfolio_snapshot(&account, Utc::now())
                                    .map_err(mapper_anyhow)?;
                                Ok(serde_json::json!({
                                    "cash_count": snapshot.cash.len(),
                                    "positions_count": snapshot.positions.len(),
                                    "status_present": account.status.is_some(),
                                    "type_present": account.account_type.is_some(),
                                }))
                            },
                        )?;
                        emit_typed_result(
                            &mut records,
                            "account_orders_typed",
                            client.account_orders_typed(&token, account_id).await,
                            |orders| {
                                let mapped_orders = orders
                                    .orders
                                    .iter()
                                    .map(|order| map_order_state(order, Utc::now()))
                                    .collect::<std::result::Result<Vec<_>, _>>()
                                    .map_err(mapper_anyhow)?;
                                Ok(serde_json::json!({
                                    "orders_count": orders.orders.len(),
                                    "mapped_orders_count": mapped_orders.len(),
                                    "active_orders_count": active_orders(&mapped_orders).count(),
                                    "terminal_orders_count": terminal_orders(&mapped_orders).count(),
                                    "blocking_unknown_status_present": has_blocking_unknown_order_statuses(&mapped_orders),
                                }))
                            },
                        )?;
                        emit_typed_result(
                            &mut records,
                            "account_trades_typed",
                            client
                                .account_trades_typed(&token, account_id, history_query)
                                .await,
                            |trades| {
                                let mapped_count = trades
                                    .trades
                                    .iter()
                                    .map(|trade| map_account_trade(account_id, trade, Utc::now()))
                                    .collect::<std::result::Result<Vec<_>, _>>()
                                    .map_err(mapper_anyhow)?
                                    .len();
                                Ok(serde_json::json!({
                                    "trades_count": trades.trades.len(),
                                    "mapped_trades_count": mapped_count,
                                }))
                            },
                        )?;
                        emit_typed_result(
                            &mut records,
                            "account_transactions_typed",
                            client
                                .account_transactions_typed(&token, account_id, history_query)
                                .await,
                            |transactions| {
                                Ok(serde_json::json!({
                                    "transactions_count": transactions.transactions.len(),
                                }))
                            },
                        )?;
                    }

                    if let Some(symbol) = symbol.as_deref() {
                        let bars_query = BarsQuery {
                            timeframe: &timeframe,
                            start_time: start_time.as_deref(),
                            end_time: end_time.as_deref(),
                        };
                        emit_typed_result(
                            &mut records,
                            "asset_typed",
                            client
                                .asset_typed(&token, symbol, account_id.as_deref())
                                .await,
                            |asset| {
                                Ok(serde_json::json!({
                                    "asset_type_present": asset.asset_type.is_some(),
                                    "future_details_present": asset.future_details.is_some(),
                                    "lot_size_present": asset.lot_size.is_some(),
                                    "min_step_present": asset.min_step.is_some(),
                                }))
                            },
                        )?;
                        emit_typed_result(
                            &mut records,
                            "asset_params_typed",
                            client
                                .asset_params_typed(&token, symbol, account_id.as_deref())
                                .await,
                            |params| {
                                Ok(serde_json::json!({
                                    "is_tradable": params.is_tradable,
                                    "tradeable": params.tradeable,
                                    "long_initial_margin_present": params.long_initial_margin.is_some(),
                                    "short_initial_margin_present": params.short_initial_margin.is_some(),
                                }))
                            },
                        )?;
                        emit_typed_result(
                            &mut records,
                            "asset_schedule_typed",
                            client.asset_schedule_typed(&token, symbol).await,
                            |schedule| {
                                Ok(serde_json::json!({
                                    "sessions_count": schedule.sessions.len(),
                                }))
                            },
                        )?;
                        emit_typed_result(
                            &mut records,
                            "last_quote_typed",
                            client.last_quote_typed(&token, symbol).await,
                            |quote| {
                                let mapped =
                                    map_quote(&quote, Utc::now()).map_err(mapper_anyhow)?;
                                Ok(serde_json::json!({
                                    "bid_present": mapped.bid.is_some(),
                                    "ask_present": mapped.ask.is_some(),
                                    "last_present": mapped.last.is_some(),
                                    "source_ts_present": mapped.source_ts.is_some(),
                                }))
                            },
                        )?;
                        emit_typed_result(
                            &mut records,
                            "latest_trades_typed",
                            client.latest_trades_typed(&token, symbol).await,
                            |trades| {
                                let mapped_count = trades
                                    .trades
                                    .iter()
                                    .map(|trade| map_latest_market_trade(symbol, trade, Utc::now()))
                                    .collect::<std::result::Result<Vec<_>, _>>()
                                    .map_err(mapper_anyhow)?
                                    .len();
                                Ok(serde_json::json!({
                                    "trades_count": trades.trades.len(),
                                    "mapped_trades_count": mapped_count,
                                }))
                            },
                        )?;
                        emit_typed_result(
                            &mut records,
                            "bars_typed",
                            client.bars_typed(&token, symbol, bars_query).await,
                            |bars| {
                                let timeframe_sec = timeframe_seconds(&timeframe)?;
                                let mapped_count = bars
                                    .bars
                                    .iter()
                                    .map(|bar| map_bar(symbol, bar, timeframe_sec))
                                    .collect::<std::result::Result<Vec<_>, _>>()
                                    .map_err(mapper_anyhow)?
                                    .len();
                                Ok(serde_json::json!({
                                    "bars_count": bars.bars.len(),
                                    "mapped_bars_count": mapped_count,
                                    "timeframe_sec": timeframe_sec,
                                }))
                            },
                        )?;
                    }
                }
                Err(error) => {
                    emit_record(
                        &mut records,
                        serde_json::json!({
                            "auth_error_kind": error.kind(),
                            "auth_error": error.to_redacted_string(),
                            "live_trading_enabled": false,
                            "typed_probe": true,
                        }),
                    )?;
                }
            }
            if let Some(output) = output {
                write_records_fixture(output, "finam-typed-readonly-redacted-v1", &records)?;
            }
        }
        Command::GatewayShadowOnce {
            config,
            secret_env,
            redis_url,
            account_id,
            symbol,
            timeframe,
            start_time,
            end_time,
            bars_lookback_minutes,
        } => {
            run_gateway_shadow_once(GatewayShadowOnceArgs {
                config,
                secret_env,
                redis_url,
                account_id,
                symbol,
                timeframe,
                start_time,
                end_time,
                bars_lookback_minutes,
            })
            .await?;
        }
        Command::GatewayRedisSmoke { redis_url, stream } => {
            run_gateway_redis_smoke(redis_url, stream).await?;
        }
    }
    Ok(())
}

#[derive(Debug)]
struct GatewayShadowOnceArgs {
    config: Option<PathBuf>,
    secret_env: String,
    redis_url: Option<String>,
    account_id: Option<String>,
    symbol: Option<String>,
    timeframe: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    bars_lookback_minutes: i64,
}

#[derive(Debug)]
struct ResolvedGatewayShadowConfig {
    secret_env: String,
    gateway_config: GatewayConfig,
    account_id: String,
    symbol: String,
    timeframe: String,
    start_time: String,
    end_time: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GatewayShadowFileConfig {
    redis_url: Option<String>,
    source: Option<String>,
    account_id: Option<String>,
    symbol: Option<String>,
    timeframe: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    streams: Option<GatewayShadowStreamsFileConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GatewayShadowStreamsFileConfig {
    health: Option<String>,
    readiness: Option<String>,
    portfolio: Option<String>,
    orders_snapshot: Option<String>,
    market_data: Option<String>,
}

async fn run_gateway_shadow_once(args: GatewayShadowOnceArgs) -> Result<()> {
    let file_config = read_gateway_shadow_file_config(args.config.as_ref())?;
    let resolved = resolve_gateway_shadow_config(args, file_config)?;
    let redis_url = resolved.gateway_config.redis.url.clone();
    let secret = SecretToken::new(std::env::var(&resolved.secret_env).with_context(|| {
        format!(
            "missing required environment variable {}",
            resolved.secret_env
        )
    })?);
    let client = FinamRestClient::try_new(FinamConfig::default())?;
    let auth_manager = FinamAuthManager::new(client.clone(), secret);
    let token = match auth_manager.access_token().await {
        Ok(token) => token,
        Err(error) => {
            print_json(serde_json::json!({
                "gateway_shadow_once": false,
                "live_trading_enabled": false,
                "error_kind": error.kind(),
                "error": error.to_redacted_string(),
            }))?;
            return Err(error).context("FINAM auth failed for shadow gateway");
        }
    };
    let sink = match RedisConnectionStreamSink::connect(&redis_url).await {
        Ok(sink) => sink,
        Err(error) => {
            emit_redis_degraded_stderr("redis_connect", &error)?;
            return Err(error).context("Redis connection failed for shadow gateway");
        }
    };
    let gateway = FinamGateway::new(resolved.gateway_config, sink);
    let received_ts = Utc::now();

    let account = client
        .account_typed(&token, &resolved.account_id)
        .await
        .context("FINAM read-only account fetch failed")?;
    let orders = client
        .account_orders_typed(&token, &resolved.account_id)
        .await
        .context("FINAM read-only account orders fetch failed")?;
    let report = match gateway
        .run_readonly_reconciliation_once(&account, &orders, received_ts)
        .await
    {
        Ok(report) => report,
        Err(error) => {
            emit_redis_degraded_stderr("readonly_reconciliation_publish", &error)?;
            return Err(error).context("Redis publication failed for shadow reconciliation");
        }
    };

    let quote = client
        .last_quote_typed(&token, &resolved.symbol)
        .await
        .context("FINAM read-only latest quote fetch failed")?;
    let mapped_quote = map_quote(&quote, Utc::now()).map_err(mapper_anyhow)?;
    if let Err(error) = gateway
        .publish_market_data_event(MarketDataEvent::Quote(mapped_quote))
        .await
    {
        emit_redis_degraded_stderr("quote_publish", &error)?;
        return Err(error).context("Redis publication failed for quote event");
    }

    let bars_query = BarsQuery {
        timeframe: &resolved.timeframe,
        start_time: Some(resolved.start_time.as_str()),
        end_time: Some(resolved.end_time.as_str()),
    };
    let bars = client
        .bars_typed(&token, &resolved.symbol, bars_query)
        .await
        .context("FINAM read-only bars fetch failed")?;
    let timeframe_sec = timeframe_seconds(&resolved.timeframe)?;
    let mut published_bars_count = 0usize;
    for bar in &bars.bars {
        let mapped_bar = map_bar(&resolved.symbol, bar, timeframe_sec).map_err(mapper_anyhow)?;
        if let Err(error) = gateway
            .publish_market_data_event(MarketDataEvent::Bar(mapped_bar))
            .await
        {
            emit_redis_degraded_stderr("bar_publish", &error)?;
            return Err(error).context("Redis publication failed for bar event");
        }
        published_bars_count += 1;
    }

    print_json(serde_json::json!({
        "gateway_shadow_once": true,
        "live_trading_enabled": false,
        "command_consumer_enabled": gateway.config().features.command_consumer_enabled,
        "order_placement_enabled": gateway.config().features.order_placement_enabled,
        "cancel_enabled": gateway.config().features.cancel_enabled,
        "stop_sltp_bracket_enabled": gateway.config().features.stop_sltp_bracket_enabled,
        "streams": {
            "health": gateway.config().redis.health_stream,
            "readiness": gateway.config().redis.readiness_stream,
            "portfolio": gateway.config().redis.portfolio_stream,
            "orders_snapshot": gateway.config().redis.order_snapshot_stream,
            "market_data": gateway.config().redis.market_data_stream,
        },
        "readiness_phase": format!("{:?}", report.readiness.phase),
        "readiness_reasons": report.readiness.reasons.iter().map(|reason| format!("{reason:?}")).collect::<Vec<_>>(),
        "summary": {
            "cash_count": report.summary.cash_count,
            "positions_count": report.summary.positions_count,
            "orders_count": report.summary.orders_count,
            "active_orders_count": report.summary.active_orders_count,
            "terminal_orders_count": report.summary.terminal_orders_count,
            "blocking_unknown_status_present": report.summary.blocking_unknown_status_present,
        },
        "market_data": {
            "quote_published": true,
            "bars_published_count": published_bars_count,
            "timeframe_sec": timeframe_sec,
        }
    }))?;
    Ok(())
}

async fn run_gateway_redis_smoke(redis_url: String, stream: String) -> Result<()> {
    let mut gateway_config = GatewayConfig {
        features: GatewayFeatureSet::default(),
        ..GatewayConfig::default()
    };
    gateway_config.redis.url = redis_url.clone();
    gateway_config.redis.health_stream = stream.clone();

    let sink = match RedisConnectionStreamSink::connect(&redis_url).await {
        Ok(sink) => sink,
        Err(error) => {
            emit_redis_degraded_stderr("redis_smoke_connect", &error)?;
            return Err(error).context("Redis smoke connection failed");
        }
    };
    let gateway = FinamGateway::new(gateway_config, sink);
    if let Err(error) = gateway
        .publish_health(default_readonly_health(gateway.config()))
        .await
    {
        emit_redis_degraded_stderr("redis_smoke_publish", &error)?;
        return Err(error).context("Redis smoke publish failed");
    }

    let client = redis::Client::open(redis_url.as_str()).context("Redis smoke URL is invalid")?;
    let mut manager = match client.get_connection_manager().await {
        Ok(manager) => manager,
        Err(error) => {
            emit_redis_degraded_stderr("redis_smoke_read_connect", &error)?;
            return Err(error).context("Redis smoke read connection failed");
        }
    };
    let reply: StreamRangeReply = redis::cmd("XREVRANGE")
        .arg(&stream)
        .arg("+")
        .arg("-")
        .arg("COUNT")
        .arg(1)
        .query_async(&mut manager)
        .await
        .context("Redis smoke XRANGE/XREVRANGE failed")?;
    let latest = reply
        .ids
        .first()
        .context("Redis smoke stream did not contain the published event")?;
    let payload: String = latest
        .get("payload")
        .context("Redis smoke entry does not contain payload field")?;
    let envelope: serde_json::Value =
        serde_json::from_str(&payload).context("Redis smoke payload is not JSON")?;
    let schema_version = envelope
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .context("Redis smoke envelope is missing schema_version")?;
    let msg_type = envelope
        .get("msg_type")
        .and_then(serde_json::Value::as_str)
        .context("Redis smoke envelope is missing msg_type")?;
    anyhow::ensure!(schema_version == 2, "Redis smoke schema_version mismatch");
    anyhow::ensure!(msg_type == "Health", "Redis smoke msg_type mismatch");

    print_json(serde_json::json!({
        "redis_smoke": true,
        "live_trading_enabled": false,
        "stream": stream,
        "entry_id_present": !latest.id.is_empty(),
        "schema_version": schema_version,
        "msg_type": msg_type,
        "payload_len": payload.len(),
    }))?;
    Ok(())
}

fn read_gateway_shadow_file_config(path: Option<&PathBuf>) -> Result<GatewayShadowFileConfig> {
    let Some(path) = path else {
        return Ok(GatewayShadowFileConfig::default());
    };
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read gateway shadow config {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse gateway shadow config {}", path.display()))
}

fn resolve_gateway_shadow_config(
    args: GatewayShadowOnceArgs,
    file_config: GatewayShadowFileConfig,
) -> Result<ResolvedGatewayShadowConfig> {
    let mut gateway_config = GatewayConfig {
        features: GatewayFeatureSet::default(),
        ..GatewayConfig::default()
    };
    apply_gateway_shadow_file_config(&mut gateway_config, &file_config);
    if let Some(redis_url) = args.redis_url {
        gateway_config.redis.url = redis_url;
    }

    let now = Utc::now();
    let lookback_minutes = args.bars_lookback_minutes.max(1);
    let end_time = args
        .end_time
        .or(file_config.end_time)
        .unwrap_or_else(|| now.to_rfc3339());
    let start_time = args
        .start_time
        .or(file_config.start_time)
        .unwrap_or_else(|| (now - Duration::minutes(lookback_minutes)).to_rfc3339());

    Ok(ResolvedGatewayShadowConfig {
        secret_env: args.secret_env,
        gateway_config,
        account_id: args
            .account_id
            .or(file_config.account_id)
            .context("missing required FINAM account_id for shadow gateway")?,
        symbol: args
            .symbol
            .or(file_config.symbol)
            .context("missing required FINAM symbol for shadow gateway")?,
        timeframe: args
            .timeframe
            .or(file_config.timeframe)
            .unwrap_or_else(|| "TIME_FRAME_M1".to_string()),
        start_time,
        end_time,
    })
}

fn apply_gateway_shadow_file_config(
    gateway_config: &mut GatewayConfig,
    file_config: &GatewayShadowFileConfig,
) {
    if let Some(redis_url) = file_config.redis_url.as_deref() {
        gateway_config.redis.url = redis_url.to_string();
    }
    if let Some(source) = file_config.source.as_deref() {
        gateway_config.source = source.to_string();
    }
    if let Some(streams) = file_config.streams.as_ref() {
        apply_gateway_shadow_streams(&mut gateway_config.redis, streams);
    }
}

fn apply_gateway_shadow_streams(
    redis_config: &mut RedisStreamConfig,
    streams: &GatewayShadowStreamsFileConfig,
) {
    if let Some(value) = streams.health.as_deref() {
        redis_config.health_stream = value.to_string();
    }
    if let Some(value) = streams.readiness.as_deref() {
        redis_config.readiness_stream = value.to_string();
    }
    if let Some(value) = streams.portfolio.as_deref() {
        redis_config.portfolio_stream = value.to_string();
    }
    if let Some(value) = streams.orders_snapshot.as_deref() {
        redis_config.order_snapshot_stream = value.to_string();
    }
    if let Some(value) = streams.market_data.as_deref() {
        redis_config.market_data_stream = value.to_string();
    }
}

fn emit_redis_degraded_stderr(stage: &str, _error: &dyn std::error::Error) -> Result<()> {
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "gateway_shadow_degraded": true,
            "live_trading_enabled": false,
            "reason": "RedisUnavailable",
            "stage": stage,
            "error_present": true,
        }))?
    );
    Ok(())
}

fn emit_probe_result(
    records: &mut Vec<serde_json::Value>,
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
            "error_kind": error.kind(),
            "error": error.to_redacted_string(),
        }),
    };
    emit_record(records, payload)
}

fn emit_typed_result<T, F>(
    records: &mut Vec<serde_json::Value>,
    name: &str,
    result: std::result::Result<T, broker_finam::FinamError>,
    summarize: F,
) -> Result<()>
where
    F: FnOnce(T) -> Result<serde_json::Value>,
{
    let payload = match result {
        Ok(value) => match summarize(value) {
            Ok(summary) => serde_json::json!({
                "probe": name,
                "ok": true,
                "summary": summary,
            }),
            Err(error) => serde_json::json!({
                "probe": name,
                "ok": false,
                "error_kind": "mapper_error",
                "error": error.to_string(),
            }),
        },
        Err(error) => serde_json::json!({
            "probe": name,
            "ok": false,
            "error_kind": error.kind(),
            "error": error.to_redacted_string(),
        }),
    };
    emit_record(records, payload)
}

fn emit_record(records: &mut Vec<serde_json::Value>, value: serde_json::Value) -> Result<()> {
    print_json(value.clone())?;
    records.push(value);
    Ok(())
}

fn write_redacted_fixture(path: PathBuf, records: &[serde_json::Value]) -> Result<()> {
    write_records_fixture(path, "finam-readonly-redacted-v1", records)
}

fn write_records_fixture(
    path: PathBuf,
    fixture_kind: &str,
    records: &[serde_json::Value],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let payload = serde_json::json!({
        "fixture_kind": fixture_kind,
        "shape_max_depth": JSON_SHAPE_MAX_DEPTH,
        "records": records,
    });
    std::fs::write(path, serde_json::to_string_pretty(&payload)?)?;
    Ok(())
}

fn mapper_anyhow(error: FinamMapperError) -> anyhow::Error {
    anyhow::anyhow!(error.to_string())
}

fn timeframe_seconds(timeframe: &str) -> Result<u32> {
    match timeframe {
        "TIME_FRAME_M1" => Ok(60),
        "TIME_FRAME_M5" => Ok(5 * 60),
        "TIME_FRAME_M10" => Ok(10 * 60),
        "TIME_FRAME_M15" => Ok(15 * 60),
        "TIME_FRAME_M30" => Ok(30 * 60),
        "TIME_FRAME_H1" => Ok(60 * 60),
        "TIME_FRAME_D" => Ok(24 * 60 * 60),
        value => Err(anyhow::anyhow!(
            "unsupported FINAM timeframe for typed bar mapping: {value}"
        )),
    }
}

fn json_shape(value: &serde_json::Value) -> serde_json::Value {
    json_shape_at(value, 0)
}

fn json_shape_at(value: &serde_json::Value, depth: usize) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => {
            let keys = object
                .keys()
                .map(|key| redact_json_key_for_diagnostics(key))
                .collect::<Vec<_>>();
            if depth >= JSON_SHAPE_MAX_DEPTH {
                return serde_json::json!({
                    "kind": "object",
                    "keys": keys,
                    "truncated": true,
                });
            }

            let fields = object
                .iter()
                .map(|(key, value)| {
                    serde_json::json!({
                        "key": redact_json_key_for_diagnostics(key),
                        "shape": json_shape_at(value, depth + 1),
                    })
                })
                .collect::<Vec<_>>();

            serde_json::json!({
                "kind": "object",
                "keys": keys,
                "fields": fields,
            })
        }
        serde_json::Value::Array(items) => {
            let item_kinds = items
                .iter()
                .map(json_kind)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let first_item_shape = items.first().map(|item| json_shape_at(item, depth + 1));
            serde_json::json!({
                "kind": "array",
                "len": items.len(),
                "item_kinds": item_kinds,
                "first_item_shape": first_item_shape,
            })
        }
        serde_json::Value::String(_) => serde_json::json!({ "kind": "string" }),
        serde_json::Value::Number(_) => serde_json::json!({ "kind": "number" }),
        serde_json::Value::Bool(_) => serde_json::json!({ "kind": "bool" }),
        serde_json::Value::Null => serde_json::json!({ "kind": "null" }),
    }
}

fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Object(_) => "object",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Null => "null",
    }
}

fn print_json(value: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape_keeps_nested_structure_without_scalar_values() {
        let payload = serde_json::json!({
            "account_id": "ACC-SECRET",
            "orders": [
                {
                    "order_id": "ORDER-SECRET",
                    "status": "filled",
                    "price": 123.45,
                    "nested": {
                        "comment": "do-not-leak"
                    }
                }
            ]
        });

        let shape = json_shape(&payload);
        let rendered = serde_json::to_string(&shape).expect("shape serializes");

        assert!(rendered.contains("account_id"));
        assert!(rendered.contains("orders"));
        assert!(rendered.contains("order_id"));
        assert!(rendered.contains("status"));
        assert!(rendered.contains("price"));
        assert!(rendered.contains("nested"));
        assert!(!rendered.contains("ACC-SECRET"));
        assert!(!rendered.contains("ORDER-SECRET"));
        assert!(!rendered.contains("filled"));
        assert!(!rendered.contains("123.45"));
        assert!(!rendered.contains("do-not-leak"));
    }

    #[test]
    fn json_shape_truncates_deep_objects() {
        let payload = serde_json::json!({
            "l1": {
                "l2": {
                    "l3": {
                        "l4": {
                            "l5": {
                                "secret": "do-not-leak"
                            }
                        }
                    }
                }
            }
        });

        let shape = json_shape(&payload);
        let rendered = serde_json::to_string(&shape).expect("shape serializes");

        assert!(rendered.contains("truncated"));
        assert!(!rendered.contains("do-not-leak"));
    }

    #[test]
    fn json_shape_does_not_leak_dynamic_object_keys() {
        let payload = serde_json::json!({
            "7502T0U": {
                "status": "active"
            },
            "ORDER-123456": {
                "price": 123.45
            },
            "SBER@MISX": {
                "lot": 1
            },
            "account_id": "ACC-SECRET"
        });

        let shape = json_shape(&payload);
        let rendered = serde_json::to_string(&shape).expect("shape serializes");

        assert!(rendered.contains("dynamic"));
        assert!(rendered.contains("schema_field"));
        assert!(rendered.contains("sha256"));
        assert!(rendered.contains("account_id"));
        assert!(rendered.contains("status"));
        assert!(rendered.contains("price"));
        assert!(rendered.contains("lot"));
        assert!(!rendered.contains("7502T0U"));
        assert!(!rendered.contains("ORDER-123456"));
        assert!(!rendered.contains("SBER@MISX"));
        assert!(!rendered.contains("ACC-SECRET"));
        assert!(!rendered.contains("active"));
        assert!(!rendered.contains("123.45"));
    }

    #[test]
    fn timeframe_seconds_rejects_unknown_timeframe() {
        assert_eq!(timeframe_seconds("TIME_FRAME_M1").expect("m1"), 60);
        assert!(timeframe_seconds("TIME_FRAME_UNKNOWN").is_err());
    }

    #[test]
    fn gateway_shadow_config_keeps_live_order_features_disabled() {
        let resolved = resolve_gateway_shadow_config(
            GatewayShadowOnceArgs {
                config: None,
                secret_env: "FINAM_SECRET_TOKEN".to_string(),
                redis_url: Some("redis://127.0.0.1:6379/".to_string()),
                account_id: Some("ACC_TEST_0001".to_string()),
                symbol: Some("TICKER@MIC".to_string()),
                timeframe: None,
                start_time: Some("2026-06-29T09:00:00Z".to_string()),
                end_time: Some("2026-06-29T09:10:00Z".to_string()),
                bars_lookback_minutes: 60,
            },
            GatewayShadowFileConfig::default(),
        )
        .expect("resolved config");

        assert!(!resolved.gateway_config.features.command_consumer_enabled);
        assert!(!resolved.gateway_config.features.order_placement_enabled);
        assert!(!resolved.gateway_config.features.cancel_enabled);
        assert!(!resolved.gateway_config.features.stop_sltp_bracket_enabled);
        assert_eq!(resolved.timeframe, "TIME_FRAME_M1");
    }

    #[test]
    fn gateway_shadow_config_accepts_stream_name_overrides() {
        let resolved = resolve_gateway_shadow_config(
            GatewayShadowOnceArgs {
                config: None,
                secret_env: "FINAM_SECRET_TOKEN".to_string(),
                redis_url: None,
                account_id: Some("ACC_TEST_0001".to_string()),
                symbol: Some("TICKER@MIC".to_string()),
                timeframe: Some("TIME_FRAME_M10".to_string()),
                start_time: Some("2026-06-29T09:00:00Z".to_string()),
                end_time: Some("2026-06-29T09:10:00Z".to_string()),
                bars_lookback_minutes: 60,
            },
            GatewayShadowFileConfig {
                redis_url: Some("redis://127.0.0.1:6379/".to_string()),
                source: Some("finam-gateway-test".to_string()),
                streams: Some(GatewayShadowStreamsFileConfig {
                    health: Some("broker.health".to_string()),
                    readiness: Some("broker.readiness".to_string()),
                    portfolio: Some("broker.portfolio.snapshot".to_string()),
                    orders_snapshot: Some("broker.orders.snapshot".to_string()),
                    market_data: Some("broker.market_data".to_string()),
                }),
                ..GatewayShadowFileConfig::default()
            },
        )
        .expect("resolved config");

        assert_eq!(resolved.gateway_config.source, "finam-gateway-test");
        assert_eq!(resolved.gateway_config.redis.health_stream, "broker.health");
        assert_eq!(
            resolved.gateway_config.redis.readiness_stream,
            "broker.readiness"
        );
        assert_eq!(
            resolved.gateway_config.redis.portfolio_stream,
            "broker.portfolio.snapshot"
        );
        assert_eq!(
            resolved.gateway_config.redis.order_snapshot_stream,
            "broker.orders.snapshot"
        );
        assert_eq!(
            resolved.gateway_config.redis.market_data_stream,
            "broker.market_data"
        );
        assert_eq!(resolved.timeframe, "TIME_FRAME_M10");
    }
}
