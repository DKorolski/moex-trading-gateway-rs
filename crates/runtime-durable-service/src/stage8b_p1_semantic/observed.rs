//! Explicit canonical V2, admitted ONLY against an independently bound full
//! source receipt. The legacy V1 builder/parser remain strict and do not route
//! here. Retained receipts use files beside existing state, not a new registry,
//! transport or lifecycle framework.

use super::*;
use broker_core::observed_m1::{ObservedM1Receipt, OBSERVED_M1_POLICY_V1};
use chrono::DateTime;
use std::sync::Arc;

pub const OBSERVED_CANONICAL_M10_DOMAIN: &str = "moex.stage8b.p1.canonical-final-m10.v2";
const M1_DOMAIN: &[u8] = b"moex.observed-source-m1.v1";
mod retained;
mod window;
pub use window::Stage8bP1ObservedPublishedWindow;

/// Immutable readmission context for ONE independently admitted closed source.
/// This does not authenticate a raw response or authorize a deployment. The
/// expected hash must come from retained source/seal evidence, never a Redis
/// message. No mutable receipt registry or implicit legacy fallback is exposed.
#[derive(Clone)]
pub struct Stage8bP1ObservedM10Binding {
    operational_identity_sha256: String,
    source: Arc<ObservedM1Receipt>,
}

impl Stage8bP1ObservedM10Binding {
    pub fn new(
        operational_identity_sha256: &str,
        source: ObservedM1Receipt,
        expected_source_receipt_sha256: &str,
    ) -> Result<Self, Stage8bP1CanonicalM10Error> {
        if !is_sha256(operational_identity_sha256) || source.instrument() != &p1_instrument() {
            return Err(Stage8bP1CanonicalM10Error::IdentityMismatch);
        }
        if !is_sha256(expected_source_receipt_sha256)
            || source.sha256() != expected_source_receipt_sha256
        {
            return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
        }
        Ok(Self {
            operational_identity_sha256: operational_identity_sha256.into(),
            source: Arc::new(source),
        })
    }

    pub fn source(&self) -> &ObservedM1Receipt {
        &self.source
    }

    pub fn operational_identity_sha256(&self) -> &str {
        &self.operational_identity_sha256
    }

    pub fn parse_exact(
        &self,
        bytes: &[u8],
        expected_operational_identity_sha256: &str,
    ) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
        if self.operational_identity_sha256 != expected_operational_identity_sha256 {
            return Err(Stage8bP1CanonicalM10Error::IdentityMismatch);
        }
        parse_bound(bytes, self)
    }
}

/// Exactly two independently admitted sources for one pending/successor
/// continuation. No mutation, lookup-by-Redis-hash, rotation or registry.
/// The previous closed-range end is a fixed cutover: even an overlapping new
/// snapshot cannot be used to rebase a previously admitted canonical candle.
#[derive(Clone)]
pub struct Stage8bP1ObservedM10WindowPair {
    predecessor: Stage8bP1ObservedM10Binding,
    successor: Stage8bP1ObservedM10Binding,
}

impl Stage8bP1ObservedM10WindowPair {
    pub fn new(
        predecessor: Stage8bP1ObservedM10Binding,
        successor: Stage8bP1ObservedM10Binding,
    ) -> Result<Self, Stage8bP1CanonicalM10Error> {
        if predecessor.operational_identity_sha256 != successor.operational_identity_sha256 {
            return Err(Stage8bP1CanonicalM10Error::IdentityMismatch);
        }
        let old = predecessor.source();
        let next = successor.source();
        if next.start() < old.start()
            || next.end() <= old.end()
            || next.received_at() < old.received_at()
            || old.end().timestamp_millis().rem_euclid(600_000) != 0
            || next.end().timestamp_millis().rem_euclid(600_000) != 0
        {
            return Err(Stage8bP1CanonicalM10Error::InvalidChronology);
        }
        // A changed minute set in the common range is a reconciliation event,
        // never evidence to overwrite an already admitted bucket.
        let start = next.start();
        let end = old.end();
        let previous = old
            .bars()
            .iter()
            .filter(|b| b.open_ts >= start && b.open_ts < end);
        let current = next
            .bars()
            .iter()
            .filter(|b| b.open_ts >= start && b.open_ts < end);
        if !previous.eq(current) {
            return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
        }
        Ok(Self {
            predecessor,
            successor,
        })
    }

    pub fn operational_identity_sha256(&self) -> &str {
        self.predecessor.operational_identity_sha256()
    }

    pub fn parse_exact(
        &self,
        bytes: &[u8],
        expected_identity: &str,
    ) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
        if bytes.len() > 16 * 1024 {
            return Err(Stage8bP1CanonicalM10Error::Decode);
        }
        let wire: EnvelopeV2 =
            serde_json::from_slice(bytes).map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
        let binding =
            if wire.payload.close_ts_utc_ms <= self.predecessor.source().end().timestamp_millis() {
                &self.predecessor
            } else {
                &self.successor
            };
        // Only choosing a fixed slot above; the selected receipt still checks
        // exact bytes, policy, source digest, minute vector and all OHLCV.
        binding.parse_exact(bytes, expected_identity)
    }
}

pub(super) enum ObservedM10Context {
    Single(Stage8bP1ObservedM10Binding),
    Pair(Stage8bP1ObservedM10WindowPair),
    Recovery(Stage8bP1ObservedRecoveryContext),
}

/// Initial source is authenticated by the configured bundle and current seal.
/// Disk resolution is available ONLY with independently sealed exact M10 IDs.
/// This is not a fresh-source registry or permission to accept a Redis hash.
pub(crate) struct Stage8bP1ObservedRecoveryContext {
    initial: Stage8bP1ObservedM10Binding,
    retained_parent: std::path::PathBuf,
    fresh_window: Option<Stage8bP1ObservedPublishedWindow>,
}

impl Stage8bP1ObservedRecoveryContext {
    pub(crate) fn new(
        initial: Stage8bP1ObservedM10Binding,
        config: &crate::Stage8bP1ValidatedBootstrapConfig,
    ) -> Result<Self, Stage8bP1CanonicalM10Error> {
        if !config.observed_source_policy()
            || initial.operational_identity_sha256() != config.operational_identity_sha256()
        {
            return Err(Stage8bP1CanonicalM10Error::IdentityMismatch);
        }
        Ok(Self {
            initial,
            retained_parent: config.durable_parent().to_path_buf(),
            fresh_window: None,
        })
    }

    pub(crate) fn operational_identity_sha256(&self) -> &str {
        self.initial.operational_identity_sha256()
    }
}

impl ObservedM10Context {
    pub(super) fn operational_identity_sha256(&self) -> &str {
        match self {
            Self::Single(binding) => binding.operational_identity_sha256(),
            Self::Pair(pair) => pair.operational_identity_sha256(),
            Self::Recovery(context) => context.operational_identity_sha256(),
        }
    }

    pub(super) fn parse_exact(
        &self,
        bytes: &[u8],
        identity: &str,
    ) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
        match self {
            Self::Single(binding) => binding.parse_exact(bytes, identity),
            Self::Pair(pair) => pair.parse_exact(bytes, identity),
            Self::Recovery(context) => context.parse_fresh_exact(bytes, identity),
        }
    }

    pub(super) fn parse_sealed_exact(
        &self,
        bytes: &[u8],
        identity: &str,
        redis_id: &str,
        semantic_sha256: &str,
        payload_sha256: &str,
    ) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
        let validated = match self {
            Self::Recovery(context) => {
                if identity != context.operational_identity_sha256() {
                    return Err(Stage8bP1CanonicalM10Error::IdentityMismatch);
                }
                match context.initial.parse_exact(bytes, identity) {
                    Ok(value) => value,
                    Err(_) => Stage8bP1ObservedM10Binding::restore_for_exact_m10(
                        &context.retained_parent,
                        bytes,
                        identity,
                        redis_id,
                        semantic_sha256,
                        payload_sha256,
                    )?
                    .parse_exact(bytes, identity)?,
                }
            }
            _ => self.parse_exact(bytes, identity)?,
        };
        if validated.redis_id() != redis_id
            || validated.semantic_id_sha256() != semantic_sha256
            || validated.payload_sha256() != payload_sha256
        {
            return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
        }
        Ok(validated)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PayloadV2 {
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
    policy: String,
    source_receipt_sha256: String,
    source_snapshot_sha256: String,
    source_range_start_utc_ms: i64,
    source_range_end_utc_ms: i64,
    actual_minute_bitmap: u16,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeV2 {
    schema_version: u16,
    message_type: String,
    identity_domain: String,
    redis_id: String,
    m10_semantic_id_sha256: String,
    m10_payload_sha256: String,
    payload: PayloadV2,
}

/// Untrusted probe only. Callers MUST compare it to independently retained
/// RequestAccepted source evidence before resolving any source receipt. The
/// full canonical parser remains responsible for the payload, hashes and
/// admitted source bytes; copied envelope hashes cannot authorize a claim.
pub(super) fn journal_source_probe(
    bytes: &[u8],
) -> Result<Stage5gP1SemanticBindingInput, Stage8bP1CanonicalM10Error> {
    if bytes.len() > 16 * 1024 {
        return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
    }
    let envelope: EnvelopeV2 =
        serde_json::from_slice(bytes).map_err(|_| Stage8bP1CanonicalM10Error::DigestMismatch)?;
    Ok(Stage5gP1SemanticBindingInput {
        operational_identity_sha256: envelope.payload.operational_identity_sha256,
        m10_redis_id: envelope.redis_id,
        m10_semantic_id_sha256: envelope.m10_semantic_id_sha256,
        m10_payload_sha256: envelope.m10_payload_sha256,
    })
}

/// Computes rather than accepts OHLCV/source rows. Source bytes are retained
/// once per receipt; a candle only carries references and minute identities.
pub fn build_stage8b_p1_observed_canonical_m10(
    operational_identity_sha256: &str,
    open_ts_utc_ms: i64,
    source: &ObservedM1Receipt,
) -> Result<Vec<u8>, Stage8bP1CanonicalM10Error> {
    if !is_sha256(operational_identity_sha256) || source.instrument() != &p1_instrument() {
        return Err(Stage8bP1CanonicalM10Error::IdentityMismatch);
    }
    let open = DateTime::from_timestamp_millis(open_ts_utc_ms)
        .ok_or(Stage8bP1CanonicalM10Error::InvalidChronology)?;
    let bucket = source
        .bucket(open)
        .map_err(|_| Stage8bP1CanonicalM10Error::InvalidChronology)?;
    let tick = Decimal::new(5, 1);
    if bucket.actual_m1.iter().any(|b| {
        [b.open, b.high, b.low, b.close]
            .iter()
            .any(|p| *p % tick != Decimal::ZERO)
    }) {
        return Err(Stage8bP1CanonicalM10Error::InvalidOhlcv);
    }
    let source_m1 = bucket
        .actual_m1
        .iter()
        .map(|bar| {
            let bytes = serde_json::to_vec(bar).map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
            Ok(Stage8bP1CanonicalM10SourceM1 {
                redis_id: format!("{}-0", bar.close_ts.timestamp_millis()),
                semantic_id_sha256: domain_sha256(M1_DOMAIN, &bytes),
                payload_sha256: sha256_hex(&bytes),
                open_ts_utc_ms: bar.open_ts.timestamp_millis(),
                close_ts_utc_ms: bar.close_ts.timestamp_millis(),
            })
        })
        .collect::<Result<Vec<_>, Stage8bP1CanonicalM10Error>>()?;
    let bar = bucket.bar;
    let payload = PayloadV2 {
        schema_version: 2,
        identity_domain: OBSERVED_CANONICAL_M10_DOMAIN.into(),
        operational_identity_sha256: operational_identity_sha256.into(),
        instrument_map_sha256: stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
        broker_id: STAGE8B_P1_BROKER_ID.into(),
        internal_symbol: STAGE8B_P1_INTERNAL_SYMBOL.into(),
        venue_symbol: STAGE8B_P1_VENUE_SYMBOL.into(),
        exchange: STAGE8B_P1_EXCHANGE.into(),
        market: STAGE8B_P1_MARKET.into(),
        timeframe_sec: 600,
        is_final: true,
        open_ts_utc_ms,
        close_ts_utc_ms: bar.close_ts.timestamp_millis(),
        open: bar.open.normalize().to_string(),
        high: bar.high.normalize().to_string(),
        low: bar.low.normalize().to_string(),
        close: bar.close.normalize().to_string(),
        volume: bar.volume.normalize().to_string(),
        source_m1,
        policy: OBSERVED_M1_POLICY_V1.into(),
        source_receipt_sha256: source.sha256().into(),
        source_snapshot_sha256: source.source_snapshot_sha256().into(),
        source_range_start_utc_ms: source.start().timestamp_millis(),
        source_range_end_utc_ms: source.end().timestamp_millis(),
        actual_minute_bitmap: bucket.minute_bitmap,
    };
    let payload_bytes =
        serde_json::to_vec(&payload).map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
    let envelope = EnvelopeV2 {
        schema_version: 2,
        message_type: STAGE8B_P1_CANONICAL_M10_MESSAGE_TYPE.into(),
        identity_domain: OBSERVED_CANONICAL_M10_DOMAIN.into(),
        redis_id: format!("{}-0", payload.close_ts_utc_ms),
        m10_semantic_id_sha256: domain_sha256(
            OBSERVED_CANONICAL_M10_DOMAIN.as_bytes(),
            &payload_bytes,
        ),
        m10_payload_sha256: sha256_hex(&payload_bytes),
        payload,
    };
    serde_json::to_vec(&envelope).map_err(|_| Stage8bP1CanonicalM10Error::Decode)
}

/// The expected receipt hash MUST originate in the retained source binding,
/// independently of `bytes`. Rehashing a shortened candle/receipt does not
/// authorize the change. No context-free V2 acceptance is provided.
pub fn parse_stage8b_p1_observed_canonical_m10(
    bytes: &[u8],
    expected_operational_identity_sha256: &str,
    source: &ObservedM1Receipt,
    expected_source_receipt_sha256: &str,
) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
    if bytes.len() > 16 * 1024 {
        return Err(Stage8bP1CanonicalM10Error::Decode);
    }
    if source.sha256() != expected_source_receipt_sha256
        || !is_sha256(expected_source_receipt_sha256)
    {
        return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
    }
    let retained =
        ObservedM1Receipt::restore(source.retained_bytes(), expected_source_receipt_sha256)
            .map_err(|_| Stage8bP1CanonicalM10Error::DigestMismatch)?;
    Stage8bP1ObservedM10Binding::new(
        expected_operational_identity_sha256,
        retained,
        expected_source_receipt_sha256,
    )?
    .parse_exact(bytes, expected_operational_identity_sha256)
}

fn parse_bound(
    bytes: &[u8],
    binding: &Stage8bP1ObservedM10Binding,
) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
    if bytes.len() > 16 * 1024 {
        return Err(Stage8bP1CanonicalM10Error::Decode);
    }
    let envelope: EnvelopeV2 =
        serde_json::from_slice(bytes).map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
    // Exact regeneration validates all fields, source identities, count/bitmap,
    // aggregate and hashes against the FULL independent source, not its claimed
    // subset. It also rejects noncanonical/unknown fields and forged policy.
    let expected = build_stage8b_p1_observed_canonical_m10(
        binding.operational_identity_sha256(),
        envelope.payload.open_ts_utc_ms,
        binding.source(),
    )?;
    if expected != bytes {
        return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
    }
    let p = envelope.payload;
    Ok(Stage8bP1ValidatedCanonicalM10 {
        source_policy: CanonicalM10SourcePolicy::ObservedV2,
        observed_binding: Some(binding.clone()),
        canonical_bytes: expected,
        // Private common-field projection only, never emitted on the V1 wire.
        envelope: Stage8bP1CanonicalM10EnvelopeV1 {
            schema_version: 2,
            message_type: envelope.message_type,
            identity_domain: envelope.identity_domain,
            redis_id: envelope.redis_id,
            m10_semantic_id_sha256: envelope.m10_semantic_id_sha256,
            m10_payload_sha256: envelope.m10_payload_sha256,
            payload: Stage8bP1CanonicalM10PayloadV1 {
                schema_version: 2,
                identity_domain: p.identity_domain,
                operational_identity_sha256: p.operational_identity_sha256,
                instrument_map_sha256: p.instrument_map_sha256,
                broker_id: p.broker_id,
                internal_symbol: p.internal_symbol,
                venue_symbol: p.venue_symbol,
                exchange: p.exchange,
                market: p.market,
                timeframe_sec: p.timeframe_sec,
                is_final: p.is_final,
                open_ts_utc_ms: p.open_ts_utc_ms,
                close_ts_utc_ms: p.close_ts_utc_ms,
                open: p.open,
                high: p.high,
                low: p.low,
                close: p.close,
                volume: p.volume,
                source_m1: p.source_m1,
            },
        },
    })
}

#[cfg(test)]
mod tests;
