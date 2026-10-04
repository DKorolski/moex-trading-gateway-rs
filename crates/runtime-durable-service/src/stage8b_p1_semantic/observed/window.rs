//! Bounded producer-to-S05 input, not a provenance oracle. The caller supplies
//! two independently admitted receipts and their exact published canonical
//! bytes. Neither a Redis digest nor discovery of a receipt file constructs it.

use super::*;

#[derive(Clone)]
pub struct Stage8bP1ObservedPublishedWindow {
    pair: Stage8bP1ObservedM10WindowPair,
    entries: [(String, Vec<u8>); 2],
}

impl Stage8bP1ObservedPublishedWindow {
    /// Pure validation of explicit inputs; does NOT prove publication or
    /// authenticate the provider. Fixed producers check Published lineage;
    /// S05 independently rereads both exact entries before admitting this input.
    pub fn new(
        predecessor: Stage8bP1ObservedM10Binding,
        predecessor_bytes: Vec<u8>,
        successor: Stage8bP1ObservedM10Binding,
        successor_bytes: Vec<u8>,
    ) -> Result<Self, Stage8bP1CanonicalM10Error> {
        let pair = Stage8bP1ObservedM10WindowPair::new(predecessor, successor)?;
        let identity = pair.operational_identity_sha256();
        let old = pair.predecessor.parse_exact(&predecessor_bytes, identity)?;
        let next = pair.successor.parse_exact(&successor_bytes, identity)?;
        if old.close_ts_utc_ms() != pair.predecessor.source().end().timestamp_millis()
            || next.close_ts_utc_ms() != pair.successor.source().end().timestamp_millis()
            || next.open_ts_utc_ms() < old.close_ts_utc_ms()
            || pair.successor.source().start() > pair.predecessor.source().end()
        {
            return Err(Stage8bP1CanonicalM10Error::InvalidChronology);
        }
        Ok(Self {
            pair,
            entries: [
                (old.redis_id().into(), predecessor_bytes),
                (next.redis_id().into(), successor_bytes),
            ],
        })
    }

    pub(crate) fn entries(&self) -> &[(String, Vec<u8>); 2] {
        &self.entries
    }

    fn validate_initial(
        &self,
        initial: &Stage8bP1ObservedM10Binding,
    ) -> Result<(), Stage8bP1CanonicalM10Error> {
        if initial.operational_identity_sha256() != self.pair.operational_identity_sha256() {
            return Err(Stage8bP1CanonicalM10Error::IdentityMismatch);
        }
        if initial.source().sha256() != self.pair.predecessor.source().sha256() {
            if self.pair.predecessor.source().start() > initial.source().end() {
                return Err(Stage8bP1CanonicalM10Error::InvalidChronology);
            }
            Stage8bP1ObservedM10WindowPair::new(initial.clone(), self.pair.predecessor.clone())?;
        }
        Ok(())
    }

    fn parse_exact(
        &self,
        bytes: &[u8],
        identity: &str,
    ) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
        if !self.entries.iter().any(|(_, exact)| exact == bytes) {
            return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
        }
        self.pair.parse_exact(bytes, identity)
    }
}

impl Stage8bP1ObservedRecoveryContext {
    pub(crate) fn validate_fresh_window(
        &self,
        input: &Stage8bP1ObservedPublishedWindow,
    ) -> Result<(), Stage8bP1CanonicalM10Error> {
        if self.fresh_window.is_some() {
            return Err(Stage8bP1CanonicalM10Error::DigestMismatch);
        }
        input.validate_initial(&self.initial)
    }

    pub(crate) fn admit_fresh_window(
        &mut self,
        input: Stage8bP1ObservedPublishedWindow,
    ) -> Result<(), Stage8bP1CanonicalM10Error> {
        self.validate_fresh_window(&input)?;
        self.fresh_window = Some(input);
        Ok(())
    }

    pub(super) fn parse_fresh_exact(
        &self,
        bytes: &[u8],
        identity: &str,
    ) -> Result<Stage8bP1ValidatedCanonicalM10, Stage8bP1CanonicalM10Error> {
        // Keep all previously admitted buckets bound to the initial receipt.
        // A new window admits only its two exact canonical entries, not every
        // possible candle derivable from either overlapping source range.
        if bytes.len() > 16 * 1024 {
            return Err(Stage8bP1CanonicalM10Error::Decode);
        }
        let wire: EnvelopeV2 =
            serde_json::from_slice(bytes).map_err(|_| Stage8bP1CanonicalM10Error::Decode)?;
        if wire.payload.close_ts_utc_ms <= self.initial.source().end().timestamp_millis() {
            self.initial.parse_exact(bytes, identity)
        } else {
            self.fresh_window
                .as_ref()
                .ok_or(Stage8bP1CanonicalM10Error::DigestMismatch)?
                .parse_exact(bytes, identity)
        }
    }
}
