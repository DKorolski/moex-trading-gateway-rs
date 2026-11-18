//! One serialized numerical writer, two independent physical source owners.
//! A crash never automatically resumes/adopts an ahead-of-source SQL prefix.
use super::{
    plan::{hash, INSTRUMENTS},
    Prepared,
};
use crate::{
    primary::{
        models_bridge::Projection, AdmittedCanonicalM10, CanonicalStatus, ExceptionReason,
        PrimaryProcessor, WireFrame,
    },
    transport::StopHandle,
};
use anyhow::{ensure, Context, Result};
use broker_core::canonical_m1::aggregate_m1_window;
use broker_finam::{sparse_m10::AdmittedClosedM1Snapshot, ShadowInstrument};
use finam_shadow_models::local::{
    contract::{Candle, Construction, Manifest, Ohlcv, Purpose},
    Commit, Journal,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub enum Progress {
    Waiting,
    ProofRequired {
        label_ms: i64,
        reason: ExceptionReason,
    },
    Committed(Commit),
    Finished,
}
pub struct Finish {
    pub output: PathBuf,
    pub evidence: Value,
}
pub(super) struct Log {
    file: File,
    index: u64,
    head: String,
}
impl Log {
    fn create(path: &Path) -> Result<Self> {
        Ok(Self {
            file: OpenOptions::new().create_new(true).write(true).open(path)?,
            index: 0,
            head: "0".repeat(64),
        })
    }
    fn append(&mut self, kind: &str, data: Value) -> Result<()> {
        let payload =
            json!({"index":self.index,"previous_sha256":self.head,"kind":kind,"data":data});
        let record = json!({"sha256":hash(&payload)?,"payload":payload});
        let bytes = serde_json::to_vec(&record)?;
        ensure!(bytes.len() <= 2 * 1024 * 1024, "control record bound");
        self.file.write_all(&bytes)?;
        self.file.write_all(b"\n")?;
        self.file.sync_all()?;
        self.head = record["sha256"].as_str().context("log hash")?.into();
        self.index += 1;
        Ok(())
    }
}
pub struct Cohort {
    prepared: Prepared,
    processors: BTreeMap<ShadowInstrument, PrimaryProcessor>,
    journal: Journal,
    log: Log,
    stop: StopHandle,
    held: bool,
    bound: std::collections::BTreeSet<ShadowInstrument>,
    output: PathBuf,
    #[cfg(test)]
    pub(super) after_sql_delay_ms: u64,
    #[cfg(test)]
    pub(super) stop_after_sql: bool,
}
impl Prepared {
    pub fn create(self, output: &Path, stop: StopHandle) -> Result<Cohort> {
        let now = crate::timing::now_ms();
        self.identities_fresh(now)?;
        ensure!(
            !stop.is_stopped()
                && now >= self.plan.start_not_before_ms
                && now < self.plan.start_before_ms,
            "fresh creation/STOP"
        );
        ensure!(!output.exists(), "new cohort directory required; no resume");
        let mut processors = BTreeMap::new();
        for i in INSTRUMENTS {
            let p = PrimaryProcessor::from_additional_source(
                self.configs[&i].clone(),
                i,
                self.calendars[&i].required().to_vec(),
                1,
                now,
                self.plan.stop_at_ms,
            )
            .map_err(|e| anyhow::anyhow!("source: {e:?}"))?;
            processors.insert(i, p);
        }
        std::fs::create_dir(output)?;
        let mut log = Log::create(&output.join("source-control.jsonl"))?;
        log.append("preparation", self.receipt()?)?;
        // Persist the exact numerical manifest separately, without changing any
        // original historical acquisition or numerical-availability clocks.
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(output.join("model-manifest.json"))?;
        file.write_all(&serde_json::to_vec(&self.manifest)?)?;
        file.sync_all()?;
        let journal = Journal::create(&output.join("models.sqlite"), self.manifest.clone())?;
        File::open(output)?.sync_all()?;
        if let Some(parent) = output.parent() {
            File::open(parent)?.sync_all()?;
        }
        Ok(Cohort {
            prepared: self,
            processors,
            journal,
            log,
            stop,
            held: false,
            bound: Default::default(),
            output: output.into(),
            #[cfg(test)]
            after_sql_delay_ms: 0,
            #[cfg(test)]
            stop_after_sql: false,
        })
    }
}
impl Cohort {
    fn guard(&self) -> Result<()> {
        ensure!(
            !self.held
                && !self.stop.is_stopped()
                && crate::timing::now_ms() < self.prepared.plan.stop_at_ms,
            "cohort HOLD/STOP/deadline"
        );
        Ok(())
    }
    fn fail<T>(&mut self, result: Result<T>) -> Result<T> {
        if result.is_err() {
            self.held = true;
            self.stop.stop();
        }
        result
    }
    pub fn bind_subscription(
        &mut self,
        subscription: &crate::socket::primary::Subscription,
    ) -> Result<()> {
        let r = (|| {
            self.guard()?;
            self.prepared.identities_fresh(crate::timing::now_ms())?;
            let i = subscription.instrument();
            ensure!(
                INSTRUMENTS.contains(&i) && !self.bound.contains(&i),
                "one matching subscription per source"
            );
            let p = self.processors.get_mut(&i).context("source")?;
            let c = self
                .prepared
                .calendars
                .remove(&i)
                .context("calendar already bound")?;
            p.bind_calendar(c)
                .map_err(|e| anyhow::anyhow!("calendar bind: {e:?}"))?;
            p.bind_subscription(subscription, crate::timing::now_ms())
                .map_err(|e| anyhow::anyhow!("subscription: {e:?}"))?;
            self.log.append("subscription",json!({"instrument":i,"generation":p.generation(),"bound_at_ms":crate::timing::now_ms()}))?;
            self.bound.insert(i);
            Ok(())
        })();
        self.fail(r)
    }
    pub fn ingest(&mut self, instrument: ShadowInstrument, frame: WireFrame) -> Result<()> {
        let r = (|| {
            self.guard()?;
            ensure!(self.bound.contains(&instrument), "unbound source");
            self.processors
                .get_mut(&instrument)
                .context("source")?
                .ingest(frame, crate::timing::now_ms())
                .map_err(|e| anyhow::anyhow!("ingress: {e:?}"))
        })();
        self.fail(r)
    }
    pub fn add_reference(
        &mut self,
        instrument: ShadowInstrument,
        source: &AdmittedClosedM1Snapshot,
    ) -> Result<()> {
        let r = (|| {
            self.guard()?;
            ensure!(
                source.instrument() == instrument && self.bound.contains(&instrument),
                "reference instrument/subscription"
            );
            ensure!(
                source.evidence().start.timestamp_millis()
                    >= self.processors[&instrument]
                        .calendar()
                        .context("calendar")?
                        .reference_start_ms()
                    && source.evidence().end.timestamp_millis()
                        <= self.prepared.manifest.freeze_at_ms,
                "planned pre-session bootstrap reference range"
            );
            self.log.append("reference",json!({"instrument":instrument,"sha256":source.sha256(),"evidence":source.evidence()}))?;
            self.processors
                .get_mut(&instrument)
                .context("source")?
                .add_reference(source)
                .map_err(|e| anyhow::anyhow!("reference: {e:?}"))
        })();
        self.fail(r)
    }
    pub fn next(&mut self, instrument: ShadowInstrument) -> Result<Progress> {
        let r = (|| {
            self.guard()?;
            ensure!(self.bound.len() == 2, "both sources must be attached");
            let status = self
                .processors
                .get_mut(&instrument)
                .context("source")?
                .prepare(crate::timing::now_ms())
                .map_err(|e| anyhow::anyhow!("prepare: {e:?}"))?;
            match status {
                CanonicalStatus::Ready(input) => {
                    Ok(Progress::Committed(self.publish(instrument, *input)?))
                }
                CanonicalStatus::PendingProof {
                    label_ms, reason, ..
                } => Ok(Progress::ProofRequired { label_ms, reason }),
                CanonicalStatus::Finished => Ok(Progress::Finished),
                CanonicalStatus::Waiting | CanonicalStatus::AwaitingCommit => Ok(Progress::Waiting),
            }
        })();
        self.fail(r)
    }
    /// A mandatory, admitted, timely source; not an optional REST comparison.
    pub fn prove(
        &mut self,
        instrument: ShadowInstrument,
        source: &AdmittedClosedM1Snapshot,
        reason: ExceptionReason,
    ) -> Result<Option<Commit>> {
        let r = (|| {
            self.guard()?;
            ensure!(
                source.instrument() == instrument && self.bound.len() == 2,
                "proof source identity"
            );
            let input = self
                .processors
                .get_mut(&instrument)
                .context("source")?
                .prepare_rest_exception(source, reason, crate::timing::now_ms())
                .map_err(|e| anyhow::anyhow!("proof: {e:?}"))?;
            input.map(|c| self.publish(instrument, *c)).transpose()
        })();
        self.fail(r)
    }
    fn publish(&mut self, i: ShadowInstrument, input: AdmittedCanonicalM10) -> Result<Commit> {
        self.guard()?;
        self.processors
            .get_mut(&i)
            .context("source")?
            .validate_pending_admission(&input, crate::timing::now_ms())
            .map_err(|e| anyhow::anyhow!("precommit: {e:?}"))?;
        let candle = project(
            &self.prepared.manifest,
            i,
            &self.prepared.configs[&i],
            &input,
        )?;
        self.log.append(
            "publication_intent",
            json!({"instrument":i,"candle":candle}),
        )?;
        self.guard()?;
        let commit = self.journal.apply(candle.clone())?;
        #[cfg(test)]
        {
            crate::timing::advance_fixture_clock(self.after_sql_delay_ms);
            if self.stop_after_sql {
                self.stop.stop();
            }
        }
        let readback = self
            .journal
            .committed_input(i.symbol(), candle.source_sequence)?
            .context("durable input absent")?;
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
            readback.candle == candle
                && readback.revision == revision
                && &readback.result_ids == ids,
            "SQL checked readback mismatch"
        );
        self.guard()?;
        self.log.append("checked_readback",json!({"instrument":i,"input_sha256":candle.sha256()?,"source_sequence":candle.source_sequence,"revision":revision,"result_ids":ids}))?;
        self.guard()?;
        // LAST operation in the publication: no required IO/model consumer
        // follows this source advance. Late/STOP leaves SQL ahead and held.
        self.processors
            .get_mut(&i)
            .context("source")?
            .committed(input, crate::timing::now_ms())
            .map_err(|e| anyhow::anyhow!("DURABLE_OUTPUT_SOURCE_NOT_ADVANCED: {e:?}"))?;
        Ok(commit)
    }
    pub fn frontier(&self, instrument: ShadowInstrument) -> Option<i64> {
        self.processors
            .get(&instrument)
            .and_then(PrimaryProcessor::next_label_ms)
    }
    pub fn export(&self) -> Result<Value> {
        self.journal.export()
    }
    /// Source-component snapshot, NOT worker join, native exit witness, EOD or
    /// an operational qualification. This component has no network workers.
    pub fn finish(mut self) -> Result<Finish> {
        self.stop.stop();
        let evidence = json!({"schema":"finam.shadow.usd-ri.source-finish.v1","held":self.held,
            "frontiers":INSTRUMENTS.iter().map(|i|json!({"instrument":i,"next_label_ms":self.frontier(*i),"committed":self.processors[i].counters().committed})).collect::<Vec<_>>(),
            "scope":"source_component_only","workers_joined":false,"observed_clean_session":false,"real_orders":false});
        self.log.append("finish", evidence.clone())?;
        Ok(Finish {
            output: self.output,
            evidence,
        })
    }
}
pub(super) fn project(
    m: &Manifest,
    i: ShadowInstrument,
    config: &str,
    input: &AdmittedCanonicalM10,
) -> Result<Candle> {
    let projection = Projection::Canonical(input);
    let r = projection.receipt();
    ensure!(
        r.config_sha256 == config
            && r.policy_sha256
                == super::plan::policy()
                    .fingerprint()
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?,
        "source contract"
    );
    let offset = r
        .label_ms
        .checked_sub(m.freeze_at_ms)
        .context("label offset")?;
    ensure!(
        (0..101 * 600000).contains(&offset) && offset % 600000 == 0,
        "model grid"
    );
    let aggregate = aggregate_m1_window(
        &i.instrument(),
        chrono::DateTime::from_timestamp_millis(r.label_ms).context("label")?,
        &r.actual_m1,
    )
    .map_err(|e| anyhow::anyhow!("M1 aggregate: {e:?}"))?;
    let expected = crate::primary::cache::runtime_bar(&aggregate.provisional_bar)
        .map_err(|e| anyhow::anyhow!("projection: {e:?}"))?;
    ensure!(
        expected == *projection.bar() && aggregate.minute_bitmap == r.minute_bitmap,
        "native aggregate/projection"
    );
    let b = aggregate.provisional_bar;
    let seed = m
        .seeds
        .iter()
        .find(|s| s.binding.instrument == i.symbol())
        .context("seed")?;
    let (source_hash, rest, _, source_json) = projection.provenance()?;
    let provenance = json!({"schema":"finam.shadow.usd-ri.native-projection.v1","implementation_sha256":super::implementation(),"canonical_receipt":r,
        "source_receipt_sha256":source_hash,"source_receipt_json":source_json,"rest_evidence":rest,
        "clock_basis":"declared_numerical_availability_not_physical_durability","observed_clean_session":false,"order_emission_allowed":false});
    Ok(Candle {
        instrument: i.symbol().into(),
        source_sequence: seed.history.len() as u64 + (offset / 600000) as u64 + 1,
        provider_id: i.venue_symbol().into(),
        binding_sha256: seed.binding.sha256()?,
        start_ms: r.label_ms,
        end_ms: r.close_ms,
        received_ms: r
            .proof_received_at_ms
            .or(r.boundary_received_at_ms)
            .context("network receipt")?,
        durable_ms: r.canonical_ready_at_ms,
        source_processed_ms: r.canonical_ready_at_ms,
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
    })
}
