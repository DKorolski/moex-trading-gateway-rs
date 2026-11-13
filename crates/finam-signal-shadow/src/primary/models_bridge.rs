//! Opt-in source integration, NOT a session runner or deployment permission.
//! Accepts only the native single-use capability. Public JSON receipts remain
//! unattested inside the numerical journal. Context still has DECLARED replay
//! clocks: a source-qualified M10 does not promote the model session to LIVE.
use super::{AdmittedCanonicalM10, DeliveryPolicy, PrimaryProcessor, WsPrimaryConfig};
use anyhow::{anyhow, ensure, Context, Result};
use broker_core::canonical_m1::aggregate_m1_window;
use finam_shadow_models::local::{
    contract::{
        Binding, Candle, Construction, ConstructionKind, Manifest, MappingBasis, Ohlcv, Purpose,
    },
    Commit, Journal,
};
use serde_json::{json, Value};
use std::path::Path;

/// Exact public mapping artifact accepted at 6a508df3; not an API body hash.
pub const MAPPING_SHA256: &str = "9e06d4ff4c66a9f88534bd4f46404b12588a1c716f1c64373a97174401376b0c";
const SCHEMA: &str = "finam.shadow.imoexf-admitted-projection.v1";

fn hash(value: &impl serde::Serialize) -> Result<String> {
    Ok(crate::seed::digest(&serde_json::to_vec(value)?))
}
fn implementation() -> String {
    crate::seed::digest(
        &[
            include_bytes!("models_bridge.rs").as_slice(),
            include_bytes!("cache.rs").as_slice(),
            include_bytes!("policy.rs").as_slice(),
            include_bytes!("../../Cargo.toml").as_slice(),
            include_bytes!("../../../../Cargo.lock").as_slice(),
        ]
        .concat(),
    )
}

/// Only two IMOEXF arms receive data in this slice. The other four journal
/// arms retain their accepted local initialization, not invented live inputs.
pub struct ImoexfModelBridge {
    journal: Journal,
    binding: Binding,
    history_len: u64,
    cutoff_ms: i64,
    config_sha256: String,
    policy_sha256: String,
    failed: bool,
    #[cfg(test)]
    pub(super) after_durable_delay_ms: u64,
}
impl ImoexfModelBridge {
    pub fn create(path: &Path, manifest: Manifest, config: &WsPrimaryConfig) -> Result<Self> {
        Self::open_or_create(path, manifest, config, true)
    }
    pub fn open(path: &Path, manifest: Manifest, config: &WsPrimaryConfig) -> Result<Self> {
        Self::open_or_create(path, manifest, config, false)
    }
    fn open_or_create(
        path: &Path,
        manifest: Manifest,
        config: &WsPrimaryConfig,
        create: bool,
    ) -> Result<Self> {
        manifest.validate()?;
        ensure!(
            config.primary.delivery == DeliveryPolicy::ObservedBoRestTailRecoveryV6,
            "bridge requires explicit accepted V6 source policy"
        );
        let config_sha256 = config
            .fingerprint()
            .map_err(|e| anyhow!("source config: {e:?}"))?;
        let policy_sha256 = config
            .primary
            .fingerprint()
            .map_err(|e| anyhow!("source policy: {e:?}"))?;
        let seed = manifest
            .seeds
            .iter()
            .find(|s| s.binding.instrument == "IMOEXF")
            .context("IMOEXF seed missing")?;
        let binding = seed.binding.clone();
        ensure!(
            binding.provider_id == "IMOEXF@RTSX"
                && binding.basis == MappingBasis::DeclaredCatalogSnapshot
                && binding.catalog_sha256 == MAPPING_SHA256
                && binding.tick == "0.5"
                && binding.lot == 1
                && binding.construction == ConstructionKind::ExternalM10V1,
            "accepted IMOEXF mapping required"
        );
        let history_len = seed.history.len() as u64;
        let cutoff_ms = manifest.freeze_at_ms;
        let journal = if create {
            Journal::create(path, manifest)?
        } else {
            Journal::open(path, manifest)?
        };
        let bridge = Self {
            journal,
            binding,
            history_len,
            cutoff_ms,
            config_sha256,
            policy_sha256,
            failed: false,
            #[cfg(test)]
            after_durable_delay_ms: 0,
        };
        // Bind reopen to the SAME consumer/source contract, not just the
        // numerical manifest. No adoption of another writer's naked JSON.
        for row in bridge.journal.export()?["inputs"]
            .as_array()
            .context("inputs")?
        {
            let c: Candle = serde_json::from_value(row["candle"].clone())?;
            ensure!(
                c.instrument == "IMOEXF",
                "foreign producer input in bridge database"
            );
            let Construction::DeclaredCanonicalProjectionV1 { provenance, .. } = c.construction
            else {
                anyhow::bail!("missing typed-consumer provenance");
            };
            bridge.check_provenance_contract(&provenance)?;
        }
        Ok(bridge)
    }
    fn check_provenance_contract(&self, v: &Value) -> Result<()> {
        ensure!(
            v["schema"] == SCHEMA
                && v["bridge_implementation_sha256"] == implementation()
                && v["mapping_sha256"] == MAPPING_SHA256
                && v["canonical_receipt"]["config_sha256"] == self.config_sha256
                && v["canonical_receipt"]["policy_sha256"] == self.policy_sha256,
            "bridge/source contract changed"
        );
        Ok(())
    }
    /// SQL commit + exact readback precede source advancement. On ANY failure
    /// this consumer becomes unusable: inspect/export, then explicitly reopen.
    /// In particular a late source completion NEVER rolls back durable output.
    pub fn consume(
        &mut self,
        producer: &mut PrimaryProcessor,
        input: AdmittedCanonicalM10,
    ) -> Result<Commit> {
        ensure!(!self.failed, "bridge held: explicit reopen required");
        self.failed = true;
        let result = self.consume_inner(producer, input);
        if result.is_ok() {
            self.failed = false;
        }
        result
    }
    fn consume_inner(
        &mut self,
        producer: &mut PrimaryProcessor,
        input: AdmittedCanonicalM10,
    ) -> Result<Commit> {
        producer
            .validate_pending_admission(&input, crate::timing::now_ms())
            .map_err(|e| anyhow!("precommit admission: {e:?}"))?;
        let receipt = input.receipt();
        ensure!(
            receipt.config_sha256 == self.config_sha256
                && receipt.policy_sha256 == self.policy_sha256,
            "source configuration/policy mismatch"
        );
        let offset = receipt
            .label_ms
            .checked_sub(self.cutoff_ms)
            .context("label range")?;
        ensure!(
            (0..101 * 600_000).contains(&offset) && offset % 600_000 == 0,
            "source outside exact model session"
        );
        let sequence = self.history_len + (offset / 600_000) as u64 + 1;
        let bar = aggregate_m1_window(
            &input.model_bar().instrument,
            chrono::DateTime::from_timestamp_millis(receipt.label_ms).context("label")?,
            &receipt.actual_m1,
        )
        .map_err(|e| anyhow!("canonical aggregation: {e:?}"))?;
        ensure!(
            bar.provisional_bar.instrument.venue_symbol.as_deref() == Some("IMOEXF@RTSX")
                && bar.minute_bitmap == receipt.minute_bitmap
                && super::cache::runtime_bar(&bar.provisional_bar)
                    .map_err(|e| anyhow!("runtime projection: {e:?}"))?
                    == *input.model_bar(),
            "admitted bar/projection mismatch"
        );
        let b = bar.provisional_bar;
        let provenance = json!({
            "schema":SCHEMA, "bridge_implementation_sha256":implementation(),
            "mapping_sha256":MAPPING_SHA256,
            "canonical_receipt":receipt,
            "canonical_receipt_sha256":input.provenance_sha256().map_err(|e| anyhow!("{e:?}"))?,
            "rest_evidence":input.rest_evidence(),
            "clock_basis":"declared_numerical_availability_not_physical_durability",
            "model_evidence":"declared_clock_replay",
            "observed_clean_session":false, "order_emission_allowed":false
        });
        let candle = Candle {
            instrument: "IMOEXF".into(),
            source_sequence: sequence,
            provider_id: self.binding.provider_id.clone(),
            binding_sha256: self.binding.sha256()?,
            start_ms: receipt.label_ms,
            end_ms: receipt.close_ms,
            received_ms: input.received_at_ms(),
            // The old numerical DTO requires an availability clock in this
            // field. It is explicitly DECLARED, never an upstream fsync claim.
            durable_ms: receipt.canonical_ready_at_ms,
            source_processed_ms: receipt.canonical_ready_at_ms,
            purpose: Purpose::Model,
            data: Ohlcv {
                open: b.open.to_string(),
                high: b.high.to_string(),
                low: b.low.to_string(),
                close: b.close.to_string(),
                volume: b.volume.to_string(),
            },
            construction: Construction::DeclaredCanonicalProjectionV1 {
                provenance_sha256: hash(&provenance)?,
                provenance,
            },
        };
        let commit = self.journal.apply(candle.clone())?;
        #[cfg(test)]
        {
            crate::timing::advance_fixture_clock(self.after_durable_delay_ms);
            crash_after_durable();
        }
        let reread = self
            .journal
            .committed_input("IMOEXF", sequence)?
            .context("durable input missing after commit")?;
        let (revision, ids) = match &commit {
            Commit::Committed {
                revision,
                result_ids,
            }
            | Commit::AlreadyCommitted {
                revision,
                result_ids,
            } => (*revision, result_ids),
        };
        ensure!(
            reread.candle == candle && reread.revision == revision && &reread.result_ids == ids,
            "durable readback differs: source NOT advanced"
        );
        producer
            .committed(input, crate::timing::now_ms())
            .map_err(|e| {
                anyhow!(
                    "DURABLE_OUTPUT_SOURCE_NOT_ADVANCED: {e:?}; reopen/inspect, no replay as new"
                )
            })?;
        Ok(commit)
    }
    /// Historical evidence only, including after a held completion.
    pub fn export(&self) -> Result<Value> {
        self.journal.export()
    }
}

#[cfg(test)]
fn crash_after_durable() {
    if let Ok(path) = std::env::var("FINAM_BRIDGE_TEST_DURABLE_MARKER") {
        use std::io::Write;
        let marker = Path::new(&path);
        let pending = marker.with_extension("writing");
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&pending)
            .unwrap();
        file.write_all(b"durable_before_source_completion").unwrap();
        file.sync_all().unwrap();
        // Publish only the completed, synced witness. Seeing an empty file
        // after create_new is NOT proof that the crash frontier was reached.
        std::fs::rename(&pending, marker).unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
