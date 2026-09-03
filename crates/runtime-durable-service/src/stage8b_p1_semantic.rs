//! Stage 8B-P1-b canonical M10 admission and local-only delivery model.
//!
//! This module deliberately has no Redis connection, command publisher,
//! provider or FINAM transport.  The local stream is an executable model of
//! the exact-ID, group-before-publish, PEL and XACK-last invariants used by the
//! later operational composition.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use broker_core::{
    Exchange, HybridRuntimeBarEvent, HybridRuntimeBarOrigin, InstrumentId, Market,
    Stage3StrategyBarProvenance,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strategy_runtime_core::{
    accept_stage5c_semantic_bar, Stage5cAcceptedSemanticBar, Stage5cSemanticBarInput,
    Stage5gLifecycleCommitmentKey, Stage5gP1SemanticBindingInput,
    Stage6Stage8bP1SemanticCommitEvidenceV1,
};

use crate::recovery::{
    P1SemanticPrepublicationPending, P1SemanticZeroIntentAckPending, Stage7bRecoveryError,
    Stage7bRecoveryReadyOwner, Stage8bP1MultiIntentBlocked, Stage8bP1SemanticCommitOutcome,
    Stage8bP1SemanticPrepublicationOwner, Stage8bP1ZeroIntentCommitReceipt,
};

use crate::stage8b_p1_bootstrap::{
    stage8b_p1_imoexf_instrument_map_fingerprint_sha256, STAGE8B_P1_BROKER_ID, STAGE8B_P1_EXCHANGE,
    STAGE8B_P1_INTERNAL_SYMBOL, STAGE8B_P1_M10_CONSUMER_GROUP, STAGE8B_P1_MARKET,
    STAGE8B_P1_TICK_SIZE, STAGE8B_P1_VENUE_SYMBOL,
};

mod redis;

pub use redis::{
    connect_stage8b_p1_redis, resolve_stage8b_p1_zero_intent_ack_with_redis,
    resume_stage8b_p1_journal_ahead_with_redis, resume_stage8b_p1_prepublication_with_redis,
    Stage8bP1RedisCommandPublicationDisposition, Stage8bP1RedisCommandPublicationReceipt,
    Stage8bP1RedisCommandPublished, Stage8bP1RedisConfig, Stage8bP1RedisM10PublishDisposition,
    Stage8bP1RedisPrepublicationPending, Stage8bP1RedisSemanticCompositionOwner,
    Stage8bP1RedisSemanticCompositionTransport, Stage8bP1RedisSemanticError,
    Stage8bP1RedisSemanticOutcome, Stage8bP1RedisZeroIntentAckDisposition,
    Stage8bP1RedisZeroIntentAckResolved,
};

pub const STAGE8B_P1_CANONICAL_M10_SCHEMA_VERSION: u16 = 1;
pub const STAGE8B_P1_CANONICAL_M10_MESSAGE_TYPE: &str = "CanonicalFinalM10";
pub const STAGE8B_P1_CANONICAL_M10_IDENTITY_DOMAIN: &str = "moex.stage8b.p1.canonical-final-m10.v1";
pub const STAGE8B_P1_LOCAL_M10_MIN_RETENTION: usize = 4_096;
const M1_MILLIS: i64 = 60_000;
const M10_MILLIS: i64 = 600_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1CanonicalM10SourceM1 {
    pub redis_id: String,
    pub semantic_id_sha256: String,
    pub payload_sha256: String,
    pub open_ts_utc_ms: i64,
    pub close_ts_utc_ms: i64,
}

pub struct Stage8bP1CanonicalM10BuildInput {
    pub operational_identity_sha256: String,
    pub open_ts_utc_ms: i64,
    pub close_ts_utc_ms: i64,
    pub open: String,
    pub high: String,
    pub low: String,
    pub close: String,
    pub volume: String,
    pub source_m1: Vec<Stage8bP1CanonicalM10SourceM1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage8bP1CanonicalM10PayloadV1 {
    schema_version: u16,
    identity_domain: String,
    operational_identity_sha256: String,
    instrument_map_sha256: String,
    broker_id: String,
    internal_symbol: String,
    venue_symbol: String,
    exchange: String,
    market: String,
    timeframe_sec: u32,
    is_final: bool,
    open_ts_utc_ms: i64,
    close_ts_utc_ms: i64,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
    source_m1: Vec<Stage8bP1CanonicalM10SourceM1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage8bP1CanonicalM10EnvelopeV1 {
    schema_version: u16,
    message_type: String,
    identity_domain: String,
    redis_id: String,
    m10_semantic_id_sha256: String,
    m10_payload_sha256: String,
    payload: Stage8bP1CanonicalM10PayloadV1,
}

/// Strict-decoded canonical semantic input. It is evidence, not lifecycle or
/// publication authority. The original canonical bytes are retained so an
/// exact Redis-ID duplicate can be compared without normalization ambiguity.
pub struct Stage8bP1ValidatedCanonicalM10 {
    envelope: Stage8bP1CanonicalM10EnvelopeV1,
    canonical_bytes: Vec<u8>,
}

impl Stage8bP1ValidatedCanonicalM10 {
    pub fn redis_id(&self) -> &str {
        &self.envelope.redis_id
    }

    pub fn semantic_id_sha256(&self) -> &str {
        &self.envelope.m10_semantic_id_sha256
    }

    pub fn payload_sha256(&self) -> &str {
        &self.envelope.m10_payload_sha256
    }

    pub fn operational_identity_sha256(&self) -> &str {
        &self.envelope.payload.operational_identity_sha256
    }

    pub fn open_ts_utc_ms(&self) -> i64 {
        self.envelope.payload.open_ts_utc_ms
    }

    pub fn close_ts_utc_ms(&self) -> i64 {
        self.envelope.payload.close_ts_utc_ms
    }

    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub fn into_stage5c_semantic_bar(
        self,
    ) -> Result<Stage5cAcceptedSemanticBar, Stage8bP1CanonicalM10Error> {
        let payload = self.envelope.payload;
        let parse = |value: &str| {
            value
                .parse::<Decimal>()
                .ok()
                .and_then(|decimal| decimal.to_string().parse::<f64>().ok())
                .filter(|value| value.is_finite())
                .ok_or(Stage8bP1CanonicalM10Error::InvalidDecimal)
        };
        let bar = HybridRuntimeBarEvent {
            instrument: p1_instrument(),
            close_time_utc: payload.close_ts_utc_ms.div_euclid(1_000),
            open: parse(&payload.open)?,
            high: parse(&payload.high)?,
            low: parse(&payload.low)?,
            close: parse(&payload.close)?,
            volume: parse(&payload.volume)?,
            origin: HybridRuntimeBarOrigin::Live,
            is_final: true,
            timeframe_sec: 600,
        };
        accept_stage5c_semantic_bar(Stage5cSemanticBarInput {
            bar,
            provenance: Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            tick_size: STAGE8B_P1_TICK_SIZE
                .parse::<f64>()
                .expect("fixed P1 tick is valid"),
        })
        .map_err(|_| Stage8bP1CanonicalM10Error::Stage5cRejected)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1CanonicalM10Error {
    #[error("canonical M10 JSON is malformed or noncanonical")]
    Decode,
    #[error("canonical M10 schema or fixed identity is invalid")]
    IdentityMismatch,
    #[error("canonical M10 timestamp or source-bar continuity is invalid")]
    InvalidChronology,
    #[error("canonical M10 decimal is invalid or noncanonical")]
    InvalidDecimal,
    #[error("canonical M10 OHLCV is invalid")]
    InvalidOhlcv,
    #[error("canonical M10 digest is invalid")]
    DigestMismatch,
    #[error("canonical M10 was rejected by the accepted Stage 5C gate")]
    Stage5cRejected,
}

pub fn build_stage8b_p1_canonical_m10(
    input: Stage8bP1CanonicalM10BuildInput,
) -> Result<Vec<u8>, Stage8bP1CanonicalM10Error> {
    let payload = Stage8bP1CanonicalM10PayloadV1 {
        schema_version: STAGE8B_P1_CANONICAL_M10_SCHEMA_VERSION,
        identity_domain: STAGE8B_P1_CANONICAL_M10_IDENTITY_DOMAIN.to_string(),
        operational_identity_sha256: input.operational_identity_sha256,
        instrument_map_sha256: stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
        broker_id: STAGE8B_P1_BROKER_ID.to_string(),
        internal_symbol: STAGE8B_P1_INTERNAL_SYMBOL.to_string(),
        venue_symbol: STAGE8B_P1_VENUE_SYMBOL.to_string(),
        exchange: STAGE8B_P1_EXCHANGE.to_string(),
        market: STAGE8B_P1_MARKET.to_string(),
        timeframe_sec: 600,
        is_final: true,
        open_ts_utc_ms: input.open_ts_utc_ms,
        close_ts_utc_ms: input.close_ts_utc_ms,
        open: input.open,
        high: input.high,
        low: input.low,
        close: input.close,
        volume: input.volume,
        source_m1: input.source_m1,
    };
    validate_payload(&payload, &payload.operational_identity_sha256)?;
    let payload_bytes =
        serde_json::to_vec(&payload).map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
    let payload_sha256 = sha256_hex(&payload_bytes);
    let semantic_id_sha256 = domain_sha256(
        STAGE8B_P1_CANONICAL_M10_IDENTITY_DOMAIN.as_bytes(),
        &payload_bytes,
    );
    let envelope = Stage8bP1CanonicalM10EnvelopeV1 {
        schema_version: STAGE8B_P1_CANONICAL_M10_SCHEMA_VERSION,
        message_type: STAGE8B_P1_CANONICAL_M10_MESSAGE_TYPE.to_string(),
        identity_domain: STAGE8B_P1_CANONICAL_M10_IDENTITY_DOMAIN.to_string(),
        redis_id: format!("{}-0", payload.close_ts_utc_ms),
        m10_semantic_id_sha256: semantic_id_sha256,
        m10_payload_sha256: payload_sha256,
        payload,
    };
    serde_json::to_vec(&envelope).map_err(|_| Stage8bP1CanonicalM10Error::Decode)
}

pub fn parse_stage8b_p1_canonical_m10(
    bytes: &[u8],
    expected_operational_identity_sha256: &str,
) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
    let envelope: Stage8bP1CanonicalM10EnvelopeV1 =
        serde_json::from_slice(bytes).map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
    let canonical_bytes =
        serde_json::to_vec(&envelope).map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
    if canonical_bytes != bytes {
        return Err(Stage8bP1CanonicalM10Error::Decode);
    }
    if envelope.schema_version != STAGE8B_P1_CANONICAL_M10_SCHEMA_VERSION
        || envelope.message_type != STAGE8B_P1_CANONICAL_M10_MESSAGE_TYPE
        || envelope.identity_domain != STAGE8B_P1_CANONICAL_M10_IDENTITY_DOMAIN
        || envelope.redis_id != format!("{}-0", envelope.payload.close_ts_utc_ms)
    {
        return Err(Stage8bP1CanonicalM10Error::IdentityMismatch);
    }
    validate_payload(&envelope.payload, expected_operational_identity_sha256)?;
    let payload_bytes =
        serde_json::to_vec(&envelope.payload).map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
    if envelope.m10_payload_sha256 != sha256_hex(&payload_bytes)
        || envelope.m10_semantic_id_sha256
            != domain_sha256(
                STAGE8B_P1_CANONICAL_M10_IDENTITY_DOMAIN.as_bytes(),
                &payload_bytes,
            )
    {
        return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
    }
    Ok(Stage8bP1ValidatedCanonicalM10 {
        envelope,
        canonical_bytes,
    })
}

fn validate_payload(
    payload: &Stage8bP1CanonicalM10PayloadV1,
    expected_operational_identity_sha256: &str,
) -> Result<(), Stage8bP1CanonicalM10Error> {
    if payload.schema_version != STAGE8B_P1_CANONICAL_M10_SCHEMA_VERSION
        || payload.identity_domain != STAGE8B_P1_CANONICAL_M10_IDENTITY_DOMAIN
        || payload.operational_identity_sha256 != expected_operational_identity_sha256
        || !is_sha256(expected_operational_identity_sha256)
        || payload.instrument_map_sha256 != stage8b_p1_imoexf_instrument_map_fingerprint_sha256()
        || payload.broker_id != STAGE8B_P1_BROKER_ID
        || payload.internal_symbol != STAGE8B_P1_INTERNAL_SYMBOL
        || payload.venue_symbol != STAGE8B_P1_VENUE_SYMBOL
        || payload.exchange != STAGE8B_P1_EXCHANGE
        || payload.market != STAGE8B_P1_MARKET
        || payload.timeframe_sec != 600
        || !payload.is_final
    {
        return Err(Stage8bP1CanonicalM10Error::IdentityMismatch);
    }
    if payload.close_ts_utc_ms <= 0
        || payload.open_ts_utc_ms <= 0
        || payload.close_ts_utc_ms - payload.open_ts_utc_ms != M10_MILLIS
        || payload.close_ts_utc_ms.rem_euclid(M10_MILLIS) != 0
        || payload.source_m1.len() != 10
    {
        return Err(Stage8bP1CanonicalM10Error::InvalidChronology);
    }
    for (index, source) in payload.source_m1.iter().enumerate() {
        let expected_open = payload.open_ts_utc_ms + (index as i64) * M1_MILLIS;
        let expected_close = expected_open + M1_MILLIS;
        if source.open_ts_utc_ms != expected_open
            || source.close_ts_utc_ms != expected_close
            || source.redis_id != format!("{}-0", source.close_ts_utc_ms)
            || !is_sha256(&source.semantic_id_sha256)
            || !is_sha256(&source.payload_sha256)
        {
            return Err(Stage8bP1CanonicalM10Error::InvalidChronology);
        }
    }
    let open = canonical_decimal(&payload.open)?;
    let high = canonical_decimal(&payload.high)?;
    let low = canonical_decimal(&payload.low)?;
    let close = canonical_decimal(&payload.close)?;
    let volume = canonical_decimal(&payload.volume)?;
    if low > high || high < open.max(close) || low > open.min(close) || volume < Decimal::ZERO {
        return Err(Stage8bP1CanonicalM10Error::InvalidOhlcv);
    }
    Ok(())
}

fn canonical_decimal(value: &str) -> Result<Decimal, Stage8bP1CanonicalM10Error> {
    if value.is_empty() || value.starts_with('+') || value.contains('e') || value.contains('E') {
        return Err(Stage8bP1CanonicalM10Error::InvalidDecimal);
    }
    let parsed = value
        .parse::<Decimal>()
        .map_err(|_| Stage8bP1CanonicalM10Error::InvalidDecimal)?;
    if parsed.normalize().to_string() != value || value == "-0" {
        return Err(Stage8bP1CanonicalM10Error::InvalidDecimal);
    }
    Ok(parsed)
}

fn p1_instrument() -> InstrumentId {
    InstrumentId {
        symbol: STAGE8B_P1_INTERNAL_SYMBOL.to_string(),
        venue_symbol: Some(STAGE8B_P1_VENUE_SYMBOL.to_string()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn domain_sha256(domain: &[u8], payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(b"\0");
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1M10PublishDisposition {
    Published,
    IdempotentExisting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1LocalM10Error {
    #[error("P1 M10 group must exist before publication")]
    GroupMissing,
    #[error("P1 M10 retention is too small for active recovery entries")]
    UnsafeRetention,
    #[error("P1 M10 exact Redis ID collides with different content")]
    TerminalCollision,
    #[error("P1 M10 exact entry is absent, acknowledged, or not pending")]
    ExactPendingEntryMissing,
    #[error("P1 M10 delivery authority does not belong to this stream/group")]
    DeliveryAuthorityMismatch,
}

struct Stage8bP1LocalM10Entry {
    canonical_bytes: Vec<u8>,
    semantic_id_sha256: String,
    payload_sha256: String,
}

/// Local-only executable model. No method opens a network connection or
/// accepts an arbitrary group name.
pub struct Stage8bP1LocalM10Stream {
    retention_limit: usize,
    group_created: bool,
    entries: BTreeMap<String, Stage8bP1LocalM10Entry>,
    available: VecDeque<String>,
    pending: BTreeSet<String>,
    acknowledged: BTreeSet<String>,
    collisions: BTreeSet<String>,
}

/// Opaque, linear delivery. It intentionally implements no Clone, serde or
/// public constructor and grants no XACK method by itself.
pub struct Stage8bP1PendingM10Delivery {
    redis_id: String,
    semantic_id_sha256: String,
    payload_sha256: String,
    canonical_bytes: Vec<u8>,
}

impl Stage8bP1PendingM10Delivery {
    pub fn redis_id(&self) -> &str {
        &self.redis_id
    }

    pub fn semantic_id_sha256(&self) -> &str {
        &self.semantic_id_sha256
    }

    pub fn payload_sha256(&self) -> &str {
        &self.payload_sha256
    }

    pub fn parse_exact(
        &self,
        expected_operational_identity_sha256: &str,
    ) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
        parse_stage8b_p1_canonical_m10(&self.canonical_bytes, expected_operational_identity_sha256)
    }
}

impl Stage8bP1LocalM10Stream {
    pub fn new(retention_limit: usize) -> Result<Self, Stage8bP1LocalM10Error> {
        if retention_limit < STAGE8B_P1_LOCAL_M10_MIN_RETENTION {
            return Err(Stage8bP1LocalM10Error::UnsafeRetention);
        }
        Ok(Self {
            retention_limit,
            group_created: false,
            entries: BTreeMap::new(),
            available: VecDeque::new(),
            pending: BTreeSet::new(),
            acknowledged: BTreeSet::new(),
            collisions: BTreeSet::new(),
        })
    }

    pub fn create_canonical_group_mkstream(&mut self) {
        self.group_created = true;
    }

    pub fn consumer_group(&self) -> &'static str {
        STAGE8B_P1_M10_CONSUMER_GROUP
    }

    pub fn publish_exact(
        &mut self,
        m10: Stage8bP1ValidatedCanonicalM10,
    ) -> Result<Stage8bP1M10PublishDisposition, Stage8bP1LocalM10Error> {
        if !self.group_created {
            return Err(Stage8bP1LocalM10Error::GroupMissing);
        }
        if let Some(existing) = self.entries.get(m10.redis_id()) {
            if existing.canonical_bytes == m10.canonical_bytes
                && existing.semantic_id_sha256 == m10.semantic_id_sha256()
                && existing.payload_sha256 == m10.payload_sha256()
            {
                return Ok(Stage8bP1M10PublishDisposition::IdempotentExisting);
            }
            self.collisions.insert(m10.redis_id().to_string());
            return Err(Stage8bP1LocalM10Error::TerminalCollision);
        }
        // A retained active entry is never trimmed. If all space is active,
        // publication fails closed instead of silently losing recovery input.
        while self.entries.len() >= self.retention_limit {
            let removable = self
                .entries
                .keys()
                .find(|id| self.acknowledged.contains(*id) && !self.pending.contains(*id));
            let Some(removable) = removable.cloned() else {
                return Err(Stage8bP1LocalM10Error::UnsafeRetention);
            };
            self.entries.remove(&removable);
            self.acknowledged.remove(&removable);
        }
        let redis_id = m10.redis_id().to_string();
        self.entries.insert(
            redis_id.clone(),
            Stage8bP1LocalM10Entry {
                canonical_bytes: m10.canonical_bytes,
                semantic_id_sha256: m10.envelope.m10_semantic_id_sha256,
                payload_sha256: m10.envelope.m10_payload_sha256,
            },
        );
        self.available.push_back(redis_id);
        Ok(Stage8bP1M10PublishDisposition::Published)
    }

    pub fn read_next_pending(
        &mut self,
    ) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1LocalM10Error> {
        let redis_id = self
            .available
            .pop_front()
            .ok_or(Stage8bP1LocalM10Error::ExactPendingEntryMissing)?;
        let entry = self
            .entries
            .get(&redis_id)
            .ok_or(Stage8bP1LocalM10Error::ExactPendingEntryMissing)?;
        if self.collisions.contains(&redis_id) {
            return Err(Stage8bP1LocalM10Error::TerminalCollision);
        }
        self.pending.insert(redis_id.clone());
        Ok(Stage8bP1PendingM10Delivery {
            redis_id,
            semantic_id_sha256: entry.semantic_id_sha256.clone(),
            payload_sha256: entry.payload_sha256.clone(),
            canonical_bytes: entry.canonical_bytes.clone(),
        })
    }

    pub fn reclaim_exact_pending(
        &self,
        redis_id: &str,
        semantic_id_sha256: &str,
        payload_sha256: &str,
    ) -> Result<Stage8bP1PendingM10Delivery, Stage8bP1LocalM10Error> {
        if self.collisions.contains(redis_id) {
            return Err(Stage8bP1LocalM10Error::TerminalCollision);
        }
        if !self.pending.contains(redis_id) || self.acknowledged.contains(redis_id) {
            return Err(Stage8bP1LocalM10Error::ExactPendingEntryMissing);
        }
        let entry = self
            .entries
            .get(redis_id)
            .ok_or(Stage8bP1LocalM10Error::ExactPendingEntryMissing)?;
        if entry.semantic_id_sha256 != semantic_id_sha256 || entry.payload_sha256 != payload_sha256
        {
            return Err(Stage8bP1LocalM10Error::DeliveryAuthorityMismatch);
        }
        Ok(Stage8bP1PendingM10Delivery {
            redis_id: redis_id.to_string(),
            semantic_id_sha256: entry.semantic_id_sha256.clone(),
            payload_sha256: entry.payload_sha256.clone(),
            canonical_bytes: entry.canonical_bytes.clone(),
        })
    }

    pub(crate) fn acknowledge_after_durable_commit(
        &mut self,
        delivery: Stage8bP1PendingM10Delivery,
    ) -> Result<(), Stage8bP1LocalM10Error> {
        let entry = self
            .entries
            .get(&delivery.redis_id)
            .ok_or(Stage8bP1LocalM10Error::ExactPendingEntryMissing)?;
        if self.collisions.contains(&delivery.redis_id)
            || !self.pending.contains(&delivery.redis_id)
            || self.acknowledged.contains(&delivery.redis_id)
            || entry.semantic_id_sha256 != delivery.semantic_id_sha256
            || entry.payload_sha256 != delivery.payload_sha256
            || entry.canonical_bytes != delivery.canonical_bytes
        {
            return Err(Stage8bP1LocalM10Error::DeliveryAuthorityMismatch);
        }
        self.pending.remove(&delivery.redis_id);
        self.acknowledged.insert(delivery.redis_id);
        Ok(())
    }

    fn inspect_zero_intent_ack_source(
        &self,
        evidence: &Stage6Stage8bP1SemanticCommitEvidenceV1,
    ) -> Result<(Stage8bP1PendingM10Delivery, Stage8bP1ZeroIntentSourceState), Stage8bP1LocalM10Error>
    {
        let redis_id = &evidence.m10_redis_id;
        if self.collisions.contains(redis_id) {
            return Err(Stage8bP1LocalM10Error::TerminalCollision);
        }
        let entry = self
            .entries
            .get(redis_id)
            .ok_or(Stage8bP1LocalM10Error::ExactPendingEntryMissing)?;
        if entry.semantic_id_sha256 != evidence.m10_semantic_id_sha256
            || entry.payload_sha256 != evidence.m10_payload_sha256
        {
            return Err(Stage8bP1LocalM10Error::DeliveryAuthorityMismatch);
        }
        let state = match (
            self.pending.contains(redis_id),
            self.acknowledged.contains(redis_id),
        ) {
            (true, false) => Stage8bP1ZeroIntentSourceState::ExactPending,
            (false, true) => Stage8bP1ZeroIntentSourceState::ExactAlreadyAcknowledged,
            _ => return Err(Stage8bP1LocalM10Error::ExactPendingEntryMissing),
        };
        Ok((
            Stage8bP1PendingM10Delivery {
                redis_id: redis_id.clone(),
                semantic_id_sha256: entry.semantic_id_sha256.clone(),
                payload_sha256: entry.payload_sha256.clone(),
                canonical_bytes: entry.canonical_bytes.clone(),
            },
            state,
        ))
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn acknowledged_count(&self) -> usize {
        self.acknowledged.len()
    }
}

enum Stage8bP1ZeroIntentSourceState {
    ExactPending,
    ExactAlreadyAcknowledged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1ZeroIntentAckDisposition {
    AcknowledgedPending,
    AlreadyAcknowledged,
}

/// Exact-source resolution result for a recovered zero-intent S1.  The
/// contained composition owner is the only continuation authority; the
/// disposition and copied evidence are diagnostics only.
pub struct Stage8bP1ZeroIntentAckResolved {
    owner: Box<Stage8bP1SemanticCompositionOwner>,
    disposition: Stage8bP1ZeroIntentAckDisposition,
    evidence: Stage6Stage8bP1SemanticCommitEvidenceV1,
    covering_seal_generation: u64,
    covering_seal_commitment_sha256: String,
    stage6_checkpoint_sha256: String,
    stage5c_callback_count: usize,
}

impl Stage8bP1ZeroIntentAckResolved {
    pub fn disposition(&self) -> Stage8bP1ZeroIntentAckDisposition {
        self.disposition
    }

    pub fn evidence(&self) -> &Stage6Stage8bP1SemanticCommitEvidenceV1 {
        &self.evidence
    }

    pub fn recovery_seal_generation(&self) -> u64 {
        self.covering_seal_generation
    }

    pub fn recovery_seal_commitment_sha256(&self) -> &str {
        &self.covering_seal_commitment_sha256
    }

    pub fn stage6_checkpoint_sha256(&self) -> &str {
        &self.stage6_checkpoint_sha256
    }

    pub fn stage5c_callback_count(&self) -> usize {
        self.stage5c_callback_count
    }

    pub fn into_ready_owner(self) -> Box<Stage8bP1SemanticCompositionOwner> {
        self.owner
    }
}

/// Sole P1-b owner for the accepted Stage 7 durable runtime and the local
/// canonical M10 group model.  Neither component is independently extractable.
pub struct Stage8bP1SemanticCompositionOwner {
    stage7: Stage7bRecoveryReadyOwner,
    m10_stream: Stage8bP1LocalM10Stream,
}

pub struct Stage8bP1LocalPrepublicationPending {
    durable: Stage8bP1SemanticPrepublicationOwner,
    m10_stream: Stage8bP1LocalM10Stream,
    pending_m10: Stage8bP1PendingM10Delivery,
}

pub struct Stage8bP1LocalMultiIntentBlocked {
    durable: Stage8bP1MultiIntentBlocked,
    _m10_stream: Stage8bP1LocalM10Stream,
    _pending_m10: Stage8bP1PendingM10Delivery,
}

pub enum Stage8bP1LocalSemanticOutcome {
    Ready {
        owner: Box<Stage8bP1SemanticCompositionOwner>,
        receipt: Box<Stage8bP1ZeroIntentCommitReceipt>,
    },
    Prepublication(Box<Stage8bP1LocalPrepublicationPending>),
    MultiIntentBlocked(Box<Stage8bP1LocalMultiIntentBlocked>),
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1SemanticCompositionError {
    #[error("canonical local M10 stream rejected operation: {0}")]
    LocalM10(#[from] Stage8bP1LocalM10Error),
    #[error("canonical M10 validation failed: {0}")]
    CanonicalM10(#[from] Stage8bP1CanonicalM10Error),
    #[error("Stage 7 durable semantic transition failed: {0}")]
    Durable(#[from] Stage7bRecoveryError),
}

impl Stage8bP1SemanticCompositionOwner {
    pub fn new(stage7: Stage7bRecoveryReadyOwner, m10_stream: Stage8bP1LocalM10Stream) -> Self {
        Self { stage7, m10_stream }
    }

    pub fn process_next(
        mut self,
        commitment_key: &Stage5gLifecycleCommitmentKey,
    ) -> Result<Stage8bP1LocalSemanticOutcome, Stage8bP1SemanticCompositionError> {
        let delivery = self.m10_stream.read_next_pending()?;
        let operational_identity_sha256 = self
            .stage7
            .stage8b_p1_operational_identity_sha256()
            .to_string();
        let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
        let accepted_bar = delivery
            .parse_exact(&operational_identity_sha256)?
            .into_stage5c_semantic_bar()?;
        match self
            .stage7
            .commit_stage8b_p1_semantic(accepted_bar, binding, commitment_key)?
        {
            Stage8bP1SemanticCommitOutcome::ZeroIntent { owner, receipt } => {
                crate::recovery::stage8b_p1_test_crash_barrier(
                    "after-zero-intent-s1-reread-before-m10-xack",
                );
                self.m10_stream.acknowledge_after_durable_commit(delivery)?;
                Ok(Stage8bP1LocalSemanticOutcome::Ready {
                    owner: Box::new(Self {
                        stage7: *owner,
                        m10_stream: self.m10_stream,
                    }),
                    receipt: Box::new(receipt),
                })
            }
            Stage8bP1SemanticCommitOutcome::OneIntentPrepublication(durable) => {
                Ok(Stage8bP1LocalSemanticOutcome::Prepublication(Box::new(
                    Stage8bP1LocalPrepublicationPending {
                        durable: *durable,
                        m10_stream: self.m10_stream,
                        pending_m10: delivery,
                    },
                )))
            }
            Stage8bP1SemanticCommitOutcome::MultiIntentBlocked(durable) => {
                Ok(Stage8bP1LocalSemanticOutcome::MultiIntentBlocked(Box::new(
                    Stage8bP1LocalMultiIntentBlocked {
                        durable,
                        _m10_stream: self.m10_stream,
                        _pending_m10: delivery,
                    },
                )))
            }
        }
    }
}

/// Resolves only the exact canonical M10 source named by a recovered
/// zero-intent S1.  It invokes no Hybrid callback and performs no journal or
/// seal mutation.  Exact pending input is acknowledged once; exact
/// already-acknowledged input is accepted idempotently.
pub fn resolve_stage8b_p1_zero_intent_ack_with_local_m10(
    pending: P1SemanticZeroIntentAckPending,
    mut m10_stream: Stage8bP1LocalM10Stream,
) -> Result<Stage8bP1ZeroIntentAckResolved, Stage8bP1SemanticCompositionError> {
    let evidence = pending.evidence().clone();
    if evidence.intent_count != 0
        || evidence.strategy_request_id.is_some()
        || evidence.canonical_command_sha256.is_some()
        || evidence.request_accepted_record_id.is_some()
        || evidence.request_accepted_source_evidence_sha256.is_some()
    {
        return Err(Stage7bRecoveryError::SealInvalid.into());
    }

    let (delivery, source_state) = m10_stream.inspect_zero_intent_ack_source(&evidence)?;
    let validated = delivery.parse_exact(pending.operational_identity_sha256())?;
    if validated.redis_id() != evidence.m10_redis_id
        || validated.semantic_id_sha256() != evidence.m10_semantic_id_sha256
        || validated.payload_sha256() != evidence.m10_payload_sha256
    {
        return Err(Stage8bP1LocalM10Error::DeliveryAuthorityMismatch.into());
    }

    let disposition = match source_state {
        Stage8bP1ZeroIntentSourceState::ExactPending => {
            m10_stream.acknowledge_after_durable_commit(delivery)?;
            Stage8bP1ZeroIntentAckDisposition::AcknowledgedPending
        }
        Stage8bP1ZeroIntentSourceState::ExactAlreadyAcknowledged => {
            Stage8bP1ZeroIntentAckDisposition::AlreadyAcknowledged
        }
    };

    let covering_seal_generation = pending.recovery_seal_generation();
    let covering_seal_commitment_sha256 = pending.recovery_seal_commitment_sha256().to_string();
    let stage6_checkpoint_sha256 = pending.stage6_checkpoint_sha256().to_string();
    let stage5c_callback_count = pending.stage5c_callback_count();
    let stage7 = pending.into_ready_after_exact_source_resolution();

    Ok(Stage8bP1ZeroIntentAckResolved {
        owner: Box::new(Stage8bP1SemanticCompositionOwner { stage7, m10_stream }),
        disposition,
        evidence,
        covering_seal_generation,
        covering_seal_commitment_sha256,
        stage6_checkpoint_sha256,
        stage5c_callback_count,
    })
}

impl Stage8bP1LocalPrepublicationPending {
    pub fn evidence(&self) -> &strategy_runtime_core::Stage6Stage8bP1SemanticCommitEvidenceV1 {
        self.durable.evidence()
    }

    pub fn pending_m10_redis_id(&self) -> &str {
        self.pending_m10.redis_id()
    }

    pub fn pending_count(&self) -> usize {
        self.m10_stream.pending_count()
    }

    pub fn command_publication_allowed(&self) -> bool {
        false
    }

    pub fn m10_xack_allowed(&self) -> bool {
        false
    }
}

impl Stage8bP1LocalMultiIntentBlocked {
    pub fn semantic_batch_id_sha256(&self) -> &str {
        self.durable.semantic_batch_id_sha256()
    }

    pub fn intent_count(&self) -> usize {
        self.durable.intent_count()
    }

    pub fn command_publication_allowed(&self) -> bool {
        false
    }

    pub fn m10_xack_allowed(&self) -> bool {
        false
    }
}

/// Completes only the exact journal-ahead candidate from the same pending M10.
/// The delivery remains retained and unacknowledged because P1-c publication
/// and feedback settlement are outside P1-b.
pub fn resume_stage8b_p1_journal_ahead_with_local_m10(
    pending: P1SemanticPrepublicationPending,
    m10_stream: Stage8bP1LocalM10Stream,
    delivery: Stage8bP1PendingM10Delivery,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1LocalPrepublicationPending, Stage8bP1SemanticCompositionError> {
    let verified = m10_stream.reclaim_exact_pending(
        delivery.redis_id(),
        delivery.semantic_id_sha256(),
        delivery.payload_sha256(),
    )?;
    if verified.canonical_bytes != delivery.canonical_bytes {
        return Err(Stage8bP1LocalM10Error::DeliveryAuthorityMismatch.into());
    }
    let operational_identity_sha256 = pending.operational_identity_sha256().to_string();
    let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
    let accepted_bar = delivery
        .parse_exact(&operational_identity_sha256)?
        .into_stage5c_semantic_bar()?;
    let durable =
        pending.complete_with_exact_semantic_input(accepted_bar, binding, commitment_key)?;
    Ok(Stage8bP1LocalPrepublicationPending {
        durable,
        m10_stream,
        pending_m10: delivery,
    })
}

fn binding_from_delivery(
    delivery: &Stage8bP1PendingM10Delivery,
    operational_identity_sha256: String,
) -> Stage5gP1SemanticBindingInput {
    Stage5gP1SemanticBindingInput {
        operational_identity_sha256,
        m10_redis_id: delivery.redis_id().to_string(),
        m10_semantic_id_sha256: delivery.semantic_id_sha256().to_string(),
        m10_payload_sha256: delivery.payload_sha256().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage8b_p1_bootstrap::{
        authorize_stage8b_p1_first_boot, first_boot_stage8b_p1, restart_stage8b_p1,
        validate_stage8b_p1_bootstrap_config, Stage8bP1BootstrapConfig,
        STAGE8B_P1_BOOTSTRAP_CONFIG_SCHEMA_VERSION, STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
        STAGE8B_P1_STRATEGY_ID,
    };
    use crate::{Stage7bRecoveryBlockReason, Stage7bRestartOutcome};
    use std::{
        fs,
        os::unix::fs::DirBuilderExt,
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    fn source_m1(open_ts: i64) -> Vec<Stage8bP1CanonicalM10SourceM1> {
        (0..10)
            .map(|index| {
                let open = open_ts + index * M1_MILLIS;
                let close = open + M1_MILLIS;
                Stage8bP1CanonicalM10SourceM1 {
                    redis_id: format!("{close}-0"),
                    semantic_id_sha256: format!("{:064x}", index + 1),
                    payload_sha256: format!("{:064x}", index + 101),
                    open_ts_utc_ms: open,
                    close_ts_utc_ms: close,
                }
            })
            .collect()
    }

    fn canonical_bytes() -> Vec<u8> {
        let close = 1_785_628_200_000_i64;
        build_stage8b_p1_canonical_m10(Stage8bP1CanonicalM10BuildInput {
            operational_identity_sha256: "11".repeat(32),
            open_ts_utc_ms: close - M10_MILLIS,
            close_ts_utc_ms: close,
            open: "2180.5".to_string(),
            high: "2184".to_string(),
            low: "2179.5".to_string(),
            close: "2183.5".to_string(),
            volume: "12001".to_string(),
            source_m1: source_m1(close - M10_MILLIS),
        })
        .unwrap()
    }

    fn temp_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stage8b-p1-semantic-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder.create(&path).unwrap();
        fs::canonicalize(path).unwrap()
    }

    fn bootstrap_config(
        parent: PathBuf,
        runtime_config_fingerprint_sha256: String,
    ) -> Stage8bP1BootstrapConfig {
        Stage8bP1BootstrapConfig {
            schema_version: STAGE8B_P1_BOOTSTRAP_CONFIG_SCHEMA_VERSION,
            broker_id: STAGE8B_P1_BROKER_ID.to_string(),
            strategy_id: STAGE8B_P1_STRATEGY_ID.to_string(),
            account_id: "ACC_TEST_0001".to_string(),
            internal_symbol: STAGE8B_P1_INTERNAL_SYMBOL.to_string(),
            venue_symbol: STAGE8B_P1_VENUE_SYMBOL.to_string(),
            exchange: STAGE8B_P1_EXCHANGE.to_string(),
            market: STAGE8B_P1_MARKET.to_string(),
            tick_size: STAGE8B_P1_TICK_SIZE.to_string(),
            runtime_config_fingerprint_sha256,
            instrument_map_fingerprint_sha256: stage8b_p1_imoexf_instrument_map_fingerprint_sha256(
            ),
            deployment_id: "finam-imoexf-paper-p1".to_string(),
            deployment_generation: 1,
            gateway_instance_id: "finam-imoexf-paper-gateway-1".to_string(),
            market_data_generation: 1,
            command_consumer_generation: 1,
            stage8a4_writer_issuer_public_key_hex: "22".repeat(32),
            durable_parent: parent,
        }
    }

    fn first_boot(
        parent: &std::path::Path,
    ) -> (
        Stage7bRecoveryReadyOwner,
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
    ) {
        let (source, export_input, key, fresh) =
            strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let config = validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent.to_path_buf(),
            fresh.stage5c_config_fingerprint(),
        ))
        .unwrap();
        let admin =
            authorize_stage8b_p1_first_boot(&config, STAGE8B_P1_FIRST_BOOT_CONFIRMATION).unwrap();
        let outcome =
            first_boot_stage8b_p1(config, admin, source, export_input, &key, fresh.clone())
                .unwrap();
        (outcome.into_owner(), key, fresh)
    }

    fn semantic_m10_bytes(operational_identity_sha256: String, close: i64) -> Vec<u8> {
        let close_ts_utc_ms = 1_785_759_000_000_i64;
        let open = close.to_string();
        build_stage8b_p1_canonical_m10(Stage8bP1CanonicalM10BuildInput {
            operational_identity_sha256,
            open_ts_utc_ms: close_ts_utc_ms - M10_MILLIS,
            close_ts_utc_ms,
            open: open.clone(),
            high: (close + 1).to_string(),
            low: (close - 1).to_string(),
            close: open,
            volume: "10000".to_string(),
            source_m1: source_m1(close_ts_utc_ms - M10_MILLIS),
        })
        .unwrap()
    }

    fn local_stream_with_one_m10(
        operational_identity_sha256: String,
        close: i64,
    ) -> Stage8bP1LocalM10Stream {
        let bytes = semantic_m10_bytes(operational_identity_sha256.clone(), close);
        let parsed = parse_stage8b_p1_canonical_m10(&bytes, &operational_identity_sha256).unwrap();
        let mut stream = Stage8bP1LocalM10Stream::new(STAGE8B_P1_LOCAL_M10_MIN_RETENTION).unwrap();
        stream.create_canonical_group_mkstream();
        stream.publish_exact(parsed).unwrap();
        stream
    }

    fn zero_intent_restart_pending(
        parent: &Path,
    ) -> (
        Box<P1SemanticZeroIntentAckPending>,
        Stage5gLifecycleCommitmentKey,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        Stage8bP1LocalM10Stream,
    ) {
        let (owner, key, fresh) = first_boot(parent);
        let operational_identity_sha256 =
            owner.stage8b_p1_operational_identity_sha256().to_string();
        let mut stream = local_stream_with_one_m10(operational_identity_sha256.clone(), 2_600);
        let delivery = stream.read_next_pending().unwrap();
        let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
        let accepted_bar = delivery
            .parse_exact(&operational_identity_sha256)
            .unwrap()
            .into_stage5c_semantic_bar()
            .unwrap();
        let Stage8bP1SemanticCommitOutcome::ZeroIntent { owner, receipt } = owner
            .commit_stage8b_p1_semantic(accepted_bar, binding, &key)
            .unwrap()
        else {
            panic!("test source must produce a zero-intent durable S1");
        };
        assert_eq!(receipt.evidence.intent_count, 0);
        assert_eq!(stream.pending_count(), 1);
        assert_eq!(stream.acknowledged_count(), 0);
        drop(owner);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.to_path_buf(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(pending) = restart else {
            panic!("zero-intent durable S1 must await exact source ACK resolution");
        };
        (pending, key, fresh, stream)
    }

    fn wait_for_crash_barrier(child: &mut Child, marker: &Path) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !marker.exists() && Instant::now() < deadline {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("P1 semantic crash child exited before barrier: {status}");
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(marker.exists(), "P1 semantic crash child missed barrier");
    }

    #[test]
    #[ignore]
    fn p1_semantic_crash_frontier_child() {
        let parent = PathBuf::from(std::env::var_os("STAGE8B_P1_TEST_PARENT").unwrap());
        let (_, _, key, fresh) = strategy_runtime_core::stage8b_p1_test_first_boot_material();
        let config = validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent,
            fresh.stage5c_config_fingerprint(),
        ))
        .unwrap();
        let restart = restart_stage8b_p1(config, &key, fresh).unwrap();
        let Stage7bRestartOutcome::Ready(owner) = restart else {
            panic!("crash child must begin from exact S0 Ready state");
        };
        let close = if std::env::var("STAGE8B_P1_TEST_CRASH_PHASE").as_deref()
            == Ok("after-zero-intent-s1-reread-before-m10-xack")
        {
            2_600
        } else {
            2_650
        };
        let stream = local_stream_with_one_m10(
            owner.stage8b_p1_operational_identity_sha256().to_string(),
            close,
        );
        let _ = Stage8bP1SemanticCompositionOwner::new(*owner, stream)
            .process_next(&key)
            .unwrap();
        panic!("configured P1 semantic crash barrier was not reached");
    }

    #[test]
    fn subprocess_kill_matrix_recovers_all_seven_prepublication_frontiers() {
        let frontiers = [
            ("before-request-accepted-append", false),
            ("after-request-accepted-fsync", true),
            ("before-s1-temp-fsync", true),
            ("after-s1-temp-fsync-before-rename", true),
            ("after-s1-rename-before-directory-fsync", false),
            ("after-s1-directory-fsync-before-reread", false),
            ("after-s1-reread-before-command-xadd", false),
        ];

        for (phase, expect_journal_ahead) in frontiers {
            let parent = temp_directory(phase);
            let (owner, key, fresh) = first_boot(&parent);
            drop(owner);
            let marker = parent.join(format!("{phase}.marker"));
            let mut child = Command::new(std::env::current_exe().unwrap())
                .arg("--ignored")
                .arg("--exact")
                .arg("stage8b_p1_semantic::tests::p1_semantic_crash_frontier_child")
                .arg("--nocapture")
                .env("STAGE8B_P1_TEST_PARENT", &parent)
                .env("STAGE8B_P1_TEST_CRASH_PHASE", phase)
                .env("STAGE8B_P1_TEST_CRASH_MARKER", &marker)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            wait_for_crash_barrier(&mut child, &marker);
            child.kill().unwrap();
            child.wait().unwrap();

            let restart = restart_stage8b_p1(
                validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    fresh.stage5c_config_fingerprint(),
                ))
                .unwrap(),
                &key,
                fresh,
            )
            .unwrap();
            if phase == "before-request-accepted-append" {
                assert!(
                    matches!(restart, Stage7bRestartOutcome::Ready(_)),
                    "{phase}"
                );
            } else if expect_journal_ahead {
                let Stage7bRestartOutcome::P1SemanticPrepublicationPending(pending) = restart
                else {
                    panic!("{phase} must recover exact journal-ahead pending");
                };
                assert!(!pending.command_publication_allowed());
                assert!(!pending.m10_xack_allowed());
            } else {
                let Stage7bRestartOutcome::P1SemanticPrepublicationReady(ready) = restart else {
                    panic!("{phase} must recover exact committed S1 prepublication state");
                };
                assert!(!ready.command_publication_allowed());
                assert!(!ready.m10_xack_allowed());
            }
            fs::remove_dir_all(parent).unwrap();
        }
    }

    #[test]
    fn canonical_roundtrip_and_stage5c_admission_are_exact() {
        let bytes = canonical_bytes();
        let first = parse_stage8b_p1_canonical_m10(&bytes, &"11".repeat(32)).unwrap();
        assert_eq!(first.redis_id(), "1785628200000-0");
        assert_eq!(first.canonical_bytes(), bytes);
        assert!(first.into_stage5c_semantic_bar().is_ok());

        let mut pretty: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        pretty["payload"]["close"] = serde_json::Value::String("2183.50".to_string());
        let changed = serde_json::to_vec(&pretty).unwrap();
        assert!(matches!(
            parse_stage8b_p1_canonical_m10(&changed, &"11".repeat(32)),
            Err(Stage8bP1CanonicalM10Error::Decode)
                | Err(Stage8bP1CanonicalM10Error::InvalidDecimal)
                | Err(Stage8bP1CanonicalM10Error::DigestMismatch)
        ));
    }

    #[test]
    fn group_precedes_publish_and_exact_duplicate_is_idempotent() {
        let bytes = canonical_bytes();
        let parsed = || parse_stage8b_p1_canonical_m10(&bytes, &"11".repeat(32)).unwrap();
        let mut stream = Stage8bP1LocalM10Stream::new(STAGE8B_P1_LOCAL_M10_MIN_RETENTION).unwrap();
        assert_eq!(
            stream.publish_exact(parsed()).unwrap_err(),
            Stage8bP1LocalM10Error::GroupMissing
        );
        stream.create_canonical_group_mkstream();
        assert_eq!(
            stream.publish_exact(parsed()).unwrap(),
            Stage8bP1M10PublishDisposition::Published
        );
        assert_eq!(
            stream.publish_exact(parsed()).unwrap(),
            Stage8bP1M10PublishDisposition::IdempotentExisting
        );
        let delivery = stream.read_next_pending().unwrap();
        assert_eq!(stream.pending_count(), 1);
        stream.acknowledge_after_durable_commit(delivery).unwrap();
        assert_eq!(stream.pending_count(), 0);
        assert_eq!(stream.acknowledged_count(), 1);
    }

    #[test]
    fn active_entry_cannot_be_trimmed_and_delivery_is_exact_bound() {
        assert_eq!(
            Stage8bP1LocalM10Stream::new(10).err(),
            Some(Stage8bP1LocalM10Error::UnsafeRetention)
        );
        let bytes = canonical_bytes();
        let parsed = parse_stage8b_p1_canonical_m10(&bytes, &"11".repeat(32)).unwrap();
        let mut stream = Stage8bP1LocalM10Stream::new(STAGE8B_P1_LOCAL_M10_MIN_RETENTION).unwrap();
        stream.create_canonical_group_mkstream();
        stream.publish_exact(parsed).unwrap();
        let delivery = stream.read_next_pending().unwrap();
        assert!(stream
            .reclaim_exact_pending(
                delivery.redis_id(),
                delivery.semantic_id_sha256(),
                delivery.payload_sha256(),
            )
            .is_ok());
        assert!(matches!(
            stream.reclaim_exact_pending(
                delivery.redis_id(),
                &"ff".repeat(32),
                delivery.payload_sha256(),
            ),
            Err(Stage8bP1LocalM10Error::DeliveryAuthorityMismatch)
        ));

        let mut forged = delivery;
        forged.payload_sha256 = "ff".repeat(32);
        assert_eq!(
            stream.acknowledge_after_durable_commit(forged).unwrap_err(),
            Stage8bP1LocalM10Error::DeliveryAuthorityMismatch
        );
        assert_eq!(stream.pending_count(), 1);
        assert_eq!(stream.acknowledged_count(), 0);
    }

    #[test]
    fn zero_intent_commits_s1_before_the_only_m10_xack() {
        let parent = temp_directory("zero-intent");
        let (owner, key, fresh) = first_boot(&parent);
        let initial_generation = owner.committed_seal().unwrap().seal_generation();
        let stream = local_stream_with_one_m10(
            owner.stage8b_p1_operational_identity_sha256().to_string(),
            2_600,
        );
        let result = Stage8bP1SemanticCompositionOwner::new(owner, stream)
            .process_next(&key)
            .unwrap();
        let Stage8bP1LocalSemanticOutcome::Ready { owner, receipt } = result else {
            panic!("zero-intent M10 must return the sole ready owner");
        };
        assert_eq!(receipt.evidence.intent_count, 0);
        assert_eq!(receipt.covering_seal_generation, initial_generation + 1);
        assert_eq!(owner.m10_stream.pending_count(), 0);
        assert_eq!(owner.m10_stream.acknowledged_count(), 1);
        assert!(owner.stage7.recovery_ready());
        let Stage8bP1SemanticCompositionOwner { stage7, m10_stream } = *owner;
        drop(stage7);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(pending) = restart else {
            panic!("zero-intent S1 must require exact source ACK resolution after restart");
        };
        let callback_count = pending.stage5c_callback_count();
        let resolved =
            resolve_stage8b_p1_zero_intent_ack_with_local_m10(*pending, m10_stream).unwrap();
        assert_eq!(
            resolved.disposition(),
            Stage8bP1ZeroIntentAckDisposition::AlreadyAcknowledged
        );
        assert_eq!(resolved.stage5c_callback_count(), callback_count);
        let owner = resolved.into_ready_owner();
        assert!(owner.stage7.recovery_ready());
        assert_eq!(owner.m10_stream.pending_count(), 0);
        assert_eq!(owner.m10_stream.acknowledged_count(), 1);
        drop(owner);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn subprocess_kill_after_zero_intent_s1_recovers_exact_ack_only() {
        let phase = "after-zero-intent-s1-reread-before-m10-xack";
        let parent = temp_directory(phase);
        let (owner, key, fresh) = first_boot(&parent);
        let operational_identity_sha256 =
            owner.stage8b_p1_operational_identity_sha256().to_string();
        drop(owner);

        let marker = parent.join(format!("{phase}.marker"));
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--ignored")
            .arg("--exact")
            .arg("stage8b_p1_semantic::tests::p1_semantic_crash_frontier_child")
            .arg("--nocapture")
            .env("STAGE8B_P1_TEST_PARENT", &parent)
            .env("STAGE8B_P1_TEST_CRASH_PHASE", phase)
            .env("STAGE8B_P1_TEST_CRASH_MARKER", &marker)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_crash_barrier(&mut child, &marker);
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticZeroIntentAckPending(pending) = restart else {
            panic!("SIGKILL frontier must recover zero-intent ACK-pending authority");
        };
        assert!(!pending.recovery_ready());
        assert!(!pending.command_publication_allowed());
        assert!(!pending.paper_provider_invocation_allowed());
        assert!(!pending.hybrid_callback_allowed());
        assert_eq!(pending.evidence().intent_count, 0);
        let generation = pending.recovery_seal_generation();
        let seal_sha256 = pending.recovery_seal_commitment_sha256().to_string();
        let checkpoint_sha256 = pending.stage6_checkpoint_sha256().to_string();
        let callback_count = pending.stage5c_callback_count();

        // The local stream models the exact Redis PEL that survives the killed
        // process; no semantic callback is used to reconstruct it.
        let mut stream = local_stream_with_one_m10(operational_identity_sha256, 2_600);
        let _ = stream.read_next_pending().unwrap();
        let resolved = resolve_stage8b_p1_zero_intent_ack_with_local_m10(*pending, stream).unwrap();
        assert_eq!(
            resolved.disposition(),
            Stage8bP1ZeroIntentAckDisposition::AcknowledgedPending
        );
        assert_eq!(resolved.evidence().intent_count, 0);
        assert!(resolved.evidence().strategy_request_id.is_none());
        assert!(resolved.evidence().canonical_command_sha256.is_none());
        assert!(resolved.evidence().request_accepted_record_id.is_none());
        assert_eq!(resolved.recovery_seal_generation(), generation);
        assert_eq!(resolved.recovery_seal_commitment_sha256(), seal_sha256);
        assert_eq!(resolved.stage6_checkpoint_sha256(), checkpoint_sha256);
        assert_eq!(resolved.stage5c_callback_count(), callback_count);
        let owner = resolved.into_ready_owner();
        assert!(owner.stage7.recovery_ready());
        assert_eq!(owner.m10_stream.pending_count(), 0);
        assert_eq!(owner.m10_stream.acknowledged_count(), 1);
        drop(owner);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn zero_intent_ack_recovery_rejects_changed_m10() {
        let parent = temp_directory("zero-ack-changed");
        let (pending, _key, _fresh, original_stream) = zero_intent_restart_pending(&parent);
        let operational_identity_sha256 = pending.operational_identity_sha256().to_string();
        drop(original_stream);
        let mut changed_stream = local_stream_with_one_m10(operational_identity_sha256, 2_599);
        let _ = changed_stream.read_next_pending().unwrap();
        assert!(matches!(
            resolve_stage8b_p1_zero_intent_ack_with_local_m10(*pending, changed_stream),
            Err(Stage8bP1SemanticCompositionError::LocalM10(
                Stage8bP1LocalM10Error::DeliveryAuthorityMismatch
            ))
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn zero_intent_ack_recovery_rejects_missing_m10() {
        let parent = temp_directory("zero-ack-missing");
        let (pending, _key, _fresh, original_stream) = zero_intent_restart_pending(&parent);
        drop(original_stream);
        let mut missing_stream =
            Stage8bP1LocalM10Stream::new(STAGE8B_P1_LOCAL_M10_MIN_RETENTION).unwrap();
        missing_stream.create_canonical_group_mkstream();
        assert!(matches!(
            resolve_stage8b_p1_zero_intent_ack_with_local_m10(*pending, missing_stream),
            Err(Stage8bP1SemanticCompositionError::LocalM10(
                Stage8bP1LocalM10Error::ExactPendingEntryMissing
            ))
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn zero_intent_ack_recovery_rejects_same_id_collision_stickily() {
        let parent = temp_directory("zero-ack-collision");
        let (pending, _key, _fresh, mut stream) = zero_intent_restart_pending(&parent);
        let operational_identity_sha256 = pending.operational_identity_sha256().to_string();
        let changed_bytes = semantic_m10_bytes(operational_identity_sha256.clone(), 2_599);
        let changed =
            parse_stage8b_p1_canonical_m10(&changed_bytes, &operational_identity_sha256).unwrap();
        assert_eq!(
            stream.publish_exact(changed).unwrap_err(),
            Stage8bP1LocalM10Error::TerminalCollision
        );
        assert!(matches!(
            resolve_stage8b_p1_zero_intent_ack_with_local_m10(*pending, stream),
            Err(Stage8bP1SemanticCompositionError::LocalM10(
                Stage8bP1LocalM10Error::TerminalCollision
            ))
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn one_intent_commits_s1_but_retains_m10_and_exposes_no_publication() {
        let parent = temp_directory("one-intent");
        let (owner, key, fresh) = first_boot(&parent);
        let initial_generation = owner.committed_seal().unwrap().seal_generation();
        let stream = local_stream_with_one_m10(
            owner.stage8b_p1_operational_identity_sha256().to_string(),
            2_650,
        );
        let result = Stage8bP1SemanticCompositionOwner::new(owner, stream)
            .process_next(&key)
            .unwrap();
        let Stage8bP1LocalSemanticOutcome::Prepublication(pending) = result else {
            panic!("breakout M10 must stop at prepublication");
        };
        assert_eq!(pending.evidence().intent_count, 1);
        assert_eq!(
            pending.durable.recovery_seal_generation(),
            initial_generation + 1
        );
        assert_eq!(pending.pending_count(), 1);
        assert_eq!(pending.m10_stream.acknowledged_count(), 0);
        assert!(!pending.command_publication_allowed());
        assert!(!pending.m10_xack_allowed());
        drop(pending);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticPrepublicationReady(restarted) = restart else {
            panic!("covering S1 must restore only opaque prepublication authority");
        };
        assert!(!restarted.command_publication_allowed());
        assert!(!restarted.paper_provider_invocation_allowed());
        assert!(!restarted.m10_xack_allowed());
        drop(restarted);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn journal_ahead_request_accepted_reconstructs_only_from_exact_pending_m10() {
        let parent = temp_directory("journal-ahead");
        let (owner, key, fresh) = first_boot(&parent);
        let operational_identity_sha256 =
            owner.stage8b_p1_operational_identity_sha256().to_string();
        let mut stream = local_stream_with_one_m10(operational_identity_sha256.clone(), 2_650);
        let delivery = stream.read_next_pending().unwrap();
        let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
        let accepted_bar = delivery
            .parse_exact(&operational_identity_sha256)
            .unwrap()
            .into_stage5c_semantic_bar()
            .unwrap();
        let before_crash = crate::recovery::stage8b_p1_test_stop_after_request_accepted(
            owner,
            accepted_bar,
            binding,
            &key,
        )
        .unwrap();
        assert_eq!(stream.pending_count(), 1);
        assert_eq!(stream.acknowledged_count(), 0);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh.clone(),
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticPrepublicationPending(pending) = restart else {
            panic!("uncovered exact RequestAccepted must be typed journal-ahead pending");
        };
        assert!(!pending.recovery_ready());
        assert!(!pending.command_publication_allowed());
        assert!(!pending.paper_provider_invocation_allowed());
        assert!(!pending.m10_xack_allowed());
        assert_eq!(
            pending.request_accepted_record_id(),
            before_crash.request_accepted_record_id.as_deref().unwrap()
        );

        let recovered =
            resume_stage8b_p1_journal_ahead_with_local_m10(*pending, stream, delivery, &key)
                .unwrap();
        assert_eq!(recovered.pending_count(), 1);
        assert_eq!(recovered.m10_stream.acknowledged_count(), 0);
        assert_eq!(
            recovered.evidence().semantic_batch_id_sha256,
            before_crash.semantic_batch_id_sha256
        );
        assert_eq!(
            recovered.evidence().strategy_request_id,
            before_crash.strategy_request_id
        );
        assert_eq!(
            recovered.evidence().canonical_command_sha256,
            before_crash.canonical_command_sha256
        );
        assert_eq!(
            recovered.evidence().request_accepted_record_id,
            before_crash.request_accepted_record_id
        );
        assert!(!recovered.command_publication_allowed());
        assert!(!recovered.m10_xack_allowed());
        drop(recovered);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        assert!(matches!(
            restart,
            Stage7bRestartOutcome::P1SemanticPrepublicationReady(_)
        ));
        drop(restart);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn journal_ahead_rejects_changed_m10_before_covering_s1() {
        let parent = temp_directory("journal-ahead-changed-m10");
        let (owner, key, fresh) = first_boot(&parent);
        let operational_identity_sha256 =
            owner.stage8b_p1_operational_identity_sha256().to_string();
        let mut original_stream =
            local_stream_with_one_m10(operational_identity_sha256.clone(), 2_650);
        let original_delivery = original_stream.read_next_pending().unwrap();
        let binding =
            binding_from_delivery(&original_delivery, operational_identity_sha256.clone());
        let accepted_bar = original_delivery
            .parse_exact(&operational_identity_sha256)
            .unwrap()
            .into_stage5c_semantic_bar()
            .unwrap();
        crate::recovery::stage8b_p1_test_stop_after_request_accepted(
            owner,
            accepted_bar,
            binding,
            &key,
        )
        .unwrap();
        drop(original_delivery);
        drop(original_stream);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticPrepublicationPending(pending) = restart else {
            panic!("journal-ahead restart must remain pending");
        };
        let mut changed_stream = local_stream_with_one_m10(operational_identity_sha256, 2_649);
        let changed_delivery = changed_stream.read_next_pending().unwrap();
        assert!(matches!(
            resume_stage8b_p1_journal_ahead_with_local_m10(
                *pending,
                changed_stream,
                changed_delivery,
                &key,
            ),
            Err(Stage8bP1SemanticCompositionError::Durable(
                Stage7bRecoveryError::SealInvalid
            ))
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn journal_ahead_rejects_already_acked_m10_before_semantic_replay() {
        let parent = temp_directory("journal-ahead-acked-m10");
        let (owner, key, fresh) = first_boot(&parent);
        let operational_identity_sha256 =
            owner.stage8b_p1_operational_identity_sha256().to_string();
        let mut stream = local_stream_with_one_m10(operational_identity_sha256.clone(), 2_650);
        let delivery = stream.read_next_pending().unwrap();
        let retained = stream
            .reclaim_exact_pending(
                delivery.redis_id(),
                delivery.semantic_id_sha256(),
                delivery.payload_sha256(),
            )
            .unwrap();
        let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
        let accepted_bar = delivery
            .parse_exact(&operational_identity_sha256)
            .unwrap()
            .into_stage5c_semantic_bar()
            .unwrap();
        crate::recovery::stage8b_p1_test_stop_after_request_accepted(
            owner,
            accepted_bar,
            binding,
            &key,
        )
        .unwrap();
        stream.acknowledge_after_durable_commit(delivery).unwrap();

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticPrepublicationPending(pending) = restart else {
            panic!("journal-ahead restart must remain pending");
        };
        assert!(matches!(
            resume_stage8b_p1_journal_ahead_with_local_m10(*pending, stream, retained, &key),
            Err(Stage8bP1SemanticCompositionError::LocalM10(
                Stage8bP1LocalM10Error::ExactPendingEntryMissing
            ))
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn journal_ahead_rejects_missing_m10_before_semantic_replay() {
        let parent = temp_directory("journal-ahead-missing-m10");
        let (owner, key, fresh) = first_boot(&parent);
        let operational_identity_sha256 =
            owner.stage8b_p1_operational_identity_sha256().to_string();
        let mut original_stream =
            local_stream_with_one_m10(operational_identity_sha256.clone(), 2_650);
        let delivery = original_stream.read_next_pending().unwrap();
        let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
        let accepted_bar = delivery
            .parse_exact(&operational_identity_sha256)
            .unwrap()
            .into_stage5c_semantic_bar()
            .unwrap();
        crate::recovery::stage8b_p1_test_stop_after_request_accepted(
            owner,
            accepted_bar,
            binding,
            &key,
        )
        .unwrap();
        drop(original_stream);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::P1SemanticPrepublicationPending(pending) = restart else {
            panic!("journal-ahead restart must remain pending");
        };
        let mut missing_stream =
            Stage8bP1LocalM10Stream::new(STAGE8B_P1_LOCAL_M10_MIN_RETENTION).unwrap();
        missing_stream.create_canonical_group_mkstream();
        assert!(matches!(
            resume_stage8b_p1_journal_ahead_with_local_m10(
                *pending,
                missing_stream,
                delivery,
                &key,
            ),
            Err(Stage8bP1SemanticCompositionError::LocalM10(
                Stage8bP1LocalM10Error::ExactPendingEntryMissing
            ))
        ));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn journal_ahead_exception_rejects_dispatch_attempt_suffix() {
        let parent = temp_directory("journal-ahead-dispatch-suffix");
        let (owner, key, fresh) = first_boot(&parent);
        let operational_identity_sha256 =
            owner.stage8b_p1_operational_identity_sha256().to_string();
        let mut stream = local_stream_with_one_m10(operational_identity_sha256.clone(), 2_650);
        let delivery = stream.read_next_pending().unwrap();
        let binding = binding_from_delivery(&delivery, operational_identity_sha256.clone());
        let accepted_bar = delivery
            .parse_exact(&operational_identity_sha256)
            .unwrap()
            .into_stage5c_semantic_bar()
            .unwrap();
        crate::recovery::stage8b_p1_test_stop_after_dispatch_attempt(
            owner,
            accepted_bar,
            binding,
            &key,
        )
        .unwrap();
        drop(stream);

        let restart = restart_stage8b_p1(
            validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                fresh.stage5c_config_fingerprint(),
            ))
            .unwrap(),
            &key,
            fresh,
        )
        .unwrap();
        let Stage7bRestartOutcome::Blocked(blocked) = restart else {
            panic!("RequestAccepted plus DispatchAttempt must not enter the P1 exception");
        };
        assert_eq!(
            blocked.reason(),
            Stage7bRecoveryBlockReason::AuthenticatedRestartRejected
        );
        assert!(!blocked.recovery_ready());
        assert!(!blocked.paper_provider_invocation_allowed());
        assert!(!blocked.xack_allowed());
        fs::remove_dir_all(parent).unwrap();
    }
}
