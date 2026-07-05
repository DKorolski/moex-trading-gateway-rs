#![recursion_limit = "256"]

use anyhow::{Context, Result};
use broker_core::command::CommandAckStatus;
use broker_core::event::{Bar, Quote};
#[cfg(feature = "m3j16-actual-one-shot")]
use broker_core::CancelPreflightApproval;
use broker_core::{
    BrokerAccountId, BrokerCapabilityMatrix, BrokerCommand, BrokerFreshnessConfig,
    BrokerLifecycleConfig, BrokerLiveEntryScope, BrokerMarketDataLifecycleInput,
    BrokerMarketDataLifecycleSnapshot, BrokerOperationalConfig, BrokerOrderId,
    BrokerPlainMicroStopOrderWaiverPolicy, BrokerReadiness, BrokerRiskLimitConfig,
    BrokerScopeConfig, BrokerTimeoutConfig, BrokerTruthSnapshot, ClientOrderId, ClosedBarFinalizer,
    ClosedBarFinalizerActionKind, Envelope, Exchange, InMemoryOrderPathStore, InstrumentId, Market,
    MarketDataEvent, MarketDataLifecyclePhase, MarketDataSourceKind, MessageType, OperatorArm,
    Order, OrderPathEvent, OrderPathRecord, OrderPreflightPolicy, OrderSide, OrderStatus,
    OrderType, PlaceOrder, PortfolioSnapshot, ReadinessPhase, ReadinessReason, StrategyRequestId,
    TimeInForce,
};
#[cfg(feature = "m3j16-actual-one-shot")]
use broker_core::{OrderPreflightContext, OrderReferencePrice};
use broker_finam::{
    active_orders, has_blocking_unknown_order_statuses, instrument_id_from_symbol,
    map_account_trade, map_bar, map_finam_broker_truth_snapshot, map_latest_market_trade,
    map_order_state, map_portfolio_snapshot, map_quote, map_ws_market_data_events,
    redact_json_key_for_diagnostics, terminal_orders, AccessToken, AllAssetsQuery, BarsQuery,
    FinamApiCapabilities, FinamAuthManager, FinamConfig, FinamError, FinamInstrumentSpecArtifacts,
    FinamMapperError, FinamRestClient, FinamWsEnvelope, FinamWsMapperError, GatewayEnabledFeatures,
    HistoryQuery, SecretToken,
};
#[cfg(feature = "m3j16-actual-one-shot")]
use broker_finam::{
    build_cancel_order_request, build_place_order_request, FinamOrderEndpointMappedResult,
    FinamOrderExecutionOutcome,
};
use broker_finam::{build_finam_canonical_readiness_package, FinamCanonicalReadinessPackageInput};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
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
use futures_util::{SinkExt, StreamExt};
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
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message as WsMessage;
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
        /// Optional output path for a single JSON evidence object.
        #[arg(long)]
        output: Option<PathBuf>,
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
        /// M4-2j no-send only: evaluate explicit plain-micro stop-order waiver in canonical package.
        #[arg(long, default_value_t = false)]
        plain_micro_stop_waiver_operator_approved_no_send: bool,
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
        /// Exact approved venue symbol for this M3j-16a run.
        #[arg(long, default_value = "IMOEXF@RTSX")]
        expected_symbol: String,
        /// Maximum source/received quote age for quote-bound reference guard.
        #[arg(long, default_value_t = 60_000)]
        reference_quote_max_age_ms: i64,
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
        /// Prove the final actual-send gate without performing POST/DELETE.
        #[arg(long)]
        pre_actual_gate_only: bool,
        /// Sensitive local-only raw FINAM order endpoint response capture path. Do not put in handoff.
        #[arg(long)]
        raw_response_output: Option<PathBuf>,
        /// M3j-20: poll readonly orders until the placed order is observed as active/working before cancel.
        #[arg(long)]
        observe_working_before_cancel: bool,
        /// M3j-20: max milliseconds to wait for active/working observation before cancel.
        #[arg(long, default_value_t = 5_000)]
        working_observation_timeout_ms: u64,
        /// M3j-20: poll interval milliseconds for active/working observation.
        #[arg(long, default_value_t = 250)]
        working_observation_poll_ms: u64,
    },
    /// M4-1c guarded tiny position lifecycle: market entry -> position snapshot -> market exit. Default is no-send.
    #[command(name = "finam-tiny-position-market-one-shot")]
    TinyPositionMarketOneShot {
        /// Environment variable that contains the Finam secret token.
        #[arg(long, default_value = "FINAM_SECRET_TOKEN")]
        secret_env: String,
        /// Finam account id. Raw value is never written to the report.
        #[arg(long, env = "FINAM_ACCOUNT_ID")]
        account_id: String,
        /// Finam venue symbol, for example IMOEXF@RTSX.
        #[arg(long, env = "FINAM_SYMBOL")]
        symbol: String,
        /// Exact approved venue symbol for this M4-1c run.
        #[arg(long, default_value = "IMOEXF@RTSX")]
        expected_symbol: String,
        /// Entry side. M4-1c currently supports buy entry with sell exit.
        #[arg(long, default_value = "buy")]
        entry_side: String,
        /// Quantity. M4-1c requires exactly 1.
        #[arg(long, default_value = "1")]
        qty: String,
        /// Maximum source/received quote age for market notional guard.
        #[arg(long, default_value_t = 180_000)]
        reference_quote_max_age_ms: i64,
        /// Request timeout bound for each order transport call.
        #[arg(long, default_value_t = 10_000)]
        request_timeout_ms: u64,
        /// Max milliseconds to wait for broker position snapshot after entry.
        #[arg(long, default_value_t = 10_000)]
        position_observation_timeout_ms: u64,
        /// Poll interval milliseconds for broker position snapshot.
        #[arg(long, default_value_t = 250)]
        position_observation_poll_ms: u64,
        /// Output path for the redacted M4-1c report.
        #[arg(
            long,
            default_value = "reports/m4/m4-1c-tiny-position-market-report.json"
        )]
        output: PathBuf,
        /// Required for the real entry/exit boundary calls. Without it the command is no-send.
        #[arg(long)]
        actual_entry_exit_i_understand_risk: bool,
        /// Prove the final actual gate without performing POST/DELETE.
        #[arg(long)]
        pre_actual_gate_only: bool,
        /// Sensitive local-only raw FINAM response capture base path. Do not put raw files in handoff.
        #[arg(long)]
        raw_response_output: Option<PathBuf>,
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
    /// Run one FINAM WebSocket market-data shadow pass. Does not place or cancel orders.
    #[command(name = "finam-ws-shadow-once")]
    FinamWsShadowOnce {
        /// Optional JSON config file with Redis streams and FINAM symbol/timeframe.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Environment variable that contains the Finam secret token.
        #[arg(long, default_value = "FINAM_SECRET_TOKEN")]
        secret_env: String,
        /// Redis connection URL. Overrides config file.
        #[arg(long, env = "FINAM_GATEWAY_REDIS_URL")]
        redis_url: Option<String>,
        /// Finam venue symbol, for example TICKER@MIC. Overrides config file.
        #[arg(long, env = "FINAM_SYMBOL")]
        symbol: Option<String>,
        /// Bars timeframe value accepted by Finam WebSocket, for example TIME_FRAME_M1.
        #[arg(long, env = "FINAM_TIMEFRAME")]
        timeframe: Option<String>,
        /// Subscribe to FINAM WebSocket BARS.
        #[arg(long, default_value_t = true)]
        subscribe_bars: bool,
        /// Subscribe to FINAM WebSocket QUOTES as diagnostics. Strategy parity uses BARS.
        #[arg(long, default_value_t = true)]
        subscribe_quotes: bool,
        /// Stop after this many received WebSocket messages.
        #[arg(long, default_value_t = 20)]
        max_messages: u64,
        /// Stop after this many seconds even if fewer messages arrive.
        #[arg(long, default_value_t = 60)]
        max_duration_seconds: u64,
    },
    /// Run periodic/reconnecting FINAM WebSocket market-data shadow loop. No live orders.
    #[command(name = "finam-ws-shadow-loop")]
    FinamWsShadowLoop {
        /// Optional JSON config file with Redis streams and FINAM symbol/timeframe.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Environment variable that contains the Finam secret token.
        #[arg(long, default_value = "FINAM_SECRET_TOKEN")]
        secret_env: String,
        /// Redis connection URL. Overrides config file.
        #[arg(long, env = "FINAM_GATEWAY_REDIS_URL")]
        redis_url: Option<String>,
        /// Finam venue symbol, for example TICKER@MIC. Overrides config file.
        #[arg(long, env = "FINAM_SYMBOL")]
        symbol: Option<String>,
        /// Bars timeframe value accepted by Finam WebSocket, for example TIME_FRAME_M1.
        #[arg(long, env = "FINAM_TIMEFRAME")]
        timeframe: Option<String>,
        /// Subscribe to FINAM WebSocket BARS.
        #[arg(long, default_value_t = true)]
        subscribe_bars: bool,
        /// Subscribe to FINAM WebSocket QUOTES as diagnostics. Disabled by default for loop.
        #[arg(long, default_value_t = false)]
        subscribe_quotes: bool,
        /// Per-connection message budget before reconnecting.
        #[arg(long, default_value_t = 10000)]
        max_messages: u64,
        /// Per-connection duration before reconnecting.
        #[arg(long, default_value_t = 3600)]
        max_duration_seconds: u64,
        /// Reconnect delay after each connection pass.
        #[arg(long, default_value_t = 5)]
        reconnect_delay_seconds: u64,
        /// Optional safety stop after N connection passes. Omit for continuous loop.
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
        Command::AuthCheck { secret_env, output } => {
            let secret = SecretToken::new(std::env::var(&secret_env)?);
            let client = FinamRestClient::try_new(FinamConfig::default())?;
            let auth_manager = FinamAuthManager::new(client.clone(), secret);
            let payload = match auth_manager.access_token().await {
                Ok(token) => {
                    let auth = serde_json::json!({
                            "auth_http": 200,
                            "jwt_present": !token.is_empty(),
                            "jwt_len": token.len(),
                    });
                    let details = match client.token_details(&token).await {
                        Ok(details) => {
                            let detail_keys = details
                                .as_object()
                                .map(|object| object.keys().cloned().collect::<Vec<_>>())
                                .unwrap_or_default();
                            serde_json::json!({
                                "details_http": 200,
                                "details_keys": detail_keys,
                            })
                        }
                        Err(error) => serde_json::json!({
                            "details_error_kind": error.kind(),
                            "details_error": error.to_redacted_string(),
                        }),
                    };
                    serde_json::json!({
                        "fixture_kind": "finam-auth-check-redacted-v2",
                        "auth": auth,
                        "details": details,
                        "raw_secret_exported": false,
                        "raw_jwt_exported": false,
                    })
                }
                Err(error) => serde_json::json!({
                    "fixture_kind": "finam-auth-check-redacted-v2",
                    "auth": {
                            "auth_error_kind": error.kind(),
                            "auth_error": error.to_redacted_string(),
                    },
                    "raw_secret_exported": false,
                    "raw_jwt_exported": false,
                }),
            };
            print_json(payload.clone())?;
            if let Some(output) = output {
                write_json_payload(&output, &payload)?;
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
            plain_micro_stop_waiver_operator_approved_no_send,
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

                    if let (Some(account_id), Some(symbol)) =
                        (account_id.as_deref(), symbol.as_deref())
                    {
                        let history_query = HistoryQuery {
                            limit: Some(limit),
                            start_time: start_time.as_deref(),
                            end_time: end_time.as_deref(),
                        };
                        let record = run_typed_canonical_readiness_package_probe(
                            &client,
                            &token,
                            account_id,
                            symbol,
                            history_query,
                            plain_micro_stop_waiver_operator_approved_no_send,
                        )
                        .await;
                        emit_record(&mut records, record)?;
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
            expected_symbol,
            reference_quote_max_age_ms,
            qty,
            request_timeout_ms,
            output,
            actual_send_i_understand_risk,
            pre_actual_gate_only,
            raw_response_output,
            observe_working_before_cancel,
            working_observation_timeout_ms,
            working_observation_poll_ms,
        } => {
            run_finam_limit_cancel_one_shot(FinamLimitCancelOneShotArgs {
                secret_env,
                account_id,
                symbol,
                limit_price,
                reference_price,
                expected_symbol,
                reference_quote_max_age_ms,
                qty,
                request_timeout_ms,
                output,
                actual_send_i_understand_risk,
                pre_actual_gate_only,
                raw_response_output,
                observe_working_before_cancel,
                working_observation_timeout_ms,
                working_observation_poll_ms,
            })
            .await?;
        }
        Command::TinyPositionMarketOneShot {
            secret_env,
            account_id,
            symbol,
            expected_symbol,
            entry_side,
            qty,
            reference_quote_max_age_ms,
            request_timeout_ms,
            position_observation_timeout_ms,
            position_observation_poll_ms,
            output,
            actual_entry_exit_i_understand_risk,
            pre_actual_gate_only,
            raw_response_output,
        } => {
            run_finam_tiny_position_market_one_shot(FinamTinyPositionMarketOneShotArgs {
                secret_env,
                account_id,
                symbol,
                expected_symbol,
                entry_side,
                qty,
                reference_quote_max_age_ms,
                request_timeout_ms,
                position_observation_timeout_ms,
                position_observation_poll_ms,
                output,
                actual_entry_exit_i_understand_risk,
                pre_actual_gate_only,
                raw_response_output,
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
        Command::FinamWsShadowOnce {
            config,
            secret_env,
            redis_url,
            symbol,
            timeframe,
            subscribe_bars,
            subscribe_quotes,
            max_messages,
            max_duration_seconds,
        } => {
            run_finam_ws_shadow_once(FinamWsShadowArgs {
                config,
                secret_env,
                redis_url,
                symbol,
                timeframe,
                subscribe_bars,
                subscribe_quotes,
                max_messages,
                max_duration_seconds,
                reconnect_delay_seconds: 0,
                max_iterations: Some(1),
            })
            .await?;
        }
        Command::FinamWsShadowLoop {
            config,
            secret_env,
            redis_url,
            symbol,
            timeframe,
            subscribe_bars,
            subscribe_quotes,
            max_messages,
            max_duration_seconds,
            reconnect_delay_seconds,
            max_iterations,
        } => {
            run_finam_ws_shadow_loop(FinamWsShadowArgs {
                config,
                secret_env,
                redis_url,
                symbol,
                timeframe,
                subscribe_bars,
                subscribe_quotes,
                max_messages,
                max_duration_seconds,
                reconnect_delay_seconds,
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

struct FinamWsShadowArgs {
    config: Option<PathBuf>,
    secret_env: String,
    redis_url: Option<String>,
    symbol: Option<String>,
    timeframe: Option<String>,
    subscribe_bars: bool,
    subscribe_quotes: bool,
    max_messages: u64,
    max_duration_seconds: u64,
    reconnect_delay_seconds: u64,
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

struct ResolvedFinamWsShadowConfig {
    secret_env: String,
    gateway_config: GatewayConfig,
    symbol: String,
    timeframe: String,
    subscribe_bars: bool,
    subscribe_quotes: bool,
    max_messages: u64,
    max_duration_seconds: u64,
    reconnect_delay_seconds: u64,
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

struct FinamWsShadowRuntime {
    resolved: ResolvedFinamWsShadowConfig,
    auth_manager: FinamAuthManager,
    gateway: FinamGateway<RedisConnectionStreamSink>,
}

#[derive(Default, Clone)]
struct FinamWsShadowMetrics {
    connection_count: u64,
    success_count: u64,
    failure_count: u64,
    text_message_count: u64,
    data_envelope_count: u64,
    event_envelope_count: u64,
    error_envelope_count: u64,
    decode_error_count: u64,
    mapper_error_count: u64,
    quote_event_count: u64,
    bar_event_count: u64,
    final_bar_event_count: u64,
    forming_bar_event_count: u64,
    history_bar_event_count: u64,
    read_only_bar_event_count: u64,
    recovery_bar_event_count: u64,
    live_bar_event_count: u64,
    final_live_bar_event_count: u64,
    forming_live_bar_event_count: u64,
    unknown_source_bar_event_count: u64,
    closed_bar_finalized_count: u64,
    final_bar_passthrough_count: u64,
    forming_bar_suppressed_count: u64,
    duplicate_final_suppressed_count: u64,
    non_monotonic_forming_dropped_count: u64,
    non_live_bar_passthrough_count: u64,
    published_market_data_count: u64,
    ping_count: u64,
    pong_count: u64,
    first_live_bar_seen: bool,
    first_live_final_bar_seen: bool,
    first_live_final_bar_close_ts: Option<DateTime<Utc>>,
    fresh_live_final_bar_seen: bool,
    first_fresh_live_final_bar_close_ts: Option<DateTime<Utc>>,
    last_history_bar_close_ts: Option<DateTime<Utc>>,
    last_recovery_bar_close_ts: Option<DateTime<Utc>>,
    last_live_bar_close_ts: Option<DateTime<Utc>>,
    last_final_live_bar_close_ts: Option<DateTime<Utc>>,
    last_fresh_live_final_bar_close_ts: Option<DateTime<Utc>>,
    latest_ws_bar_close_ts: Option<DateTime<Utc>>,
    latest_ws_final_bar_close_ts: Option<DateTime<Utc>>,
    latest_live_final_bar_stale_for_sec: Option<u64>,
    max_live_final_bar_stale_for_sec: Option<u64>,
    stale_live_final_bar_count: u64,
    final_bar_gap_detected_count: u64,
    first_final_bar_gap_expected_close_ts: Option<DateTime<Utc>>,
    first_final_bar_gap_actual_close_ts: Option<DateTime<Utc>>,
    last_final_bar_gap_expected_close_ts: Option<DateTime<Utc>>,
    last_final_bar_gap_actual_close_ts: Option<DateTime<Utc>>,
    last_bar_source_kind: Option<MarketDataSourceKind>,
    first_decode_error_text_len: Option<usize>,
    first_decode_error_shape: Option<serde_json::Value>,
    first_mapper_error_kind: Option<String>,
    first_mapper_error_subscription_type: Option<String>,
    first_mapper_error_payload_shape: Option<serde_json::Value>,
}

struct FinamWsShadowIterationReport {
    iteration: u64,
    elapsed_ms: u128,
    stop_reason: String,
    metrics: FinamWsShadowMetrics,
    market_data_lifecycle: BrokerMarketDataLifecycleSnapshot,
    readiness_phase: String,
    readiness_reasons: Vec<String>,
}

async fn run_finam_ws_shadow_once(args: FinamWsShadowArgs) -> Result<()> {
    let runtime = setup_finam_ws_shadow_runtime(args).await?;
    let report = run_finam_ws_shadow_iteration(&runtime, 1).await?;
    print_json(finam_ws_shadow_report_json("once", &runtime, &report))?;
    Ok(())
}

async fn run_finam_ws_shadow_loop(args: FinamWsShadowArgs) -> Result<()> {
    let runtime = setup_finam_ws_shadow_runtime(args).await?;
    let started_at = Instant::now();
    let mut iteration = 0_u64;
    let mut success_count = 0_u64;
    let mut failure_count = 0_u64;

    loop {
        iteration += 1;
        match run_finam_ws_shadow_iteration(&runtime, iteration).await {
            Ok(report) => {
                success_count += 1;
                print_json(finam_ws_shadow_report_json("loop", &runtime, &report))?;
            }
            Err(error) => {
                failure_count += 1;
                publish_degraded_state(
                    &runtime.gateway,
                    ReadinessReason::MarketDataNotLive,
                    "finam_ws_shadow",
                )
                .await?;
                print_json(serde_json::json!({
                    "finam_ws_shadow": false,
                    "mode": "loop",
                    "iteration": iteration,
                    "live_trading_enabled": false,
                    "order_placement_enabled": false,
                    "cancel_enabled": false,
                    "command_consumer_enabled": false,
                    "readiness_phase": "Degraded",
                    "readiness_reasons": ["MarketDataNotLive"],
                    "error_present": true,
                    "error": error.to_string(),
                }))?;
            }
        }

        if runtime
            .resolved
            .max_iterations
            .is_some_and(|max_iterations| iteration >= max_iterations)
        {
            publish_stopped_state(&runtime.gateway, "finam_ws_shadow_max_iterations").await?;
            print_json(serde_json::json!({
                "finam_ws_shadow_loop": "stopped",
                "stop_reason": "max_iterations",
                "elapsed_ms": started_at.elapsed().as_millis(),
                "iterations": iteration,
                "success_count": success_count,
                "failure_count": failure_count,
                "live_trading_enabled": false,
            }))?;
            break;
        }

        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                if signal.is_err() {
                    emit_redis_degraded_stderr("finam_ws_ctrl_c_signal", &std::io::Error::other("ctrl_c signal failed"))?;
                }
                publish_stopped_state(&runtime.gateway, "finam_ws_shadow_ctrl_c").await?;
                print_json(serde_json::json!({
                    "finam_ws_shadow_loop": "stopped",
                    "stop_reason": "ctrl_c",
                    "elapsed_ms": started_at.elapsed().as_millis(),
                    "iterations": iteration,
                    "success_count": success_count,
                    "failure_count": failure_count,
                    "live_trading_enabled": false,
                }))?;
                break;
            }
            _ = tokio::time::sleep(StdDuration::from_secs(runtime.resolved.reconnect_delay_seconds)) => {}
        }
    }

    Ok(())
}

async fn setup_finam_ws_shadow_runtime(args: FinamWsShadowArgs) -> Result<FinamWsShadowRuntime> {
    let file_config = read_gateway_shadow_file_config(args.config.as_ref())?;
    let resolved = resolve_finam_ws_shadow_config(args, file_config)?;
    let redis_url = resolved.gateway_config.redis.url.clone();
    let secret = SecretToken::new(std::env::var(&resolved.secret_env).with_context(|| {
        format!(
            "missing required environment variable {}",
            resolved.secret_env
        )
    })?);
    let client = FinamRestClient::try_new(FinamConfig::default())?;
    let auth_manager = FinamAuthManager::new(client, secret);
    let sink = RedisConnectionStreamSink::connect(&redis_url)
        .await
        .context("Redis connection failed for FINAM WS shadow gateway")?;
    let gateway = FinamGateway::new(resolved.gateway_config.clone(), sink);
    Ok(FinamWsShadowRuntime {
        resolved,
        auth_manager,
        gateway,
    })
}

fn resolve_finam_ws_shadow_config(
    args: FinamWsShadowArgs,
    file_config: GatewayShadowFileConfig,
) -> Result<ResolvedFinamWsShadowConfig> {
    anyhow::ensure!(
        args.subscribe_bars || args.subscribe_quotes,
        "at least one FINAM WS subscription must be enabled"
    );
    let mut gateway_config = GatewayConfig {
        features: GatewayFeatureSet::default(),
        ..GatewayConfig::default()
    };
    apply_gateway_shadow_file_config(&mut gateway_config, &file_config);
    if let Some(redis_url) = args.redis_url {
        gateway_config.redis.url = redis_url;
    }
    gateway_config.features.command_consumer_enabled = false;
    gateway_config.features.order_placement_enabled = false;
    gateway_config.features.cancel_enabled = false;
    gateway_config.features.stop_sltp_bracket_enabled = false;

    Ok(ResolvedFinamWsShadowConfig {
        secret_env: args.secret_env,
        gateway_config,
        symbol: args
            .symbol
            .or(file_config.symbol)
            .context("missing required FINAM symbol for WS shadow gateway")?,
        timeframe: args
            .timeframe
            .or(file_config.timeframe)
            .unwrap_or_else(|| "TIME_FRAME_M1".to_string()),
        subscribe_bars: args.subscribe_bars,
        subscribe_quotes: args.subscribe_quotes,
        max_messages: args.max_messages.max(1),
        max_duration_seconds: args.max_duration_seconds.max(1),
        reconnect_delay_seconds: args.reconnect_delay_seconds.max(1),
        max_iterations: args
            .max_iterations
            .or(file_config.max_iterations)
            .filter(|value| *value > 0),
    })
}

async fn run_finam_ws_shadow_iteration(
    runtime: &FinamWsShadowRuntime,
    iteration: u64,
) -> Result<FinamWsShadowIterationReport> {
    let started_at = Instant::now();
    let token = runtime.auth_manager.access_token().await?;
    let mut request = FinamConfig::default()
        .websocket_endpoint
        .into_client_request()
        .context("FINAM WebSocket request build failed")?;
    request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(token.as_str()).context("FINAM WebSocket auth header failed")?,
    );
    let (mut ws, _response) = connect_async(request)
        .await
        .context("FINAM WebSocket connect failed")?;
    let mut metrics = FinamWsShadowMetrics {
        connection_count: 1,
        ..FinamWsShadowMetrics::default()
    };
    let mut closed_bar_finalizer = ClosedBarFinalizer::default();

    runtime
        .gateway
        .publish_health(default_readonly_health(runtime.gateway.config()))
        .await?;

    if runtime.resolved.subscribe_quotes {
        let request = broker_finam::build_ws_subscribe_quotes_request(
            std::slice::from_ref(&runtime.resolved.symbol),
            &token,
        );
        ws.send(WsMessage::Text(request.to_string()))
            .await
            .context("FINAM WebSocket quote subscribe send failed")?;
    }
    if runtime.resolved.subscribe_bars {
        let request = broker_finam::build_ws_subscribe_bars_request(
            &runtime.resolved.symbol,
            &runtime.resolved.timeframe,
            &token,
        );
        ws.send(WsMessage::Text(request.to_string()))
            .await
            .context("FINAM WebSocket bars subscribe send failed")?;
    }

    let timeframe_sec = timeframe_seconds(&runtime.resolved.timeframe)?;
    let timeout = tokio::time::sleep(StdDuration::from_secs(
        runtime.resolved.max_duration_seconds,
    ));
    tokio::pin!(timeout);
    let mut stop_reason = "max_messages".to_string();

    loop {
        if metrics.text_message_count >= runtime.resolved.max_messages {
            break;
        }
        tokio::select! {
            _ = &mut timeout => {
                stop_reason = "max_duration".to_string();
                break;
            }
            next = ws.next() => {
                let Some(message) = next else {
                    stop_reason = "stream_closed".to_string();
                    break;
                };
                let message = message.context("FINAM WebSocket receive failed")?;
                match message {
                    WsMessage::Text(text) => {
                        metrics.text_message_count += 1;
                        handle_finam_ws_text_message(
                            runtime,
                            text.as_ref(),
                            timeframe_sec,
                            &mut metrics,
                            &mut closed_bar_finalizer,
                        ).await?;
                    }
                    WsMessage::Ping(payload) => {
                        metrics.ping_count += 1;
                        ws.send(WsMessage::Pong(payload)).await.context("FINAM WebSocket pong send failed")?;
                    }
                    WsMessage::Pong(_) => {
                        metrics.pong_count += 1;
                    }
                    WsMessage::Close(_) => {
                        stop_reason = "close_frame".to_string();
                        break;
                    }
                    WsMessage::Binary(_) | WsMessage::Frame(_) => {}
                }
            }
        }
    }

    let market_data_lifecycle =
        finam_ws_shadow_market_data_lifecycle(runtime, &metrics, timeframe_sec, Utc::now());
    let readiness = finam_ws_shadow_readiness(runtime, &market_data_lifecycle);
    runtime.gateway.publish_readiness(readiness.clone()).await?;
    metrics.success_count = 1;

    Ok(FinamWsShadowIterationReport {
        iteration,
        elapsed_ms: started_at.elapsed().as_millis(),
        stop_reason,
        metrics,
        market_data_lifecycle,
        readiness_phase: format!("{:?}", readiness.phase),
        readiness_reasons: readiness
            .reasons
            .iter()
            .map(|reason| format!("{reason:?}"))
            .collect(),
    })
}

fn finam_ws_shadow_readiness(
    runtime: &FinamWsShadowRuntime,
    lifecycle: &BrokerMarketDataLifecycleSnapshot,
) -> BrokerReadiness {
    finam_ws_shadow_readiness_from_lifecycle(runtime.resolved.subscribe_bars, lifecycle)
}

fn finam_ws_shadow_readiness_from_lifecycle(
    subscribe_bars: bool,
    lifecycle: &BrokerMarketDataLifecycleSnapshot,
) -> BrokerReadiness {
    if !subscribe_bars || lifecycle.phase == MarketDataLifecyclePhase::Degraded {
        BrokerReadiness {
            phase: ReadinessPhase::Degraded,
            reasons: vec![ReadinessReason::MarketDataNotLive],
            checked_ts: Utc::now(),
        }
    } else if !lifecycle.first_live_final_bar_seen {
        BrokerReadiness {
            phase: ReadinessPhase::Degraded,
            reasons: vec![ReadinessReason::FirstLiveBarMissing],
            checked_ts: Utc::now(),
        }
    } else {
        BrokerReadiness {
            phase: ReadinessPhase::Reconciliation,
            reasons: vec![ReadinessReason::OperatorLiveArmMissing],
            checked_ts: Utc::now(),
        }
    }
}

fn finam_ws_shadow_market_data_lifecycle(
    runtime: &FinamWsShadowRuntime,
    metrics: &FinamWsShadowMetrics,
    timeframe_sec: u32,
    checked_ts: DateTime<Utc>,
) -> BrokerMarketDataLifecycleSnapshot {
    finam_ws_shadow_market_data_lifecycle_from_metrics(
        runtime.resolved.subscribe_bars,
        runtime.resolved.subscribe_quotes,
        metrics,
        timeframe_sec,
        checked_ts,
    )
}

fn finam_ws_shadow_market_data_lifecycle_from_metrics(
    subscribe_bars: bool,
    subscribe_quotes: bool,
    metrics: &FinamWsShadowMetrics,
    timeframe_sec: u32,
    checked_ts: DateTime<Utc>,
) -> BrokerMarketDataLifecycleSnapshot {
    let stale_after_sec = finam_ws_shadow_freshness_threshold_sec(timeframe_sec);
    broker_core::evaluate_market_data_lifecycle(BrokerMarketDataLifecycleInput {
        bars_subscription_enabled: subscribe_bars,
        quotes_subscription_enabled: subscribe_quotes,
        transport_connected: metrics.connection_count > 0,
        strategy_source_kind: MarketDataSourceKind::LiveStream,
        rest_bars_used_for_strategy: false,
        rest_market_data_used_for_strategy: false,
        history_bar_count: metrics.history_bar_event_count + metrics.read_only_bar_event_count,
        recovery_bar_count: metrics.recovery_bar_event_count,
        live_bar_count: metrics.live_bar_event_count,
        live_final_bar_count: metrics.final_live_bar_event_count,
        live_forming_bar_count: metrics.forming_live_bar_event_count,
        quote_count: metrics.quote_event_count,
        first_live_bar_seen: metrics.first_live_bar_seen,
        first_live_final_bar_seen: metrics.first_live_final_bar_seen,
        first_live_final_bar_close_ts: metrics.first_live_final_bar_close_ts,
        last_history_bar_close_ts: metrics.last_history_bar_close_ts,
        last_recovery_bar_close_ts: metrics.last_recovery_bar_close_ts,
        last_live_bar_close_ts: metrics.last_live_bar_close_ts,
        last_final_live_bar_close_ts: metrics.last_final_live_bar_close_ts,
        stale_after_sec: Some(stale_after_sec),
        checked_ts,
    })
}

fn finam_ws_shadow_freshness_threshold_sec(timeframe_sec: u32) -> u64 {
    u64::from(timeframe_sec).saturating_mul(3).max(60)
}

async fn handle_finam_ws_text_message(
    runtime: &FinamWsShadowRuntime,
    text: &str,
    timeframe_sec: u32,
    metrics: &mut FinamWsShadowMetrics,
    closed_bar_finalizer: &mut ClosedBarFinalizer,
) -> Result<()> {
    let envelope = match serde_json::from_str::<FinamWsEnvelope>(text) {
        Ok(envelope) => envelope,
        Err(_) => {
            metrics.decode_error_count += 1;
            if metrics.first_decode_error_shape.is_none() {
                metrics.first_decode_error_text_len = Some(text.len());
                metrics.first_decode_error_shape = Some(match serde_json::from_str(text) {
                    Ok(value) => json_shape(&value),
                    Err(_) => serde_json::json!({
                        "kind": "non_json_text",
                        "utf8_len": text.len(),
                    }),
                });
            }
            return Ok(());
        }
    };
    if envelope.envelope_type.eq_ignore_ascii_case("ERROR") {
        metrics.error_envelope_count += 1;
        return Ok(());
    }
    if envelope.envelope_type.eq_ignore_ascii_case("EVENT") {
        metrics.event_envelope_count += 1;
        return Ok(());
    }
    if !envelope.envelope_type.eq_ignore_ascii_case("DATA") {
        return Ok(());
    }
    metrics.data_envelope_count += 1;

    let events = match map_ws_market_data_events(
        &envelope,
        Some(&runtime.resolved.symbol),
        timeframe_sec,
        Utc::now(),
    ) {
        Ok(events) => events,
        Err(error) => {
            metrics.mapper_error_count += 1;
            if metrics.first_mapper_error_kind.is_none() {
                metrics.first_mapper_error_kind =
                    Some(finam_ws_mapper_error_kind(&error).to_string());
                metrics.first_mapper_error_subscription_type = envelope.subscription_type.clone();
                metrics.first_mapper_error_payload_shape =
                    envelope.payload.as_ref().map(json_shape);
            }
            return Ok(());
        }
    };

    for event in events {
        match event {
            MarketDataEvent::Quote(quote) => {
                metrics.quote_event_count += 1;
                runtime
                    .gateway
                    .publish_market_data_event(MarketDataEvent::Quote(quote))
                    .await?;
                metrics.published_market_data_count += 1;
            }
            MarketDataEvent::Bar(bar) => {
                record_inbound_ws_bar_metrics(metrics, &bar);
                let action = closed_bar_finalizer.observe_bar(bar);
                record_closed_bar_finalizer_action(metrics, action.kind);
                if let Some(final_bar) = action.emitted {
                    record_canonical_ws_bar_metrics(
                        metrics,
                        &final_bar,
                        Utc::now(),
                        finam_ws_shadow_freshness_threshold_sec(timeframe_sec),
                    );
                    runtime
                        .gateway
                        .publish_market_data_event(MarketDataEvent::Bar(final_bar))
                        .await?;
                    metrics.published_market_data_count += 1;
                }
            }
            MarketDataEvent::OrderBook(order_book) => {
                runtime
                    .gateway
                    .publish_market_data_event(MarketDataEvent::OrderBook(order_book))
                    .await?;
                metrics.published_market_data_count += 1;
            }
            MarketDataEvent::LatestTrade(trade) => {
                runtime
                    .gateway
                    .publish_market_data_event(MarketDataEvent::LatestTrade(trade))
                    .await?;
                metrics.published_market_data_count += 1;
            }
        }
    }

    Ok(())
}

fn record_inbound_ws_bar_metrics(metrics: &mut FinamWsShadowMetrics, bar: &Bar) {
    metrics.bar_event_count += 1;
    metrics.latest_ws_bar_close_ts =
        Some(max_datetime(metrics.latest_ws_bar_close_ts, bar.close_ts));
    if bar.is_final {
        metrics.final_bar_event_count += 1;
        metrics.latest_ws_final_bar_close_ts = Some(max_datetime(
            metrics.latest_ws_final_bar_close_ts,
            bar.close_ts,
        ));
    } else {
        metrics.forming_bar_event_count += 1;
    }
    metrics.last_bar_source_kind = Some(bar.source_kind);
    match bar.source_kind {
        MarketDataSourceKind::HistoricalPoll => {
            metrics.history_bar_event_count += 1;
            metrics.last_history_bar_close_ts = Some(bar.close_ts);
        }
        MarketDataSourceKind::ReadOnlyPoll => {
            metrics.read_only_bar_event_count += 1;
            metrics.last_history_bar_close_ts = Some(bar.close_ts);
        }
        MarketDataSourceKind::Recovery => {
            metrics.recovery_bar_event_count += 1;
            metrics.last_recovery_bar_close_ts = Some(bar.close_ts);
        }
        MarketDataSourceKind::LiveStream => {
            metrics.live_bar_event_count += 1;
            metrics.first_live_bar_seen = true;
            metrics.last_live_bar_close_ts = Some(bar.close_ts);
            if !bar.is_final {
                metrics.forming_live_bar_event_count += 1;
            }
        }
        MarketDataSourceKind::Unknown => {
            metrics.unknown_source_bar_event_count += 1;
        }
    }
}

fn record_canonical_ws_bar_metrics(
    metrics: &mut FinamWsShadowMetrics,
    bar: &Bar,
    observed_ts: DateTime<Utc>,
    freshness_threshold_sec: u64,
) {
    record_final_bar_gap_metrics(metrics, bar);
    if bar.source_kind == MarketDataSourceKind::LiveStream && bar.is_final {
        metrics.final_live_bar_event_count += 1;
        metrics.last_final_live_bar_close_ts = Some(bar.close_ts);
        if !metrics.first_live_final_bar_seen {
            metrics.first_live_final_bar_seen = true;
            metrics.first_live_final_bar_close_ts = Some(bar.close_ts);
        }
        let stale_for_sec = live_final_bar_stale_for_sec(bar.close_ts, observed_ts);
        metrics.latest_live_final_bar_stale_for_sec = Some(stale_for_sec);
        metrics.max_live_final_bar_stale_for_sec = Some(
            metrics
                .max_live_final_bar_stale_for_sec
                .map_or(stale_for_sec, |previous| previous.max(stale_for_sec)),
        );
        if is_fresh_live_final_bar(bar.close_ts, observed_ts, freshness_threshold_sec) {
            metrics.fresh_live_final_bar_seen = true;
            metrics.last_fresh_live_final_bar_close_ts = Some(bar.close_ts);
            if metrics.first_fresh_live_final_bar_close_ts.is_none() {
                metrics.first_fresh_live_final_bar_close_ts = Some(bar.close_ts);
            }
        } else {
            metrics.stale_live_final_bar_count += 1;
        }
    }
}

fn record_final_bar_gap_metrics(metrics: &mut FinamWsShadowMetrics, bar: &Bar) {
    if !bar.is_final || bar.timeframe_sec == 0 {
        return;
    }
    if let Some(previous_close_ts) = metrics.last_final_live_bar_close_ts {
        let expected_close_ts =
            previous_close_ts + ChronoDuration::seconds(i64::from(bar.timeframe_sec));
        if bar.close_ts > expected_close_ts {
            metrics.final_bar_gap_detected_count += 1;
            metrics.last_final_bar_gap_expected_close_ts = Some(expected_close_ts);
            metrics.last_final_bar_gap_actual_close_ts = Some(bar.close_ts);
            if metrics.first_final_bar_gap_expected_close_ts.is_none() {
                metrics.first_final_bar_gap_expected_close_ts = Some(expected_close_ts);
                metrics.first_final_bar_gap_actual_close_ts = Some(bar.close_ts);
            }
        }
    }
}

fn live_final_bar_stale_for_sec(close_ts: DateTime<Utc>, observed_ts: DateTime<Utc>) -> u64 {
    observed_ts
        .signed_duration_since(close_ts)
        .num_seconds()
        .max(0) as u64
}

fn is_fresh_live_final_bar(
    close_ts: DateTime<Utc>,
    observed_ts: DateTime<Utc>,
    freshness_threshold_sec: u64,
) -> bool {
    close_ts <= observed_ts
        && live_final_bar_stale_for_sec(close_ts, observed_ts) <= freshness_threshold_sec
}

fn max_datetime(current: Option<DateTime<Utc>>, candidate: DateTime<Utc>) -> DateTime<Utc> {
    current.map_or(candidate, |current| current.max(candidate))
}

fn record_closed_bar_finalizer_action(
    metrics: &mut FinamWsShadowMetrics,
    action: ClosedBarFinalizerActionKind,
) {
    match action {
        ClosedBarFinalizerActionKind::BufferedForming
        | ClosedBarFinalizerActionKind::UpdatedForming => {
            metrics.forming_bar_suppressed_count += 1;
        }
        ClosedBarFinalizerActionKind::EmittedClosedFromNextBar => {
            metrics.closed_bar_finalized_count += 1;
            metrics.forming_bar_suppressed_count += 1;
        }
        ClosedBarFinalizerActionKind::PassedThroughFinal => {
            metrics.final_bar_passthrough_count += 1;
        }
        ClosedBarFinalizerActionKind::SuppressedDuplicateFinal => {
            metrics.duplicate_final_suppressed_count += 1;
        }
        ClosedBarFinalizerActionKind::DroppedNonMonotonicForming => {
            metrics.non_monotonic_forming_dropped_count += 1;
        }
        ClosedBarFinalizerActionKind::PassedThroughNonLiveSource => {
            metrics.non_live_bar_passthrough_count += 1;
        }
    }
}

fn finam_ws_shadow_report_json(
    mode: &str,
    runtime: &FinamWsShadowRuntime,
    report: &FinamWsShadowIterationReport,
) -> serde_json::Value {
    let streams = serde_json::json!({
        "health": runtime.gateway.config().redis.health_stream,
        "readiness": runtime.gateway.config().redis.readiness_stream,
        "market_data": runtime.gateway.config().redis.market_data_stream,
    });
    let market_data_lifecycle =
        serde_json::to_value(&report.market_data_lifecycle).expect("lifecycle serializes");
    let market_data = serde_json::json!({
        "source_kind": "LiveStream",
        "closed_bar_finalizer_enabled": true,
        "strategy_bars_are_final_only": true,
        "raw_forming_bars_published_for_strategy": false,
        "freshness_threshold_sec": finam_ws_shadow_freshness_threshold_sec(timeframe_seconds(&runtime.resolved.timeframe).unwrap_or(60)),
        "published_market_data_count": report.metrics.published_market_data_count,
        "quote_event_count": report.metrics.quote_event_count,
        "bar_event_count": report.metrics.bar_event_count,
        "final_bar_event_count": report.metrics.final_bar_event_count,
        "forming_bar_event_count": report.metrics.forming_bar_event_count,
        "history_bar_event_count": report.metrics.history_bar_event_count,
        "read_only_bar_event_count": report.metrics.read_only_bar_event_count,
        "recovery_bar_event_count": report.metrics.recovery_bar_event_count,
        "live_bar_event_count": report.metrics.live_bar_event_count,
        "final_live_bar_event_count": report.metrics.final_live_bar_event_count,
        "forming_live_bar_event_count": report.metrics.forming_live_bar_event_count,
        "unknown_source_bar_event_count": report.metrics.unknown_source_bar_event_count,
        "closed_bar_finalized_count": report.metrics.closed_bar_finalized_count,
        "final_bar_passthrough_count": report.metrics.final_bar_passthrough_count,
        "forming_bar_suppressed_count": report.metrics.forming_bar_suppressed_count,
        "duplicate_final_suppressed_count": report.metrics.duplicate_final_suppressed_count,
        "non_monotonic_forming_dropped_count": report.metrics.non_monotonic_forming_dropped_count,
        "non_live_bar_passthrough_count": report.metrics.non_live_bar_passthrough_count,
        "first_live_bar_seen": report.metrics.first_live_bar_seen,
        "first_live_final_bar_seen": report.metrics.first_live_final_bar_seen,
        "first_live_final_bar_close_ts": report.metrics.first_live_final_bar_close_ts,
        "fresh_live_final_bar_seen": report.metrics.fresh_live_final_bar_seen,
        "first_fresh_live_final_bar_close_ts": report.metrics.first_fresh_live_final_bar_close_ts,
        "last_live_bar_close_ts": report.metrics.last_live_bar_close_ts,
        "last_final_live_bar_close_ts": report.metrics.last_final_live_bar_close_ts,
        "last_fresh_live_final_bar_close_ts": report.metrics.last_fresh_live_final_bar_close_ts,
        "latest_ws_bar_close_ts": report.metrics.latest_ws_bar_close_ts,
        "latest_ws_final_bar_close_ts": report.metrics.latest_ws_final_bar_close_ts,
        "latest_live_final_bar_stale_for_sec": report.metrics.latest_live_final_bar_stale_for_sec,
        "max_live_final_bar_stale_for_sec": report.metrics.max_live_final_bar_stale_for_sec,
        "stale_live_final_bar_count": report.metrics.stale_live_final_bar_count,
        "final_bar_gap_detected_count": report.metrics.final_bar_gap_detected_count,
        "first_final_bar_gap_expected_close_ts": report.metrics.first_final_bar_gap_expected_close_ts,
        "first_final_bar_gap_actual_close_ts": report.metrics.first_final_bar_gap_actual_close_ts,
        "last_final_bar_gap_expected_close_ts": report.metrics.last_final_bar_gap_expected_close_ts,
        "last_final_bar_gap_actual_close_ts": report.metrics.last_final_bar_gap_actual_close_ts,
        "ws_backlog_or_stale_bars_detected": report.metrics.stale_live_final_bar_count > 0,
        "fresh_live_readiness_evidence_missing": !report.metrics.fresh_live_final_bar_seen,
        "last_bar_source_kind": report.metrics.last_bar_source_kind,
    });
    let metrics = finam_ws_shadow_metrics_json(&report.metrics);

    serde_json::json!({
        "finam_ws_shadow": true,
        "mode": mode,
        "iteration": report.iteration,
        "elapsed_ms": report.elapsed_ms,
        "stop_reason": report.stop_reason,
        "live_trading_enabled": false,
        "command_consumer_enabled": runtime.gateway.config().features.command_consumer_enabled,
        "order_placement_enabled": runtime.gateway.config().features.order_placement_enabled,
        "cancel_enabled": runtime.gateway.config().features.cancel_enabled,
        "stop_sltp_bracket_enabled": runtime.gateway.config().features.stop_sltp_bracket_enabled,
        "symbol_present": !runtime.resolved.symbol.is_empty(),
        "timeframe": runtime.resolved.timeframe,
        "subscribe_bars": runtime.resolved.subscribe_bars,
        "subscribe_quotes": runtime.resolved.subscribe_quotes,
        "strategy_market_data_source": if runtime.resolved.subscribe_bars {
            "FinamWebSocketBarsLiveStream"
        } else {
            "NoStrategyMarketDataSource"
        },
        "rest_bars_used_for_strategy": false,
        "rest_market_data_used_for_strategy": false,
        "quotes_role": if runtime.resolved.subscribe_quotes {
            "diagnostic_only"
        } else {
            "disabled"
        },
        "bars_stream_required_for_strategy_parity": true,
        "streams": streams,
        "readiness_phase": report.readiness_phase,
        "readiness_reasons": report.readiness_reasons,
        "market_data_lifecycle": market_data_lifecycle,
        "market_data": market_data,
        "metrics": metrics,
    })
}

fn finam_ws_shadow_metrics_json(metrics: &FinamWsShadowMetrics) -> serde_json::Value {
    serde_json::json!({
        "connection_count": metrics.connection_count,
        "success_count": metrics.success_count,
        "failure_count": metrics.failure_count,
        "text_message_count": metrics.text_message_count,
        "data_envelope_count": metrics.data_envelope_count,
        "event_envelope_count": metrics.event_envelope_count,
        "error_envelope_count": metrics.error_envelope_count,
        "decode_error_count": metrics.decode_error_count,
        "mapper_error_count": metrics.mapper_error_count,
        "quote_event_count": metrics.quote_event_count,
        "bar_event_count": metrics.bar_event_count,
        "final_bar_event_count": metrics.final_bar_event_count,
        "forming_bar_event_count": metrics.forming_bar_event_count,
        "history_bar_event_count": metrics.history_bar_event_count,
        "read_only_bar_event_count": metrics.read_only_bar_event_count,
        "recovery_bar_event_count": metrics.recovery_bar_event_count,
        "live_bar_event_count": metrics.live_bar_event_count,
        "final_live_bar_event_count": metrics.final_live_bar_event_count,
        "forming_live_bar_event_count": metrics.forming_live_bar_event_count,
        "unknown_source_bar_event_count": metrics.unknown_source_bar_event_count,
        "closed_bar_finalized_count": metrics.closed_bar_finalized_count,
        "final_bar_passthrough_count": metrics.final_bar_passthrough_count,
        "forming_bar_suppressed_count": metrics.forming_bar_suppressed_count,
        "duplicate_final_suppressed_count": metrics.duplicate_final_suppressed_count,
        "non_monotonic_forming_dropped_count": metrics.non_monotonic_forming_dropped_count,
        "non_live_bar_passthrough_count": metrics.non_live_bar_passthrough_count,
        "published_market_data_count": metrics.published_market_data_count,
        "ping_count": metrics.ping_count,
        "pong_count": metrics.pong_count,
        "first_live_bar_seen": metrics.first_live_bar_seen,
        "first_live_final_bar_seen": metrics.first_live_final_bar_seen,
        "first_live_final_bar_close_ts": metrics.first_live_final_bar_close_ts,
        "fresh_live_final_bar_seen": metrics.fresh_live_final_bar_seen,
        "first_fresh_live_final_bar_close_ts": metrics.first_fresh_live_final_bar_close_ts,
        "last_history_bar_close_ts": metrics.last_history_bar_close_ts,
        "last_recovery_bar_close_ts": metrics.last_recovery_bar_close_ts,
        "last_live_bar_close_ts": metrics.last_live_bar_close_ts,
        "last_final_live_bar_close_ts": metrics.last_final_live_bar_close_ts,
        "last_fresh_live_final_bar_close_ts": metrics.last_fresh_live_final_bar_close_ts,
        "latest_ws_bar_close_ts": metrics.latest_ws_bar_close_ts,
        "latest_ws_final_bar_close_ts": metrics.latest_ws_final_bar_close_ts,
        "latest_live_final_bar_stale_for_sec": metrics.latest_live_final_bar_stale_for_sec,
        "max_live_final_bar_stale_for_sec": metrics.max_live_final_bar_stale_for_sec,
        "stale_live_final_bar_count": metrics.stale_live_final_bar_count,
        "final_bar_gap_detected_count": metrics.final_bar_gap_detected_count,
        "first_final_bar_gap_expected_close_ts": metrics.first_final_bar_gap_expected_close_ts,
        "first_final_bar_gap_actual_close_ts": metrics.first_final_bar_gap_actual_close_ts,
        "last_final_bar_gap_expected_close_ts": metrics.last_final_bar_gap_expected_close_ts,
        "last_final_bar_gap_actual_close_ts": metrics.last_final_bar_gap_actual_close_ts,
        "last_bar_source_kind": metrics.last_bar_source_kind,
        "first_decode_error_text_len": metrics.first_decode_error_text_len,
        "first_decode_error_shape": metrics.first_decode_error_shape,
        "first_mapper_error_kind": metrics.first_mapper_error_kind,
        "first_mapper_error_subscription_type": metrics.first_mapper_error_subscription_type,
        "first_mapper_error_payload_shape": metrics.first_mapper_error_payload_shape,
    })
}

fn finam_ws_mapper_error_kind(error: &FinamWsMapperError) -> &'static str {
    match error {
        FinamWsMapperError::NotData => "NotData",
        FinamWsMapperError::UnsupportedSubscriptionType(_) => "UnsupportedSubscriptionType",
        FinamWsMapperError::MissingPayload => "MissingPayload",
        FinamWsMapperError::MissingSymbol => "MissingSymbol",
        FinamWsMapperError::Decode(_) => "Decode",
        FinamWsMapperError::Mapper(_) => "Mapper",
    }
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
    expected_symbol: String,
    reference_quote_max_age_ms: i64,
    qty: String,
    request_timeout_ms: u64,
    output: PathBuf,
    actual_send_i_understand_risk: bool,
    pre_actual_gate_only: bool,
    raw_response_output: Option<PathBuf>,
    observe_working_before_cancel: bool,
    working_observation_timeout_ms: u64,
    working_observation_poll_ms: u64,
}

async fn run_finam_limit_cancel_one_shot(args: FinamLimitCancelOneShotArgs) -> Result<()> {
    let now = Utc::now();
    let limit_price = Decimal::from_str(&args.limit_price).context("invalid limit price")?;
    let operator_reference_price =
        Decimal::from_str(&args.reference_price).context("invalid reference price")?;
    let qty = Decimal::from_str(&args.qty).context("invalid qty")?;
    let qty_one = qty == Decimal::ONE;
    let compile_feature_enabled = cfg!(feature = "m3j16-actual-one-shot");
    let symbol_exact_match = args.symbol == args.expected_symbol;

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

    let quote = client.last_quote_typed(&token, &args.symbol).await?;
    let mapped_quote = map_quote(&quote, now).map_err(mapper_anyhow)?;
    let quote_reference_price = mapped_quote
        .last
        .or(mapped_quote.bid)
        .or(mapped_quote.ask)
        .context("M3j16 quote has no last/bid/ask reference price")?;
    let quote_ts = mapped_quote.source_ts.unwrap_or(mapped_quote.received_ts);
    let quote_age_ms = now.signed_duration_since(quote_ts).num_milliseconds();
    let reference_quote_fresh =
        quote_age_ms >= 0 && quote_age_ms <= args.reference_quote_max_age_ms;
    let reference_quote_artifact_digest = sha256_hex(
        format!(
            "{}:{}:{}:{}:{}",
            sha256_hex(args.symbol.as_bytes()),
            quote_reference_price,
            quote_ts.to_rfc3339(),
            args.reference_quote_max_age_ms,
            now.to_rfc3339()
        )
        .as_bytes(),
    );
    let operator_approval_digest = sha256_hex(
        format!(
            "{}:{}:{}:{}:{}:{}",
            sha256_hex(args.account_id.as_bytes()),
            sha256_hex(args.symbol.as_bytes()),
            args.limit_price,
            args.qty,
            args.reference_price,
            reference_quote_artifact_digest
        )
        .as_bytes(),
    );
    let account_operator_binding_ok =
        account_bound && !operator_approval_digest.is_empty() && symbol_exact_match;
    let reference_quote_bound_to_fresh_artifact =
        reference_quote_fresh && !reference_quote_artifact_digest.is_empty();
    let price_below_reference = limit_price < quote_reference_price;

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
        && qty_one
        && symbol_exact_match
        && account_operator_binding_ok
        && reference_quote_bound_to_fresh_artifact;

    let mut gate_report = m3j16_limit_cancel_one_shot_report(M3j16LimitCancelOneShotInput {
        generated_at_label: now.to_rfc3339(),
        m3j15_preflight_accepted,
        explicit_operator_limit_cancel_approval_current: true,
        actual_send_flag_present: args.actual_send_i_understand_risk,
        compile_feature_enabled,
        account_bound,
        symbol_bound: !args.symbol.is_empty(),
        symbol_exact_match_or_hash: symbol_exact_match,
        account_operator_binding_ok,
        reference_quote_bound_to_fresh_artifact,
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
    let mut working_observation_attempted = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let working_observation_attempted = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut working_observed = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let working_observed = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut working_observation_poll_count = 0usize;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let working_observation_poll_count = 0usize;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut working_observation_last_status: Option<String> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let working_observation_last_status: Option<String> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut working_observation_last_native_status: Option<String> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let working_observation_last_native_status: Option<String> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut working_observation_order_found = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let working_observation_order_found = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let actual_error = None::<String>;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let actual_error = if gate_report.actual_send_allowed {
        Some("compiled without m3j16-actual-one-shot feature".to_string())
    } else {
        None
    };

    if gate_report.actual_send_allowed && !args.pre_actual_gate_only {
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
                max_notional_per_order: Some(operator_reference_price * qty),
                max_notional_per_run: Some(operator_reference_price * qty),
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
                    raw_response_capture_path: args
                        .raw_response_output
                        .as_ref()
                        .map(|path| path.display().to_string()),
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
                if args.observe_working_before_cancel {
                    working_observation_attempted = true;
                    let started = Instant::now();
                    let timeout =
                        StdDuration::from_millis(args.working_observation_timeout_ms.max(1));
                    let poll_interval =
                        StdDuration::from_millis(args.working_observation_poll_ms.max(1));
                    loop {
                        working_observation_poll_count += 1;
                        let observation_now = Utc::now();
                        let observed_orders = client
                            .account_orders_typed(&token, &args.account_id)
                            .await
                            .context("M3j20 working observation orders poll")?;
                        let matched_order = observed_orders.orders.iter().find(|order| {
                            order.order_id.as_deref() == Some(broker_order_id.as_str())
                                || order.order.client_order_id.as_deref()
                                    == Some(client_order_id.as_str())
                        });
                        if let Some(order) = matched_order {
                            working_observation_order_found = true;
                            working_observation_last_native_status = Some(order.status.clone());
                            let mapped = map_order_state(order, observation_now)
                                .map_err(mapper_anyhow)
                                .context("M3j20 working observation order map")?;
                            working_observation_last_status = Some(format!("{:?}", mapped.status));
                            if matches!(mapped.status, OrderStatus::New | OrderStatus::Working) {
                                working_observed = true;
                                break;
                            }
                        }
                        if started.elapsed() >= timeout {
                            break;
                        }
                        tokio::time::sleep(poll_interval).await;
                    }
                }
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
            "operator_reference_price": operator_reference_price.to_string(),
            "quote_reference_price": quote_reference_price.to_string(),
            "expected_symbol_len": args.expected_symbol.len(),
            "expected_symbol_sha256": sha256_hex(args.expected_symbol.as_bytes()),
            "request_timeout_ms": args.request_timeout_ms,
            "reference_quote_max_age_ms": args.reference_quote_max_age_ms,
            "limit_price_below_reference": price_below_reference,
            "no_stop_sltp_bracket": true,
            "place_then_cancel_only": true,
            "observe_working_before_cancel": args.observe_working_before_cancel,
            "working_observation_timeout_ms": args.working_observation_timeout_ms,
            "working_observation_poll_ms": args.working_observation_poll_ms
        },
        "redacted_bindings": {
            "account_id_len": args.account_id.len(),
            "account_id_sha256": sha256_hex(args.account_id.as_bytes()),
            "symbol_len": args.symbol.len(),
            "symbol_sha256": sha256_hex(args.symbol.as_bytes()),
            "symbol_exact_match_or_hash": symbol_exact_match,
            "operator_approval_digest": operator_approval_digest,
            "reference_quote_artifact_digest": reference_quote_artifact_digest,
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
        "reference_quote_redacted": {
            "quote_reference_price": quote_reference_price.to_string(),
            "quote_ts_present": true,
            "quote_age_ms": quote_age_ms,
            "reference_quote_fresh": reference_quote_fresh,
            "reference_quote_bound_to_fresh_artifact": reference_quote_bound_to_fresh_artifact
        },
        "execution_redacted": {
            "actual_send_flag_present": args.actual_send_i_understand_risk,
            "pre_actual_gate_only": args.pre_actual_gate_only,
            "compile_feature_enabled": compile_feature_enabled,
            "place_attempted": place_attempted,
            "cancel_attempted": cancel_attempted,
            "broker_order_id_present": broker_order_id_present,
            "place_response_kind": place_response_kind,
            "cancel_response_kind": cancel_response_kind,
            "place_post_send_semantics": place_post_send_semantics,
            "cancel_post_send_semantics": cancel_post_send_semantics,
            "actual_error": actual_error,
            "raw_response_capture_requested": args.raw_response_output.is_some(),
            "working_observation_attempted": working_observation_attempted,
            "working_observed": working_observed,
            "working_observation_order_found": working_observation_order_found,
            "working_observation_poll_count": working_observation_poll_count,
            "working_observation_last_status": working_observation_last_status,
            "working_observation_last_native_status": working_observation_last_native_status
        }
    });
    print_json(payload.clone())?;
    write_json_payload(&args.output, &payload)?;
    Ok(())
}

struct FinamTinyPositionMarketOneShotArgs {
    secret_env: String,
    account_id: String,
    symbol: String,
    expected_symbol: String,
    entry_side: String,
    qty: String,
    reference_quote_max_age_ms: i64,
    request_timeout_ms: u64,
    position_observation_timeout_ms: u64,
    position_observation_poll_ms: u64,
    output: PathBuf,
    actual_entry_exit_i_understand_risk: bool,
    pre_actual_gate_only: bool,
    raw_response_output: Option<PathBuf>,
}

async fn run_finam_tiny_position_market_one_shot(
    args: FinamTinyPositionMarketOneShotArgs,
) -> Result<()> {
    let now = Utc::now();
    let qty = Decimal::from_str(&args.qty).context("invalid qty")?;
    let qty_one = qty == Decimal::ONE;
    let entry_side_buy = args.entry_side.eq_ignore_ascii_case("buy");
    let symbol_exact_match = args.symbol == args.expected_symbol;
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

    let quote = client.last_quote_typed(&token, &args.symbol).await?;
    let mapped_quote = map_quote(&quote, now).map_err(mapper_anyhow)?;
    let quote_reference_price = mapped_quote
        .last
        .or(mapped_quote.ask)
        .or(mapped_quote.bid)
        .context("M4-1c quote has no last/ask/bid reference price")?;
    let quote_ts = mapped_quote.source_ts.unwrap_or(mapped_quote.received_ts);
    let quote_age_ms = now.signed_duration_since(quote_ts).num_milliseconds();
    let reference_quote_fresh =
        quote_age_ms >= 0 && quote_age_ms <= args.reference_quote_max_age_ms;
    let reference_quote_bound_to_fresh_artifact = reference_quote_fresh;

    let account = client.account_typed(&token, &args.account_id).await?;
    let orders = client
        .account_orders_typed(&token, &args.account_id)
        .await?;
    let target_instrument = instrument_id_from_symbol(&args.symbol, Some("FUTURES"));
    let broker_truth =
        map_finam_broker_truth_snapshot(&account, &orders, now).map_err(mapper_anyhow)?;
    let canonical_summary = broker_truth.summarize_for_instrument(&target_instrument);
    let initial_account_positions_count = canonical_summary.account_open_positions_count;
    let initial_positions_count = canonical_summary.target_open_positions_count;
    let flat_position = broker_truth.target_is_flat(&target_instrument);
    let active_orders_count = canonical_summary.account_active_orders_count;
    let terminal_or_ignored_orders_count = canonical_summary.target_terminal_orders_count;
    let unknown_active_orders_count = canonical_summary.account_unknown_orders_count;
    let orphan_active_orders_count = 0usize;
    let broker_truth_clean = active_orders_count == 0
        && unknown_active_orders_count == 0
        && orphan_active_orders_count == 0
        && flat_position;
    let target_position_qty = broker_truth.target_position_qty(&target_instrument);

    let actual_send_allowed = compile_feature_enabled
        && args.actual_entry_exit_i_understand_risk
        && full_trade_token_scope_present
        && account_bound
        && symbol_exact_match
        && entry_side_buy
        && qty_one
        && broker_truth_clean
        && reference_quote_bound_to_fresh_artifact;

    let mut blockers = Vec::new();
    if !compile_feature_enabled {
        blockers.push("compiled without m3j16-actual-one-shot feature");
    }
    if !args.actual_entry_exit_i_understand_risk {
        blockers.push("actual entry/exit flag is not present");
    }
    if !full_trade_token_scope_present {
        blockers.push("token is not full-trade scoped");
    }
    if !account_bound {
        blockers.push("token is not bound to account");
    }
    if !symbol_exact_match {
        blockers.push("symbol does not match expected symbol");
    }
    if !entry_side_buy {
        blockers.push("only buy entry / sell exit is supported in M4-1c");
    }
    if !qty_one {
        blockers.push("qty must be exactly 1");
    }
    if !broker_truth_clean {
        blockers.push("broker truth is not flat/clean");
    }
    if !reference_quote_bound_to_fresh_artifact {
        blockers.push("reference quote is not fresh");
    }

    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut entry_attempted = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let entry_attempted = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut exit_attempted = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let exit_attempted = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut entry_response_kind: Option<String> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let entry_response_kind: Option<String> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut exit_response_kind: Option<String> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let exit_response_kind: Option<String> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut entry_post_send_semantics: Option<String> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let entry_post_send_semantics: Option<String> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut exit_post_send_semantics: Option<String> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let exit_post_send_semantics: Option<String> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut entry_broker_order_id_present = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let entry_broker_order_id_present = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut exit_broker_order_id_present = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let exit_broker_order_id_present = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut position_observation_attempted = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let position_observation_attempted = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut position_observed = false;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let position_observed = false;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut position_observation_poll_count = 0usize;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let position_observation_poll_count = 0usize;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut observed_positions_count = 0usize;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let observed_positions_count = 0usize;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut final_active_orders_count: Option<usize> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let final_active_orders_count: Option<usize> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut final_positions_count: Option<usize> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let final_positions_count: Option<usize> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut final_account_positions_count: Option<usize> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let final_account_positions_count: Option<usize> = None;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let mut actual_error: Option<String> = None;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let actual_error: Option<String> = None;

    if actual_send_allowed && !args.pre_actual_gate_only {
        #[cfg(feature = "m3j16-actual-one-shot")]
        {
            let gate =
                finam_gateway::EndpointGateApproved::m4_1c_tiny_position_market_after_operator_approval(
                    "M4-1c-tiny-position-market-one-shot",
                    true,
                )
                .context("M4-1c endpoint gate")?;
            let operator_arm = OperatorArm {
                session_id: format!("M4-1C-{}", now.timestamp()),
                armed_until: now + ChronoDuration::minutes(2),
                endpoint_calls_enabled: true,
                one_shot: true,
                endpoint_attempted: false,
                preflight_digest: sha256_hex(
                    format!(
                        "{}:{}:{}:{}",
                        sha256_hex(args.account_id.as_bytes()),
                        sha256_hex(args.symbol.as_bytes()),
                        args.entry_side,
                        args.qty
                    )
                    .as_bytes(),
                ),
            };
            let policy = OrderPreflightPolicy {
                allowed_accounts: vec![BrokerAccountId::new(args.account_id.clone())],
                allowed_venue_symbols: vec![args.symbol.clone()],
                allowed_order_types: vec![OrderType::Market],
                allowed_time_in_force: vec![TimeInForce::Day],
                min_qty: Decimal::ONE,
                qty_step: Decimal::ONE,
                max_qty: Decimal::ONE,
                price_step: None,
                max_market_qty: Decimal::ONE,
                max_notional_per_order: Some(quote_reference_price * qty),
                max_notional_per_run: Some(quote_reference_price * qty * Decimal::new(2, 0)),
                max_limit_deviation_bps: None,
                max_reference_age_ms: args.reference_quote_max_age_ms as u64,
                allow_cancel_by_broker_order_id_without_mapping: false,
                operator_arm,
            };
            let preflight_context = OrderPreflightContext {
                reference_price: Some(OrderReferencePrice {
                    price: quote_reference_price,
                    received_ts: quote_ts,
                }),
                current_run_notional: Decimal::ZERO,
            };

            let entry_client_order_id = ClientOrderId::new(format!(
                "M41CE{:012}",
                now.timestamp_millis().rem_euclid(1_000_000_000_000)
            ))
            .context("M4-1c entry client order id")?;
            let entry_order = PlaceOrder {
                request_id: StrategyRequestId::from(Uuid::from_u128(
                    now.timestamp_millis().max(0) as u128
                )),
                created_ts: now,
                ttl_ms: Some(60_000),
                account_id: BrokerAccountId::new(args.account_id.clone()),
                client_order_id: entry_client_order_id,
                instrument: InstrumentId {
                    symbol: "IMOEXF".to_string(),
                    venue_symbol: Some(args.symbol.clone()),
                    exchange: Exchange::Moex,
                    market: Market::Futures,
                },
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                qty,
                limit_price: None,
                time_in_force: TimeInForce::Day,
                comment: None,
            };
            let approved_entry = policy
                .approve_place_order_with_context(&entry_order, now, &preflight_context)
                .context("M4-1c entry market preflight")?;
            let entry_spec = build_place_order_request(&approved_entry, None)
                .context("M4-1c FINAM entry request build")?;
            let entry_capture = args.raw_response_output.as_ref().map(|path| {
                context_named_capture_path(path, "entry")
                    .display()
                    .to_string()
            });
            let entry_transport =
                M3d2RealOrderEndpointTransport::try_new(M3d2RealOrderEndpointTransportConfig {
                    rest_base_url: FinamConfig::default().rest_base_url,
                    request_timeout_ms: args.request_timeout_ms,
                    authorization_header_mode: FinamAuthorizationHeaderMode::BearerJwt,
                    external_endpoint_mode:
                        M3d2ExternalOrderEndpointMode::M3j16ActualOneShotExternalFinam,
                    raw_response_capture_path: entry_capture,
                })
                .map_err(|error| anyhow::anyhow!("M4-1c entry transport: {error:?}"))?;
            entry_attempted = true;
            let entry_execution = entry_transport
                .place_order_execution(&gate, &token, &entry_spec)
                .await;
            let entry_outcome = entry_execution.redacted_outcome();
            entry_response_kind = entry_outcome.response_kind.map(|kind| format!("{kind:?}"));
            entry_post_send_semantics = Some(format!("{:?}", entry_outcome.post_send_semantics));
            let entry_broker_order_id =
                entry_execution
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
            entry_broker_order_id_present = entry_broker_order_id.is_some();

            if entry_broker_order_id.is_some() {
                position_observation_attempted = true;
                let started = Instant::now();
                let timeout = StdDuration::from_millis(args.position_observation_timeout_ms.max(1));
                let poll_interval =
                    StdDuration::from_millis(args.position_observation_poll_ms.max(1));
                loop {
                    position_observation_poll_count += 1;
                    let account = client.account_typed(&token, &args.account_id).await?;
                    let observation_orders = client
                        .account_orders_typed(&token, &args.account_id)
                        .await?;
                    let observation_truth =
                        map_finam_broker_truth_snapshot(&account, &observation_orders, Utc::now())
                            .map_err(mapper_anyhow)?;
                    observed_positions_count = observation_truth
                        .summarize_for_instrument(&target_instrument)
                        .target_open_positions_count;
                    if observed_positions_count > 0 {
                        position_observed = true;
                        break;
                    }
                    if started.elapsed() >= timeout {
                        break;
                    }
                    tokio::time::sleep(poll_interval).await;
                }
            }

            if position_observed {
                let exit_now = Utc::now();
                let exit_client_order_id = ClientOrderId::new(format!(
                    "M41CX{:012}",
                    exit_now.timestamp_millis().rem_euclid(1_000_000_000_000)
                ))
                .context("M4-1c exit client order id")?;
                let exit_order = PlaceOrder {
                    request_id: StrategyRequestId::from(Uuid::from_u128(
                        exit_now.timestamp_millis().max(0) as u128 + 1,
                    )),
                    created_ts: exit_now,
                    ttl_ms: Some(60_000),
                    account_id: BrokerAccountId::new(args.account_id.clone()),
                    client_order_id: exit_client_order_id,
                    instrument: InstrumentId {
                        symbol: "IMOEXF".to_string(),
                        venue_symbol: Some(args.symbol.clone()),
                        exchange: Exchange::Moex,
                        market: Market::Futures,
                    },
                    side: OrderSide::Sell,
                    order_type: OrderType::Market,
                    qty,
                    limit_price: None,
                    time_in_force: TimeInForce::Day,
                    comment: None,
                };
                let exit_context = OrderPreflightContext {
                    reference_price: Some(OrderReferencePrice {
                        price: quote_reference_price,
                        received_ts: quote_ts,
                    }),
                    current_run_notional: quote_reference_price * qty,
                };
                let approved_exit = policy
                    .approve_place_order_with_context(&exit_order, exit_now, &exit_context)
                    .context("M4-1c exit market preflight")?;
                let exit_spec = build_place_order_request(&approved_exit, None)
                    .context("M4-1c FINAM exit request build")?;
                let exit_capture = args.raw_response_output.as_ref().map(|path| {
                    context_named_capture_path(path, "exit")
                        .display()
                        .to_string()
                });
                let exit_transport =
                    M3d2RealOrderEndpointTransport::try_new(M3d2RealOrderEndpointTransportConfig {
                        rest_base_url: FinamConfig::default().rest_base_url,
                        request_timeout_ms: args.request_timeout_ms,
                        authorization_header_mode: FinamAuthorizationHeaderMode::BearerJwt,
                        external_endpoint_mode:
                            M3d2ExternalOrderEndpointMode::M3j16ActualOneShotExternalFinam,
                        raw_response_capture_path: exit_capture,
                    })
                    .map_err(|error| anyhow::anyhow!("M4-1c exit transport: {error:?}"))?;
                exit_attempted = true;
                let exit_execution = exit_transport
                    .place_order_execution(&gate, &token, &exit_spec)
                    .await;
                let exit_outcome = exit_execution.redacted_outcome();
                exit_response_kind = exit_outcome.response_kind.map(|kind| format!("{kind:?}"));
                exit_post_send_semantics = Some(format!("{:?}", exit_outcome.post_send_semantics));
                exit_broker_order_id_present = exit_execution
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
                    })
                    .is_some();
            } else if entry_attempted {
                actual_error = Some(
                    "entry accepted but position snapshot was not observed; exit not sent"
                        .to_string(),
                );
            }

            let final_account = client.account_typed(&token, &args.account_id).await?;
            let final_orders = client
                .account_orders_typed(&token, &args.account_id)
                .await?;
            let final_truth =
                map_finam_broker_truth_snapshot(&final_account, &final_orders, Utc::now())
                    .map_err(mapper_anyhow)?;
            let final_summary = final_truth.summarize_for_instrument(&target_instrument);
            final_account_positions_count = Some(final_summary.account_open_positions_count);
            final_positions_count = Some(final_summary.target_open_positions_count);
            final_active_orders_count = Some(final_summary.account_active_orders_count);
        }
    }

    #[cfg(feature = "m3j16-actual-one-shot")]
    let final_flat = final_positions_count.unwrap_or(initial_positions_count) == 0;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let final_flat = initial_positions_count == 0;
    #[cfg(feature = "m3j16-actual-one-shot")]
    let final_no_active_orders = final_active_orders_count.unwrap_or(active_orders_count) == 0;
    #[cfg(not(feature = "m3j16-actual-one-shot"))]
    let final_no_active_orders = active_orders_count == 0;
    let payload = serde_json::json!({
        "fixture_kind": "m4-1c-tiny-position-market-one-shot-redacted-v1",
        "generated_at": now.to_rfc3339(),
        "report": {
            "m4_step": "M4-1c",
            "actual_send_allowed": actual_send_allowed,
            "decision": if actual_send_allowed { "ActualSendAllowed" } else { "Blocked" },
            "blockers": blockers,
            "boundary_invocation_performed": entry_attempted || exit_attempted,
            "real_finam_order_endpoint_used": entry_attempted || exit_attempted,
            "runtime_live_attachment_allowed": false,
            "command_consumer_to_real_finam_allowed": false,
            "stop_sltp_bracket_replace_multileg_allowed": false,
            "market_order_authorized_by_operator": args.actual_entry_exit_i_understand_risk,
            "position_lifecycle_only": true,
            "final_flat": final_flat,
            "final_no_active_orders": final_no_active_orders
        },
        "operator_scope": {
            "symbol_len": args.symbol.len(),
            "symbol_sha256": sha256_hex(args.symbol.as_bytes()),
            "expected_symbol_len": args.expected_symbol.len(),
            "expected_symbol_sha256": sha256_hex(args.expected_symbol.as_bytes()),
            "entry_side": "buy",
            "entry_type": "market",
            "exit_side": "sell",
            "exit_type": "market",
            "qty": args.qty,
            "max_orders_total": 2,
            "position_observation_timeout_ms": args.position_observation_timeout_ms,
            "position_observation_poll_ms": args.position_observation_poll_ms,
            "request_timeout_ms": args.request_timeout_ms,
            "reference_quote_max_age_ms": args.reference_quote_max_age_ms
        },
        "pre_boundary_broker_truth": {
            "truth_source": "BrokerTruthSnapshot",
            "token_readonly_flag_present": token_details.readonly.is_some(),
            "token_readonly_flag_value": token_details.readonly,
            "token_account_hash_match": account_bound,
            "positions_count": initial_positions_count,
            "account_positions_count": initial_account_positions_count,
            "positions_scope": "target_symbol_nonzero",
            "target_position_qty": target_position_qty.to_string(),
            "target_is_flat": flat_position,
            "orders_total": orders.orders.len(),
            "active_orders_count": active_orders_count,
            "unknown_active_orders_count": unknown_active_orders_count,
            "orphan_active_orders_count": orphan_active_orders_count,
            "terminal_or_ignored_orders_count": terminal_or_ignored_orders_count,
            "canonical_summary": {
                "target_open_positions_count": canonical_summary.target_open_positions_count,
                "account_open_positions_count": canonical_summary.account_open_positions_count,
                "target_active_orders_count": canonical_summary.target_active_orders_count,
                "target_unknown_orders_count": canonical_summary.target_unknown_orders_count,
                "target_terminal_orders_count": canonical_summary.target_terminal_orders_count,
                "target_inconsistent_orders_count": canonical_summary.target_inconsistent_orders_count,
                "account_active_orders_count": canonical_summary.account_active_orders_count,
                "account_unknown_orders_count": canonical_summary.account_unknown_orders_count,
                "account_orphan_orders_count": canonical_summary.account_orphan_orders_count,
                "other_symbol_active_orders_count": canonical_summary.other_symbol_active_orders_count
            },
            "broker_truth_clean": broker_truth_clean
        },
        "reference_quote_redacted": {
            "quote_reference_price": quote_reference_price.to_string(),
            "quote_ts_present": true,
            "quote_age_ms": quote_age_ms,
            "reference_quote_fresh": reference_quote_fresh,
            "reference_quote_bound_to_fresh_artifact": reference_quote_bound_to_fresh_artifact
        },
        "execution_redacted": {
            "actual_entry_exit_flag_present": args.actual_entry_exit_i_understand_risk,
            "pre_actual_gate_only": args.pre_actual_gate_only,
            "compile_feature_enabled": compile_feature_enabled,
            "entry_attempted": entry_attempted,
            "entry_response_kind": entry_response_kind,
            "entry_post_send_semantics": entry_post_send_semantics,
            "entry_broker_order_id_present": entry_broker_order_id_present,
            "position_observation_attempted": position_observation_attempted,
            "position_observed": position_observed,
            "position_observation_poll_count": position_observation_poll_count,
            "observed_positions_count": observed_positions_count,
            "observed_positions_scope": "target_symbol_nonzero",
            "exit_attempted": exit_attempted,
            "exit_response_kind": exit_response_kind,
            "exit_post_send_semantics": exit_post_send_semantics,
            "exit_broker_order_id_present": exit_broker_order_id_present,
            "final_active_orders_count": final_active_orders_count,
            "final_positions_count": final_positions_count,
            "final_account_positions_count": final_account_positions_count,
            "final_positions_scope": "target_symbol_nonzero",
            "final_truth_source": "BrokerTruthSnapshot",
            "actual_error": actual_error,
            "raw_response_capture_requested": args.raw_response_output.is_some()
        }
    });
    print_json(payload.clone())?;
    write_json_payload(&args.output, &payload)?;
    Ok(())
}

#[cfg(feature = "m3j16-actual-one-shot")]
fn context_named_capture_path(path: &std::path::Path, label: &str) -> std::path::PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("raw-order-response.json");
    let context_file_name = file_name
        .strip_suffix(".json")
        .map(|stem| format!("{stem}.{label}.json"))
        .unwrap_or_else(|| format!("{file_name}.{label}.json"));
    path.with_file_name(context_file_name)
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

async fn run_typed_canonical_readiness_package_probe(
    client: &FinamRestClient,
    token: &AccessToken,
    account_id: &str,
    symbol: &str,
    history_query: HistoryQuery<'_>,
    plain_micro_stop_waiver_operator_approved_no_send: bool,
) -> serde_json::Value {
    let account = match client.account_typed(token, account_id).await {
        Ok(account) => account,
        Err(error) => return canonical_readiness_package_error_record("account_typed", &error),
    };
    let orders = match client.account_orders_typed(token, account_id).await {
        Ok(orders) => orders,
        Err(error) => {
            return canonical_readiness_package_error_record("account_orders_typed", &error)
        }
    };
    let trades_result = client
        .account_trades_typed(token, account_id, history_query)
        .await;
    let transactions_result = client
        .account_transactions_typed(token, account_id, history_query)
        .await;
    let asset = match client.asset_typed(token, symbol, Some(account_id)).await {
        Ok(asset) => asset,
        Err(error) => return canonical_readiness_package_error_record("asset_typed", &error),
    };
    let params = match client
        .asset_params_typed(token, symbol, Some(account_id))
        .await
    {
        Ok(params) => params,
        Err(error) => {
            return canonical_readiness_package_error_record("asset_params_typed", &error)
        }
    };
    let schedule = match client.asset_schedule_typed(token, symbol).await {
        Ok(schedule) => schedule,
        Err(error) => {
            return canonical_readiness_package_error_record("asset_schedule_typed", &error)
        }
    };
    let quote = match client.last_quote_typed(token, symbol).await {
        Ok(quote) => quote,
        Err(error) => return canonical_readiness_package_error_record("last_quote_typed", &error),
    };
    let trades_error = trades_result
        .as_ref()
        .err()
        .map(FinamError::to_redacted_string);
    let trades = trades_result.ok();
    let transactions_error = transactions_result
        .as_ref()
        .err()
        .map(FinamError::to_redacted_string);
    let transactions_count = transactions_result
        .as_ref()
        .ok()
        .map(|transactions| transactions.transactions.len());
    let received_ts = Utc::now();
    let instrument_artifacts = [FinamInstrumentSpecArtifacts {
        asset: &asset,
        params: &params,
        schedule: &schedule,
    }];
    let target = instrument_id_from_symbol(symbol, Some("FUTURES"));
    let scope = BrokerLiveEntryScope {
        account_id: BrokerAccountId::new(account_id),
        symbol: symbol.to_string(),
        order_type: "market".to_string(),
    };
    let operational_config = typed_readonly_canonical_operational_config(account_id, symbol);
    let capabilities = typed_readonly_canonical_capabilities();
    let pre_waiver_package =
        match build_finam_canonical_readiness_package(FinamCanonicalReadinessPackageInput {
            account: &account,
            orders: &orders,
            trades: trades.as_ref(),
            quote: Some(&quote),
            instruments: &instrument_artifacts,
            schedule: Some(&schedule),
            target_instrument: &target,
            side: OrderSide::Buy,
            qty: Decimal::ONE,
            reference_price: Decimal::ONE,
            operational_config: &operational_config,
            capabilities: &capabilities,
            live_entry_scope: &scope,
            stop_order_waiver_policy: None,
            received_ts,
        }) {
            Ok(package) => package,
            Err(error) => {
                return serde_json::json!({
                    "probe": "canonical_readiness_package_typed",
                    "ok": false,
                    "error_kind": "mapper_error",
                    "error": error.to_string(),
                    "live_trading_enabled": false,
                    "order_endpoints_used": false,
                    "typed_probe": true,
                })
            }
        };
    let waiver_policy = plain_micro_stop_waiver_operator_approved_no_send
        .then(|| typed_readonly_plain_micro_stop_waiver_policy(account_id, symbol));
    let package =
        match build_finam_canonical_readiness_package(FinamCanonicalReadinessPackageInput {
            account: &account,
            orders: &orders,
            trades: trades.as_ref(),
            quote: Some(&quote),
            instruments: &instrument_artifacts,
            schedule: Some(&schedule),
            target_instrument: &target,
            side: OrderSide::Buy,
            qty: Decimal::ONE,
            reference_price: Decimal::ONE,
            operational_config: &operational_config,
            capabilities: &capabilities,
            live_entry_scope: &scope,
            stop_order_waiver_policy: waiver_policy.as_ref(),
            received_ts,
        }) {
            Ok(package) => package,
            Err(error) => {
                return serde_json::json!({
                    "probe": "canonical_readiness_package_typed",
                    "ok": false,
                    "error_kind": "mapper_error",
                    "error": error.to_string(),
                    "live_trading_enabled": false,
                    "order_endpoints_used": false,
                    "typed_probe": true,
                })
            }
        };
    let enriched_orders_count = package
        .broker_truth
        .orders
        .iter()
        .filter(|order| {
            order.broker_asset_id.is_some()
                || order.board.is_some()
                || order.expiration_date.is_some()
        })
        .count();
    let enriched_trades_count = package
        .broker_truth
        .trades
        .iter()
        .filter(|trade| {
            trade.broker_asset_id.is_some()
                || trade.board.is_some()
                || trade.expiration_date.is_some()
        })
        .count();
    let orphan_order_reasons_by_kind = orphan_order_reasons_by_kind(&package.broker_truth);
    let filled_orders_count = package
        .broker_truth
        .orders
        .iter()
        .filter(|order| order.filled_qty > Decimal::ZERO)
        .count();
    let canonical_preflight_blocks = package
        .canonical_preflight_decision
        .blocks
        .iter()
        .map(|block| format!("{block:?}"))
        .collect::<Vec<_>>();
    let pre_waiver_canonical_preflight_blocks = pre_waiver_package
        .canonical_preflight_decision
        .blocks
        .iter()
        .map(|block| format!("{block:?}"))
        .collect::<Vec<_>>();
    let waiver_decision = &package
        .canonical_preflight_decision
        .stop_order_waiver_decision;
    let waiver_rejections = waiver_decision
        .rejections
        .iter()
        .map(|rejection| format!("{rejection:?}"))
        .collect::<Vec<_>>();
    let m4_2j_no_send_pre_authorization_ready = plain_micro_stop_waiver_operator_approved_no_send
        && package.canonical_preflight_decision.allowed
        && package.no_live_authorization;
    let quote_observed_ts = package.broker_readiness.quotes.observed_ts;
    let quote_age_ms_at_package_ts = quote_observed_ts.map(|observed_ts| {
        received_ts
            .signed_duration_since(observed_ts)
            .num_milliseconds()
    });
    let quote_fresh_at_package_ts = package.broker_readiness.quotes.is_fresh_at(received_ts);
    let quote_timestamp_present = quote.quote.timestamp.is_some();
    let quotes_stale_block_present = package
        .canonical_preflight_decision
        .readiness_decision
        .blocks
        .iter()
        .any(|block| format!("{block:?}") == "QuotesStale");
    let final_decision = if m4_2j_no_send_pre_authorization_ready {
        "NoSendPreAuthorizationReady"
    } else {
        "NoSendBlocked"
    };
    let operator_scope = serde_json::json!({
        "qty": "1",
        "side": "buy",
        "order_type": "market",
        "one_account": true,
        "one_symbol": true,
        "runtime_live_enabled": false,
        "command_consumer_to_real_finam_enabled": false,
        "stop_sltp_bracket_replace_multi_leg_enabled": false
    });
    let account_orders_history_window = serde_json::json!({
        "kind": "account_orders_unwindowed_current_endpoint",
        "explicit_window_supported_by_current_cli": false,
    });
    let readonly_broker_call_kinds = serde_json::json!([
        "account",
        "account_orders",
        "account_trades",
        "account_transactions",
        "asset",
        "asset_params",
        "asset_schedule",
        "last_quote"
    ]);
    let mut summary_map = serde_json::Map::new();
    summary_map.insert(
        "truth_source".to_string(),
        json_value("BrokerTruthSnapshot"),
    );
    summary_map.insert(
        "readiness_source".to_string(),
        json_value("BrokerReadinessSnapshot"),
    );
    summary_map.insert(
        "package_source".to_string(),
        json_value("FinamCanonicalReadinessPackage"),
    );
    summary_map.insert(
        "orders_count".to_string(),
        json_value(package.broker_truth.orders.len()),
    );
    summary_map.insert(
        "positions_count".to_string(),
        json_value(package.broker_truth.positions.len()),
    );
    summary_map.insert(
        "trades_count".to_string(),
        json_value(package.broker_truth.trades.len()),
    );
    summary_map.insert(
        "instruments_count".to_string(),
        json_value(package.broker_truth.instruments.len()),
    );
    summary_map.insert(
        "enriched_orders_count".to_string(),
        json_value(enriched_orders_count),
    );
    summary_map.insert(
        "enriched_trades_count".to_string(),
        json_value(enriched_trades_count),
    );
    summary_map.insert(
        "account_orphan_orders_count".to_string(),
        json_value(
            package
                .canonical_preflight_decision
                .truth_summary
                .account_orphan_orders_count,
        ),
    );
    summary_map.insert(
        "target_active_orders_count".to_string(),
        json_value(
            package
                .canonical_preflight_decision
                .truth_summary
                .target_active_orders_count,
        ),
    );
    summary_map.insert(
        "account_active_orders_count".to_string(),
        json_value(
            package
                .canonical_preflight_decision
                .truth_summary
                .account_active_orders_count,
        ),
    );
    summary_map.insert(
        "pre_waiver_canonical_preflight_allowed".to_string(),
        json_value(pre_waiver_package.canonical_preflight_decision.allowed),
    );
    summary_map.insert(
        "pre_waiver_canonical_preflight_blocks_count".to_string(),
        json_value(pre_waiver_package.canonical_preflight_decision.blocks.len()),
    );
    summary_map.insert(
        "pre_waiver_canonical_preflight_blocks".to_string(),
        json_value(pre_waiver_canonical_preflight_blocks),
    );
    summary_map.insert(
        "canonical_preflight_allowed".to_string(),
        json_value(package.canonical_preflight_decision.allowed),
    );
    summary_map.insert(
        "canonical_preflight_blocks_count".to_string(),
        json_value(package.canonical_preflight_decision.blocks.len()),
    );
    summary_map.insert(
        "canonical_preflight_blocks".to_string(),
        json_value(canonical_preflight_blocks),
    );
    summary_map.insert(
        "margin_sufficiency".to_string(),
        json_value(format!("{:?}", package.margin_sufficiency)),
    );
    summary_map.insert("package_received_ts".to_string(), json_value(received_ts));
    summary_map.insert("quote_probe_ok".to_string(), json_value(true));
    summary_map.insert(
        "quote_timestamp_present".to_string(),
        json_value(quote_timestamp_present),
    );
    summary_map.insert(
        "quote_observed_ts".to_string(),
        json_value(quote_observed_ts),
    );
    summary_map.insert(
        "quote_age_ms_at_package_ts".to_string(),
        json_value(quote_age_ms_at_package_ts),
    );
    summary_map.insert(
        "quote_max_age_ms".to_string(),
        json_value(package.broker_readiness.quotes.max_age_ms),
    );
    summary_map.insert(
        "quote_fresh_at_package_ts".to_string(),
        json_value(quote_fresh_at_package_ts),
    );
    summary_map.insert(
        "quotes_stale_block_present".to_string(),
        json_value(quotes_stale_block_present),
    );
    summary_map.insert(
        "plain_micro_stop_waiver_requested".to_string(),
        json_value(plain_micro_stop_waiver_operator_approved_no_send),
    );
    summary_map.insert(
        "plain_micro_stop_waiver_operator_approval_present".to_string(),
        json_value(plain_micro_stop_waiver_operator_approved_no_send),
    );
    summary_map.insert(
        "plain_micro_stop_waiver_source".to_string(),
        json_value(format!("{:?}", waiver_decision.source)),
    );
    summary_map.insert(
        "stop_order_waiver_applied".to_string(),
        json_value(
            package
                .canonical_preflight_decision
                .stop_order_waiver_decision
                .applied,
        ),
    );
    summary_map.insert(
        "plain_micro_stop_waiver_rejections".to_string(),
        json_value(waiver_rejections),
    );
    summary_map.insert("m4_2j_pre_authorization_gate".to_string(), json_value(true));
    summary_map.insert(
        "m4_2j_no_send_pre_authorization_ready".to_string(),
        json_value(m4_2j_no_send_pre_authorization_ready),
    );
    summary_map.insert(
        "pre_authorization_evidence_only".to_string(),
        json_value(true),
    );
    summary_map.insert("final_decision".to_string(), json_value(final_decision));
    summary_map.insert("actual_send_allowed".to_string(), json_value(false));
    summary_map.insert("operator_scope".to_string(), operator_scope);
    summary_map.insert(
        "no_live_authorization".to_string(),
        json_value(package.no_live_authorization),
    );
    summary_map.insert(
        "filled_orders_count".to_string(),
        json_value(filled_orders_count),
    );
    summary_map.insert(
        "orphan_order_reasons_by_kind".to_string(),
        json_value(orphan_order_reasons_by_kind),
    );
    summary_map.insert(
        "trades_window_start_ts".to_string(),
        json_value(history_query.start_time),
    );
    summary_map.insert(
        "trades_window_end_ts".to_string(),
        json_value(history_query.end_time),
    );
    summary_map.insert(
        "trades_window_explicit".to_string(),
        json_value(history_query.start_time.is_some() && history_query.end_time.is_some()),
    );
    summary_map.insert(
        "trades_probe_ok".to_string(),
        json_value(trades_error.is_none()),
    );
    summary_map.insert(
        "trades_error_present".to_string(),
        json_value(trades_error.is_some()),
    );
    summary_map.insert(
        "transactions_probe_ok".to_string(),
        json_value(transactions_error.is_none()),
    );
    summary_map.insert(
        "transactions_error_present".to_string(),
        json_value(transactions_error.is_some()),
    );
    summary_map.insert(
        "transactions_count".to_string(),
        json_value(transactions_count),
    );
    summary_map.insert(
        "account_orders_history_window".to_string(),
        account_orders_history_window,
    );
    summary_map.insert(
        "readonly_broker_calls_performed".to_string(),
        json_value(true),
    );
    summary_map.insert(
        "readonly_broker_call_kinds".to_string(),
        readonly_broker_call_kinds,
    );
    summary_map.insert(
        "order_post_delete_calls_performed".to_string(),
        json_value(false),
    );
    summary_map.insert("live_order_calls_performed".to_string(), json_value(false));
    let summary = serde_json::Value::Object(summary_map);

    serde_json::json!({
        "probe": "canonical_readiness_package_typed",
        "ok": true,
        "summary": summary,
        "live_trading_enabled": false,
        "order_endpoints_used": false,
        "typed_probe": true,
    })
}

fn orphan_order_reasons_by_kind(truth: &BrokerTruthSnapshot) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for order in truth.account_orphan_orders() {
        for reason in truth.orphan_reasons_for_order(order) {
            *counts.entry(format!("{reason:?}")).or_insert(0) += 1;
        }
    }
    counts
}

fn typed_readonly_plain_micro_stop_waiver_policy(
    account_id: &str,
    symbol: &str,
) -> BrokerPlainMicroStopOrderWaiverPolicy {
    BrokerPlainMicroStopOrderWaiverPolicy {
        enabled: true,
        operator_approved: true,
        max_qty: Decimal::ONE,
        allowed_accounts: vec![BrokerAccountId::new(account_id)],
        allowed_symbols: vec![symbol.to_string()],
        allowed_order_types: vec!["market".to_string(), "limit".to_string()],
        runtime_live_enabled: false,
        command_consumer_to_real_finam_enabled: false,
        stop_sltp_bracket_replace_multi_leg_enabled: false,
    }
}

fn json_value<T: serde::Serialize>(value: T) -> serde_json::Value {
    serde_json::to_value(value).expect("redacted diagnostic JSON value must serialize")
}

fn canonical_readiness_package_error_record(stage: &str, error: &FinamError) -> serde_json::Value {
    serde_json::json!({
        "probe": "canonical_readiness_package_typed",
        "ok": false,
        "error_stage": stage,
        "error_kind": error.kind(),
        "error": error.to_redacted_string(),
        "live_trading_enabled": false,
        "order_endpoints_used": false,
        "typed_probe": true,
    })
}

fn typed_readonly_canonical_operational_config(
    account_id: &str,
    symbol: &str,
) -> BrokerOperationalConfig {
    BrokerOperationalConfig {
        timeouts: BrokerTimeoutConfig {
            connect_timeout_ms: 5_000,
            request_timeout_ms: 10_000,
            order_submit_timeout_ms: 10_000,
            cancel_timeout_ms: 10_000,
            reconcile_timeout_ms: 30_000,
            stream_heartbeat_timeout_ms: 70_000,
        },
        freshness: BrokerFreshnessConfig {
            account_snapshot_max_age_ms: 120_000,
            positions_max_age_ms: 120_000,
            orders_max_age_ms: 120_000,
            trades_max_age_ms: 120_000,
            quotes_max_age_ms: 30_000,
            instrument_spec_max_age_ms: 86_400_000,
            schedule_max_age_ms: 86_400_000,
        },
        risk_limits: BrokerRiskLimitConfig {
            max_orders_per_run: 1,
            max_position_qty: Decimal::ONE,
            max_position_lifetime_sec: 60,
            require_cash_margin_sufficiency: true,
        },
        scope: BrokerScopeConfig {
            allowed_accounts: vec![BrokerAccountId::new(account_id)],
            allowed_symbols: vec![symbol.to_string()],
            allowed_order_types: vec!["market".to_string(), "limit".to_string()],
            allowed_sessions: vec!["main".to_string()],
        },
        lifecycle: BrokerLifecycleConfig {
            begin_submit_persistence_required: true,
            request_cancel_persistence_required: true,
            idempotency_marker_required: true,
            one_shot_marker_required: true,
            crash_recovery_state_required: true,
            blind_retry_after_ambiguous_send_allowed: false,
        },
    }
}

fn typed_readonly_canonical_capabilities() -> BrokerCapabilityMatrix {
    BrokerCapabilityMatrix {
        supports_market_order: true,
        supports_limit_order: true,
        supports_cancel: true,
        supports_replace: false,
        supports_stop_sltp: false,
        supports_brackets: false,
        supports_multi_leg: false,
        supports_readonly_orders: true,
        supports_readonly_trades: true,
        supports_readonly_positions: true,
        supports_streaming_order_updates: false,
        supports_streaming_position_updates: false,
    }
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
    use chrono::TimeZone;
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
    fn finam_ws_shadow_config_keeps_live_order_features_disabled() {
        let resolved = resolve_finam_ws_shadow_config(
            FinamWsShadowArgs {
                config: None,
                secret_env: "FINAM_SECRET_TOKEN".to_string(),
                redis_url: None,
                symbol: Some("TICKER@MIC".to_string()),
                timeframe: None,
                subscribe_bars: true,
                subscribe_quotes: true,
                max_messages: 0,
                max_duration_seconds: 0,
                reconnect_delay_seconds: 0,
                max_iterations: None,
            },
            GatewayShadowFileConfig {
                redis_url: Some("redis://127.0.0.1:6379/".to_string()),
                source: Some("finam-ws-shadow-test".to_string()),
                timeframe: Some("TIME_FRAME_M1".to_string()),
                max_iterations: Some(3),
                streams: Some(GatewayShadowStreamsFileConfig {
                    health: Some("finam_ws_shadow:health".to_string()),
                    readiness: Some("finam_ws_shadow:readiness".to_string()),
                    portfolio: Some("finam_ws_shadow:portfolio:snapshot_disabled".to_string()),
                    orders_snapshot: Some("finam_ws_shadow:orders:snapshot_disabled".to_string()),
                    market_data: Some("finam_ws_shadow:market_data".to_string()),
                    command_ack: Some("finam_ws_shadow:command_acks_disabled".to_string()),
                    runtime_bridge_dlq: Some("finam_ws_shadow:runtime_bridge:dlq".to_string()),
                }),
                ..GatewayShadowFileConfig::default()
            },
        )
        .expect("resolved ws config");

        assert_eq!(resolved.gateway_config.source, "finam-ws-shadow-test");
        assert_eq!(resolved.timeframe, "TIME_FRAME_M1");
        assert_eq!(resolved.max_messages, 1);
        assert_eq!(resolved.max_duration_seconds, 1);
        assert_eq!(resolved.reconnect_delay_seconds, 1);
        assert_eq!(resolved.max_iterations, Some(3));
        assert!(!resolved.gateway_config.features.command_consumer_enabled);
        assert!(!resolved.gateway_config.features.order_placement_enabled);
        assert!(!resolved.gateway_config.features.cancel_enabled);
        assert!(!resolved.gateway_config.features.stop_sltp_bracket_enabled);
        assert_eq!(
            resolved.gateway_config.redis.market_data_stream,
            "finam_ws_shadow:market_data"
        );
        assert_eq!(
            resolved.gateway_config.redis.command_ack_stream,
            "finam_ws_shadow:command_acks_disabled"
        );
    }

    #[test]
    fn finam_ws_shadow_bars_are_required_for_strategy_parity_readiness() {
        let now = Utc::now();
        let no_bars_lifecycle = finam_ws_shadow_market_data_lifecycle_from_metrics(
            true,
            false,
            &FinamWsShadowMetrics {
                connection_count: 1,
                quote_event_count: 5,
                ..FinamWsShadowMetrics::default()
            },
            60,
            now,
        );
        let no_bars = finam_ws_shadow_readiness_from_lifecycle(true, &no_bars_lifecycle);
        assert_eq!(
            no_bars_lifecycle.phase,
            MarketDataLifecyclePhase::LiveSubscribing
        );
        assert_eq!(no_bars.phase, ReadinessPhase::Degraded);
        assert_eq!(no_bars.reasons, vec![ReadinessReason::FirstLiveBarMissing]);

        let forming_lifecycle = finam_ws_shadow_market_data_lifecycle_from_metrics(
            true,
            false,
            &FinamWsShadowMetrics {
                connection_count: 1,
                bar_event_count: 1,
                forming_bar_event_count: 1,
                live_bar_event_count: 1,
                forming_live_bar_event_count: 1,
                first_live_bar_seen: true,
                last_live_bar_close_ts: Some(now),
                ..FinamWsShadowMetrics::default()
            },
            60,
            now,
        );
        let forming = finam_ws_shadow_readiness_from_lifecycle(true, &forming_lifecycle);
        assert_eq!(
            forming_lifecycle.phase,
            MarketDataLifecyclePhase::LiveSubscribing
        );
        assert_eq!(forming.phase, ReadinessPhase::Degraded);
        assert_eq!(forming.reasons, vec![ReadinessReason::FirstLiveBarMissing]);

        let no_bar_subscription_lifecycle = finam_ws_shadow_market_data_lifecycle_from_metrics(
            false,
            true,
            &FinamWsShadowMetrics {
                connection_count: 1,
                quote_event_count: 5,
                ..FinamWsShadowMetrics::default()
            },
            60,
            now,
        );
        let no_bar_subscription =
            finam_ws_shadow_readiness_from_lifecycle(false, &no_bar_subscription_lifecycle);
        assert_eq!(no_bar_subscription.phase, ReadinessPhase::Degraded);
        assert_eq!(
            no_bar_subscription.reasons,
            vec![ReadinessReason::MarketDataNotLive]
        );

        let close_ts = now - ChronoDuration::seconds(60);
        let with_live_final_lifecycle = finam_ws_shadow_market_data_lifecycle_from_metrics(
            true,
            false,
            &FinamWsShadowMetrics {
                connection_count: 1,
                bar_event_count: 1,
                final_bar_event_count: 1,
                live_bar_event_count: 1,
                final_live_bar_event_count: 1,
                first_live_bar_seen: true,
                first_live_final_bar_seen: true,
                first_live_final_bar_close_ts: Some(close_ts),
                last_live_bar_close_ts: Some(close_ts),
                last_final_live_bar_close_ts: Some(close_ts),
                ..FinamWsShadowMetrics::default()
            },
            60,
            now,
        );
        let with_bars = finam_ws_shadow_readiness_from_lifecycle(true, &with_live_final_lifecycle);
        assert_eq!(
            with_live_final_lifecycle.phase,
            MarketDataLifecyclePhase::LiveReady
        );
        assert_eq!(with_bars.phase, ReadinessPhase::Reconciliation);
        assert_eq!(
            with_bars.reasons,
            vec![ReadinessReason::OperatorLiveArmMissing]
        );
    }

    fn sample_live_final_bar(close_ts: DateTime<Utc>) -> Bar {
        Bar {
            instrument: InstrumentId {
                symbol: "IMOEXF".to_string(),
                venue_symbol: Some("IMOEXF@RTSX".to_string()),
                exchange: Exchange::Moex,
                market: Market::Futures,
            },
            source_kind: MarketDataSourceKind::LiveStream,
            timeframe_sec: 60,
            open_ts: close_ts - ChronoDuration::seconds(60),
            close_ts,
            open: rust_decimal::Decimal::new(2250, 0),
            high: rust_decimal::Decimal::new(2251, 0),
            low: rust_decimal::Decimal::new(2249, 0),
            close: rust_decimal::Decimal::new(2250, 0),
            volume: rust_decimal::Decimal::new(1, 0),
            is_final: true,
        }
    }

    #[test]
    fn finam_ws_shadow_metrics_distinguish_fresh_and_stale_live_final_bars() {
        let close_ts = Utc.with_ymd_and_hms(2026, 7, 5, 10, 0, 0).unwrap();
        let fresh_observed_ts = Utc.with_ymd_and_hms(2026, 7, 5, 10, 1, 0).unwrap();
        let stale_observed_ts = Utc.with_ymd_and_hms(2026, 7, 5, 10, 10, 0).unwrap();

        let mut fresh_metrics = FinamWsShadowMetrics::default();
        record_canonical_ws_bar_metrics(
            &mut fresh_metrics,
            &sample_live_final_bar(close_ts),
            fresh_observed_ts,
            180,
        );

        assert!(fresh_metrics.fresh_live_final_bar_seen);
        assert_eq!(
            fresh_metrics.first_fresh_live_final_bar_close_ts,
            Some(close_ts)
        );
        assert_eq!(fresh_metrics.stale_live_final_bar_count, 0);
        assert_eq!(fresh_metrics.latest_live_final_bar_stale_for_sec, Some(60));

        let mut stale_metrics = FinamWsShadowMetrics::default();
        record_canonical_ws_bar_metrics(
            &mut stale_metrics,
            &sample_live_final_bar(close_ts),
            stale_observed_ts,
            180,
        );

        assert!(stale_metrics.first_live_final_bar_seen);
        assert!(!stale_metrics.fresh_live_final_bar_seen);
        assert_eq!(stale_metrics.stale_live_final_bar_count, 1);
        assert_eq!(stale_metrics.latest_live_final_bar_stale_for_sec, Some(600));
    }

    #[test]
    fn finam_ws_shadow_metrics_detect_final_bar_close_ts_gap() {
        let first_close = Utc.with_ymd_and_hms(2026, 7, 5, 10, 1, 0).unwrap();
        let gap_close = Utc.with_ymd_and_hms(2026, 7, 5, 10, 3, 0).unwrap();
        let observed_ts = Utc.with_ymd_and_hms(2026, 7, 5, 10, 3, 5).unwrap();
        let expected_missing_close = Utc.with_ymd_and_hms(2026, 7, 5, 10, 2, 0).unwrap();
        let mut metrics = FinamWsShadowMetrics::default();

        record_canonical_ws_bar_metrics(
            &mut metrics,
            &sample_live_final_bar(first_close),
            observed_ts,
            180,
        );
        record_canonical_ws_bar_metrics(
            &mut metrics,
            &sample_live_final_bar(gap_close),
            observed_ts,
            180,
        );

        assert_eq!(metrics.final_bar_gap_detected_count, 1);
        assert_eq!(
            metrics.first_final_bar_gap_expected_close_ts,
            Some(expected_missing_close)
        );
        assert_eq!(metrics.first_final_bar_gap_actual_close_ts, Some(gap_close));
        assert_eq!(
            metrics.last_final_bar_gap_expected_close_ts,
            Some(expected_missing_close)
        );
        assert_eq!(metrics.last_final_bar_gap_actual_close_ts, Some(gap_close));
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

    #[test]
    fn m4_1c_canonical_report_golden_requires_broker_truth_snapshot_source() {
        let report = serde_json::json!({
            "fixture_kind": "m4-1c-tiny-position-market-one-shot-redacted-v1",
            "pre_boundary_broker_truth": {
                "truth_source": "BrokerTruthSnapshot",
                "positions_count": 0,
                "account_positions_count": 1,
                "positions_scope": "target_symbol_nonzero",
                "target_position_qty": "0",
                "target_is_flat": true,
                "active_orders_count": 0,
                "unknown_active_orders_count": 0,
                "broker_truth_clean": true,
                "canonical_summary": {
                    "target_open_positions_count": 0,
                    "account_open_positions_count": 1,
                    "target_active_orders_count": 0,
                    "target_unknown_orders_count": 0,
                    "target_terminal_orders_count": 1,
                    "target_inconsistent_orders_count": 0,
                    "account_active_orders_count": 0,
                    "account_unknown_orders_count": 0,
                    "account_orphan_orders_count": 0,
                    "other_symbol_active_orders_count": 0
                }
            },
            "execution_redacted": {
                "final_truth_source": "BrokerTruthSnapshot",
                "final_positions_count": 0,
                "final_active_orders_count": 0
            }
        });
        let truth = &report["pre_boundary_broker_truth"];
        let summary = &truth["canonical_summary"];

        assert_eq!(truth["truth_source"], "BrokerTruthSnapshot");
        assert_eq!(
            truth["positions_count"],
            summary["target_open_positions_count"]
        );
        assert_eq!(
            truth["account_positions_count"],
            summary["account_open_positions_count"]
        );
        assert_eq!(
            truth["active_orders_count"],
            summary["account_active_orders_count"]
        );
        assert_eq!(
            truth["unknown_active_orders_count"],
            summary["account_unknown_orders_count"]
        );
        assert_eq!(truth["target_position_qty"], "0");
        assert_eq!(truth["target_is_flat"], true);
        assert_eq!(
            report["execution_redacted"]["final_truth_source"],
            "BrokerTruthSnapshot"
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
