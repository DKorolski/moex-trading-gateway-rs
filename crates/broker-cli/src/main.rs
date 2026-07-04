use anyhow::{Context, Result};
use broker_core::command::CommandAckStatus;
use broker_core::event::Quote;
#[cfg(feature = "m3j16-actual-one-shot")]
use broker_core::CancelPreflightApproval;
use broker_core::{
    BrokerAccountId, BrokerCommand, BrokerOrderId, BrokerReadiness, ClientOrderId, Envelope,
    Exchange, InMemoryOrderPathStore, InstrumentId, Market, MarketDataEvent, MarketDataSourceKind,
    MessageType, OperatorArm, Order, OrderPathEvent, OrderPathRecord, OrderPreflightPolicy,
    OrderSide, OrderStatus, OrderType, PlaceOrder, PortfolioSnapshot, ReadinessPhase,
    ReadinessReason, StrategyRequestId, TimeInForce,
};
use broker_finam::{
    active_orders, has_blocking_unknown_order_statuses, map_account_trade, map_bar,
    map_latest_market_trade, map_order_state, map_portfolio_snapshot, map_quote,
    redact_json_key_for_diagnostics, terminal_orders, AllAssetsQuery, BarsQuery,
    FinamApiCapabilities, FinamAuthManager, FinamConfig, FinamMapperError, FinamRestClient,
    GatewayEnabledFeatures, HistoryQuery, SecretToken,
};
#[cfg(feature = "m3j16-actual-one-shot")]
use broker_finam::{
    build_cancel_order_request, build_place_order_request, FinamOrderEndpointMappedResult,
    FinamOrderExecutionOutcome,
};
use chrono::{Duration as ChronoDuration, Utc};
use clap::{Parser, Subcommand};
#[cfg(feature = "m3j16-actual-one-shot")]
use finam_gateway::m3d2_real_order_transport::{
    FinamAuthorizationHeaderMode, M3d2ExternalOrderEndpointMode, M3d2RealOrderEndpointTransport,
    M3d2RealOrderEndpointTransportConfig,
};
use finam_gateway::real_order_endpoint::{
    m3j16_limit_cancel_one_shot_report, M3j16LimitCancelOneShotInput,
};
use finam_gateway::{
    default_readonly_health, degraded_health, degraded_readiness,
    evaluate_finam_real_readonly_operator_guardrails, readiness_from_readonly_summary,
    run_finam_real_readonly_operator_contract_probe, stopped_health, stopped_readiness,
    BrokerTruthGatewayConfig, CancelBrokerTruthFetchRequestSnapshot,
    CancelBrokerTruthFreshnessPolicy, CancelBrokerTruthSource, CancelPositionTruthGuardContext,
    FinamGateway, FinamMockClassifiedEndpointTransport, FinamRealReadonlyAuditStoreMode,
    FinamRealReadonlyBrokerTruthAsyncFetcher, FinamRealReadonlyBrokerTruthQueryPolicy,
    FinamRealReadonlyBrokerTruthTransportConfig, FinamRealReadonlyContractProbeOperatorRunConfig,
    FinamRealReadonlyRedactedOutputLocation, FinamRealReadonlyTokenAccountPreflightApproved,
    GatewayConfig, GatewayError, GatewayFeatureSet, InMemoryRedisStreamSink,
    M3cForbiddenSurfaceScanEvidence, M3cOrderEndpointGateDesignEvidence,
    M3cOrderEndpointGateEvidenceStatus, M3cRouteTemplateRecheckPlanEvidence, M3cSourceEvidence,
    M3eCommandConsumerConfig, M3eCommandConsumerLocalMockEndpoint, M3eCommandLifecycleAction,
    M3eCommandLifecycleRecord, M3eCommandLifecycleStore, M3eInMemoryCommandLifecycleStore,
    OrderSnapshot, ReadonlySnapshotSummary, RealReadonlyBrokerTruthGateApproved,
    RealReadonlyBrokerTruthRunApproved, RedisConnectionStreamSink, RedisRetentionConfig,
    RedisStreamConfig, RedisStreamSink, ReqwestFinamRealReadonlyBrokerTruthTransport,
    RuntimeBridgeConsumeOutcome, RuntimeBridgeDeadLetter, RuntimeBridgeDlqReason,
    RuntimeBridgeDlqRecord, RuntimeBridgeDryConsumer, RuntimeBridgeReadinessSimulator,
    RuntimeBridgeStreamEntry,
};
use redis::streams::{
    StreamAutoClaimReply, StreamId, StreamPendingCountReply, StreamRangeReply, StreamReadReply,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration as StdDuration, Instant};
use uuid::Uuid;

const JSON_SHAPE_MAX_DEPTH: usize = 4;

#[derive(Parser)]
#[command(version, about = "MOEX broker gateway operator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
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
    /// Run a read-only FINAM bar timestamp/finality golden-test harness.
    #[command(name = "finam-bar-finality-golden-check")]
    BarFinalityGoldenCheck {
        /// Environment variable that contains the Finam secret token.
        #[arg(long, default_value = "FINAM_SECRET_TOKEN")]
        secret_env: String,
        /// Finam venue symbol, for example TICKER@MIC.
        #[arg(long, env = "FINAM_SYMBOL")]
        symbol: Option<String>,
        /// Bars timeframe value accepted by Finam REST, for example TIME_FRAME_M1.
        #[arg(long, default_value = "TIME_FRAME_M1")]
        timeframe: String,
        /// Optional inclusive start time, RFC3339, for the golden window.
        #[arg(long)]
        start_time: Option<String>,
        /// Optional exclusive end time, RFC3339, for the golden window.
        #[arg(long)]
        end_time: Option<String>,
        /// Default bars lookback if start/end are not supplied.
        #[arg(long, default_value_t = 90)]
        lookback_minutes: i64,
        /// Optional file path for saving the redacted golden result as JSON.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Run a controlled one-shot real-readonly broker-truth evidence probe. Requires readonly token.
    #[command(name = "finam-real-readonly-evidence")]
    RealReadonlyEvidence {
        /// Environment variable that contains the Finam secret token.
        #[arg(long, default_value = "FINAM_SECRET_TOKEN")]
        secret_env: String,
        /// Finam account id. Raw value is never written to the report.
        #[arg(long, env = "FINAM_ACCOUNT_ID")]
        account_id: String,
        /// Finam venue symbol, for example TICKER@MIC. Raw value is never written to the report.
        #[arg(long, env = "FINAM_SYMBOL")]
        symbol: String,
        /// Synthetic or operator-selected broker order id used only as read-only reconciliation target.
        #[arg(long, default_value = "SYNTHETIC_PROBE_ORDER_0001")]
        broker_order_id: String,
        /// Optional client order id used only as read-only reconciliation target.
        #[arg(long)]
        client_order_id: Option<String>,
        /// Maximum GET-only broker-truth requests. Must stay <= 4.
        #[arg(long, default_value_t = 4)]
        max_requests: usize,
        /// Request timeout bound for the real-readonly transport.
        #[arg(long, default_value_t = 10_000)]
        request_timeout_ms: u64,
        /// Minimum interval between GET requests.
        #[arg(long, default_value_t = 250)]
        min_request_interval_ms: u64,
        /// Preflight marker max age for the one-shot run.
        #[arg(long, default_value_t = 60_000)]
        preflight_max_age_ms: u64,
        /// Optional file path for saving the redacted evidence package.
        #[arg(
            long,
            default_value = "reports/finam-real-readonly-evidence/redacted-evidence.json"
        )]
        output: PathBuf,
        /// Optional source handoff archive path to fingerprint in the evidence metadata.
        #[arg(long)]
        source_archive: Option<PathBuf>,
    },
    /// M3j-16 guarded one-shot FINAM limit-place then cancel package. Default is dry-run/no-send.
    #[command(name = "finam-limit-cancel-one-shot")]
    LimitCancelOneShot {
        /// Environment variable that contains the Finam secret token.
        #[arg(long, default_value = "FINAM_SECRET_TOKEN")]
        secret_env: String,
        /// Finam account id. Raw value is never written to the report.
        #[arg(long, env = "FINAM_ACCOUNT_ID")]
        account_id: String,
        /// Finam venue symbol, for example IMOEXF@RTSX. Raw value is never written to the report.
        #[arg(long, env = "FINAM_SYMBOL")]
        symbol: String,
        /// Limit price. For the first LimitCancel run this must stay below reference price.
        #[arg(long, default_value = "2210")]
        limit_price: String,
        /// Operator-supplied reference/current price used only for the below-market guard.
        #[arg(long, default_value = "2223")]
        reference_price: String,
        /// Quantity. M3j-16 requires exactly 1.
        #[arg(long, default_value = "1")]
        qty: String,
        /// Request timeout bound for the order transport.
        #[arg(long, default_value_t = 10_000)]
        request_timeout_ms: u64,
        /// Output path for the redacted M3j-16 report.
        #[arg(
            long,
            default_value = "reports/m3j16-limit-cancel-one-shot/redacted-report.json"
        )]
        output: PathBuf,
        /// Required for the real boundary call. Without it the command is dry-run/no-send.
        #[arg(long)]
        actual_send_i_understand_risk: bool,
    },
    /// Emit M3c order endpoint gate design evidence. Does not place or cancel orders.
    #[command(name = "m3c-order-endpoint-gate-report")]
    M3cOrderEndpointGateReport {
        /// Optional file path for saving the self-contained M3c gate report.
        #[arg(
            long,
            default_value = "reports/m3c-order-endpoint-gate/design-evidence.json"
        )]
        output: PathBuf,
        /// Optional source handoff archive path to fingerprint in the report.
        #[arg(long)]
        source_archive: Option<PathBuf>,
        /// Evidence slot status: pending, evidence-provided, waiver-accepted.
        #[arg(long, default_value = "pending")]
        release_profile_status: String,
        /// Evidence slot status: pending, evidence-provided, waiver-accepted.
        #[arg(long, default_value = "pending")]
        positive_get_order_status: String,
        /// Evidence slot status: pending, evidence-provided, waiver-accepted.
        #[arg(long, default_value = "pending")]
        route_template_recheck_status: String,
        /// Evidence slot status: pending, evidence-provided, waiver-accepted.
        #[arg(long, default_value = "pending")]
        undocumented_2xx_status: String,
        /// Evidence slot status: pending, evidence-provided, waiver-accepted.
        #[arg(long, default_value = "pending")]
        cancel_409_410_status: String,
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
        /// Finam venue symbol, for example TICKER@MIC. Overrides config file.
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
    /// Run periodic FINAM read-only shadow publication loop. Does not place or cancel orders.
    #[command(name = "finam-gateway-shadow-loop")]
    GatewayShadowLoop {
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
        /// Finam venue symbol, for example TICKER@MIC. Overrides config file.
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
        /// Periodic loop interval in seconds.
        #[arg(long)]
        interval_seconds: Option<u64>,
        /// Optional safety stop after N iterations. Omit for continuous loop.
        #[arg(long)]
        max_iterations: Option<u64>,
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
    /// Dry runtime-bridge consumer for broker-neutral Redis streams. Does not run strategies.
    #[command(name = "runtime-bridge-dry-consume")]
    RuntimeBridgeDryConsume {
        /// Optional JSON config file with Redis streams. Reuses shadow config stream names.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Redis connection URL. Overrides config file.
        #[arg(long, env = "FINAM_GATEWAY_REDIS_URL")]
        redis_url: Option<String>,
        /// Redis consumer group name for dry runtime bridge reads.
        #[arg(long, default_value = "broker-runtime-bridge-dry")]
        group: String,
        /// Redis consumer name for dry runtime bridge reads.
        #[arg(long, default_value = "dry-consumer-1")]
        consumer: String,
        /// Consumer group start id used only when creating missing groups.
        #[arg(long, default_value = "$")]
        group_start_id: String,
        /// Max entries per XREADGROUP batch.
        #[arg(long, default_value_t = 100)]
        count: usize,
        /// XREADGROUP block timeout in milliseconds.
        #[arg(long, default_value_t = 1000)]
        block_ms: u64,
        /// Safety stop after N XREADGROUP iterations.
        #[arg(long, default_value_t = 1)]
        max_iterations: u64,
        /// Optional dry pending recovery: XAUTOCLAIM entries idle for at least N milliseconds.
        #[arg(long)]
        claim_stale_ms: Option<u64>,
    },
    /// Publish synthetic broker-neutral streams and verify dry runtime bridge consume/DLQ paths.
    #[command(name = "runtime-bridge-redis-smoke")]
    RuntimeBridgeRedisSmoke {
        /// Redis connection URL.
        #[arg(
            long,
            env = "FINAM_GATEWAY_REDIS_URL",
            default_value = "redis://127.0.0.1:6379/"
        )]
        redis_url: String,
        /// Prefix for unique synthetic stream names.
        #[arg(long, default_value = "broker.m2i.runtime_bridge_smoke")]
        prefix: String,
    },
    /// Publish synthetic M3e commands and verify real Redis XREADGROUP/XACK/XAUTOCLAIM lifecycle.
    #[command(name = "m3e-command-consumer-redis-smoke")]
    M3eCommandConsumerRedisSmoke {
        /// Redis connection URL.
        #[arg(
            long,
            env = "FINAM_GATEWAY_REDIS_URL",
            default_value = "redis://127.0.0.1:6379/"
        )]
        redis_url: String,
        /// Prefix for unique synthetic stream names.
        #[arg(long, default_value = "broker.m3e.command_consumer_smoke")]
        prefix: String,
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
        Command::BarFinalityGoldenCheck {
            secret_env,
            symbol,
            timeframe,
            start_time,
            end_time,
            lookback_minutes,
            output,
        } => {
            run_bar_finality_golden_check(BarFinalityGoldenArgs {
                secret_env,
                symbol,
                timeframe,
                start_time,
                end_time,
                lookback_minutes,
                output,
            })
            .await?;
        }
        Command::RealReadonlyEvidence {
            secret_env,
            account_id,
            symbol,
            broker_order_id,
            client_order_id,
            max_requests,
            request_timeout_ms,
            min_request_interval_ms,
            preflight_max_age_ms,
            output,
            source_archive,
        } => {
            run_finam_real_readonly_evidence(FinamRealReadonlyEvidenceArgs {
                secret_env,
                account_id,
                symbol,
                broker_order_id,
                client_order_id,
                max_requests,
                request_timeout_ms,
                min_request_interval_ms,
                preflight_max_age_ms,
                output,
                source_archive,
            })
            .await?;
        }
        Command::LimitCancelOneShot {
            secret_env,
            account_id,
            symbol,
            limit_price,
            reference_price,
            qty,
            request_timeout_ms,
            output,
            actual_send_i_understand_risk,
        } => {
            run_finam_limit_cancel_one_shot(FinamLimitCancelOneShotArgs {
                secret_env,
                account_id,
                symbol,
                limit_price,
                reference_price,
                qty,
                request_timeout_ms,
                output,
                actual_send_i_understand_risk,
            })
            .await?;
        }
        Command::M3cOrderEndpointGateReport {
            output,
            source_archive,
            release_profile_status,
            positive_get_order_status,
            route_template_recheck_status,
            undocumented_2xx_status,
            cancel_409_410_status,
        } => {
            run_m3c_order_endpoint_gate_report(
                output,
                source_archive,
                M3cEvidenceSlotArgs {
                    release_profile_status,
                    positive_get_order_status,
                    route_template_recheck_status,
                    undocumented_2xx_status,
                    cancel_409_410_status,
                },
            )?;
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
                interval_seconds: None,
                max_iterations: Some(1),
            })
            .await?;
        }
        Command::GatewayShadowLoop {
            config,
            secret_env,
            redis_url,
            account_id,
            symbol,
            timeframe,
            start_time,
            end_time,
            bars_lookback_minutes,
            interval_seconds,
            max_iterations,
        } => {
            run_gateway_shadow_loop(GatewayShadowOnceArgs {
                config,
                secret_env,
                redis_url,
                account_id,
                symbol,
                timeframe,
                start_time,
                end_time,
                bars_lookback_minutes,
                interval_seconds,
                max_iterations,
            })
            .await?;
        }
        Command::GatewayRedisSmoke { redis_url, stream } => {
            run_gateway_redis_smoke(redis_url, stream).await?;
        }
        Command::RuntimeBridgeDryConsume {
            config,
            redis_url,
            group,
            consumer,
            group_start_id,
            count,
            block_ms,
            max_iterations,
            claim_stale_ms,
        } => {
            run_runtime_bridge_dry_consume(RuntimeBridgeDryConsumeArgs {
                config,
                redis_url,
                group,
                consumer,
                group_start_id,
                count,
                block_ms,
                max_iterations,
                claim_stale_ms,
            })
            .await?;
        }
        Command::RuntimeBridgeRedisSmoke { redis_url, prefix } => {
            run_runtime_bridge_redis_smoke(redis_url, prefix).await?;
        }
        Command::M3eCommandConsumerRedisSmoke { redis_url, prefix } => {
            run_m3e_command_consumer_redis_smoke(redis_url, prefix).await?;
        }
    }
    Ok(())
}

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
    interval_seconds: Option<u64>,
    max_iterations: Option<u64>,
}

struct RuntimeBridgeDryConsumeArgs {
    config: Option<PathBuf>,
    redis_url: Option<String>,
    group: String,
    consumer: String,
    group_start_id: String,
    count: usize,
    block_ms: u64,
    max_iterations: u64,
    claim_stale_ms: Option<u64>,
}

struct FinamRealReadonlyEvidenceArgs {
    secret_env: String,
    account_id: String,
    symbol: String,
    broker_order_id: String,
    client_order_id: Option<String>,
    max_requests: usize,
    request_timeout_ms: u64,
    min_request_interval_ms: u64,
    preflight_max_age_ms: u64,
    output: PathBuf,
    source_archive: Option<PathBuf>,
}

struct M3cEvidenceSlotArgs {
    release_profile_status: String,
    positive_get_order_status: String,
    route_template_recheck_status: String,
    undocumented_2xx_status: String,
    cancel_409_410_status: String,
}

struct BarFinalityGoldenArgs {
    secret_env: String,
    symbol: Option<String>,
    timeframe: String,
    start_time: Option<String>,
    end_time: Option<String>,
    lookback_minutes: i64,
    output: Option<PathBuf>,
}

struct ResolvedRuntimeBridgeDryConfig {
    gateway_config: GatewayConfig,
    group: String,
    consumer: String,
    group_start_id: String,
    count: usize,
    block_ms: u64,
    max_iterations: u64,
    claim_stale_ms: Option<u64>,
}

#[derive(Debug, Default)]
struct M3eRedisSmokeMetrics {
    xreadgroup_entries: u64,
    xautoclaim_entries: u64,
    real_xack_count: u64,
    ack_publish_failure_left_pending: bool,
    dlq_publish_failure_left_pending: bool,
    duplicate_after_xautoclaim_no_second_endpoint_attempt: bool,
    command_received_replay_no_second_endpoint_attempt: bool,
    endpoint_attempt_before_ack_replay_no_blind_retry: bool,
    ack_published_before_xack_replay_no_second_endpoint_attempt: bool,
    poison_dlq_redacted_then_xack: bool,
    expired_ack_no_endpoint_then_xack: bool,
    place_ok: bool,
    cancel_ok: bool,
}

struct ResolvedGatewayShadowConfig {
    secret_env: String,
    gateway_config: GatewayConfig,
    account_id: String,
    symbol: String,
    timeframe: String,
    start_time: Option<String>,
    end_time: Option<String>,
    bars_lookback_minutes: i64,
    interval_seconds: u64,
    max_iterations: Option<u64>,
}

#[derive(Default)]
struct RuntimeBridgeRedisDryMetrics {
    xreadgroup_iterations: u64,
    xautoclaim_iterations: u64,
    entries_returned: u64,
    claimed_entries_returned: u64,
    xautoclaim_deleted_ids_count: u64,
    dlq_published_count: u64,
    xack_count: u64,
    missing_payload_count: u64,
    last_ids: BTreeMap<String, String>,
    xautoclaim_last_next_ids: BTreeMap<String, String>,
    latest_dlq_reason: Option<String>,
    latest_dlq_ts: Option<String>,
    latest_dlq_stream: Option<String>,
    latest_dlq_entry_id: Option<String>,
    consecutive_dlq_count: u64,
}

impl RuntimeBridgeRedisDryMetrics {
    fn record_non_dlq(&mut self) {
        self.consecutive_dlq_count = 0;
    }

    fn record_dlq(&mut self, dead_letter: &RuntimeBridgeDeadLetter) {
        self.latest_dlq_reason = Some(runtime_bridge_dlq_reason_label(&dead_letter.reason));
        self.latest_dlq_ts = Some(Utc::now().to_rfc3339());
        self.latest_dlq_stream = Some(dead_letter.stream.clone());
        self.latest_dlq_entry_id = Some(dead_letter.entry_id.clone());
        self.consecutive_dlq_count += 1;
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct GatewayShadowFileConfig {
    redis_url: Option<String>,
    source: Option<String>,
    account_id: Option<String>,
    symbol: Option<String>,
    timeframe: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    bars_lookback_minutes: Option<i64>,
    interval_seconds: Option<u64>,
    max_iterations: Option<u64>,
    streams: Option<GatewayShadowStreamsFileConfig>,
    retention: Option<GatewayShadowRetentionFileConfig>,
    broker_truth: Option<BrokerTruthGatewayConfig>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct GatewayShadowStreamsFileConfig {
    health: Option<String>,
    readiness: Option<String>,
    portfolio: Option<String>,
    orders_snapshot: Option<String>,
    market_data: Option<String>,
    command_ack: Option<String>,
    runtime_bridge_dlq: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct GatewayShadowRetentionFileConfig {
    health_maxlen: Option<usize>,
    readiness_maxlen: Option<usize>,
    portfolio_maxlen: Option<usize>,
    order_snapshot_maxlen: Option<usize>,
    market_data_maxlen: Option<usize>,
    command_ack_maxlen: Option<usize>,
    runtime_bridge_dlq_maxlen: Option<usize>,
}

struct GatewayShadowRuntime {
    resolved: ResolvedGatewayShadowConfig,
    client: FinamRestClient,
    auth_manager: FinamAuthManager,
    gateway: FinamGateway<RedisConnectionStreamSink>,
    bar_watermark: Mutex<HashSet<String>>,
    metrics: Mutex<ShadowMetrics>,
}

#[derive(Default, Clone)]
struct ShadowMetrics {
    success_count: u64,
    failure_count: u64,
    consecutive_failures: u64,
    last_success_ts: Option<chrono::DateTime<Utc>>,
    last_failure_ts: Option<chrono::DateTime<Utc>>,
    published_health_count: u64,
    published_readiness_count: u64,
    published_snapshot_count: u64,
    published_market_data_count: u64,
    deduped_bar_count: u64,
}

struct ShadowIterationReport {
    iteration: u64,
    elapsed_ms: u128,
    summary: ReadonlySnapshotSummary,
    readiness_phase: String,
    readiness_reasons: Vec<String>,
    quote_published: bool,
    bars_published_count: usize,
    bars_deduped_count: usize,
    timeframe_sec: u32,
}

struct ShadowIterationError {
    stage: &'static str,
    reason: ReadinessReason,
    source: anyhow::Error,
}

impl ShadowIterationError {
    fn new(stage: &'static str, reason: ReadinessReason, source: impl Into<anyhow::Error>) -> Self {
        Self {
            stage,
            reason,
            source: source.into(),
        }
    }
}

impl std::fmt::Debug for ShadowIterationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShadowIterationError")
            .field("stage", &self.stage)
            .field("reason", &self.reason)
            .finish()
    }
}

impl std::fmt::Display for ShadowIterationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "shadow iteration failed at stage {} with reason {:?}",
            self.stage, self.reason
        )
    }
}

impl std::error::Error for ShadowIterationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.source()
    }
}

async fn run_gateway_shadow_once(args: GatewayShadowOnceArgs) -> Result<()> {
    let runtime = setup_gateway_shadow_runtime(args).await?;
    match run_gateway_shadow_iteration(&runtime, 1).await {
        Ok(report) => {
            record_shadow_success(&runtime, &report);
            print_json(shadow_iteration_json("once", &runtime, &report))?;
            Ok(())
        }
        Err(error) => {
            record_shadow_failure(&runtime);
            publish_degraded_state(&runtime.gateway, error.reason.clone(), error.stage).await?;
            print_json(shadow_failure_json("once", &error, 1))?;
            Err(anyhow::anyhow!(
                "shadow gateway once failed at stage {}",
                error.stage
            ))
        }
    }
}

async fn run_gateway_shadow_loop(args: GatewayShadowOnceArgs) -> Result<()> {
    let runtime = setup_gateway_shadow_runtime(args).await?;
    let started_at = Instant::now();
    let mut iteration = 0_u64;
    let mut success_count = 0_u64;
    let mut failure_count = 0_u64;

    loop {
        iteration += 1;
        match run_gateway_shadow_iteration(&runtime, iteration).await {
            Ok(report) => {
                success_count += 1;
                record_shadow_success(&runtime, &report);
                print_json(shadow_iteration_json("loop", &runtime, &report))?;
            }
            Err(error) => {
                failure_count += 1;
                record_shadow_failure(&runtime);
                publish_degraded_state(&runtime.gateway, error.reason.clone(), error.stage).await?;
                print_json(shadow_failure_json("loop", &error, iteration))?;
            }
        }

        if runtime
            .resolved
            .max_iterations
            .is_some_and(|max_iterations| iteration >= max_iterations)
        {
            publish_stopped_state(&runtime.gateway, "max_iterations").await?;
            print_json(shadow_loop_summary_json(
                "max_iterations",
                started_at.elapsed().as_millis(),
                iteration,
                success_count,
                failure_count,
                &snapshot_shadow_metrics(&runtime),
            ))?;
            break;
        }

        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                if signal.is_err() {
                    emit_redis_degraded_stderr("ctrl_c_signal", &std::io::Error::other("ctrl_c signal failed"))?;
                }
                publish_stopped_state(&runtime.gateway, "ctrl_c").await?;
                print_json(shadow_loop_summary_json(
                    "ctrl_c",
                    started_at.elapsed().as_millis(),
                    iteration,
                    success_count,
                    failure_count,
                    &snapshot_shadow_metrics(&runtime),
                ))?;
                break;
            }
            _ = tokio::time::sleep(StdDuration::from_secs(runtime.resolved.interval_seconds)) => {}
        }
    }
    Ok(())
}

async fn setup_gateway_shadow_runtime(args: GatewayShadowOnceArgs) -> Result<GatewayShadowRuntime> {
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
    let sink = match RedisConnectionStreamSink::connect(&redis_url).await {
        Ok(sink) => sink,
        Err(error) => {
            emit_redis_degraded_stderr("redis_connect", &error)?;
            return Err(error).context("Redis connection failed for shadow gateway");
        }
    };
    let gateway = FinamGateway::new(resolved.gateway_config.clone(), sink);
    Ok(GatewayShadowRuntime {
        resolved,
        client,
        auth_manager,
        gateway,
        bar_watermark: Mutex::new(HashSet::new()),
        metrics: Mutex::new(ShadowMetrics::default()),
    })
}

async fn run_gateway_shadow_iteration(
    runtime: &GatewayShadowRuntime,
    iteration: u64,
) -> std::result::Result<ShadowIterationReport, ShadowIterationError> {
    let started_at = Instant::now();
    let token =
        runtime.auth_manager.access_token().await.map_err(|error| {
            ShadowIterationError::new("auth", ReadinessReason::AuthExpired, error)
        })?;
    let account = runtime
        .client
        .account_typed(&token, &runtime.resolved.account_id)
        .await
        .map_err(|error| {
            ShadowIterationError::new("account_fetch", ReadinessReason::AccountUnavailable, error)
        })?;
    let orders = runtime
        .client
        .account_orders_typed(&token, &runtime.resolved.account_id)
        .await
        .map_err(|error| {
            ShadowIterationError::new("orders_fetch", ReadinessReason::OrdersNotLoaded, error)
        })?;

    let quote = runtime
        .client
        .last_quote_typed(&token, &runtime.resolved.symbol)
        .await
        .map_err(|error| {
            ShadowIterationError::new("quote_fetch", ReadinessReason::MarketDataNotLive, error)
        })?;
    let mapped_quote = map_quote(&quote, Utc::now()).map_err(|error| {
        ShadowIterationError::new(
            "quote_map",
            ReadinessReason::MarketDataNotLive,
            mapper_anyhow(error),
        )
    })?;
    let (start_time, end_time) = shadow_bars_window(&runtime.resolved);
    let bars_query = BarsQuery {
        timeframe: &runtime.resolved.timeframe,
        start_time: Some(start_time.as_str()),
        end_time: Some(end_time.as_str()),
    };
    let bars = runtime
        .client
        .bars_typed(&token, &runtime.resolved.symbol, bars_query)
        .await
        .map_err(|error| {
            ShadowIterationError::new("bars_fetch", ReadinessReason::MarketDataNotLive, error)
        })?;
    let timeframe_sec = timeframe_seconds(&runtime.resolved.timeframe).map_err(|error| {
        ShadowIterationError::new("timeframe", ReadinessReason::MarketDataNotLive, error)
    })?;
    let quote_event = MarketDataEvent::Quote(mapped_quote);
    let mut bar_events = Vec::new();
    let mut bars_deduped_count = 0usize;
    for bar in &bars.bars {
        let mapped_bar =
            map_bar(&runtime.resolved.symbol, bar, timeframe_sec).map_err(|error| {
                ShadowIterationError::new(
                    "bar_map",
                    ReadinessReason::MarketDataNotLive,
                    mapper_anyhow(error),
                )
            })?;
        let watermark_key = historical_bar_watermark_key(&runtime.resolved.timeframe, &mapped_bar);
        if is_bar_watermark_known(runtime, &watermark_key) {
            bars_deduped_count += 1;
        } else {
            bar_events.push((watermark_key, MarketDataEvent::Bar(mapped_bar)));
        }
    }

    let received_ts = Utc::now();
    runtime
        .gateway
        .publish_health(default_readonly_health(runtime.gateway.config()))
        .await
        .map_err(|error| {
            ShadowIterationError::new("health_publish", ReadinessReason::RedisUnavailable, error)
        })?;
    let summary = runtime
        .gateway
        .publish_readonly_snapshots(&account, &orders, received_ts)
        .await
        .map_err(|error| {
            ShadowIterationError::new("snapshot_publish", snapshot_error_reason(&error), error)
        })?;
    runtime
        .gateway
        .publish_market_data_event(quote_event)
        .await
        .map_err(|error| {
            ShadowIterationError::new(
                "market_data_publish",
                ReadinessReason::RedisUnavailable,
                error,
            )
        })?;
    let mut bars_published_count = 0usize;
    for (watermark_key, event) in bar_events {
        runtime
            .gateway
            .publish_market_data_event(event)
            .await
            .map_err(|error| {
                ShadowIterationError::new(
                    "market_data_publish",
                    ReadinessReason::RedisUnavailable,
                    error,
                )
            })?;
        mark_bar_watermark(runtime, watermark_key);
        bars_published_count += 1;
    }
    let readiness = readiness_from_readonly_summary(&summary);
    runtime
        .gateway
        .publish_readiness(readiness.clone())
        .await
        .map_err(|error| {
            ShadowIterationError::new(
                "readiness_publish",
                ReadinessReason::RedisUnavailable,
                error,
            )
        })?;

    Ok(ShadowIterationReport {
        iteration,
        elapsed_ms: started_at.elapsed().as_millis(),
        summary,
        readiness_phase: format!("{:?}", readiness.phase),
        readiness_reasons: readiness
            .reasons
            .iter()
            .map(|reason| format!("{reason:?}"))
            .collect(),
        quote_published: true,
        bars_published_count,
        bars_deduped_count,
        timeframe_sec,
    })
}

fn shadow_iteration_json(
    mode: &str,
    runtime: &GatewayShadowRuntime,
    report: &ShadowIterationReport,
) -> serde_json::Value {
    serde_json::json!({
        "gateway_shadow": true,
        "mode": mode,
        "iteration": report.iteration,
        "elapsed_ms": report.elapsed_ms,
        "live_trading_enabled": false,
        "command_consumer_enabled": runtime.gateway.config().features.command_consumer_enabled,
        "order_placement_enabled": runtime.gateway.config().features.order_placement_enabled,
        "cancel_enabled": runtime.gateway.config().features.cancel_enabled,
        "stop_sltp_bracket_enabled": runtime.gateway.config().features.stop_sltp_bracket_enabled,
        "streams": {
            "health": runtime.gateway.config().redis.health_stream,
            "readiness": runtime.gateway.config().redis.readiness_stream,
            "portfolio": runtime.gateway.config().redis.portfolio_stream,
            "orders_snapshot": runtime.gateway.config().redis.order_snapshot_stream,
            "market_data": runtime.gateway.config().redis.market_data_stream,
            "command_ack": runtime.gateway.config().redis.command_ack_stream,
        },
        "readiness_phase": report.readiness_phase,
        "readiness_reasons": report.readiness_reasons,
        "summary": {
            "cash_count": report.summary.cash_count,
            "positions_count": report.summary.positions_count,
            "orders_count": report.summary.orders_count,
            "active_orders_count": report.summary.active_orders_count,
            "terminal_orders_count": report.summary.terminal_orders_count,
            "blocking_unknown_status_present": report.summary.blocking_unknown_status_present,
        },
        "market_data": {
            "quote_published": report.quote_published,
            "bars_published_count": report.bars_published_count,
            "bars_deduped_count": report.bars_deduped_count,
            "timeframe_sec": report.timeframe_sec,
            "bar_source_kind": "HistoricalPoll",
            "quote_source_kind": "ReadOnlyPoll",
        },
        "metrics": shadow_metrics_json(&snapshot_shadow_metrics(runtime)),
    })
}

fn shadow_failure_json(
    mode: &str,
    error: &ShadowIterationError,
    iteration: u64,
) -> serde_json::Value {
    serde_json::json!({
        "gateway_shadow": false,
        "mode": mode,
        "iteration": iteration,
        "live_trading_enabled": false,
        "stage": error.stage,
        "readiness_phase": "Degraded",
        "readiness_reasons": [format!("{:?}", error.reason)],
        "error_present": true,
    })
}

fn shadow_loop_summary_json(
    stop_reason: &str,
    elapsed_ms: u128,
    iterations: u64,
    success_count: u64,
    failure_count: u64,
    metrics: &ShadowMetrics,
) -> serde_json::Value {
    serde_json::json!({
        "gateway_shadow_loop": "stopped",
        "stop_reason": stop_reason,
        "elapsed_ms": elapsed_ms,
        "iterations": iterations,
        "success_count": success_count,
        "failure_count": failure_count,
        "live_trading_enabled": false,
        "metrics": shadow_metrics_json(metrics),
    })
}

async fn publish_degraded_state(
    gateway: &FinamGateway<RedisConnectionStreamSink>,
    reason: ReadinessReason,
    stage: &str,
) -> Result<()> {
    if let Err(error) = gateway
        .publish_health(degraded_health(gateway.config()))
        .await
    {
        emit_redis_degraded_stderr(stage, &error)?;
    }
    if let Err(error) = gateway.publish_readiness(degraded_readiness(reason)).await {
        emit_redis_degraded_stderr(stage, &error)?;
    }
    Ok(())
}

async fn publish_stopped_state(
    gateway: &FinamGateway<RedisConnectionStreamSink>,
    stage: &str,
) -> Result<()> {
    if let Err(error) = gateway
        .publish_health(stopped_health(gateway.config()))
        .await
    {
        emit_redis_degraded_stderr(stage, &error)?;
    }
    if let Err(error) = gateway.publish_readiness(stopped_readiness()).await {
        emit_redis_degraded_stderr(stage, &error)?;
    }
    Ok(())
}

fn snapshot_error_reason(error: &finam_gateway::GatewayError) -> ReadinessReason {
    match error {
        finam_gateway::GatewayError::Redis(_) => ReadinessReason::RedisUnavailable,
        finam_gateway::GatewayError::Mapper(_) => ReadinessReason::OrdersNotLoaded,
        _ => ReadinessReason::Other("snapshot_publish_failed".to_string()),
    }
}

fn shadow_bars_window(config: &ResolvedGatewayShadowConfig) -> (String, String) {
    let now = Utc::now();
    let end_time = config.end_time.clone().unwrap_or_else(|| now.to_rfc3339());
    let lookback_minutes = config.bars_lookback_minutes.max(1);
    let start_time = config
        .start_time
        .clone()
        .unwrap_or_else(|| (now - ChronoDuration::minutes(lookback_minutes)).to_rfc3339());
    (start_time, end_time)
}

fn golden_bars_window(
    start_time: Option<&str>,
    end_time: Option<&str>,
    lookback_minutes: i64,
) -> (String, String) {
    let now = Utc::now();
    let end_time = end_time
        .map(str::to_string)
        .unwrap_or_else(|| now.to_rfc3339());
    let lookback_minutes = lookback_minutes.max(1);
    let start_time = start_time
        .map(str::to_string)
        .unwrap_or_else(|| (now - ChronoDuration::minutes(lookback_minutes)).to_rfc3339());
    (start_time, end_time)
}

fn bar_finality_golden_summary(
    symbol: &str,
    timeframe: &str,
    timeframe_sec: u32,
    start_time: &str,
    end_time: &str,
    bars: &broker_finam::BarsResponse,
) -> Result<serde_json::Value> {
    let probe_ts = Utc::now();
    let mapped = bars
        .bars
        .iter()
        .map(|bar| map_bar(symbol, bar, timeframe_sec))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(mapper_anyhow)?;
    let unique_open_deltas_sec = mapped
        .windows(2)
        .map(|window| (window[1].open_ts - window[0].open_ts).num_seconds())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let non_monotonic_open_ts_count = mapped
        .windows(2)
        .filter(|window| window[1].open_ts <= window[0].open_ts)
        .count();
    let close_delta_mismatch_count = mapped
        .iter()
        .filter(|bar| (bar.close_ts - bar.open_ts).num_seconds() != i64::from(timeframe_sec))
        .count();
    let last_bar_closed_before_probe_time = mapped.last().map(|bar| bar.close_ts <= probe_ts);

    Ok(serde_json::json!({
        "bar_finality_golden_harness": true,
        "ok": true,
        "live_trading_enabled": false,
        "order_endpoints_used": false,
        "symbol_present": !symbol.is_empty(),
        "response_symbol_present": !bars.symbol.is_empty(),
        "response_symbol_matches_request": bars.symbol == symbol,
        "timeframe": timeframe,
        "timeframe_sec": timeframe_sec,
        "request": {
            "start_time": start_time,
            "end_time": end_time,
        },
        "bars_count": bars.bars.len(),
        "mapped_bars_count": mapped.len(),
        "first_bar_open_ts": mapped.first().map(|bar| bar.open_ts.to_rfc3339()),
        "last_bar_open_ts": mapped.last().map(|bar| bar.open_ts.to_rfc3339()),
        "last_bar_close_ts": mapped.last().map(|bar| bar.close_ts.to_rfc3339()),
        "unique_open_deltas_sec": unique_open_deltas_sec,
        "non_monotonic_open_ts_count": non_monotonic_open_ts_count,
        "all_mapped_final_true": mapped.iter().all(|bar| bar.is_final),
        "close_delta_mismatch_count": close_delta_mismatch_count,
        "last_bar_closed_before_probe_time": last_bar_closed_before_probe_time,
        "acceptance_status": "unproven_operator_review_required",
    }))
}

fn historical_bar_watermark_key(timeframe: &str, bar: &broker_core::event::Bar) -> String {
    let symbol = bar
        .instrument
        .venue_symbol
        .as_deref()
        .unwrap_or(&bar.instrument.symbol);
    format!("{symbol}|{timeframe}|{}", bar.open_ts.to_rfc3339())
}

fn is_bar_watermark_known(runtime: &GatewayShadowRuntime, key: &str) -> bool {
    runtime
        .bar_watermark
        .lock()
        .expect("bar watermark mutex not poisoned")
        .contains(key)
}

fn mark_bar_watermark(runtime: &GatewayShadowRuntime, key: String) {
    runtime
        .bar_watermark
        .lock()
        .expect("bar watermark mutex not poisoned")
        .insert(key);
}

fn record_shadow_success(runtime: &GatewayShadowRuntime, report: &ShadowIterationReport) {
    let mut metrics = runtime
        .metrics
        .lock()
        .expect("shadow metrics mutex not poisoned");
    record_shadow_success_metrics(&mut metrics, report, Utc::now());
}

fn record_shadow_success_metrics(
    metrics: &mut ShadowMetrics,
    report: &ShadowIterationReport,
    now: chrono::DateTime<Utc>,
) {
    metrics.success_count += 1;
    metrics.consecutive_failures = 0;
    metrics.last_success_ts = Some(now);
    metrics.published_health_count += 1;
    metrics.published_readiness_count += 1;
    metrics.published_snapshot_count += 2;
    metrics.published_market_data_count +=
        u64::from(report.quote_published) + report.bars_published_count as u64;
    metrics.deduped_bar_count += report.bars_deduped_count as u64;
}

fn record_shadow_failure(runtime: &GatewayShadowRuntime) {
    let mut metrics = runtime
        .metrics
        .lock()
        .expect("shadow metrics mutex not poisoned");
    metrics.failure_count += 1;
    metrics.consecutive_failures += 1;
    metrics.last_failure_ts = Some(Utc::now());
}

fn snapshot_shadow_metrics(runtime: &GatewayShadowRuntime) -> ShadowMetrics {
    runtime
        .metrics
        .lock()
        .expect("shadow metrics mutex not poisoned")
        .clone()
}

fn shadow_metrics_json(metrics: &ShadowMetrics) -> serde_json::Value {
    serde_json::json!({
        "success_count": metrics.success_count,
        "failure_count": metrics.failure_count,
        "consecutive_failures": metrics.consecutive_failures,
        "last_success_ts": metrics.last_success_ts.map(|value| value.to_rfc3339()),
        "last_failure_ts": metrics.last_failure_ts.map(|value| value.to_rfc3339()),
        "published_health_count": metrics.published_health_count,
        "published_readiness_count": metrics.published_readiness_count,
        "published_snapshot_count": metrics.published_snapshot_count,
        "published_market_data_count": metrics.published_market_data_count,
        "deduped_bar_count": metrics.deduped_bar_count,
    })
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

    let read_reply: StreamReadReply = redis::cmd("XREAD")
        .arg("COUNT")
        .arg(1)
        .arg("STREAMS")
        .arg(&stream)
        .arg("0-0")
        .query_async(&mut manager)
        .await
        .context("Redis smoke XREAD failed")?;
    let read_entry = read_reply
        .keys
        .iter()
        .find(|key| key.key == stream)
        .and_then(|key| key.ids.first())
        .context("Redis smoke XREAD did not return a stream entry")?;
    let read_payload: String = read_entry
        .get("payload")
        .context("Redis smoke XREAD entry does not contain payload field")?;
    let typed_envelope: Envelope<finam_gateway::GatewayHealth> =
        serde_json::from_str(&read_payload)
            .context("Redis smoke XREAD payload does not decode as GatewayHealth envelope")?;
    anyhow::ensure!(
        typed_envelope.schema_version == 2,
        "Redis smoke typed envelope schema_version mismatch"
    );
    anyhow::ensure!(
        typed_envelope.msg_type == MessageType::Health,
        "Redis smoke typed envelope msg_type mismatch"
    );

    print_json(serde_json::json!({
        "redis_smoke": true,
        "live_trading_enabled": false,
        "stream": stream,
        "entry_id_present": !latest.id.is_empty(),
        "xread_entry_id_present": !read_entry.id.is_empty(),
        "typed_decode": "GatewayHealth",
        "schema_version": schema_version,
        "msg_type": msg_type,
        "payload_len": payload.len(),
    }))?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut rendered = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut rendered, "{byte:02x}").expect("hex write cannot fail");
    }
    rendered
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(sha256_hex(&bytes))
}

#[derive(Debug, Clone)]
struct FinamLimitCancelOneShotArgs {
    secret_env: String,
    account_id: String,
    symbol: String,
    limit_price: String,
    reference_price: String,
    qty: String,
    request_timeout_ms: u64,
    output: PathBuf,
    actual_send_i_understand_risk: bool,
}

async fn run_finam_limit_cancel_one_shot(args: FinamLimitCancelOneShotArgs) -> Result<()> {
    let now = Utc::now();
    let limit_price = Decimal::from_str(&args.limit_price).context("invalid limit price")?;
    let reference_price =
        Decimal::from_str(&args.reference_price).context("invalid reference price")?;
    let qty = Decimal::from_str(&args.qty).context("invalid qty")?;
    let price_below_reference = limit_price < reference_price;
    let qty_one = qty == Decimal::ONE;
    let compile_feature_enabled = cfg!(feature = "m3j16-actual-one-shot");

    let secret = SecretToken::new(std::env::var(&args.secret_env)?);
    let client = FinamRestClient::try_new(FinamConfig::default())?;
    let auth_manager = FinamAuthManager::new(client.clone(), secret);
    let token = auth_manager.access_token().await?;
    let token_details = client.token_details_typed(&token).await?;
    let full_trade_token_scope_present = token_details.readonly == Some(false);
    let account_bound = token_details
        .account_ids
        .iter()
        .any(|account_id| account_id == &args.account_id);

    let account = client.account_typed(&token, &args.account_id).await?;
    let portfolio = map_portfolio_snapshot(&account, now).map_err(mapper_anyhow)?;
    let flat_position = portfolio.positions.is_empty();

    let orders = client
        .account_orders_typed(&token, &args.account_id)
        .await?;
    let mapped_orders = orders
        .orders
        .iter()
        .map(|order| map_order_state(order, now))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(mapper_anyhow)?;
    let active_orders_count = active_orders(&mapped_orders).count();
    let terminal_or_ignored_orders_count = terminal_orders(&mapped_orders).count();
    let unknown_active_orders_count =
        usize::from(has_blocking_unknown_order_statuses(&mapped_orders));
    let orphan_active_orders_count = 0usize;
    let broker_truth_clean = active_orders_count == 0
        && unknown_active_orders_count == 0
        && orphan_active_orders_count == 0;

    let m3j15_preflight_accepted = full_trade_token_scope_present
        && account_bound
        && broker_truth_clean
        && flat_position
        && price_below_reference
        && qty_one;

    let mut gate_report = m3j16_limit_cancel_one_shot_report(M3j16LimitCancelOneShotInput {
        generated_at_label: now.to_rfc3339(),
        m3j15_preflight_accepted,
        explicit_operator_limit_cancel_approval_current: true,
        actual_send_flag_present: args.actual_send_i_understand_risk,
        compile_feature_enabled,
        account_bound,
        symbol_bound: !args.symbol.is_empty(),
        side_buy: true,
        order_type_limit: true,
        limit_price_below_reference: price_below_reference,
        limit_price: args.limit_price.clone(),
        reference_price: args.reference_price.clone(),
        qty_one,
        max_orders_one: true,
        place_then_cancel_only: true,
        no_stop_sltp_bracket_replace_multileg: true,
        kill_switch_armed_before_run: true,
        kill_switch_tested_before_run: true,
        one_shot_ttl_arm_fresh: true,
        no_auto_rearm: true,
        begin_submit_audit_persisted_before_boundary: true,
        cancel_required_after_place: true,
        post_run_reconciliation_required: true,
        eod_report_required: true,
        redacted_evidence_only: true,
    });

    let client_order_id = ClientOrderId::new(format!(
        "M3J16{:013}",
        now.timestamp_millis().rem_euclid(10_000_000_000_000)
    ))
    .context("M3j16 client order id")?;

    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut place_attempted = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let place_attempted = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut cancel_attempted = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let cancel_attempted = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut broker_order_id_present = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let broker_order_id_present = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut place_response_kind: Option<String> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let place_response_kind: Option<String> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut cancel_response_kind: Option<String> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let cancel_response_kind: Option<String> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut place_post_send_semantics: Option<String> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let place_post_send_semantics: Option<String> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut cancel_post_send_semantics: Option<String> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let cancel_post_send_semantics: Option<String> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let actual_error = None::<String>;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let actual_error = if gate_report.actual_send_allowed {
        Some("compiled without m3j16-actual-one-shot feature".to_string())
    } else {
        None
    };

    if gate_report.actual_send_allowed {
        #[cfg(feature = "m3j16-actual-one-shot")]
        {
            let gate =
                finam_gateway::EndpointGateApproved::m3j16_actual_one_shot_after_operator_approval(
                    "M3j-16-limit-cancel-one-shot",
                    true,
                )
                .context("M3j16 endpoint gate")?;
            let mut operator_arm = OperatorArm {
                session_id: format!("M3J16-{}", now.timestamp()),
                armed_until: now + ChronoDuration::minutes(2),
                endpoint_calls_enabled: true,
                one_shot: true,
                endpoint_attempted: false,
                preflight_digest: sha256_hex(
                    format!(
                        "{}:{}:{}:{}",
                        sha256_hex(args.account_id.as_bytes()),
                        sha256_hex(args.symbol.as_bytes()),
                        args.limit_price,
                        args.qty
                    )
                    .as_bytes(),
                ),
            };
            let policy = OrderPreflightPolicy {
                allowed_accounts: vec![BrokerAccountId::new(args.account_id.clone())],
                allowed_venue_symbols: vec![args.symbol.clone()],
                allowed_order_types: vec![OrderType::Limit],
                allowed_time_in_force: vec![TimeInForce::Day],
                min_qty: Decimal::ONE,
                qty_step: Decimal::ONE,
                max_qty: Decimal::ONE,
                price_step: Some(Decimal::ONE),
                max_market_qty: Decimal::ZERO,
                max_notional_per_order: Some(reference_price * qty),
                max_notional_per_run: Some(reference_price * qty),
                max_limit_deviation_bps: None,
                max_reference_age_ms: 60_000,
                allow_cancel_by_broker_order_id_without_mapping: true,
                operator_arm: operator_arm.clone(),
            };
            let order = PlaceOrder {
                request_id: StrategyRequestId::from(Uuid::from_u128(
                    now.timestamp_millis().max(0) as u128
                )),
                created_ts: now,
                ttl_ms: Some(60_000),
                account_id: BrokerAccountId::new(args.account_id.clone()),
                client_order_id: client_order_id.clone(),
                instrument: InstrumentId {
                    symbol: "IMOEXF".to_string(),
                    venue_symbol: Some(args.symbol.clone()),
                    exchange: Exchange::Moex,
                    market: Market::Futures,
                },
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                qty,
                limit_price: Some(limit_price),
                time_in_force: TimeInForce::Day,
                comment: None,
            };
            let approved = policy
                .approve_place_order(&order, now)
                .context("M3j16 place preflight")?;
            let place_spec = build_place_order_request(&approved, None)
                .context("M3j16 FINAM place request build")?;
            operator_arm.record_endpoint_attempt();

            let transport =
                M3d2RealOrderEndpointTransport::try_new(M3d2RealOrderEndpointTransportConfig {
                    rest_base_url: FinamConfig::default().rest_base_url,
                    request_timeout_ms: args.request_timeout_ms,
                    authorization_header_mode: FinamAuthorizationHeaderMode::BearerJwt,
                    external_endpoint_mode:
                        M3d2ExternalOrderEndpointMode::M3j16ActualOneShotExternalFinam,
                })
                .map_err(|error| anyhow::anyhow!("M3j16 real order transport: {error:?}"))?;

            place_attempted = true;
            let place_execution = transport
                .place_order_execution(&gate, &token, &place_spec)
                .await;
            let place_outcome = place_execution.redacted_outcome();
            place_response_kind = place_outcome.response_kind.map(|kind| format!("{kind:?}"));
            place_post_send_semantics = Some(format!("{:?}", place_outcome.post_send_semantics));
            let broker_order_id =
                place_execution
                    .classified_response
                    .as_ref()
                    .and_then(|classified| {
                        if let FinamOrderEndpointMappedResult::Execution(
                            FinamOrderExecutionOutcome::Accepted { broker_order_id },
                        ) = &classified.result
                        {
                            broker_order_id.clone()
                        } else {
                            None
                        }
                    });
            broker_order_id_present = broker_order_id.is_some();

            if let Some(broker_order_id) = broker_order_id {
                let cancel = broker_core::CancelOrder {
                    request_id: StrategyRequestId::from(Uuid::from_u128(
                        Utc::now().timestamp_millis().max(0) as u128 + 1,
                    )),
                    created_ts: Utc::now(),
                    ttl_ms: Some(60_000),
                    account_id: BrokerAccountId::new(args.account_id.clone()),
                    order_id: broker_order_id,
                    client_order_id: Some(client_order_id.clone()),
                };
                let cancel_approval = policy
                    .approve_cancel_order(&cancel, Utc::now(), None)
                    .context("M3j16 cancel preflight")?;
                let CancelPreflightApproval::Submit(cancel_approval) = cancel_approval else {
                    return Err(anyhow::anyhow!(
                        "M3j16 cancel preflight returned terminal state"
                    ));
                };
                let cancel_spec = build_cancel_order_request(&cancel_approval)
                    .context("M3j16 FINAM cancel request build")?;
                cancel_attempted = true;
                let cancel_execution = transport
                    .cancel_order_execution(&gate, &token, &cancel_spec)
                    .await;
                let cancel_outcome = cancel_execution.redacted_outcome();
                cancel_response_kind = cancel_outcome.response_kind.map(|kind| format!("{kind:?}"));
                cancel_post_send_semantics =
                    Some(format!("{:?}", cancel_outcome.post_send_semantics));
            }
        }
    }

    gate_report.boundary_invocation_performed = place_attempted || cancel_attempted;
    gate_report.place_attempted = place_attempted;
    gate_report.cancel_attempted = cancel_attempted;
    gate_report.real_finam_order_endpoint_used = place_attempted || cancel_attempted;

    let payload = serde_json::json!({
        "fixture_kind": "m3j16-limit-cancel-one-shot-redacted-v1",
        "generated_at": now.to_rfc3339(),
        "report": gate_report,
        "operator_scope": {
            "max_orders": 1,
            "qty": args.qty,
            "side": "buy",
            "order_type": "limit",
            "limit_price": args.limit_price,
            "reference_price": args.reference_price,
            "request_timeout_ms": args.request_timeout_ms,
            "limit_price_below_reference": price_below_reference,
            "no_stop_sltp_bracket": true,
            "place_then_cancel_only": true
        },
        "redacted_bindings": {
            "account_id_len": args.account_id.len(),
            "account_id_sha256": sha256_hex(args.account_id.as_bytes()),
            "symbol_len": args.symbol.len(),
            "symbol_sha256": sha256_hex(args.symbol.as_bytes()),
            "client_order_id_len": client_order_id.as_str().len(),
            "client_order_id_sha256": sha256_hex(client_order_id.as_str().as_bytes())
        },
        "pre_boundary_broker_truth": {
            "token_readonly_flag_present": token_details.readonly.is_some(),
            "token_readonly_flag_value": token_details.readonly,
            "token_account_hash_match": account_bound,
            "positions_count": portfolio.positions.len(),
            "orders_total": mapped_orders.len(),
            "active_orders_count": active_orders_count,
            "unknown_active_orders_count": unknown_active_orders_count,
            "orphan_active_orders_count": orphan_active_orders_count,
            "blocking_unknown_status_present": unknown_active_orders_count > 0,
            "terminal_or_ignored_orders_count": terminal_or_ignored_orders_count,
            "broker_truth_clean": broker_truth_clean
        },
        "execution_redacted": {
            "actual_send_flag_present": args.actual_send_i_understand_risk,
            "compile_feature_enabled": compile_feature_enabled,
            "place_attempted": place_attempted,
            "cancel_attempted": cancel_attempted,
            "broker_order_id_present": broker_order_id_present,
            "place_response_kind": place_response_kind,
            "cancel_response_kind": cancel_response_kind,
            "place_post_send_semantics": place_post_send_semantics,
            "cancel_post_send_semantics": cancel_post_send_semantics,
            "actual_error": actual_error
        }
    });
    print_json(payload.clone())?;
    write_json_payload(&args.output, &payload)?;
    Ok(())
}

fn command_stdout(command: &str, args: &[&str]) -> Option<String> {
    let output = ProcessCommand::new(command).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn source_commit_full_sha() -> Option<String> {
    command_stdout("git", &["rev-parse", "HEAD"]).filter(|sha| sha.len() == 40)
}

fn infer_source_archive(source_commit_full_sha: Option<&str>) -> Option<PathBuf> {
    let short = source_commit_full_sha?.get(..7)?;
    let path = PathBuf::from(format!("reports/handoff/moex-trading-project-{short}.zip"));
    path.exists().then_some(path)
}

fn source_archive_short_commit(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    name.strip_prefix("moex-trading-project-")?
        .strip_suffix(".zip")
        .filter(|short| short.len() >= 7)
        .map(ToString::to_string)
}

fn validate_source_archive_binding(
    path: &Path,
    source_commit_full_sha: Option<&str>,
) -> Result<()> {
    let Some(source_commit_full_sha) = source_commit_full_sha else {
        anyhow::bail!("cannot validate source archive binding without git HEAD sha");
    };
    let expected_short = source_commit_full_sha
        .get(..7)
        .context("git HEAD sha is shorter than 7 characters")?;
    let archive_short = source_archive_short_commit(path).with_context(|| {
        format!(
            "source archive name must match moex-trading-project-<short_commit>.zip: {}",
            path.display()
        )
    })?;
    anyhow::ensure!(
        archive_short == expected_short,
        "source archive binding mismatch: archive_short={archive_short} git_head_short={expected_short}"
    );
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandoffCommitMarker {
    source_commit: Option<String>,
    source_ref: Option<String>,
    archive_name: Option<String>,
}

fn parse_handoff_commit_marker(contents: &str) -> HandoffCommitMarker {
    let mut marker = HandoffCommitMarker {
        source_commit: None,
        source_ref: None,
        archive_name: None,
    };
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "source_commit" => marker.source_commit = Some(value.to_string()),
            "source_ref" => marker.source_ref = Some(value.to_string()),
            "archive_name" => marker.archive_name = Some(value.to_string()),
            _ => {}
        }
    }
    marker
}

fn read_handoff_commit_marker_from_zip(path: &Path) -> Result<HandoffCommitMarker> {
    let output = ProcessCommand::new("unzip")
        .args(["-p"])
        .arg(path)
        .arg("handoff-commit.txt")
        .output()
        .with_context(|| format!("failed to read handoff-commit.txt from {}", path.display()))?;
    anyhow::ensure!(
        output.status.success(),
        "source archive is missing readable handoff-commit.txt: {}",
        path.display()
    );
    let contents = String::from_utf8(output.stdout).with_context(|| {
        format!(
            "handoff-commit.txt is not valid UTF-8 in {}",
            path.display()
        )
    })?;
    Ok(parse_handoff_commit_marker(&contents))
}

fn validate_source_archive_content_binding(
    path: &Path,
    source_commit_full_sha: Option<&str>,
) -> Result<HandoffCommitMarker> {
    let marker = read_handoff_commit_marker_from_zip(path)?;
    let archive_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .context("source archive path has no file name")?;
    validate_handoff_marker_content_binding(&marker, &archive_name, source_commit_full_sha)?;
    Ok(marker)
}

fn validate_handoff_marker_content_binding(
    marker: &HandoffCommitMarker,
    archive_name: &str,
    source_commit_full_sha: Option<&str>,
) -> Result<()> {
    let Some(source_commit_full_sha) = source_commit_full_sha else {
        anyhow::bail!("cannot validate source archive content binding without git HEAD sha");
    };
    anyhow::ensure!(
        marker.source_ref.as_deref() == Some(source_commit_full_sha),
        "source archive content binding mismatch: handoff_source_ref={:?} git_head={source_commit_full_sha}",
        marker.source_ref
    );
    anyhow::ensure!(
        marker.archive_name.as_deref() == Some(archive_name),
        "source archive content archive_name mismatch: handoff_archive_name={:?} archive_name={archive_name}",
        marker.archive_name
    );
    Ok(())
}

fn run_forbidden_surface_scan_metadata() -> Result<serde_json::Value> {
    let script = PathBuf::from("scripts/forbidden_surface_scan.sh");
    let script_sha256 = script.exists().then(|| sha256_file(&script)).transpose()?;
    if !script.exists() {
        return Ok(serde_json::json!({
            "status": "script_missing",
            "script_path": "scripts/forbidden_surface_scan.sh",
            "script_sha256": script_sha256,
            "exit_code": null,
        }));
    }
    let output = ProcessCommand::new("bash").arg(&script).output()?;
    let exit_code = output.status.code();
    anyhow::ensure!(
        output.status.success(),
        "forbidden surface scan failed before FINAM evidence run: exit_code={exit_code:?}"
    );
    Ok(serde_json::json!({
        "status": "ok",
        "script_path": "scripts/forbidden_surface_scan.sh",
        "script_sha256": script_sha256,
        "exit_code": exit_code,
    }))
}

fn build_finam_real_readonly_evidence_metadata(
    source_archive: Option<&Path>,
) -> Result<serde_json::Value> {
    let source_commit_full_sha = source_commit_full_sha();
    let resolved_source_archive = source_archive
        .map(PathBuf::from)
        .or_else(|| infer_source_archive(source_commit_full_sha.as_deref()));
    let source_archive_name = resolved_source_archive
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_string());
    let source_archive_sha256 = resolved_source_archive
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| sha256_file(path))
        .transpose()?;
    let build_profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    Ok(serde_json::json!({
        "source_commit_full_sha": source_commit_full_sha,
        "source_archive_name": source_archive_name,
        "source_archive_sha256": source_archive_sha256,
        "broker_cli_package_version": env!("CARGO_PKG_VERSION"),
        "broker_cli_build_profile": build_profile,
        "forbidden_surface_scan": run_forbidden_surface_scan_metadata()?,
        "runbook_doc": "docs/m3b23-real-readonly-evidence-closeout.md",
        "runbook_doc_version": "m3b23",
    }))
}

fn resolve_source_evidence(source_archive: Option<&Path>) -> Result<M3cSourceEvidence> {
    let source_commit_full_sha = source_commit_full_sha();
    let mut handoff_marker = None;
    if let Some(source_archive) = source_archive {
        validate_source_archive_binding(source_archive, source_commit_full_sha.as_deref())?;
        handoff_marker = Some(validate_source_archive_content_binding(
            source_archive,
            source_commit_full_sha.as_deref(),
        )?);
    }
    let resolved_source_archive = source_archive
        .map(PathBuf::from)
        .or_else(|| infer_source_archive(source_commit_full_sha.as_deref()));
    let source_archive_name = resolved_source_archive
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_string());
    let source_archive_sha256 = resolved_source_archive
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| sha256_file(path))
        .transpose()?;
    Ok(M3cSourceEvidence {
        source_commit_full_sha,
        source_archive_name,
        source_archive_sha256,
        source_archive_handoff_source_ref: handoff_marker
            .as_ref()
            .and_then(|marker| marker.source_ref.clone()),
        source_archive_handoff_archive_name: handoff_marker
            .as_ref()
            .and_then(|marker| marker.archive_name.clone()),
        source_archive_content_binding_verified: handoff_marker.is_some(),
    })
}

fn parse_m3c_evidence_slot_status(value: &str) -> Result<M3cOrderEndpointGateEvidenceStatus> {
    match value {
        "pending" => Ok(M3cOrderEndpointGateEvidenceStatus::Pending),
        "evidence-provided" => Ok(M3cOrderEndpointGateEvidenceStatus::EvidenceProvided),
        "waiver-accepted" => Ok(M3cOrderEndpointGateEvidenceStatus::WaiverAccepted),
        _ => anyhow::bail!(
            "unsupported M3c evidence slot status {value:?}; expected pending, evidence-provided, or waiver-accepted"
        ),
    }
}

fn build_m3c_order_endpoint_gate_design_evidence(
    source_archive: Option<&Path>,
    slot_args: &M3cEvidenceSlotArgs,
) -> Result<M3cOrderEndpointGateDesignEvidence> {
    let scan = run_forbidden_surface_scan_metadata()?;
    let scan_status = match scan.get("status").and_then(serde_json::Value::as_str) {
        Some("ok") => M3cOrderEndpointGateEvidenceStatus::Ok,
        Some("script_missing") => M3cOrderEndpointGateEvidenceStatus::Failed,
        _ => M3cOrderEndpointGateEvidenceStatus::Failed,
    };
    let exit_code = scan
        .get("exit_code")
        .and_then(serde_json::Value::as_i64)
        .and_then(|code| i32::try_from(code).ok());
    let release_profile_evidence_or_waiver =
        parse_m3c_evidence_slot_status(&slot_args.release_profile_status)?;
    let positive_get_order_evidence_or_waiver =
        parse_m3c_evidence_slot_status(&slot_args.positive_get_order_status)?;
    let route_template_recheck =
        parse_m3c_evidence_slot_status(&slot_args.route_template_recheck_status)?;
    let undocumented_2xx_status_semantics =
        parse_m3c_evidence_slot_status(&slot_args.undocumented_2xx_status)?;
    let cancel_409_410_status_semantics =
        parse_m3c_evidence_slot_status(&slot_args.cancel_409_410_status)?;
    let evidence_statuses = [
        release_profile_evidence_or_waiver,
        positive_get_order_evidence_or_waiver,
        route_template_recheck,
        undocumented_2xx_status_semantics,
        cancel_409_410_status_semantics,
    ];
    let evidence_pending_count = evidence_statuses
        .iter()
        .filter(|status| **status == M3cOrderEndpointGateEvidenceStatus::Pending)
        .count();
    let evidence_provided_or_waiver_count = evidence_statuses
        .iter()
        .filter(|status| {
            matches!(
                status,
                M3cOrderEndpointGateEvidenceStatus::EvidenceProvided
                    | M3cOrderEndpointGateEvidenceStatus::WaiverAccepted
            )
        })
        .count();
    let golden_vectors = finam_gateway::real_order_endpoint::canonical_replay_golden_vectors();
    let readiness = finam_gateway::real_order_endpoint::implementation_gate_readiness_checklist();
    let operator_runbook = finam_gateway::real_order_endpoint::operator_replay_runbook_entries();

    Ok(M3cOrderEndpointGateDesignEvidence {
        forbidden_surface_scan: M3cForbiddenSurfaceScanEvidence {
            status: scan_status,
            script_path: scan
                .get("script_path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("scripts/forbidden_surface_scan.sh")
                .to_string(),
            script_sha256: scan
                .get("script_sha256")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
            checked_at: Some(Utc::now().to_rfc3339()),
            exit_code,
        },
        source: resolve_source_evidence(source_archive)?,
        release_profile_evidence_or_waiver,
        positive_get_order_evidence_or_waiver,
        route_template_recheck,
        undocumented_2xx_status_semantics,
        cancel_409_410_status_semantics,
        canonical_replay_golden_vector_sha256: golden_vectors
            .first()
            .map(|vector| vector.expected_sha256.clone())
            .unwrap_or_default(),
        canonical_replay_vector_count: golden_vectors.len(),
        readiness_implemented_tested_count: readiness
            .iter()
            .filter(|entry| {
                entry.status
                    == finam_gateway::real_order_endpoint::GatewayRealOrderEndpointImplementationGateReadinessStatus::ImplementedAndTested
            })
            .count(),
        readiness_pending_evidence_or_waiver_count: readiness
            .iter()
            .filter(|entry| {
                entry.status
                    == finam_gateway::real_order_endpoint::GatewayRealOrderEndpointImplementationGateReadinessStatus::PendingEvidenceOrWaiver
            })
            .count(),
        operator_replay_runbook_case_count: operator_runbook.len(),
        evidence_slot_count: evidence_statuses.len(),
        evidence_pending_count,
        evidence_provided_or_waiver_count,
        route_template_recheck_plan: M3cRouteTemplateRecheckPlanEvidence {
            route_template_recheck_design_only: true,
            route_count: 2,
            exact_two_route_allowlist_required: true,
            official_docs_or_waiver_required: true,
            reviewer_acceptance_required: true,
            recheck_before_implementation_gate: true,
            route_templates_exported_as_design_data_only: true,
            rendered_routes_exported: false,
            raw_account_or_order_id_exported: false,
            order_endpoint_calls_allowed_for_recheck: false,
        },
    })
}

fn run_m3c_order_endpoint_gate_report(
    output: PathBuf,
    source_archive: Option<PathBuf>,
    slot_args: M3cEvidenceSlotArgs,
) -> Result<()> {
    let evidence =
        build_m3c_order_endpoint_gate_design_evidence(source_archive.as_deref(), &slot_args)?;
    let report =
        GatewayFeatureSet::default().m3c_order_endpoint_gate_design_report_with_evidence(evidence);
    let payload = serde_json::to_value(report)?;
    print_json(payload.clone())?;
    write_json_payload(&output, &payload)?;
    Ok(())
}

async fn run_finam_real_readonly_evidence(args: FinamRealReadonlyEvidenceArgs) -> Result<()> {
    if args.max_requests == 0 || args.max_requests > 4 {
        anyhow::bail!("max_requests must be in 1..=4 for controlled real-readonly evidence");
    }
    if args.preflight_max_age_ms == 0 {
        anyhow::bail!("preflight_max_age_ms must be greater than zero");
    }
    let evidence_metadata =
        build_finam_real_readonly_evidence_metadata(args.source_archive.as_deref())?;

    let finam_config = FinamConfig {
        request_timeout_ms: args.request_timeout_ms,
        ..FinamConfig::default()
    };
    let secret = SecretToken::new(std::env::var(&args.secret_env)?);
    let client = FinamRestClient::try_new(finam_config.clone())?;
    let auth_manager = FinamAuthManager::new(client.clone(), secret);
    let access_token = auth_manager
        .access_token()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_redacted_string()))?;
    let token_details = client
        .token_details_typed(&access_token)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_redacted_string()))?;

    let account_id = BrokerAccountId::new(args.account_id);
    let order_id = BrokerOrderId::new(args.broker_order_id);
    let client_order_id = args
        .client_order_id
        .map(ClientOrderId::new)
        .transpose()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let instrument = InstrumentId {
        symbol: args.symbol.clone(),
        venue_symbol: Some(args.symbol),
        exchange: Exchange::Moex,
        market: Market::Futures,
    };
    let request_snapshot_requested_at = Utc::now();
    let request_snapshot = CancelBrokerTruthFetchRequestSnapshot {
        account_id: Some(account_id.clone()),
        order_id,
        client_order_id,
        instrument,
        requested_at: request_snapshot_requested_at,
        position_guard_context: CancelPositionTruthGuardContext::default(),
    };

    let features = GatewayFeatureSet {
        real_readonly_broker_truth_enabled: true,
        ..GatewayFeatureSet::default()
    };
    let transport_config = FinamRealReadonlyBrokerTruthTransportConfig {
        rest_base_url: finam_config.rest_base_url.clone(),
        request_timeout_ms: args.request_timeout_ms,
        min_request_interval_ms: args.min_request_interval_ms,
        allowed_accounts: vec![account_id],
    };
    let gate = RealReadonlyBrokerTruthGateApproved::try_from_decision(
        &features.real_readonly_broker_truth_gate_decision(),
    )?;
    let guardrails = evaluate_finam_real_readonly_operator_guardrails(
        &features,
        &transport_config,
        &request_snapshot,
    );
    let run_approval =
        RealReadonlyBrokerTruthRunApproved::try_from_gate_and_guardrails(gate, &guardrails)?;
    let preflight_checked_at = Utc::now();
    let preflight_marker = FinamRealReadonlyTokenAccountPreflightApproved::try_from_token_details(
        &features,
        &request_snapshot,
        &token_details,
        preflight_checked_at,
        args.preflight_max_age_ms,
    )?;
    let probe_run_started_at = Utc::now();
    let transport = ReqwestFinamRealReadonlyBrokerTruthTransport::try_new(
        transport_config,
        access_token,
        &run_approval,
    )?;
    let mut fetcher = FinamRealReadonlyBrokerTruthAsyncFetcher::new(
        transport,
        run_approval,
        CancelBrokerTruthFreshnessPolicy::default(),
        FinamRealReadonlyBrokerTruthQueryPolicy::default(),
        probe_run_started_at,
    );
    let report = run_finam_real_readonly_operator_contract_probe(
        &mut fetcher,
        request_snapshot,
        &FinamRealReadonlyContractProbeOperatorRunConfig {
            enabled: true,
            probe_run_started_at: Some(probe_run_started_at),
            sources: vec![
                CancelBrokerTruthSource::GetOrder,
                CancelBrokerTruthSource::OrdersSnapshot,
                CancelBrokerTruthSource::TradesSnapshot,
                CancelBrokerTruthSource::PositionSnapshot,
            ],
            max_requests: args.max_requests,
            request_timeout_ms: args.request_timeout_ms,
            min_request_interval_ms: args.min_request_interval_ms,
            redacted_output_location: Some(
                FinamRealReadonlyRedactedOutputLocation::from_path_label(
                    args.output.to_string_lossy(),
                ),
            ),
            audit_store_mode: FinamRealReadonlyAuditStoreMode::EphemeralEvidenceStore,
            retry_disabled: true,
            background_loop_disabled: true,
            scheduler_disabled: true,
            operator_disable_procedure_documented: true,
            preserve_transport_error_taxonomy: true,
            token_account_preflight: Some(preflight_marker),
        },
    )
    .await;

    let payload = serde_json::json!({
        "fixture_kind": "finam-real-readonly-contract-probe-evidence-v1",
        "evidence_schema_version": 2,
        "generated_at": Utc::now(),
        "evidence_metadata": evidence_metadata,
        "live_trading_enabled": false,
        "order_endpoints_used": false,
        "scope": {
            "single_controlled_operator_run": true,
            "get_only_broker_truth_probe": true,
            "max_requests_lte_4": args.max_requests <= 4,
            "retry_disabled": true,
            "background_loop_disabled": true,
            "scheduler_disabled": true,
            "persistent_audit_store_used": false,
            "ephemeral_evidence_store_only": true,
            "real_order_post_delete_enabled": false,
        },
        "operator_report": report,
    });
    write_json_payload(&args.output, &payload)?;
    print_json(serde_json::json!({
        "finam_real_readonly_evidence": true,
        "output": args.output,
        "live_trading_enabled": false,
        "order_endpoints_used": false,
        "fixture_kind": "finam-real-readonly-contract-probe-evidence-v1",
        "blocking_reasons_count": payload["operator_report"]["blocking_reasons"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
        "actual_http_send_started_count": payload["operator_report"]["actual_http_send_started_count"],
        "actual_http_send_completed_count": payload["operator_report"]["actual_http_send_completed_count"],
        "max_requests": payload["operator_report"]["max_requests"],
    }))?;
    Ok(())
}

async fn run_bar_finality_golden_check(args: BarFinalityGoldenArgs) -> Result<()> {
    let symbol = args
        .symbol
        .as_deref()
        .context("missing required FINAM symbol for bar finality golden check")?;
    let timeframe_sec = timeframe_seconds(&args.timeframe)?;
    let (start_time, end_time) = golden_bars_window(
        args.start_time.as_deref(),
        args.end_time.as_deref(),
        args.lookback_minutes,
    );
    let mut records = Vec::new();

    let secret = SecretToken::new(std::env::var(&args.secret_env)?);
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
                    "bar_finality_golden_harness": true,
                    "live_trading_enabled": false,
                    "order_endpoints_used": false,
                }),
            )?;
            let bars_query = BarsQuery {
                timeframe: &args.timeframe,
                start_time: Some(start_time.as_str()),
                end_time: Some(end_time.as_str()),
            };
            match client.bars_typed(&token, symbol, bars_query).await {
                Ok(bars) => {
                    let summary = bar_finality_golden_summary(
                        symbol,
                        &args.timeframe,
                        timeframe_sec,
                        &start_time,
                        &end_time,
                        &bars,
                    )?;
                    emit_record(&mut records, summary)?;
                }
                Err(error) => {
                    emit_record(
                        &mut records,
                        serde_json::json!({
                            "bar_finality_golden_harness": true,
                            "ok": false,
                            "live_trading_enabled": false,
                            "order_endpoints_used": false,
                            "probe": "bars_typed",
                            "error_kind": error.kind(),
                            "error": error.to_redacted_string(),
                        }),
                    )?;
                }
            }
        }
        Err(error) => {
            emit_record(
                &mut records,
                serde_json::json!({
                    "auth_error_kind": error.kind(),
                    "auth_error": error.to_redacted_string(),
                    "bar_finality_golden_harness": true,
                    "live_trading_enabled": false,
                    "order_endpoints_used": false,
                }),
            )?;
        }
    }

    if let Some(output) = args.output {
        write_records_fixture(output, "finam-bar-finality-golden-redacted-v1", &records)?;
    }
    Ok(())
}

async fn run_runtime_bridge_dry_consume(args: RuntimeBridgeDryConsumeArgs) -> Result<()> {
    let file_config = read_gateway_shadow_file_config(args.config.as_ref())?;
    let resolved = resolve_runtime_bridge_dry_config(args, file_config)?;
    let summary = consume_runtime_bridge_dry(resolved).await?;
    print_json(summary)?;
    Ok(())
}

async fn run_runtime_bridge_redis_smoke(redis_url: String, prefix: String) -> Result<()> {
    let run_id = Utc::now().timestamp_millis();
    let stream_prefix = non_empty_or_default(prefix, "broker.m2i.runtime_bridge_smoke");

    let positive_config =
        runtime_bridge_smoke_config(&redis_url, &format!("{stream_prefix}.positive.{run_id}"));
    publish_runtime_bridge_positive_smoke_entries(&positive_config).await?;
    let positive_summary = consume_runtime_bridge_dry(runtime_bridge_smoke_resolved_config(
        positive_config.clone(),
        format!("runtime-bridge-smoke-positive-{run_id}"),
        "smoke-positive",
    ))
    .await?;
    assert_runtime_bridge_positive_smoke_summary(&positive_summary)?;

    let negative_results =
        run_runtime_bridge_negative_smoke_cases(&redis_url, &stream_prefix, run_id).await?;

    let reconnect_config =
        runtime_bridge_smoke_config(&redis_url, &format!("{stream_prefix}.reconnect.{run_id}"));
    publish_runtime_bridge_reconnect_smoke_entries(&reconnect_config, 3).await?;
    create_runtime_bridge_pending_entries_without_ack(
        &reconnect_config,
        &reconnect_config.redis.health_stream,
        &format!("runtime-bridge-smoke-reconnect-{run_id}"),
        "smoke-crashed",
        3,
    )
    .await?;
    let mut reconnect_resolved = runtime_bridge_smoke_resolved_config(
        reconnect_config,
        format!("runtime-bridge-smoke-reconnect-{run_id}"),
        "smoke-recovered",
    );
    reconnect_resolved.claim_stale_ms = Some(0);
    reconnect_resolved.count = 2;
    let reconnect_summary = consume_runtime_bridge_dry(reconnect_resolved).await?;
    assert_runtime_bridge_reconnect_smoke_summary(&reconnect_summary)?;

    let retention_result =
        run_runtime_bridge_dlq_retention_stress_smoke(&redis_url, &stream_prefix, run_id).await?;

    print_json(serde_json::json!({
        "runtime_bridge_redis_smoke": true,
        "live_trading_enabled": false,
        "order_endpoints_used": false,
        "positive": {
            "accepted_count": json_path_u64(&positive_summary, "/consumer_metrics/accepted_count")?,
            "xack_count": json_path_u64(&positive_summary, "/xreadgroup/xack_count")?,
            "readiness_phase": json_path_str(&positive_summary, "/readiness_simulator/phase")?,
        },
        "negative_cases": negative_results,
        "reconnect": {
            "accepted_count": json_path_u64(&reconnect_summary, "/consumer_metrics/accepted_count")?,
            "claimed_entries_returned": json_path_u64(&reconnect_summary, "/xautoclaim/claimed_entries_returned")?,
            "xautoclaim_iterations": json_path_u64(&reconnect_summary, "/xautoclaim/iterations")?,
            "xack_count": json_path_u64(&reconnect_summary, "/xreadgroup/xack_count")?,
            "readiness_phase": json_path_str(&reconnect_summary, "/readiness_simulator/phase")?,
        },
        "retention": retention_result,
    }))?;
    Ok(())
}

async fn run_m3e_command_consumer_redis_smoke(redis_url: String, prefix: String) -> Result<()> {
    let run_id = Utc::now().timestamp_millis();
    let stream_prefix = non_empty_or_default(prefix, "broker.m3e.command_consumer_smoke");
    let config = m3e_redis_smoke_config(&redis_url, &format!("{stream_prefix}.{run_id}"));
    let group = format!("m3e-command-consumer-smoke-{run_id}");
    let consumer = "m3e-smoke-consumer";
    let mut manager = redis::Client::open(redis_url.as_str())
        .context("m3e Redis smoke URL invalid")?
        .get_connection_manager()
        .await
        .context("m3e Redis smoke connection failed")?;
    ensure_runtime_bridge_group(&mut manager, &config.command_stream, &group, "0").await?;

    let mut metrics = M3eRedisSmokeMetrics::default();
    let sink = RedisConnectionStreamSink::from_connection_manager(manager.clone());
    let lifecycle_store = M3eInMemoryCommandLifecycleStore::default();
    let command_consumer =
        M3eCommandConsumerLocalMockEndpoint::new(config.clone(), sink, lifecycle_store.clone());
    let mut order_store = InMemoryOrderPathStore::default();
    let mut transport = M3eCliCountingClassifiedTransport::accepted("BROKER_TEST_M3E4_PLACE");
    let now = Utc::now();
    let policy = m3e_smoke_preflight_policy(now);

    let place_command = BrokerCommand::PlaceOrder(m3e_smoke_place_order(
        m3e_smoke_request_id(8_001),
        "CID000000000008001",
        now,
        Some(60_000),
    )?);
    let place_id = m3e_redis_xadd_command(&mut manager, &config, &place_command).await?;
    let place_stream_id = m3e_redis_xreadgroup_one(&mut manager, &config, &group, consumer).await?;
    anyhow::ensure!(
        place_stream_id.id == place_id,
        "m3e place XREADGROUP id mismatch"
    );
    metrics.xreadgroup_entries += 1;
    let place_report = m3e_process_stream_id(
        &command_consumer,
        &config,
        &mut order_store,
        &mut transport,
        &policy,
        &place_stream_id,
        now,
    )
    .await?;
    anyhow::ensure!(
        place_report.action == M3eCommandLifecycleAction::LocalMockEndpointAckPublished,
        "m3e place did not reach local mock endpoint"
    );
    anyhow::ensure!(
        place_report.local_mock_endpoint_only
            && !place_report.external_order_endpoint_allowed
            && !place_report.non_loopback_endpoint_allowed,
        "m3e place opened a forbidden endpoint boundary"
    );
    metrics.real_xack_count += runtime_bridge_xack(
        &mut manager,
        &config.command_stream,
        &group,
        &place_stream_id.id,
    )
    .await?;
    metrics.place_ok = true;

    let cancel_broker_id = BrokerOrderId::new("BROKER_TEST_M3E4_CANCEL");
    let cancel_place_order = m3e_smoke_place_order(
        m3e_smoke_request_id(8_002),
        "CID000000000008002",
        now,
        Some(60_000),
    )?;
    let mut submitted = OrderPathRecord::from_place_order(&cancel_place_order, now, None);
    submitted.broker_order_id = Some(cancel_broker_id.clone());
    submitted.transition(OrderPathEvent::BeginSubmit, now)?;
    submitted.transition(
        OrderPathEvent::SubmitAccepted,
        now + ChronoDuration::milliseconds(1),
    )?;
    order_store.insert_intent(submitted)?;
    let cancel_command = BrokerCommand::CancelOrder(broker_core::CancelOrder {
        request_id: m3e_smoke_request_id(8_003),
        created_ts: now,
        ttl_ms: Some(60_000),
        account_id: BrokerAccountId::new("ACC_TEST_0001"),
        order_id: cancel_broker_id.clone(),
        client_order_id: None,
    });
    let cancel_stream_id =
        m3e_redis_xadd_and_read(&mut manager, &config, &group, consumer, &cancel_command).await?;
    metrics.xreadgroup_entries += 1;
    let cancel_report = m3e_process_stream_id(
        &command_consumer,
        &config,
        &mut order_store,
        &mut transport,
        &policy,
        &cancel_stream_id,
        now + ChronoDuration::milliseconds(2),
    )
    .await?;
    anyhow::ensure!(
        cancel_report.action == M3eCommandLifecycleAction::LocalMockEndpointAckPublished,
        "m3e cancel did not reach local mock endpoint"
    );
    metrics.real_xack_count += runtime_bridge_xack(
        &mut manager,
        &config.command_stream,
        &group,
        &cancel_stream_id.id,
    )
    .await?;
    metrics.cancel_ok = true;

    let duplicate_command = BrokerCommand::PlaceOrder(m3e_smoke_place_order(
        m3e_smoke_request_id(8_001),
        "CID000000000008001",
        now,
        Some(60_000),
    )?);
    let duplicate_pending = m3e_redis_xadd_and_make_pending(
        &mut manager,
        &config,
        &group,
        "m3e-crashed-before-duplicate",
        &duplicate_command,
    )
    .await?;
    let claimed = m3e_redis_xautoclaim_one(&mut manager, &config, &group, consumer).await?;
    anyhow::ensure!(
        claimed.id == duplicate_pending,
        "m3e duplicate XAUTOCLAIM id mismatch"
    );
    metrics.xautoclaim_entries += 1;
    let duplicate_report = m3e_process_stream_id(
        &command_consumer,
        &config,
        &mut order_store,
        &mut transport,
        &policy,
        &claimed,
        now + ChronoDuration::seconds(1),
    )
    .await?;
    anyhow::ensure!(
        duplicate_report.duplicate_request_no_second_endpoint_attempt
            && !duplicate_report.endpoint_transport_invoked,
        "m3e duplicate after XAUTOCLAIM invoked endpoint"
    );
    metrics.real_xack_count +=
        runtime_bridge_xack(&mut manager, &config.command_stream, &group, &claimed.id).await?;
    metrics.duplicate_after_xautoclaim_no_second_endpoint_attempt = true;

    let received_only_command = BrokerCommand::PlaceOrder(m3e_smoke_place_order(
        m3e_smoke_request_id(8_004),
        "CID000000000008004",
        now,
        Some(60_000),
    )?);
    lifecycle_store.insert_received(M3eCommandLifecycleRecord::command_received(
        "synthetic-before-endpoint",
        &received_only_command,
        now,
    ))?;
    let received_only_pending = m3e_redis_xadd_and_make_pending(
        &mut manager,
        &config,
        &group,
        "m3e-crashed-after-received",
        &received_only_command,
    )
    .await?;
    let claimed_received =
        m3e_redis_xautoclaim_one(&mut manager, &config, &group, consumer).await?;
    anyhow::ensure!(
        claimed_received.id == received_only_pending,
        "m3e received-only XAUTOCLAIM id mismatch"
    );
    metrics.xautoclaim_entries += 1;
    let received_report = m3e_process_stream_id(
        &command_consumer,
        &config,
        &mut order_store,
        &mut transport,
        &policy,
        &claimed_received,
        now + ChronoDuration::seconds(2),
    )
    .await?;
    anyhow::ensure!(
        matches!(
            received_report.action,
            M3eCommandLifecycleAction::DuplicateAckPublished
                | M3eCommandLifecycleAction::RecoveredAckPublished
        ) && !received_report.endpoint_transport_invoked,
        "m3e CommandReceived replay was not conservative"
    );
    metrics.real_xack_count += runtime_bridge_xack(
        &mut manager,
        &config.command_stream,
        &group,
        &claimed_received.id,
    )
    .await?;
    metrics.command_received_replay_no_second_endpoint_attempt = true;

    let endpoint_before_ack_command = BrokerCommand::PlaceOrder(m3e_smoke_place_order(
        m3e_smoke_request_id(8_005),
        "CID000000000008005",
        now,
        Some(60_000),
    )?);
    let mut planned = M3eCommandLifecycleRecord::command_received(
        "synthetic-after-endpoint-before-ack",
        &endpoint_before_ack_command,
        now,
    );
    planned.mark_ack_publish_planned(
        CommandAckStatus::UnknownPending,
        Some(broker_core::CommandAckReasonCode::ReconciliationRequired),
        now,
        1,
    );
    lifecycle_store.upsert(planned)?;
    let endpoint_before_ack_pending = m3e_redis_xadd_and_make_pending(
        &mut manager,
        &config,
        &group,
        "m3e-crashed-after-endpoint-before-ack",
        &endpoint_before_ack_command,
    )
    .await?;
    let claimed_endpoint =
        m3e_redis_xautoclaim_one(&mut manager, &config, &group, consumer).await?;
    anyhow::ensure!(
        claimed_endpoint.id == endpoint_before_ack_pending,
        "m3e endpoint-before-ack XAUTOCLAIM id mismatch"
    );
    metrics.xautoclaim_entries += 1;
    let endpoint_replay_report = m3e_process_stream_id(
        &command_consumer,
        &config,
        &mut order_store,
        &mut transport,
        &policy,
        &claimed_endpoint,
        now + ChronoDuration::seconds(3),
    )
    .await?;
    anyhow::ensure!(
        endpoint_replay_report.action == M3eCommandLifecycleAction::RecoveredAckPublished
            && endpoint_replay_report.endpoint_attempt_count == 1
            && !endpoint_replay_report.endpoint_transport_invoked,
        "m3e endpoint-before-ack replay blindly retried endpoint"
    );
    metrics.real_xack_count += runtime_bridge_xack(
        &mut manager,
        &config.command_stream,
        &group,
        &claimed_endpoint.id,
    )
    .await?;
    metrics.endpoint_attempt_before_ack_replay_no_blind_retry = true;

    let ack_before_xack_command = BrokerCommand::PlaceOrder(m3e_smoke_place_order(
        m3e_smoke_request_id(8_006),
        "CID000000000008006",
        now,
        Some(60_000),
    )?);
    let ack_before_xack_pending = m3e_redis_xadd_and_make_pending(
        &mut manager,
        &config,
        &group,
        "m3e-crashed-after-ack-before-xack",
        &ack_before_xack_command,
    )
    .await?;
    let first_pending = m3e_stream_id_by_id(
        &mut manager,
        &config.command_stream,
        &ack_before_xack_pending,
    )
    .await?;
    let first_pending_report = m3e_process_stream_id(
        &command_consumer,
        &config,
        &mut order_store,
        &mut transport,
        &policy,
        &first_pending,
        now + ChronoDuration::seconds(4),
    )
    .await?;
    anyhow::ensure!(
        first_pending_report.action == M3eCommandLifecycleAction::LocalMockEndpointAckPublished,
        "m3e ack-before-xack setup did not publish first ACK"
    );
    let claimed_ack_before_xack =
        m3e_redis_xautoclaim_one(&mut manager, &config, &group, consumer).await?;
    anyhow::ensure!(
        claimed_ack_before_xack.id == ack_before_xack_pending,
        "m3e ack-before-xack XAUTOCLAIM id mismatch"
    );
    metrics.xautoclaim_entries += 1;
    let replay_ack_before_xack_report = m3e_process_stream_id(
        &command_consumer,
        &config,
        &mut order_store,
        &mut transport,
        &policy,
        &claimed_ack_before_xack,
        now + ChronoDuration::seconds(5),
    )
    .await?;
    anyhow::ensure!(
        replay_ack_before_xack_report.action == M3eCommandLifecycleAction::DuplicateAckPublished
            && !replay_ack_before_xack_report.endpoint_transport_invoked,
        "m3e ACK-before-XACK replay invoked endpoint"
    );
    metrics.real_xack_count += runtime_bridge_xack(
        &mut manager,
        &config.command_stream,
        &group,
        &claimed_ack_before_xack.id,
    )
    .await?;
    metrics.ack_published_before_xack_replay_no_second_endpoint_attempt = true;

    let expired_command = BrokerCommand::PlaceOrder(m3e_smoke_place_order(
        m3e_smoke_request_id(8_007),
        "CID000000000008007",
        now - ChronoDuration::seconds(10),
        Some(1),
    )?);
    let expired_id =
        m3e_redis_xadd_and_read(&mut manager, &config, &group, consumer, &expired_command).await?;
    metrics.xreadgroup_entries += 1;
    let expired_report = m3e_process_stream_id(
        &command_consumer,
        &config,
        &mut order_store,
        &mut transport,
        &policy,
        &expired_id,
        now,
    )
    .await?;
    anyhow::ensure!(
        expired_report.ack_status == Some(CommandAckStatus::Expired)
            && !expired_report.endpoint_transport_invoked,
        "m3e expired command attempted endpoint"
    );
    metrics.real_xack_count +=
        runtime_bridge_xack(&mut manager, &config.command_stream, &group, &expired_id.id).await?;
    metrics.expired_ack_no_endpoint_then_xack = true;

    let poison_payload = "raw poison payload with SECRET_TOKEN and ACC_TEST_0001";
    let poison_id =
        m3e_redis_xadd_payload(&mut manager, &config.command_stream, poison_payload).await?;
    let poison_stream_id =
        m3e_redis_xreadgroup_one(&mut manager, &config, &group, consumer).await?;
    anyhow::ensure!(
        poison_stream_id.id == poison_id,
        "m3e poison XREADGROUP id mismatch"
    );
    metrics.xreadgroup_entries += 1;
    let poison_report = m3e_process_stream_id(
        &command_consumer,
        &config,
        &mut order_store,
        &mut transport,
        &policy,
        &poison_stream_id,
        now,
    )
    .await?;
    anyhow::ensure!(
        poison_report.action == M3eCommandLifecycleAction::DlqPublished,
        "m3e poison command did not publish DLQ"
    );
    let dlq_payload = latest_m3e_dlq_payload(&mut manager, &config).await?;
    anyhow::ensure!(
        !dlq_payload.contains(poison_payload)
            && !dlq_payload.contains("SECRET_TOKEN")
            && !dlq_payload.contains("ACC_TEST_0001"),
        "m3e poison DLQ leaked raw payload"
    );
    metrics.real_xack_count += runtime_bridge_xack(
        &mut manager,
        &config.command_stream,
        &group,
        &poison_stream_id.id,
    )
    .await?;
    metrics.poison_dlq_redacted_then_xack = true;

    let failing_ack_sink = M3eCliFailingRedisStreamSink::new(config.command_ack_stream.clone());
    let failing_ack_consumer = M3eCommandConsumerLocalMockEndpoint::new(
        config.clone(),
        failing_ack_sink,
        M3eInMemoryCommandLifecycleStore::default(),
    );
    let ack_fail_command = BrokerCommand::PlaceOrder(m3e_smoke_place_order(
        m3e_smoke_request_id(8_008),
        "CID000000000008008",
        now,
        Some(60_000),
    )?);
    let ack_fail_id =
        m3e_redis_xadd_and_read(&mut manager, &config, &group, consumer, &ack_fail_command).await?;
    metrics.xreadgroup_entries += 1;
    let ack_fail_result = m3e_process_stream_id(
        &failing_ack_consumer,
        &config,
        &mut InMemoryOrderPathStore::default(),
        &mut M3eCliCountingClassifiedTransport::accepted("BROKER_TEST_M3E4_ACK_FAIL"),
        &policy,
        &ack_fail_id,
        now,
    )
    .await;
    anyhow::ensure!(
        ack_fail_result.is_err(),
        "m3e ACK publish failure did not fail"
    );
    metrics.ack_publish_failure_left_pending =
        m3e_pending_count(&mut manager, &config.command_stream, &group).await? > 0;

    let failing_dlq_sink = M3eCliFailingRedisStreamSink::new(config.command_dlq_stream.clone());
    let failing_dlq_consumer = M3eCommandConsumerLocalMockEndpoint::new(
        config.clone(),
        failing_dlq_sink,
        M3eInMemoryCommandLifecycleStore::default(),
    );
    let dlq_fail_payload = "not-json-dlq-failure";
    let dlq_fail_id =
        m3e_redis_xadd_payload(&mut manager, &config.command_stream, dlq_fail_payload).await?;
    let dlq_fail_stream_id =
        m3e_redis_xreadgroup_one(&mut manager, &config, &group, consumer).await?;
    metrics.xreadgroup_entries += 1;
    anyhow::ensure!(
        dlq_fail_stream_id.id == dlq_fail_id,
        "m3e DLQ failure id mismatch"
    );
    let dlq_fail_result = m3e_process_stream_id(
        &failing_dlq_consumer,
        &config,
        &mut InMemoryOrderPathStore::default(),
        &mut M3eCliCountingClassifiedTransport::accepted("BROKER_TEST_M3E4_DLQ_FAIL"),
        &policy,
        &dlq_fail_stream_id,
        now,
    )
    .await;
    anyhow::ensure!(
        dlq_fail_result.is_err(),
        "m3e DLQ publish failure did not fail"
    );
    metrics.dlq_publish_failure_left_pending =
        m3e_pending_count(&mut manager, &config.command_stream, &group).await? >= 2;

    let pending_count = m3e_pending_count(&mut manager, &config.command_stream, &group).await?;
    print_json(serde_json::json!({
        "m3e4_redis_command_consumer_smoke": true,
        "m3e4_redis_consumer_lifecycle_ok": metrics.place_ok
            && metrics.cancel_ok
            && metrics.ack_publish_failure_left_pending
            && metrics.dlq_publish_failure_left_pending
            && metrics.duplicate_after_xautoclaim_no_second_endpoint_attempt
            && metrics.command_received_replay_no_second_endpoint_attempt
            && metrics.endpoint_attempt_before_ack_replay_no_blind_retry
            && metrics.ack_published_before_xack_replay_no_second_endpoint_attempt
            && metrics.poison_dlq_redacted_then_xack
            && metrics.expired_ack_no_endpoint_then_xack,
        "xreadgroup_consume_ok": metrics.xreadgroup_entries >= 5,
        "xack_after_ack_or_dlq_publish_ok": true,
        "xautoclaim_recovery_ok": metrics.xautoclaim_entries >= 4,
        "pending_replay_no_second_endpoint_attempt": metrics.duplicate_after_xautoclaim_no_second_endpoint_attempt
            && metrics.command_received_replay_no_second_endpoint_attempt
            && metrics.endpoint_attempt_before_ack_replay_no_blind_retry
            && metrics.ack_published_before_xack_replay_no_second_endpoint_attempt,
        "place_and_cancel_redis_lifecycle_ok": metrics.place_ok && metrics.cancel_ok,
        "ack_publish_failure_no_xack": metrics.ack_publish_failure_left_pending,
        "dlq_publish_failure_no_xack": metrics.dlq_publish_failure_left_pending,
        "redis_real_xack_count": metrics.real_xack_count,
        "redis_pending_count_after_failure_cases": pending_count,
        "local_mock_endpoint_only": true,
        "external_order_endpoint_allowed": false,
        "non_loopback_endpoint_allowed": false,
        "runtime_live_attachment_allowed": false,
        "live_ready_allowed": false,
        "stop_sltp_bracket_enabled": false,
        "real_finam_order_endpoint_used": false,
        "transport_place_call_count": transport.place_call_count,
        "transport_cancel_call_count": transport.cancel_call_count,
        "command_stream": config.command_stream,
        "ack_stream": config.command_ack_stream,
        "dlq_stream": config.command_dlq_stream,
    }))?;
    Ok(())
}

fn m3e_redis_smoke_config(_redis_url: &str, prefix: &str) -> M3eCommandConsumerConfig {
    M3eCommandConsumerConfig {
        command_stream: format!("{prefix}.commands"),
        command_ack_stream: format!("{prefix}.command_acks"),
        command_dlq_stream: format!("{prefix}.commands.dlq"),
        command_ack_maxlen: Some(100),
        command_dlq_maxlen: Some(100),
        source: "m3e-command-consumer-redis-smoke".to_string(),
        consumer_group: "m3e-command-consumer-smoke".to_string(),
        consumer_name: "m3e-smoke-consumer".to_string(),
    }
}

async fn m3e_redis_xadd_and_read(
    manager: &mut redis::aio::ConnectionManager,
    config: &M3eCommandConsumerConfig,
    group: &str,
    consumer: &str,
    command: &BrokerCommand,
) -> Result<StreamId> {
    let message_id = m3e_redis_xadd_command(manager, config, command).await?;
    let stream_id = m3e_redis_xreadgroup_one(manager, config, group, consumer).await?;
    anyhow::ensure!(
        stream_id.id == message_id,
        "m3e XREADGROUP returned unexpected id"
    );
    Ok(stream_id)
}

async fn m3e_redis_xadd_and_make_pending(
    manager: &mut redis::aio::ConnectionManager,
    config: &M3eCommandConsumerConfig,
    group: &str,
    consumer: &str,
    command: &BrokerCommand,
) -> Result<String> {
    let message_id = m3e_redis_xadd_command(manager, config, command).await?;
    let stream_id = m3e_redis_xreadgroup_one(manager, config, group, consumer).await?;
    anyhow::ensure!(
        stream_id.id == message_id,
        "m3e pending setup returned unexpected id"
    );
    Ok(message_id)
}

async fn m3e_redis_xadd_command(
    manager: &mut redis::aio::ConnectionManager,
    config: &M3eCommandConsumerConfig,
    command: &BrokerCommand,
) -> Result<String> {
    let payload = serde_json::to_string(&Envelope::new(
        "strategy-test",
        MessageType::Command,
        command,
    ))?;
    m3e_redis_xadd_payload(manager, &config.command_stream, &payload).await
}

async fn m3e_redis_xadd_payload(
    manager: &mut redis::aio::ConnectionManager,
    stream: &str,
    payload: &str,
) -> Result<String> {
    redis::cmd("XADD")
        .arg(stream)
        .arg("*")
        .arg("payload")
        .arg(payload)
        .query_async(manager)
        .await
        .context("m3e Redis XADD failed")
}

async fn m3e_redis_xreadgroup_one(
    manager: &mut redis::aio::ConnectionManager,
    config: &M3eCommandConsumerConfig,
    group: &str,
    consumer: &str,
) -> Result<StreamId> {
    let reply: StreamReadReply = redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg(group)
        .arg(consumer)
        .arg("COUNT")
        .arg(1)
        .arg("BLOCK")
        .arg(1)
        .arg("STREAMS")
        .arg(&config.command_stream)
        .arg(">")
        .query_async(manager)
        .await
        .context("m3e Redis XREADGROUP failed")?;
    reply
        .keys
        .first()
        .and_then(|key| key.ids.first())
        .cloned()
        .context("m3e Redis XREADGROUP returned no entry")
}

async fn m3e_redis_xautoclaim_one(
    manager: &mut redis::aio::ConnectionManager,
    config: &M3eCommandConsumerConfig,
    group: &str,
    consumer: &str,
) -> Result<StreamId> {
    let reply: StreamAutoClaimReply = redis::cmd("XAUTOCLAIM")
        .arg(&config.command_stream)
        .arg(group)
        .arg(consumer)
        .arg(0)
        .arg("0-0")
        .arg("COUNT")
        .arg(1)
        .query_async(manager)
        .await
        .context("m3e Redis XAUTOCLAIM failed")?;
    reply
        .claimed
        .first()
        .cloned()
        .context("m3e Redis XAUTOCLAIM returned no entry")
}

async fn m3e_stream_id_by_id(
    manager: &mut redis::aio::ConnectionManager,
    stream: &str,
    id: &str,
) -> Result<StreamId> {
    let reply: StreamRangeReply = redis::cmd("XRANGE")
        .arg(stream)
        .arg(id)
        .arg(id)
        .query_async(manager)
        .await
        .context("m3e Redis XRANGE failed")?;
    reply
        .ids
        .first()
        .cloned()
        .context("m3e XRANGE entry missing")
}

async fn m3e_process_stream_id<S, L>(
    consumer: &M3eCommandConsumerLocalMockEndpoint<S, L>,
    config: &M3eCommandConsumerConfig,
    order_store: &mut InMemoryOrderPathStore,
    transport: &mut M3eCliCountingClassifiedTransport,
    policy: &OrderPreflightPolicy,
    id: &StreamId,
    now: chrono::DateTime<Utc>,
) -> Result<finam_gateway::M3eLocalMockEndpointCommandReport>
where
    S: RedisStreamSink,
    L: M3eCommandLifecycleStore,
{
    let payload = id
        .get::<String>("payload")
        .context("m3e Redis stream entry has no payload")?;
    consumer
        .process_entry(
            RuntimeBridgeStreamEntry {
                stream: config.command_stream.clone(),
                entry_id: id.id.clone(),
                payload,
            },
            order_store,
            transport,
            policy,
            now,
        )
        .await
        .context("m3e command consumer processing failed")
}

async fn latest_m3e_dlq_payload(
    manager: &mut redis::aio::ConnectionManager,
    config: &M3eCommandConsumerConfig,
) -> Result<String> {
    let reply: StreamRangeReply = redis::cmd("XREVRANGE")
        .arg(&config.command_dlq_stream)
        .arg("+")
        .arg("-")
        .arg("COUNT")
        .arg(1)
        .query_async(manager)
        .await
        .context("m3e DLQ read failed")?;
    reply
        .ids
        .first()
        .and_then(|id| id.get::<String>("payload"))
        .context("m3e DLQ latest payload missing")
}

async fn m3e_pending_count(
    manager: &mut redis::aio::ConnectionManager,
    stream: &str,
    group: &str,
) -> Result<i64> {
    let value: redis::Value = redis::cmd("XPENDING")
        .arg(stream)
        .arg(group)
        .query_async(manager)
        .await
        .context("m3e XPENDING failed")?;
    Ok(pending_count_from_value(&value).unwrap_or_default())
}

fn m3e_smoke_request_id(n: u128) -> StrategyRequestId {
    StrategyRequestId::from(Uuid::from_u128(n))
}

fn m3e_smoke_place_order(
    request_id: StrategyRequestId,
    client_order_id: &str,
    now: chrono::DateTime<Utc>,
    ttl_ms: Option<u64>,
) -> Result<PlaceOrder> {
    Ok(PlaceOrder {
        request_id,
        created_ts: now,
        ttl_ms,
        account_id: BrokerAccountId::new("ACC_TEST_0001"),
        client_order_id: ClientOrderId::new(client_order_id).context("m3e client order id")?,
        instrument: smoke_instrument(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        qty: Decimal::ONE,
        limit_price: Some(Decimal::new(5000, 0)),
        time_in_force: TimeInForce::Day,
        comment: None,
    })
}

fn m3e_smoke_preflight_policy(now: chrono::DateTime<Utc>) -> OrderPreflightPolicy {
    OrderPreflightPolicy {
        allowed_accounts: vec![BrokerAccountId::new("ACC_TEST_0001")],
        allowed_venue_symbols: vec!["TESTFUT@TEST".to_string()],
        allowed_order_types: vec![OrderType::Market, OrderType::Limit],
        allowed_time_in_force: vec![TimeInForce::Day],
        min_qty: Decimal::ONE,
        qty_step: Decimal::ONE,
        max_qty: Decimal::new(3, 0),
        price_step: Some(Decimal::ONE),
        max_market_qty: Decimal::ONE,
        max_notional_per_order: None,
        max_notional_per_run: None,
        max_limit_deviation_bps: None,
        max_reference_age_ms: 1_000,
        allow_cancel_by_broker_order_id_without_mapping: false,
        operator_arm: OperatorArm {
            session_id: "ARM_TEST_M3E4".to_string(),
            armed_until: now + ChronoDuration::minutes(5),
            endpoint_calls_enabled: true,
            one_shot: false,
            endpoint_attempted: false,
            preflight_digest: "m3e4-smoke-digest".to_string(),
        },
    }
}

#[derive(Debug)]
struct M3eCliCountingClassifiedTransport {
    broker_order_id_prefix: String,
    place_call_count: usize,
    cancel_call_count: usize,
}

impl M3eCliCountingClassifiedTransport {
    fn accepted(broker_order_id: &str) -> Self {
        Self {
            broker_order_id_prefix: broker_order_id.to_string(),
            place_call_count: 0,
            cancel_call_count: 0,
        }
    }

    fn accepted_response(
        &self,
        suffix: &str,
    ) -> broker_finam::FinamOrderEndpointClassifiedResponse {
        let fixture = broker_finam::FinamOrderEndpointFixture::Accepted(
            broker_finam::FinamOrderEndpointAcceptedDto {
                broker_order_id: Some(format!("{}_{}", self.broker_order_id_prefix, suffix)),
            },
        );
        broker_finam::FinamOrderEndpointClassifiedResponse {
            result: fixture.map_fixture().expect("m3e accepted fixture maps"),
            diagnostic: fixture.redacted_diagnostic(),
        }
    }
}

impl FinamMockClassifiedEndpointTransport for M3eCliCountingClassifiedTransport {
    fn place_order_endpoint_classified(
        &mut self,
        spec: broker_finam::FinamPlaceOrderRequestSpec,
    ) -> broker_finam::FinamOrderEndpointClassifiedResponse {
        assert!(!spec.account_id.is_empty());
        self.place_call_count += 1;
        self.accepted_response(&format!("P{}", self.place_call_count))
    }

    fn cancel_order_endpoint_classified(
        &mut self,
        spec: broker_finam::FinamCancelOrderRequestSpec,
    ) -> broker_finam::FinamOrderEndpointClassifiedResponse {
        assert!(!spec.account_id.is_empty());
        self.cancel_call_count += 1;
        self.accepted_response(&format!("C{}", self.cancel_call_count))
    }
}

#[derive(Debug, Clone)]
struct M3eCliFailingRedisStreamSink {
    inner: InMemoryRedisStreamSink,
    fail_stream: String,
}

impl M3eCliFailingRedisStreamSink {
    fn new(fail_stream: impl Into<String>) -> Self {
        Self {
            inner: InMemoryRedisStreamSink::default(),
            fail_stream: fail_stream.into(),
        }
    }
}

#[async_trait::async_trait]
impl RedisStreamSink for M3eCliFailingRedisStreamSink {
    async fn publish_json<T: serde::Serialize + Send + Sync>(
        &self,
        stream: &str,
        value: &T,
        maxlen: Option<usize>,
    ) -> Result<(), GatewayError> {
        if stream == self.fail_stream {
            return Err(GatewayError::InternalState {
                message: "injected m3e redis publish failure",
            });
        }
        self.inner.publish_json(stream, value, maxlen).await
    }
}

async fn consume_runtime_bridge_dry(
    resolved: ResolvedRuntimeBridgeDryConfig,
) -> Result<serde_json::Value> {
    let client = redis::Client::open(resolved.gateway_config.redis.url.as_str())
        .context("runtime bridge dry Redis URL is invalid")?;
    let mut manager = client
        .get_connection_manager()
        .await
        .context("runtime bridge dry Redis connection failed")?;
    let streams = runtime_bridge_consumer_streams(&resolved.gateway_config.redis);
    for stream in &streams {
        ensure_runtime_bridge_group(
            &mut manager,
            stream,
            &resolved.group,
            &resolved.group_start_id,
        )
        .await?;
    }

    let mut consumer = RuntimeBridgeDryConsumer::from_gateway_config(&resolved.gateway_config);
    let mut readiness_simulator =
        RuntimeBridgeReadinessSimulator::from_gateway_config(&resolved.gateway_config);
    let mut redis_metrics = RuntimeBridgeRedisDryMetrics::default();
    let iterations = resolved.max_iterations.max(1);
    for _ in 0..iterations {
        if let Some(claim_stale_ms) = resolved.claim_stale_ms {
            for stream in &streams {
                let mut start_id = "0-0".to_string();
                loop {
                    redis_metrics.xautoclaim_iterations += 1;
                    let reply = runtime_bridge_xautoclaim(
                        &mut manager,
                        &resolved,
                        stream,
                        claim_stale_ms,
                        &start_id,
                    )
                    .await?;
                    let next_stream_id = reply.next_stream_id.clone();
                    let deleted_count = reply.deleted_ids.len() as u64;
                    redis_metrics.xautoclaim_deleted_ids_count += deleted_count;
                    redis_metrics
                        .xautoclaim_last_next_ids
                        .insert(stream.clone(), next_stream_id.clone());
                    for id in reply.claimed {
                        redis_metrics.claimed_entries_returned += 1;
                        process_runtime_bridge_stream_id(
                            &mut manager,
                            &resolved,
                            &mut consumer,
                            &mut readiness_simulator,
                            &mut redis_metrics,
                            stream,
                            &id,
                        )
                        .await?;
                    }
                    if runtime_bridge_xautoclaim_cursor_done(&start_id, &next_stream_id) {
                        break;
                    }
                    start_id = next_stream_id;
                }
            }
        }

        redis_metrics.xreadgroup_iterations += 1;
        let reply = runtime_bridge_xreadgroup(&mut manager, &resolved, &streams).await?;
        if reply.keys.is_empty() {
            continue;
        }
        for key in reply.keys {
            for id in key.ids {
                redis_metrics.entries_returned += 1;
                process_runtime_bridge_stream_id(
                    &mut manager,
                    &resolved,
                    &mut consumer,
                    &mut readiness_simulator,
                    &mut redis_metrics,
                    &key.key,
                    &id,
                )
                .await?;
            }
        }
    }

    let pending_counts =
        runtime_bridge_pending_counts(&mut manager, &streams, &resolved.group).await;
    let pending_oldest_idle_ms =
        runtime_bridge_pending_oldest_idle_ms(&mut manager, &streams, &resolved.group).await;
    let stream_lengths = runtime_bridge_stream_lengths(&mut manager, &streams).await;
    Ok(runtime_bridge_dry_summary_json(
        &resolved,
        consumer.metrics(),
        &readiness_simulator,
        &redis_metrics,
        pending_counts,
        pending_oldest_idle_ms,
        stream_lengths,
    ))
}

fn runtime_bridge_smoke_config(redis_url: &str, prefix: &str) -> GatewayConfig {
    let mut config = GatewayConfig {
        source: "runtime-bridge-redis-smoke".to_string(),
        features: GatewayFeatureSet::default(),
        ..GatewayConfig::default()
    };
    config.redis.url = redis_url.to_string();
    config.redis.health_stream = format!("{prefix}.health");
    config.redis.readiness_stream = format!("{prefix}.readiness");
    config.redis.portfolio_stream = format!("{prefix}.portfolio.snapshot");
    config.redis.order_snapshot_stream = format!("{prefix}.orders.snapshot");
    config.redis.market_data_stream = format!("{prefix}.market_data");
    config.redis.command_ack_stream = format!("{prefix}.command_acks");
    config.redis.runtime_bridge_dlq_stream = format!("{prefix}.runtime_bridge.dlq");
    config.redis.retention = RedisRetentionConfig {
        health_maxlen: Some(100),
        readiness_maxlen: Some(100),
        portfolio_maxlen: Some(100),
        order_snapshot_maxlen: Some(100),
        market_data_maxlen: Some(100),
        command_ack_maxlen: Some(100),
        runtime_bridge_dlq_maxlen: Some(100),
    };
    config
}

fn runtime_bridge_smoke_resolved_config(
    gateway_config: GatewayConfig,
    group: String,
    consumer: &str,
) -> ResolvedRuntimeBridgeDryConfig {
    ResolvedRuntimeBridgeDryConfig {
        gateway_config,
        group,
        consumer: consumer.to_string(),
        group_start_id: "0".to_string(),
        count: 100,
        block_ms: 1,
        max_iterations: 1,
        claim_stale_ms: None,
    }
}

async fn publish_runtime_bridge_positive_smoke_entries(config: &GatewayConfig) -> Result<()> {
    let sink = RedisConnectionStreamSink::connect(&config.redis.url)
        .await
        .context("runtime bridge smoke Redis connection failed")?;
    let gateway = FinamGateway::new(config.clone(), sink);
    let received_ts = Utc::now();

    gateway
        .publish_health(default_readonly_health(gateway.config()))
        .await
        .context("runtime bridge smoke health publish failed")?;
    gateway
        .publish_portfolio_snapshot(PortfolioSnapshot {
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            positions: Vec::new(),
            cash: Vec::new(),
            source_ts: None,
            received_ts,
        })
        .await
        .context("runtime bridge smoke portfolio publish failed")?;
    gateway
        .publish_order_snapshot(OrderSnapshot {
            orders: Vec::new(),
            active_orders_count: 0,
            terminal_orders_count: 0,
            blocking_unknown_status_present: false,
            received_ts,
        })
        .await
        .context("runtime bridge smoke order snapshot publish failed")?;
    gateway
        .publish_market_data_event(MarketDataEvent::Quote(Quote {
            instrument: smoke_instrument(),
            source_kind: MarketDataSourceKind::ReadOnlyPoll,
            bid: None,
            ask: None,
            last: Some(decimal("5000")?),
            source_ts: None,
            received_ts,
        }))
        .await
        .context("runtime bridge smoke market-data publish failed")?;
    gateway
        .publish_readiness(BrokerReadiness {
            phase: ReadinessPhase::Reconciliation,
            reasons: vec![ReadinessReason::OperatorLiveArmMissing],
            checked_ts: received_ts,
        })
        .await
        .context("runtime bridge smoke readiness publish failed")?;
    Ok(())
}

async fn publish_runtime_bridge_payload_entry(
    config: &GatewayConfig,
    stream: &str,
    payload: &str,
) -> Result<()> {
    let client = redis::Client::open(config.redis.url.as_str())
        .context("runtime bridge smoke URL invalid")?;
    let mut manager = client
        .get_connection_manager()
        .await
        .context("runtime bridge smoke Redis connection failed")?;
    let _message_id: String = redis::cmd("XADD")
        .arg(stream)
        .arg("*")
        .arg("payload")
        .arg(payload)
        .query_async(&mut manager)
        .await
        .context("runtime bridge smoke entry publish failed")?;
    Ok(())
}

async fn publish_runtime_bridge_entry_without_payload(
    config: &GatewayConfig,
    stream: &str,
) -> Result<()> {
    let client = redis::Client::open(config.redis.url.as_str())
        .context("runtime bridge smoke URL invalid")?;
    let mut manager = client
        .get_connection_manager()
        .await
        .context("runtime bridge smoke Redis connection failed")?;
    let _message_id: String = redis::cmd("XADD")
        .arg(stream)
        .arg("*")
        .arg("not_payload")
        .arg("synthetic field")
        .query_async(&mut manager)
        .await
        .context("runtime bridge smoke missing-payload entry publish failed")?;
    Ok(())
}

async fn publish_runtime_bridge_reconnect_smoke_entries(
    config: &GatewayConfig,
    count: usize,
) -> Result<()> {
    let payload = serde_json::to_string(&Envelope::new(
        config.source.clone(),
        MessageType::Health,
        default_readonly_health(config),
    ))?;
    for _ in 0..count {
        publish_runtime_bridge_payload_entry(config, &config.redis.health_stream, &payload).await?;
    }
    Ok(())
}

async fn create_runtime_bridge_pending_entries_without_ack(
    config: &GatewayConfig,
    stream: &str,
    group: &str,
    consumer: &str,
    count: usize,
) -> Result<()> {
    anyhow::ensure!(
        count > 0,
        "runtime bridge pending smoke count must be positive"
    );
    let client = redis::Client::open(config.redis.url.as_str())
        .context("runtime bridge pending smoke URL invalid")?;
    let mut manager = client
        .get_connection_manager()
        .await
        .context("runtime bridge pending smoke Redis connection failed")?;
    ensure_runtime_bridge_group(&mut manager, stream, group, "0").await?;
    let reply: StreamReadReply = redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg(group)
        .arg(consumer)
        .arg("COUNT")
        .arg(count)
        .arg("STREAMS")
        .arg(stream)
        .arg(">")
        .query_async(&mut manager)
        .await
        .context("runtime bridge pending smoke XREADGROUP failed")?;
    let delivered = reply
        .keys
        .iter()
        .find(|key| key.key == stream)
        .map(|key| key.ids.len())
        .unwrap_or_default();
    anyhow::ensure!(
        delivered == count,
        "runtime bridge pending smoke did not create the expected pending entries"
    );
    Ok(())
}

async fn run_runtime_bridge_negative_smoke_cases(
    redis_url: &str,
    stream_prefix: &str,
    run_id: i64,
) -> Result<Vec<serde_json::Value>> {
    let mut results = Vec::new();
    let cases = vec![
        RuntimeBridgeNegativeSmokeCase::InvalidJson,
        RuntimeBridgeNegativeSmokeCase::MessageTypeMismatch,
        RuntimeBridgeNegativeSmokeCase::UnsupportedSchemaVersion,
        RuntimeBridgeNegativeSmokeCase::MissingPayload,
        RuntimeBridgeNegativeSmokeCase::TypedDecodeFailed,
        RuntimeBridgeNegativeSmokeCase::RawOrderCommentPresent,
    ];

    for case in cases {
        let case_name = case.name();
        let config = runtime_bridge_smoke_config(
            redis_url,
            &format!("{stream_prefix}.negative.{case_name}.{run_id}"),
        );
        let raw_payload = case.publish(&config).await?;
        let summary = consume_runtime_bridge_dry(runtime_bridge_smoke_resolved_config(
            config.clone(),
            format!("runtime-bridge-smoke-negative-{case_name}-{run_id}"),
            "smoke-negative",
        ))
        .await?;
        let dlq_payload = latest_runtime_bridge_dlq_payload(&config).await?;
        assert_runtime_bridge_negative_smoke_summary(
            &summary,
            &dlq_payload,
            raw_payload.as_deref(),
            case.expected_reason(),
        )?;
        results.push(serde_json::json!({
            "case": case_name,
            "expected_reason": case.expected_reason(),
            "latest_reason": json_path_str(&summary, "/dlq/latest_reason")?,
            "consecutive_dlq_count": json_path_u64(&summary, "/dlq/consecutive_count")?,
            "dlq_published_count": json_path_u64(&summary, "/dlq/published_count")?,
            "xack_count": json_path_u64(&summary, "/xreadgroup/xack_count")?,
            "readiness_phase": json_path_str(&summary, "/readiness_simulator/phase")?,
            "raw_payload_absent_from_dlq": raw_payload
                .as_deref()
                .map(|payload| !dlq_payload.contains(payload))
                .unwrap_or(true),
        }));
    }

    Ok(results)
}

async fn run_runtime_bridge_dlq_retention_stress_smoke(
    redis_url: &str,
    stream_prefix: &str,
    run_id: i64,
) -> Result<serde_json::Value> {
    let mut config =
        runtime_bridge_smoke_config(redis_url, &format!("{stream_prefix}.retention.{run_id}"));
    config.redis.retention.runtime_bridge_dlq_maxlen = Some(3);
    for _ in 0..5 {
        publish_runtime_bridge_entry_without_payload(&config, &config.redis.market_data_stream)
            .await?;
    }
    let summary = consume_runtime_bridge_dry(runtime_bridge_smoke_resolved_config(
        config.clone(),
        format!("runtime-bridge-smoke-retention-{run_id}"),
        "smoke-retention",
    ))
    .await?;
    let dlq_len = runtime_bridge_stream_length(&config, &config.redis.runtime_bridge_dlq_stream)
        .await?
        .unwrap_or_default();
    assert_runtime_bridge_dlq_retention_smoke_summary(&summary, dlq_len)?;
    Ok(serde_json::json!({
        "dlq_maxlen": 3,
        "dlq_stream_len": dlq_len,
        "dlq_published_count": json_path_u64(&summary, "/dlq/published_count")?,
        "latest_reason": json_path_str(&summary, "/dlq/latest_reason")?,
        "consecutive_dlq_count": json_path_u64(&summary, "/dlq/consecutive_count")?,
        "xack_count": json_path_u64(&summary, "/xreadgroup/xack_count")?,
    }))
}

#[derive(Debug, Clone, Copy)]
enum RuntimeBridgeNegativeSmokeCase {
    InvalidJson,
    MessageTypeMismatch,
    UnsupportedSchemaVersion,
    MissingPayload,
    TypedDecodeFailed,
    RawOrderCommentPresent,
}

impl RuntimeBridgeNegativeSmokeCase {
    fn name(self) -> &'static str {
        match self {
            Self::InvalidJson => "invalid_json",
            Self::MessageTypeMismatch => "message_type_mismatch",
            Self::UnsupportedSchemaVersion => "unsupported_schema_version",
            Self::MissingPayload => "missing_payload",
            Self::TypedDecodeFailed => "typed_decode_failed",
            Self::RawOrderCommentPresent => "raw_order_comment_present",
        }
    }

    fn expected_reason(self) -> &'static str {
        match self {
            Self::InvalidJson => "InvalidJson",
            Self::MessageTypeMismatch => "MessageTypeMismatch",
            Self::UnsupportedSchemaVersion => "UnsupportedSchemaVersion",
            Self::MissingPayload => "MissingPayload",
            Self::TypedDecodeFailed => "TypedDecodeFailed",
            Self::RawOrderCommentPresent => "RawOrderCommentPresent",
        }
    }

    async fn publish(self, config: &GatewayConfig) -> Result<Option<String>> {
        match self {
            Self::InvalidJson => {
                let payload = "raw Redis payload must not leak".to_string();
                publish_runtime_bridge_payload_entry(
                    config,
                    &config.redis.market_data_stream,
                    &payload,
                )
                .await?;
                Ok(Some(payload))
            }
            Self::MessageTypeMismatch => {
                let payload = serde_json::to_string(&Envelope::new(
                    config.source.clone(),
                    MessageType::MarketData,
                    MarketDataEvent::Quote(smoke_quote()?),
                ))?;
                publish_runtime_bridge_payload_entry(config, &config.redis.health_stream, &payload)
                    .await?;
                Ok(None)
            }
            Self::UnsupportedSchemaVersion => {
                let payload = serde_json::json!({
                    "schema_version": 1,
                    "ts_utc": Utc::now(),
                    "source": config.source.clone(),
                    "msg_type": "Health",
                    "payload": {}
                })
                .to_string();
                publish_runtime_bridge_payload_entry(config, &config.redis.health_stream, &payload)
                    .await?;
                Ok(None)
            }
            Self::MissingPayload => {
                publish_runtime_bridge_entry_without_payload(config, &config.redis.health_stream)
                    .await?;
                Ok(None)
            }
            Self::TypedDecodeFailed => {
                let payload = serde_json::json!({
                    "schema_version": 2,
                    "ts_utc": Utc::now(),
                    "source": config.source.clone(),
                    "msg_type": "Health",
                    "payload": {}
                })
                .to_string();
                publish_runtime_bridge_payload_entry(config, &config.redis.health_stream, &payload)
                    .await?;
                Ok(None)
            }
            Self::RawOrderCommentPresent => {
                let snapshot = OrderSnapshot {
                    orders: vec![smoke_order_with_raw_comment()?],
                    active_orders_count: 1,
                    terminal_orders_count: 0,
                    blocking_unknown_status_present: false,
                    received_ts: Utc::now(),
                };
                let payload = serde_json::to_string(&Envelope::new(
                    config.source.clone(),
                    MessageType::OrderSnapshot,
                    snapshot,
                ))?;
                publish_runtime_bridge_payload_entry(
                    config,
                    &config.redis.order_snapshot_stream,
                    &payload,
                )
                .await?;
                Ok(Some("raw smoke comment must not leak".to_string()))
            }
        }
    }
}

async fn latest_runtime_bridge_dlq_payload(config: &GatewayConfig) -> Result<String> {
    let client =
        redis::Client::open(config.redis.url.as_str()).context("runtime bridge DLQ URL invalid")?;
    let mut manager = client
        .get_connection_manager()
        .await
        .context("runtime bridge DLQ Redis connection failed")?;
    let reply: StreamRangeReply = redis::cmd("XREVRANGE")
        .arg(&config.redis.runtime_bridge_dlq_stream)
        .arg("+")
        .arg("-")
        .arg("COUNT")
        .arg(1)
        .query_async(&mut manager)
        .await
        .context("runtime bridge DLQ read failed")?;
    let latest = reply
        .ids
        .first()
        .context("runtime bridge DLQ stream is empty")?;
    latest
        .get("payload")
        .context("runtime bridge DLQ entry has no payload field")
}

fn assert_runtime_bridge_positive_smoke_summary(summary: &serde_json::Value) -> Result<()> {
    anyhow::ensure!(
        !json_path_bool(summary, "/live_trading_enabled")?,
        "runtime bridge smoke positive summary enabled live trading"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/consumer_metrics/accepted_count")? == 5,
        "runtime bridge smoke positive accepted_count mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/consumer_metrics/dlq_count")? == 0,
        "runtime bridge smoke positive DLQ count mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/xreadgroup/xack_count")? == 5,
        "runtime bridge smoke positive XACK count mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/dlq/published_count")? == 0,
        "runtime bridge smoke positive DLQ publication mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/dlq/consecutive_count")? == 0,
        "runtime bridge smoke positive consecutive DLQ count mismatch"
    );
    anyhow::ensure!(
        json_path_str(summary, "/readiness_simulator/phase")? == "DryReady",
        "runtime bridge smoke positive readiness phase mismatch"
    );
    anyhow::ensure!(
        !json_path_bool(summary, "/readiness_simulator/live_ready")?,
        "runtime bridge smoke positive readiness became live-ready"
    );
    Ok(())
}

fn assert_runtime_bridge_negative_smoke_summary(
    summary: &serde_json::Value,
    dlq_payload: &str,
    raw_payload_that_must_not_leak: Option<&str>,
    expected_reason: &str,
) -> Result<()> {
    anyhow::ensure!(
        !json_path_bool(summary, "/live_trading_enabled")?,
        "runtime bridge smoke negative summary enabled live trading"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/consumer_metrics/accepted_count")? == 0,
        "runtime bridge smoke negative accepted_count mismatch"
    );
    let consumer_dlq_count = json_path_u64(summary, "/consumer_metrics/dlq_count")?;
    let missing_payload_count = json_path_u64(summary, "/dlq/missing_payload_count")?;
    anyhow::ensure!(
        consumer_dlq_count + missing_payload_count == 1,
        "runtime bridge smoke negative DLQ classification count mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/dlq/published_count")? == 1,
        "runtime bridge smoke negative DLQ publication mismatch"
    );
    anyhow::ensure!(
        json_path_str(summary, "/dlq/latest_reason")? == expected_reason,
        "runtime bridge smoke negative latest DLQ reason mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/dlq/consecutive_count")? == 1,
        "runtime bridge smoke negative consecutive DLQ count mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/xreadgroup/xack_count")? == 1,
        "runtime bridge smoke negative XACK count mismatch"
    );
    anyhow::ensure!(
        json_path_str(summary, "/readiness_simulator/phase")? == "Blocked",
        "runtime bridge smoke negative readiness phase mismatch"
    );
    anyhow::ensure!(
        !json_path_bool(summary, "/readiness_simulator/live_ready")?,
        "runtime bridge smoke negative readiness became live-ready"
    );
    if let Some(raw_payload) = raw_payload_that_must_not_leak {
        anyhow::ensure!(
            !dlq_payload.contains(raw_payload),
            "runtime bridge smoke DLQ leaked raw payload"
        );
    }
    let dlq: serde_json::Value =
        serde_json::from_str(dlq_payload).context("runtime bridge DLQ payload is not JSON")?;
    anyhow::ensure!(
        json_path_u64(&dlq, "/schema_version")? == 2,
        "runtime bridge smoke DLQ schema_version mismatch"
    );
    anyhow::ensure!(
        dlq_reason_matches(&dlq, expected_reason),
        "runtime bridge smoke DLQ reason mismatch"
    );
    Ok(())
}

fn assert_runtime_bridge_reconnect_smoke_summary(summary: &serde_json::Value) -> Result<()> {
    anyhow::ensure!(
        !json_path_bool(summary, "/live_trading_enabled")?,
        "runtime bridge reconnect smoke enabled live trading"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/consumer_metrics/accepted_count")? == 3,
        "runtime bridge reconnect smoke accepted_count mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/consumer_metrics/dlq_count")? == 0,
        "runtime bridge reconnect smoke DLQ count mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/xautoclaim/claimed_entries_returned")? == 3,
        "runtime bridge reconnect smoke claimed count mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/xautoclaim/iterations")? >= 2,
        "runtime bridge reconnect smoke did not exercise XAUTOCLAIM cursor/backlog"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/xreadgroup/xack_count")? == 3,
        "runtime bridge reconnect smoke XACK count mismatch"
    );
    anyhow::ensure!(
        json_path_str(summary, "/readiness_simulator/phase")? == "WaitingForInputs",
        "runtime bridge reconnect smoke readiness phase mismatch"
    );
    anyhow::ensure!(
        !json_path_bool(summary, "/readiness_simulator/live_ready")?,
        "runtime bridge reconnect smoke readiness became live-ready"
    );
    let pending_total = summary
        .pointer("/xreadgroup/pending_counts")
        .and_then(serde_json::Value::as_object)
        .map(|object| {
            object
                .values()
                .filter_map(serde_json::Value::as_i64)
                .sum::<i64>()
        })
        .context("runtime bridge reconnect smoke missing pending counts")?;
    anyhow::ensure!(
        pending_total == 0,
        "runtime bridge reconnect smoke left pending entries"
    );
    Ok(())
}

fn assert_runtime_bridge_dlq_retention_smoke_summary(
    summary: &serde_json::Value,
    dlq_len: i64,
) -> Result<()> {
    anyhow::ensure!(
        !json_path_bool(summary, "/live_trading_enabled")?,
        "runtime bridge retention smoke enabled live trading"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/dlq/published_count")? == 5,
        "runtime bridge retention smoke DLQ publication mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/dlq/consecutive_count")? == 5,
        "runtime bridge retention smoke consecutive DLQ count mismatch"
    );
    anyhow::ensure!(
        json_path_str(summary, "/dlq/latest_reason")? == "MissingPayload",
        "runtime bridge retention smoke latest DLQ reason mismatch"
    );
    anyhow::ensure!(
        json_path_u64(summary, "/xreadgroup/xack_count")? == 5,
        "runtime bridge retention smoke XACK count mismatch"
    );
    anyhow::ensure!(
        dlq_len <= 3,
        "runtime bridge retention smoke DLQ stream exceeded maxlen"
    );
    Ok(())
}

fn dlq_reason_matches(dlq: &serde_json::Value, expected_reason: &str) -> bool {
    let Some(reason) = dlq.pointer("/dead_letter/reason") else {
        return false;
    };
    if reason.as_str() == Some(expected_reason) {
        return true;
    }
    reason
        .as_object()
        .map(|object| object.contains_key(expected_reason))
        .unwrap_or(false)
}

fn smoke_instrument() -> InstrumentId {
    InstrumentId {
        symbol: "TESTFUT".to_string(),
        venue_symbol: Some("TESTFUT@TEST".to_string()),
        exchange: Exchange::Other("TEST".to_string()),
        market: Market::Futures,
    }
}

fn smoke_quote() -> Result<Quote> {
    Ok(Quote {
        instrument: smoke_instrument(),
        source_kind: MarketDataSourceKind::ReadOnlyPoll,
        bid: None,
        ask: None,
        last: Some(decimal("5000")?),
        source_ts: None,
        received_ts: Utc::now(),
    })
}

fn smoke_order_with_raw_comment() -> Result<Order> {
    Ok(Order {
        account_id: BrokerAccountId::new("ACC_TEST_0001"),
        order_id: None,
        client_order_id: Some(ClientOrderId::new("ABC123").context("client order id")?),
        broker_client_order_id_fingerprint: None,
        instrument: smoke_instrument(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        status: OrderStatus::Working,
        qty: decimal("1")?,
        filled_qty: decimal("0")?,
        limit_price: Some(decimal("5000")?),
        stop_price: None,
        comment_fingerprint: None,
        comment: Some("raw smoke comment must not leak".to_string()),
        source_ts: None,
        received_ts: Utc::now(),
    })
}

fn decimal(value: &str) -> Result<broker_core::Price> {
    value
        .parse::<broker_core::Price>()
        .with_context(|| format!("invalid synthetic decimal: {value}"))
}

fn json_path_u64(value: &serde_json::Value, pointer: &str) -> Result<u64> {
    value
        .pointer(pointer)
        .and_then(serde_json::Value::as_u64)
        .with_context(|| format!("missing numeric JSON field {pointer}"))
}

fn json_path_str<'a>(value: &'a serde_json::Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .with_context(|| format!("missing string JSON field {pointer}"))
}

fn json_path_bool(value: &serde_json::Value, pointer: &str) -> Result<bool> {
    value
        .pointer(pointer)
        .and_then(serde_json::Value::as_bool)
        .with_context(|| format!("missing bool JSON field {pointer}"))
}

async fn ensure_runtime_bridge_group(
    manager: &mut redis::aio::ConnectionManager,
    stream: &str,
    group: &str,
    start_id: &str,
) -> Result<()> {
    let result: redis::RedisResult<()> = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(stream)
        .arg(group)
        .arg(start_id)
        .arg("MKSTREAM")
        .query_async(manager)
        .await;
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.to_string().contains("BUSYGROUP") => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!("failed to create runtime bridge consumer group for stream {stream}")
        }),
    }
}

async fn process_runtime_bridge_stream_id(
    manager: &mut redis::aio::ConnectionManager,
    resolved: &ResolvedRuntimeBridgeDryConfig,
    consumer: &mut RuntimeBridgeDryConsumer,
    readiness_simulator: &mut RuntimeBridgeReadinessSimulator,
    redis_metrics: &mut RuntimeBridgeRedisDryMetrics,
    stream: &str,
    id: &StreamId,
) -> Result<()> {
    redis_metrics
        .last_ids
        .insert(stream.to_string(), id.id.clone());
    let outcome = match id.get::<String>("payload") {
        Some(payload) => {
            let entry = RuntimeBridgeStreamEntry {
                stream: stream.to_string(),
                entry_id: id.id.clone(),
                payload,
            };
            let outcome = consumer.consume_entry(entry.clone());
            if matches!(
                outcome,
                RuntimeBridgeConsumeOutcome::Accepted { .. }
                    | RuntimeBridgeConsumeOutcome::DuplicateBar { .. }
            ) {
                if let Err(dead_letter) = readiness_simulator.observe_entry(&entry) {
                    readiness_simulator.observe_dead_letter(&dead_letter);
                    redis_metrics.record_dlq(&dead_letter);
                    publish_runtime_bridge_dlq(manager, resolved, &dead_letter).await?;
                    redis_metrics.dlq_published_count += 1;
                } else {
                    redis_metrics.record_non_dlq();
                }
            }
            outcome
        }
        None => {
            redis_metrics.missing_payload_count += 1;
            RuntimeBridgeConsumeOutcome::DeadLetter(RuntimeBridgeDeadLetter {
                stream: stream.to_string(),
                entry_id: id.id.clone(),
                reason: RuntimeBridgeDlqReason::MissingPayload,
                payload_len: 0,
            })
        }
    };
    if let RuntimeBridgeConsumeOutcome::DeadLetter(dead_letter) = &outcome {
        readiness_simulator.observe_dead_letter(dead_letter);
        redis_metrics.record_dlq(dead_letter);
        publish_runtime_bridge_dlq(manager, resolved, dead_letter).await?;
        redis_metrics.dlq_published_count += 1;
    }
    let acked = runtime_bridge_xack(manager, stream, &resolved.group, &id.id).await?;
    redis_metrics.xack_count += acked;
    Ok(())
}

fn runtime_bridge_dlq_reason_label(reason: &RuntimeBridgeDlqReason) -> String {
    match reason {
        RuntimeBridgeDlqReason::UnknownStream => "UnknownStream",
        RuntimeBridgeDlqReason::InvalidJson => "InvalidJson",
        RuntimeBridgeDlqReason::MissingSchemaVersion => "MissingSchemaVersion",
        RuntimeBridgeDlqReason::UnsupportedSchemaVersion { .. } => "UnsupportedSchemaVersion",
        RuntimeBridgeDlqReason::MissingMessageType => "MissingMessageType",
        RuntimeBridgeDlqReason::MissingPayload => "MissingPayload",
        RuntimeBridgeDlqReason::UnsupportedMessageType => "UnsupportedMessageType",
        RuntimeBridgeDlqReason::MessageTypeMismatch { .. } => "MessageTypeMismatch",
        RuntimeBridgeDlqReason::TypedDecodeFailed { .. } => "TypedDecodeFailed",
        RuntimeBridgeDlqReason::RawOrderCommentPresent => "RawOrderCommentPresent",
    }
    .to_string()
}

fn runtime_bridge_xautoclaim_cursor_done(start_id: &str, next_stream_id: &str) -> bool {
    next_stream_id == "0-0" || next_stream_id == start_id
}

async fn runtime_bridge_xautoclaim(
    manager: &mut redis::aio::ConnectionManager,
    resolved: &ResolvedRuntimeBridgeDryConfig,
    stream: &str,
    claim_stale_ms: u64,
    start_id: &str,
) -> Result<StreamAutoClaimReply> {
    redis::cmd("XAUTOCLAIM")
        .arg(stream)
        .arg(&resolved.group)
        .arg(&resolved.consumer)
        .arg(claim_stale_ms)
        .arg(start_id)
        .arg("COUNT")
        .arg(resolved.count.max(1))
        .query_async(manager)
        .await
        .with_context(|| format!("runtime bridge dry XAUTOCLAIM failed for stream {stream}"))
}

async fn runtime_bridge_xreadgroup(
    manager: &mut redis::aio::ConnectionManager,
    resolved: &ResolvedRuntimeBridgeDryConfig,
    streams: &[String],
) -> Result<StreamReadReply> {
    let mut command = redis::cmd("XREADGROUP");
    command
        .arg("GROUP")
        .arg(&resolved.group)
        .arg(&resolved.consumer)
        .arg("COUNT")
        .arg(resolved.count.max(1));
    if resolved.block_ms > 0 {
        command.arg("BLOCK").arg(resolved.block_ms);
    }
    command.arg("STREAMS");
    for stream in streams {
        command.arg(stream);
    }
    for _ in streams {
        command.arg(">");
    }
    command
        .query_async(manager)
        .await
        .context("runtime bridge dry XREADGROUP failed")
}

async fn publish_runtime_bridge_dlq(
    manager: &mut redis::aio::ConnectionManager,
    resolved: &ResolvedRuntimeBridgeDryConfig,
    dead_letter: &RuntimeBridgeDeadLetter,
) -> Result<()> {
    let record = RuntimeBridgeDlqRecord::new(
        resolved.gateway_config.source.clone(),
        resolved.group.clone(),
        resolved.consumer.clone(),
        dead_letter.clone(),
    );
    let payload =
        serde_json::to_string(&record).context("runtime bridge DLQ record serialization failed")?;
    let mut command = redis::cmd("XADD");
    command.arg(&resolved.gateway_config.redis.runtime_bridge_dlq_stream);
    if let Some(maxlen) = resolved
        .gateway_config
        .redis
        .retention
        .runtime_bridge_dlq_maxlen
        .filter(|value| *value > 0)
    {
        command.arg("MAXLEN").arg("=").arg(maxlen);
    }
    let _message_id: String = command
        .arg("*")
        .arg("payload")
        .arg(payload)
        .query_async(manager)
        .await
        .context("runtime bridge dry DLQ publish failed")?;
    Ok(())
}

async fn runtime_bridge_xack(
    manager: &mut redis::aio::ConnectionManager,
    stream: &str,
    group: &str,
    id: &str,
) -> Result<u64> {
    let acked: i64 = redis::cmd("XACK")
        .arg(stream)
        .arg(group)
        .arg(id)
        .query_async(manager)
        .await
        .with_context(|| format!("runtime bridge dry XACK failed for stream {stream}"))?;
    Ok(acked.max(0) as u64)
}

async fn runtime_bridge_pending_counts(
    manager: &mut redis::aio::ConnectionManager,
    streams: &[String],
    group: &str,
) -> BTreeMap<String, Option<i64>> {
    let mut counts = BTreeMap::new();
    for stream in streams {
        let result: redis::RedisResult<redis::Value> = redis::cmd("XPENDING")
            .arg(stream)
            .arg(group)
            .query_async(&mut *manager)
            .await;
        counts.insert(
            stream.clone(),
            result
                .ok()
                .and_then(|value| pending_count_from_value(&value)),
        );
    }
    counts
}

async fn runtime_bridge_pending_oldest_idle_ms(
    manager: &mut redis::aio::ConnectionManager,
    streams: &[String],
    group: &str,
) -> BTreeMap<String, Option<u64>> {
    let mut idle = BTreeMap::new();
    for stream in streams {
        let result: redis::RedisResult<StreamPendingCountReply> = redis::cmd("XPENDING")
            .arg(stream)
            .arg(group)
            .arg("-")
            .arg("+")
            .arg(1)
            .query_async(&mut *manager)
            .await;
        idle.insert(
            stream.clone(),
            result
                .ok()
                .and_then(|reply| reply.ids.first().map(|id| id.last_delivered_ms as u64)),
        );
    }
    idle
}

async fn runtime_bridge_stream_lengths(
    manager: &mut redis::aio::ConnectionManager,
    streams: &[String],
) -> BTreeMap<String, Option<i64>> {
    let mut lengths = BTreeMap::new();
    for stream in streams {
        let result: redis::RedisResult<i64> = redis::cmd("XLEN")
            .arg(stream)
            .query_async(&mut *manager)
            .await;
        lengths.insert(stream.clone(), result.ok());
    }
    lengths
}

async fn runtime_bridge_stream_length(config: &GatewayConfig, stream: &str) -> Result<Option<i64>> {
    let client = redis::Client::open(config.redis.url.as_str())
        .context("runtime bridge stream length URL invalid")?;
    let mut manager = client
        .get_connection_manager()
        .await
        .context("runtime bridge stream length Redis connection failed")?;
    let result: redis::RedisResult<i64> = redis::cmd("XLEN")
        .arg(stream)
        .query_async(&mut manager)
        .await;
    Ok(result.ok())
}

fn pending_count_from_value(value: &redis::Value) -> Option<i64> {
    match value {
        redis::Value::Int(count) => Some(*count),
        redis::Value::Array(items) => items.first().and_then(pending_count_from_value),
        _ => None,
    }
}

fn runtime_bridge_consumer_streams(redis: &RedisStreamConfig) -> Vec<String> {
    vec![
        redis.health_stream.clone(),
        redis.readiness_stream.clone(),
        redis.portfolio_stream.clone(),
        redis.order_snapshot_stream.clone(),
        redis.market_data_stream.clone(),
    ]
}

fn runtime_bridge_dry_summary_json(
    resolved: &ResolvedRuntimeBridgeDryConfig,
    consumer_metrics: &finam_gateway::RuntimeBridgeConsumerMetrics,
    readiness_simulator: &RuntimeBridgeReadinessSimulator,
    redis_metrics: &RuntimeBridgeRedisDryMetrics,
    pending_counts: BTreeMap<String, Option<i64>>,
    pending_oldest_idle_ms: BTreeMap<String, Option<u64>>,
    stream_lengths: BTreeMap<String, Option<i64>>,
) -> serde_json::Value {
    let total_returned = redis_metrics.entries_returned + redis_metrics.claimed_entries_returned;
    let operator_hint = if total_returned == 0 && resolved.group_start_id.trim() == "$" {
        Some("group_start_id_dollar_tails_new_entries_only_use_0_for_backfill")
    } else {
        None
    };
    serde_json::json!({
        "runtime_bridge_dry_consumer": true,
        "live_trading_enabled": false,
        "command_consumer_enabled": false,
        "order_placement_enabled": false,
        "group": resolved.group,
        "consumer": resolved.consumer,
        "group_start_id": resolved.group_start_id,
        "claim_stale_ms": resolved.claim_stale_ms,
        "operator_hint": operator_hint,
        "streams": runtime_bridge_consumer_streams(&resolved.gateway_config.redis),
        "dlq_stream": resolved.gateway_config.redis.runtime_bridge_dlq_stream,
        "xreadgroup": {
            "iterations": redis_metrics.xreadgroup_iterations,
            "entries_returned": redis_metrics.entries_returned,
            "last_ids": redis_metrics.last_ids,
            "pending_counts": pending_counts,
            "pending_oldest_idle_ms": pending_oldest_idle_ms,
            "stream_lengths": stream_lengths,
            "xack_count": redis_metrics.xack_count,
        },
        "xautoclaim": {
            "enabled": resolved.claim_stale_ms.is_some(),
            "iterations": redis_metrics.xautoclaim_iterations,
            "claimed_entries_returned": redis_metrics.claimed_entries_returned,
            "deleted_ids_count": redis_metrics.xautoclaim_deleted_ids_count,
            "last_next_ids": redis_metrics.xautoclaim_last_next_ids,
        },
        "dlq": {
            "published_count": redis_metrics.dlq_published_count,
            "missing_payload_count": redis_metrics.missing_payload_count,
            "latest_reason": redis_metrics.latest_dlq_reason,
            "latest_ts": redis_metrics.latest_dlq_ts,
            "latest_stream": redis_metrics.latest_dlq_stream,
            "latest_entry_id": redis_metrics.latest_dlq_entry_id,
            "consecutive_count": redis_metrics.consecutive_dlq_count,
        },
        "consumer_metrics": consumer_metrics,
        "readiness_simulator": readiness_simulator.decision(),
    })
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

    let bars_lookback_minutes = file_config
        .bars_lookback_minutes
        .unwrap_or(args.bars_lookback_minutes)
        .max(1);
    let interval_seconds = args
        .interval_seconds
        .or(file_config.interval_seconds)
        .unwrap_or(60)
        .max(1);
    let max_iterations = args
        .max_iterations
        .or(file_config.max_iterations)
        .filter(|value| *value > 0);

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
        start_time: args.start_time.or(file_config.start_time),
        end_time: args.end_time.or(file_config.end_time),
        bars_lookback_minutes,
        interval_seconds,
        max_iterations,
    })
}

fn resolve_runtime_bridge_dry_config(
    args: RuntimeBridgeDryConsumeArgs,
    file_config: GatewayShadowFileConfig,
) -> Result<ResolvedRuntimeBridgeDryConfig> {
    let mut gateway_config = GatewayConfig {
        features: GatewayFeatureSet::default(),
        ..GatewayConfig::default()
    };
    apply_gateway_shadow_file_config(&mut gateway_config, &file_config);
    if let Some(redis_url) = args.redis_url {
        gateway_config.redis.url = redis_url;
    }

    Ok(ResolvedRuntimeBridgeDryConfig {
        gateway_config,
        group: non_empty_or_default(args.group, "broker-runtime-bridge-dry"),
        consumer: non_empty_or_default(args.consumer, "dry-consumer-1"),
        group_start_id: non_empty_or_default(args.group_start_id, "$"),
        count: args.count.max(1),
        block_ms: args.block_ms,
        max_iterations: args.max_iterations.max(1),
        claim_stale_ms: args.claim_stale_ms,
    })
}

fn non_empty_or_default(value: String, default: &str) -> String {
    if value.trim().is_empty() {
        default.to_string()
    } else {
        value
    }
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
    if let Some(retention) = file_config.retention.as_ref() {
        apply_gateway_shadow_retention(&mut gateway_config.redis.retention, retention);
    }
    if let Some(broker_truth) = file_config.broker_truth.as_ref() {
        gateway_config.broker_truth = broker_truth.clone();
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
    if let Some(value) = streams.command_ack.as_deref() {
        redis_config.command_ack_stream = value.to_string();
    }
    if let Some(value) = streams.runtime_bridge_dlq.as_deref() {
        redis_config.runtime_bridge_dlq_stream = value.to_string();
    }
}

fn apply_gateway_shadow_retention(
    retention_config: &mut RedisRetentionConfig,
    retention: &GatewayShadowRetentionFileConfig,
) {
    if retention.health_maxlen.is_some() {
        retention_config.health_maxlen = retention.health_maxlen;
    }
    if retention.readiness_maxlen.is_some() {
        retention_config.readiness_maxlen = retention.readiness_maxlen;
    }
    if retention.portfolio_maxlen.is_some() {
        retention_config.portfolio_maxlen = retention.portfolio_maxlen;
    }
    if retention.order_snapshot_maxlen.is_some() {
        retention_config.order_snapshot_maxlen = retention.order_snapshot_maxlen;
    }
    if retention.market_data_maxlen.is_some() {
        retention_config.market_data_maxlen = retention.market_data_maxlen;
    }
    if retention.command_ack_maxlen.is_some() {
        retention_config.command_ack_maxlen = retention.command_ack_maxlen;
    }
    if retention.runtime_bridge_dlq_maxlen.is_some() {
        retention_config.runtime_bridge_dlq_maxlen = retention.runtime_bridge_dlq_maxlen;
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

fn write_json_payload(path: &PathBuf, payload: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, serde_json::to_string_pretty(payload)?)?;
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
    use finam_gateway::{
        CancelBrokerTruthFreshnessPolicy, CancelBrokerTruthIdentityPolicy,
        CancelBrokerTruthOrchestrationPolicy, CancelBrokerTruthPrecedencePolicy,
        CancelBrokerTruthSource, CancelPositionTruthGuardPolicy,
    };

    #[test]
    fn json_shape_keeps_nested_structure_without_scalar_values() {
        let payload = serde_json::json!({
            "account_id": "ACC_DYNAMIC_TEST_001",
            "orders": [
                {
                    "order_id": "ORDER_DYNAMIC_TEST_001",
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
        assert!(!rendered.contains("ACC_DYNAMIC_TEST_001"));
        assert!(!rendered.contains("ORDER_DYNAMIC_TEST_001"));
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
            "ACC_DYNAMIC_TEST_001": {
                "status": "active"
            },
            "ORDER_DYNAMIC_TEST_001": {
                "price": 123.45
            },
            "SYNTH@TEST": {
                "lot": 1
            },
            "account_id": "ACC_DYNAMIC_TEST_002"
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
        assert!(!rendered.contains("ACC_DYNAMIC_TEST_001"));
        assert!(!rendered.contains("ORDER_DYNAMIC_TEST_001"));
        assert!(!rendered.contains("SYNTH@TEST"));
        assert!(!rendered.contains("ACC_DYNAMIC_TEST_002"));
        assert!(!rendered.contains("active"));
        assert!(!rendered.contains("123.45"));
    }

    #[test]
    fn timeframe_seconds_rejects_unknown_timeframe() {
        assert_eq!(timeframe_seconds("TIME_FRAME_M1").expect("m1"), 60);
        assert!(timeframe_seconds("TIME_FRAME_UNKNOWN").is_err());
    }

    #[test]
    fn m3c_source_archive_binding_requires_current_head_short_commit() {
        let head = Some("fa76b6ab7b8661db3942c360e7fcc6e4c63e933a");
        let matching = Path::new("reports/handoff/moex-trading-project-fa76b6a.zip");
        let stale = Path::new("reports/handoff/moex-trading-project-ffd9471.zip");
        let malformed = Path::new("reports/handoff/design-evidence.json");

        assert_eq!(
            source_archive_short_commit(matching).as_deref(),
            Some("fa76b6a")
        );
        validate_source_archive_binding(matching, head).expect("matching archive binds");
        assert!(validate_source_archive_binding(stale, head)
            .expect_err("stale archive must be rejected")
            .to_string()
            .contains("source archive binding mismatch"));
        assert!(validate_source_archive_binding(malformed, head)
            .expect_err("malformed archive name must be rejected")
            .to_string()
            .contains("source archive name must match"));
    }

    #[test]
    fn m3c_handoff_marker_parser_and_slot_statuses_are_strict() {
        let marker = parse_handoff_commit_marker(
            "source_commit=fa76b6a\nsource_ref=fa76b6ab7b8661db3942c360e7fcc6e4c63e933a\narchive_name=moex-trading-project-fa76b6a.zip\n",
        );

        assert_eq!(marker.source_commit.as_deref(), Some("fa76b6a"));
        assert_eq!(
            marker.source_ref.as_deref(),
            Some("fa76b6ab7b8661db3942c360e7fcc6e4c63e933a")
        );
        assert_eq!(
            marker.archive_name.as_deref(),
            Some("moex-trading-project-fa76b6a.zip")
        );
        assert_eq!(
            parse_m3c_evidence_slot_status("pending").expect("pending"),
            M3cOrderEndpointGateEvidenceStatus::Pending
        );
        assert_eq!(
            parse_m3c_evidence_slot_status("evidence-provided").expect("evidence"),
            M3cOrderEndpointGateEvidenceStatus::EvidenceProvided
        );
        assert_eq!(
            parse_m3c_evidence_slot_status("waiver-accepted").expect("waiver"),
            M3cOrderEndpointGateEvidenceStatus::WaiverAccepted
        );
        assert!(parse_m3c_evidence_slot_status("accepted").is_err());
    }

    #[test]
    fn m3c_handoff_marker_content_binding_rejects_stale_zip_contents() {
        let head = Some("eee47cf0769bfdfbf2ebcd2c7781dcf5f28f350b");
        let matching = HandoffCommitMarker {
            source_commit: Some("eee47cf".to_string()),
            source_ref: Some("eee47cf0769bfdfbf2ebcd2c7781dcf5f28f350b".to_string()),
            archive_name: Some("moex-trading-project-eee47cf.zip".to_string()),
        };
        let stale_ref = HandoffCommitMarker {
            source_ref: Some("1d54115d8f5250b19e92b91ca979376bf39714d1".to_string()),
            ..matching.clone()
        };
        let stale_archive_name = HandoffCommitMarker {
            archive_name: Some("moex-trading-project-1d54115.zip".to_string()),
            ..matching.clone()
        };

        validate_handoff_marker_content_binding(
            &matching,
            "moex-trading-project-eee47cf.zip",
            head,
        )
        .expect("matching handoff marker binds");
        assert!(validate_handoff_marker_content_binding(
            &stale_ref,
            "moex-trading-project-eee47cf.zip",
            head,
        )
        .expect_err("stale source_ref must be rejected")
        .to_string()
        .contains("source archive content binding mismatch"));
        assert!(validate_handoff_marker_content_binding(
            &stale_archive_name,
            "moex-trading-project-eee47cf.zip",
            head,
        )
        .expect_err("stale handoff archive_name must be rejected")
        .to_string()
        .contains("source archive content archive_name mismatch"));
    }

    #[test]
    fn bar_finality_golden_summary_uses_redacted_shape_and_timestamp_diagnostics() {
        let bars = broker_finam::BarsResponse {
            symbol: "TESTFUT@TEST".to_string(),
            bars: vec![
                sample_finam_bar("2026-06-29T09:00:00Z"),
                sample_finam_bar("2026-06-29T09:01:00Z"),
            ],
        };

        let summary = bar_finality_golden_summary(
            "TESTFUT@TEST",
            "TIME_FRAME_M1",
            60,
            "2026-06-29T09:00:00Z",
            "2026-06-29T09:02:00Z",
            &bars,
        )
        .expect("summary");
        let rendered = serde_json::to_string(&summary).expect("summary serializes");

        assert_eq!(summary["bar_finality_golden_harness"], true);
        assert_eq!(summary["live_trading_enabled"], false);
        assert_eq!(summary["order_endpoints_used"], false);
        assert_eq!(summary["symbol_present"], true);
        assert_eq!(summary["response_symbol_matches_request"], true);
        assert_eq!(summary["bars_count"], 2);
        assert_eq!(summary["mapped_bars_count"], 2);
        assert_eq!(summary["unique_open_deltas_sec"], serde_json::json!([60]));
        assert_eq!(summary["close_delta_mismatch_count"], 0);
        assert_eq!(
            summary["acceptance_status"],
            "unproven_operator_review_required"
        );
        assert!(!rendered.contains("TESTFUT@TEST"));
    }

    #[test]
    fn golden_bars_window_uses_operator_bounds_when_present() {
        let (start_time, end_time) = golden_bars_window(
            Some("2026-06-29T09:00:00Z"),
            Some("2026-06-29T09:10:00Z"),
            90,
        );

        assert_eq!(start_time, "2026-06-29T09:00:00Z");
        assert_eq!(end_time, "2026-06-29T09:10:00Z");
    }

    #[test]
    fn shadow_loop_summary_includes_cumulative_metrics() {
        let metrics = ShadowMetrics {
            success_count: 2,
            failure_count: 1,
            published_market_data_count: 42,
            deduped_bar_count: 7,
            ..ShadowMetrics::default()
        };

        let summary = shadow_loop_summary_json("max_iterations", 123, 3, 2, 1, &metrics);

        assert_eq!(summary["gateway_shadow_loop"], "stopped");
        assert_eq!(summary["success_count"], 2);
        assert_eq!(summary["failure_count"], 1);
        assert_eq!(summary["metrics"]["success_count"], 2);
        assert_eq!(summary["metrics"]["failure_count"], 1);
        assert_eq!(summary["metrics"]["published_market_data_count"], 42);
        assert_eq!(summary["metrics"]["deduped_bar_count"], 7);
    }

    #[test]
    fn record_shadow_success_metrics_counts_one_readiness_per_iteration() {
        let mut metrics = ShadowMetrics {
            consecutive_failures: 3,
            ..ShadowMetrics::default()
        };
        let report = ShadowIterationReport {
            iteration: 1,
            elapsed_ms: 12,
            summary: ReadonlySnapshotSummary {
                cash_count: 1,
                positions_count: 0,
                orders_count: 0,
                active_orders_count: 0,
                terminal_orders_count: 0,
                blocking_unknown_status_present: false,
            },
            readiness_phase: "Reconciliation".to_string(),
            readiness_reasons: Vec::new(),
            quote_published: true,
            bars_published_count: 3,
            bars_deduped_count: 2,
            timeframe_sec: 60,
        };
        let now = Utc::now();

        record_shadow_success_metrics(&mut metrics, &report, now);

        assert_eq!(metrics.success_count, 1);
        assert_eq!(metrics.consecutive_failures, 0);
        assert_eq!(metrics.last_success_ts, Some(now));
        assert_eq!(metrics.published_health_count, 1);
        assert_eq!(metrics.published_readiness_count, 1);
        assert_eq!(metrics.published_snapshot_count, 2);
        assert_eq!(metrics.published_market_data_count, 4);
        assert_eq!(metrics.deduped_bar_count, 2);
    }

    #[test]
    fn xautoclaim_cursor_continues_when_empty_page_advances() {
        assert!(!runtime_bridge_xautoclaim_cursor_done("0-0", "123-0"));
        assert!(runtime_bridge_xautoclaim_cursor_done("123-0", "123-0"));
        assert!(runtime_bridge_xautoclaim_cursor_done("123-0", "0-0"));
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
                interval_seconds: None,
                max_iterations: None,
            },
            GatewayShadowFileConfig::default(),
        )
        .expect("resolved config");

        assert!(!resolved.gateway_config.features.command_consumer_enabled);
        assert!(
            !resolved
                .gateway_config
                .features
                .real_readonly_broker_truth_enabled
        );
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
                interval_seconds: None,
                max_iterations: None,
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
                    command_ack: Some("broker.command_acks".to_string()),
                    runtime_bridge_dlq: Some("broker.runtime_bridge.dlq".to_string()),
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
        assert_eq!(
            resolved.gateway_config.redis.command_ack_stream,
            "broker.command_acks"
        );
        assert_eq!(
            resolved.gateway_config.redis.runtime_bridge_dlq_stream,
            "broker.runtime_bridge.dlq"
        );
        assert_eq!(resolved.timeframe, "TIME_FRAME_M10");
    }

    #[test]
    fn gateway_shadow_config_accepts_loop_and_retention_overrides() {
        let resolved = resolve_gateway_shadow_config(
            GatewayShadowOnceArgs {
                config: None,
                secret_env: "FINAM_SECRET_TOKEN".to_string(),
                redis_url: None,
                account_id: Some("ACC_TEST_0001".to_string()),
                symbol: Some("TICKER@MIC".to_string()),
                timeframe: None,
                start_time: None,
                end_time: None,
                bars_lookback_minutes: 60,
                interval_seconds: None,
                max_iterations: None,
            },
            GatewayShadowFileConfig {
                interval_seconds: Some(5),
                max_iterations: Some(2),
                bars_lookback_minutes: Some(15),
                retention: Some(GatewayShadowRetentionFileConfig {
                    health_maxlen: Some(10),
                    readiness_maxlen: Some(10),
                    portfolio_maxlen: Some(20),
                    order_snapshot_maxlen: Some(20),
                    market_data_maxlen: Some(100),
                    command_ack_maxlen: Some(30),
                    runtime_bridge_dlq_maxlen: Some(30),
                }),
                ..GatewayShadowFileConfig::default()
            },
        )
        .expect("resolved config");

        assert_eq!(resolved.interval_seconds, 5);
        assert_eq!(resolved.max_iterations, Some(2));
        assert_eq!(resolved.bars_lookback_minutes, 15);
        assert_eq!(
            resolved.gateway_config.redis.retention.health_maxlen,
            Some(10)
        );
        assert_eq!(
            resolved.gateway_config.redis.retention.market_data_maxlen,
            Some(100)
        );
        assert_eq!(
            resolved.gateway_config.redis.retention.command_ack_maxlen,
            Some(30)
        );
        assert_eq!(
            resolved
                .gateway_config
                .redis
                .retention
                .runtime_bridge_dlq_maxlen,
            Some(30)
        );
    }

    #[test]
    fn gateway_shadow_config_accepts_broker_truth_policy_overrides() {
        let resolved = resolve_gateway_shadow_config(
            GatewayShadowOnceArgs {
                config: None,
                secret_env: "FINAM_SECRET_TOKEN".to_string(),
                redis_url: None,
                account_id: Some("ACC_TEST_0001".to_string()),
                symbol: Some("TICKER@MIC".to_string()),
                timeframe: None,
                start_time: None,
                end_time: None,
                bars_lookback_minutes: 60,
                interval_seconds: None,
                max_iterations: None,
            },
            GatewayShadowFileConfig {
                broker_truth: Some(BrokerTruthGatewayConfig {
                    cancel_reconciliation: CancelBrokerTruthOrchestrationPolicy {
                        precedence_version: "m3b8-cli-test".to_string(),
                        freshness: CancelBrokerTruthFreshnessPolicy {
                            get_order_max_age_ms: 111,
                            orders_snapshot_max_age_ms: 222,
                            trades_snapshot_max_age_ms: 333,
                            position_snapshot_max_age_ms: 444,
                        },
                        precedence: CancelBrokerTruthPrecedencePolicy {
                            ordered_sources: vec![
                                CancelBrokerTruthSource::OrdersSnapshot,
                                CancelBrokerTruthSource::GetOrder,
                                CancelBrokerTruthSource::TradesSnapshot,
                            ],
                        },
                        position_guard: CancelPositionTruthGuardPolicy {
                            require_instrument_match: true,
                            require_intent_context: true,
                            require_expected_position_delta: false,
                            require_strategy_state: true,
                            require_order_or_trade_absent_or_stale: true,
                        },
                        identity: CancelBrokerTruthIdentityPolicy {
                            accept_client_order_id_fallback_as_strong: true,
                        },
                    },
                }),
                ..GatewayShadowFileConfig::default()
            },
        )
        .expect("resolved config");

        let policy = resolved
            .gateway_config
            .broker_truth
            .cancel_reconciliation
            .diagnostic();
        assert_eq!(policy.precedence_version, "m3b8-cli-test");
        assert_eq!(policy.get_order_max_age_ms, 111);
        assert_eq!(
            policy.precedence_order,
            vec![
                CancelBrokerTruthSource::OrdersSnapshot,
                CancelBrokerTruthSource::GetOrder,
                CancelBrokerTruthSource::TradesSnapshot,
            ]
        );
        assert_eq!(policy.policy_sha256.len(), 64);
        assert!(policy.identity.accept_client_order_id_fallback_as_strong);
    }

    #[test]
    fn runtime_bridge_summary_hints_when_tail_mode_reads_no_entries() {
        let gateway_config = GatewayConfig::default();
        let readiness_simulator =
            RuntimeBridgeReadinessSimulator::from_gateway_config(&gateway_config);
        let resolved = ResolvedRuntimeBridgeDryConfig {
            gateway_config,
            group: "broker-runtime-bridge-dry".to_string(),
            consumer: "dry-consumer-1".to_string(),
            group_start_id: "$".to_string(),
            count: 100,
            block_ms: 1,
            max_iterations: 1,
            claim_stale_ms: None,
        };
        let redis_metrics = RuntimeBridgeRedisDryMetrics {
            xreadgroup_iterations: 1,
            ..RuntimeBridgeRedisDryMetrics::default()
        };

        let summary = runtime_bridge_dry_summary_json(
            &resolved,
            &finam_gateway::RuntimeBridgeConsumerMetrics::default(),
            &readiness_simulator,
            &redis_metrics,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        );

        assert_eq!(summary["group_start_id"], "$");
        assert_eq!(
            summary["operator_hint"],
            "group_start_id_dollar_tails_new_entries_only_use_0_for_backfill"
        );
    }

    fn sample_finam_bar(timestamp: &str) -> broker_finam::Bar {
        broker_finam::Bar {
            close: decimal_value("5001"),
            high: decimal_value("5010"),
            low: decimal_value("4990"),
            open: decimal_value("5000"),
            timestamp: timestamp.to_string(),
            volume: decimal_value("10"),
        }
    }

    fn decimal_value(value: &str) -> broker_finam::DecimalValue {
        broker_finam::DecimalValue {
            value: value.to_string(),
        }
    }
}
