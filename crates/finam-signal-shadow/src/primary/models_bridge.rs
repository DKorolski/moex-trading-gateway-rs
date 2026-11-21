//! Opt-in source integration, NOT a session runner or deployment permission.
//! Accepts only the native single-use capability. Public JSON receipts remain
//! unattested inside the numerical journal. Context still has DECLARED replay
//! clocks: a source-qualified M10 does not promote the model session to LIVE.
use super::{
    AdmittedCanonicalM10, CanonicalReceipt, DeliveryPolicy, PrimaryProcessor, RecoveryInput,
    WsPrimaryConfig, WsRecoveryInput,
};
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
pub(crate) const SCHEMA: &str = "finam.shadow.imoexf-admitted-projection.v1";

fn hash(value: &impl serde::Serialize) -> Result<String> {
    Ok(crate::seed::digest(&serde_json::to_vec(value)?))
}
pub(crate) fn implementation() -> String {
    implementation_with_workspace(include_bytes!("../../../../Cargo.toml"))
}
fn implementation_with_workspace(workspace: &[u8]) -> String {
    crate::seed::digest(
        &[
            include_bytes!("models_bridge.rs").as_slice(),
            include_bytes!("cache.rs").as_slice(),
            include_bytes!("cache/recovery.rs").as_slice(),
            include_bytes!("cache/recovery/ws.rs").as_slice(),
            include_bytes!("models_entry.rs").as_slice(),
            include_bytes!("models_entry/seed.rs").as_slice(),
            include_bytes!("models_entry/reader.rs").as_slice(),
            include_bytes!("models_entry/reader/post_join.rs").as_slice(),
            include_bytes!("models_entry/reader/post_join/facts.rs").as_slice(),
            include_bytes!("models_entry/reader/publication.rs").as_slice(),
            include_bytes!("models_entry/artifacts.rs").as_slice(),
            include_bytes!("entry.rs").as_slice(),
            include_bytes!("restart/journal.rs").as_slice(),
            include_bytes!("verify.rs").as_slice(),
            include_bytes!("../../src/models_cli.rs").as_slice(),
            include_bytes!("../../src/runner/primary/writer.rs").as_slice(),
            include_bytes!("../../src/runner/primary/runtimes.rs").as_slice(),
            include_bytes!("../../src/runner/primary.rs").as_slice(),
            include_bytes!("policy.rs").as_slice(),
            include_bytes!("../../Cargo.toml").as_slice(),
            include_bytes!("../../../../Cargo.lock").as_slice(),
            workspace,
        ]
        .concat(),
    )
}

/// Borrowed native capabilities, never reconstructed from JSON. The writer
/// validates ownership before publication and consumes the original only after
/// every consumer has committed and passed its STOP/deadline barrier.
pub(crate) enum Projection<'a> {
    Canonical(&'a AdmittedCanonicalM10),
    Rest(
        &'a RecoveryInput,
        &'a broker_finam::sparse_m10::AdmittedClosedM1Snapshot,
    ),
    Ws(&'a WsRecoveryInput),
}
impl Projection<'_> {
    pub(crate) fn receipt(&self) -> &CanonicalReceipt {
        match self {
            Self::Canonical(i) => i.receipt(),
            Self::Rest(i, _) => &i.receipt().canonical,
            Self::Ws(i) => &i.receipt().canonical,
        }
    }
    pub(crate) fn bar(&self) -> &broker_core::HybridRuntimeBarEvent {
        match self {
            Self::Canonical(i) => i.model_bar(),
            Self::Rest(i, _) => i.model_bar(),
            Self::Ws(i) => i.model_bar(),
        }
    }
    pub(crate) fn provenance(&self) -> Result<(String, Value, Value, String)> {
        Ok(match self {
            Self::Canonical(i) => (
                i.provenance_sha256().map_err(|e| anyhow!("{e:?}"))?,
                serde_json::to_value(i.rest_evidence())?,
                Value::Null,
                serde_json::to_string(i.receipt())?,
            ),
            Self::Rest(i, source) => {
                ensure!(
                    i.receipt().canonical.rest_snapshot_sha256.as_deref() == Some(source.sha256()),
                    "recovery REST evidence binding"
                );
                (
                    i.provenance_sha256().map_err(|e| anyhow!("{e:?}"))?,
                    // The native observer durably retained this complete GET
                    // BEFORE this borrowed projection. Do not duplicate a six
                    // hour raw response inside every bounded 64 KiB candle.
                    json!({"storage":"native_control_journal_source_snapshot",
                        "snapshot_sha256":source.sha256(),
                        "evidence_sha256":hash(source.evidence())?}),
                    serde_json::to_value(i.receipt())?,
                    serde_json::to_string(i.receipt())?,
                )
            }
            Self::Ws(i) => (
                i.provenance_sha256().map_err(|e| anyhow!("{e:?}"))?,
                Value::Null,
                serde_json::to_value(i.receipt())?,
                serde_json::to_string(i.receipt())?,
            ),
        })
    }
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
            matches!(
                config.primary.delivery,
                DeliveryPolicy::ObservedBoRestTailRecoveryV6
                    | DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8
            ),
            "bridge requires explicit accepted V6 or V8 source policy"
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
        ensure!(
            config.base.observation_start_utc_ms == cutoff_ms,
            "model freeze and source session start differ"
        );
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
    /// Fresh-only attachment. Matching labels do NOT attest shared prefix
    /// provenance after a crash. Reopening/exporting remains diagnostic; native
    /// cohort restart needs a separately qualified shared-prefix protocol.
    pub(crate) fn validate_frontier(&self, producer: &PrimaryProcessor) -> Result<()> {
        ensure!(
            !self.failed && producer.config_sha256() == self.config_sha256,
            "model/source binding mismatch"
        );
        let exported = self.export()?;
        let rows = exported["inputs"].as_array().context("inputs")?;
        ensure!(
            rows.is_empty()
                && producer.next_label_ms() == Some(self.cutoff_ms)
                && producer.counters().committed == 0
                && !producer.gap_unresolved()
                && producer.state() != super::PipelineState::Hold,
            "only fresh shared source/model attachment is qualified"
        );
        let ready = exported["states"]
            .as_array()
            .context("states")?
            .iter()
            .filter(|s| {
                s["arm"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("IMOEXF/"))
            })
            .filter(|s| s["ready"] == true && s["bars_processed"] == 0)
            .count();
        ensure!(
            ready == 2,
            "both IMOEXF contexts must be ready at attachment"
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
        let commit = self.publish_inner(Projection::Canonical(&input))?;
        producer
            .committed(input, crate::timing::now_ms())
            .map_err(|e| {
                anyhow!(
                    "DURABLE_OUTPUT_SOURCE_NOT_ADVANCED: {e:?}; reopen/inspect, no replay as new"
                )
            })?;
        Ok(commit)
    }
    pub(crate) fn publish(&mut self, input: Projection<'_>) -> Result<Commit> {
        ensure!(!self.failed, "bridge held: explicit reopen required");
        self.failed = true;
        let result = self.publish_inner(input);
        if result.is_ok() {
            self.failed = false;
        }
        result
    }
    fn publish_inner(&mut self, input: Projection<'_>) -> Result<Commit> {
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
            &input.bar().instrument,
            chrono::DateTime::from_timestamp_millis(receipt.label_ms).context("label")?,
            &receipt.actual_m1,
        )
        .map_err(|e| anyhow!("canonical aggregation: {e:?}"))?;
        let mut expected = super::cache::runtime_bar(&bar.provisional_bar)
            .map_err(|e| anyhow!("runtime projection: {e:?}"))?;
        if !matches!(input, Projection::Canonical(_)) {
            expected.origin = broker_core::HybridRuntimeBarOrigin::Replay;
        }
        ensure!(
            bar.provisional_bar.instrument.venue_symbol.as_deref() == Some("IMOEXF@RTSX")
                && bar.minute_bitmap == receipt.minute_bitmap
                && expected == *input.bar(),
            "admitted bar/projection mismatch"
        );
        let b = bar.provisional_bar;
        let (source_hash, rest_evidence, recovery_receipt, source_json) = input.provenance()?;
        let source_receipt = if recovery_receipt.is_null() {
            serde_json::to_value(receipt)?
        } else {
            recovery_receipt.clone()
        };
        let provenance = json!({
            "schema":SCHEMA, "bridge_implementation_sha256":implementation(),
            "mapping_sha256":MAPPING_SHA256,
            "canonical_receipt":receipt,
            "canonical_receipt_sha256":hash(receipt)?,
            "canonical_receipt_json":serde_json::to_string(receipt)?,
            "source_receipt_sha256":source_hash,
            "source_receipt_json":source_json,
            "source_receipt":source_receipt,
            "rest_evidence":rest_evidence,
            "recovery_receipt":recovery_receipt,
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
            received_ms: receipt
                .proof_received_at_ms
                .or(receipt.boundary_received_at_ms)
                .context("native network receipt missing")?,
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
        Ok(commit)
    }
    /// Historical evidence only, including after a held completion.
    pub fn export(&self) -> Result<Value> {
        self.journal.export()
    }
    pub(crate) fn model_eod_ready(&self) -> bool {
        if self.failed {
            return false;
        }
        let Ok(value) = self.export() else {
            return false;
        };
        value["inputs"]
            .as_array()
            .is_some_and(|rows| rows.len() == 101)
            && value["states"].as_array().is_some_and(|states| {
                states
                    .iter()
                    .filter(|s| {
                        s["arm"]
                            .as_str()
                            .is_some_and(|id| id.starts_with("IMOEXF/"))
                    })
                    .filter(|s| s["ready"] == true && s["full_grid_flat"] == true)
                    .count()
                    == 2
            })
            && self
                .journal
                .committed_input("IMOEXF", self.history_len + 101)
                .is_ok_and(|v| v.is_some())
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

#[cfg(test)]
mod fingerprint_tests {
    #[test]
    fn workspace_only_features_change_bridge_contract() {
        let original = include_str!("../../../../Cargo.toml");
        let altered = original.replace(
            "serde_json = \"1\"",
            "serde_json = { version = \"1\", features = [\"arbitrary_precision\"] }",
        );
        assert_ne!(original, altered);
        assert_eq!(
            super::implementation(),
            super::implementation_with_workspace(original.as_bytes())
        );
        assert_ne!(
            super::implementation(),
            super::implementation_with_workspace(altered.as_bytes())
        );
    }
}
