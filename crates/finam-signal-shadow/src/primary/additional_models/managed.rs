//! Explicit library composition, not an operational CLI or deployment grant.
//! Two existing V8 owners, one serialized SQL/readback/advance-last barrier.
use super::{
    cohort::{project_native, Log},
    plan::{policy, INSTRUMENTS},
    Prepared,
};
use crate::{
    auth::AuthLifecycle,
    primary::{
        models_bridge::Projection, AdmittedCanonicalM10, ContinuityGate, PrimaryProcessor,
        RecoveryInput, WsRecoveryInput,
    },
    runner::primary::{
        live::{self, RunReport},
        runtimes::RuntimeSettings,
        Coordinator,
    },
    session::{CanonicalPublication, SessionError},
    transport::StopHandle,
};
use anyhow::{ensure, Context, Result};
use broker_finam::{
    sparse_m10::AdmittedClosedM1Snapshot, ShadowInstrument, Stage8bP1fO2GetOnlyClientV1,
};
use finam_shadow_models::local::{Commit, Journal};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub mod reader;
#[cfg(test)]
pub(super) mod tests;

pub(crate) enum Input<'a> {
    Canonical(AdmittedCanonicalM10),
    Rest(RecoveryInput, &'a AdmittedClosedM1Snapshot),
    Ws(WsRecoveryInput),
}
impl Input<'_> {
    fn projection(&self) -> Projection<'_> {
        match self {
            Self::Canonical(i) => Projection::Canonical(i),
            Self::Rest(i, s) => Projection::Rest(i, s),
            Self::Ws(i) => Projection::Ws(i),
        }
    }
    fn validate(&self, p: &mut PrimaryProcessor) -> Result<()> {
        let now = crate::timing::now_ms();
        match self {
            Self::Canonical(i) => p.validate_pending_admission(i, now),
            Self::Rest(i, _) => p.validate_recovery_admission(i, now),
            Self::Ws(i) => p.validate_ws_recovery_admission(i, now),
        }
        .map_err(|e| anyhow::anyhow!("native admission: {e:?}"))
    }
    fn advance(self, p: &mut PrimaryProcessor, now: i64) -> Result<()> {
        match self {
            Self::Canonical(i) => p.committed(i, now),
            Self::Rest(i, _) => p.recovered(i, now),
            Self::Ws(i) => p.recovered_ws(i, now),
        }
        .map_err(|e| anyhow::anyhow!("DURABLE_OUTPUT_SOURCE_NOT_ADVANCED: {e:?}"))
    }
}
pub(super) struct Store {
    prepared: Prepared,
    journal: Journal,
    log: Log,
    stop: StopHandle,
    held: bool,
    closed: bool,
    clock: (i64, std::time::Instant),
    #[cfg(test)]
    stop_after_sql: bool,
    #[cfg(test)]
    after_sql_delay_ms: u64,
    #[cfg(test)]
    fault_after_sql: bool,
    #[cfg(test)]
    fault_before_advance: bool,
}
impl Store {
    fn guard(&self) -> Result<()> {
        let now = crate::timing::sample_clock();
        ensure!(
            now.0 >= self.clock.0
                && now
                    .1
                    .checked_duration_since(self.clock.1)
                    .is_some_and(|d| (i128::from(now.0)
                        - i128::from(self.clock.0)
                        - d.as_millis() as i128)
                        .unsigned_abs()
                        <= 100),
            "shared writer UTC/monotonic discontinuity"
        );
        ensure!(
            !self.held
                && !self.closed
                && !self.stop.is_stopped()
                && crate::timing::now_ms() < self.prepared.plan.stop_at_ms,
            "shared writer HOLD/STOP/deadline"
        );
        Ok(())
    }
    fn append(&mut self, kind: &str, data: Value) -> Result<()> {
        ensure!(!self.closed, "closed managed journal");
        let r = self.log.append(kind, data);
        if r.is_err() {
            self.held = true;
            self.stop.stop();
        }
        r
    }
    fn publish(
        &mut self,
        instrument: ShadowInstrument,
        p: &mut PrimaryProcessor,
        input: Input<'_>,
        gate: Option<&ContinuityGate>,
    ) -> Result<CanonicalPublication> {
        let result = (|| {
            self.guard()?;
            let continuity = || -> Result<()> {
                ensure!(
                    gate.is_some_and(|g| !matches!(input, Input::Canonical(_)) || g.healthy()),
                    "live continuity not bound/healthy"
                );
                Ok(())
            };
            continuity()?;
            ensure!(
                p.source_instrument() == instrument
                    && p.config_sha256() == self.prepared.configs[&instrument],
                "typed source identity"
            );
            input.validate(p)?;
            if let Input::Rest(_, s) = &input {
                self.reference(instrument, s)?;
            }
            let projection = input.projection();
            let receipt = projection.receipt();
            let label = receipt.label_ms;
            let source_sha256 = projection.provenance()?.0;
            let candle = project_native(
                &self.prepared.manifest,
                instrument,
                &self.prepared.configs[&instrument],
                projection,
            )?;
            self.append(
                "publication_intent",
                json!({"instrument":instrument,"candle":candle,"at_ms":crate::timing::now_ms()}),
            )?;
            self.guard()?;
            continuity()?;
            input.validate(p)?;
            let commit = self.journal.apply(candle.clone())?;
            #[cfg(test)]
            if self.stop_after_sql {
                self.stop.stop();
            }
            #[cfg(test)]
            {
                crate::timing::advance_fixture_clock(self.after_sql_delay_ms);
                if self.fault_after_sql {
                    gate.unwrap().fault(crate::socket::primary::Failure {
                        generation: p.generation(),
                        kind: crate::socket::primary::Kind::Close,
                        liveness: None,
                    });
                }
            }
            let read = self
                .journal
                .committed_input(instrument.symbol(), candle.source_sequence)?
                .context("SQL input missing")?;
            let (revision, ids) = match commit {
                Commit::Committed {
                    revision,
                    result_ids,
                }
                | Commit::AlreadyCommitted {
                    revision,
                    result_ids,
                } => (revision, result_ids),
            };
            ensure!(
                read.candle == candle && read.revision == revision && read.result_ids == ids,
                "exact SQL readback"
            );
            self.guard()?;
            continuity()?;
            input.validate(p)?;
            self.append("checked_readback",json!({"instrument":instrument,"input_sha256":candle.sha256()?,"source_sequence":candle.source_sequence,"revision":revision,"result_ids":ids,"at_ms":crate::timing::now_ms()}))?;
            self.guard()?;
            continuity()?;
            input.validate(p)?;
            #[cfg(test)]
            if self.fault_before_advance {
                let gate = gate.unwrap().clone();
                let generation = p.generation();
                std::thread::spawn(move || {
                    gate.fault(crate::socket::primary::Failure {
                        generation,
                        kind: crate::socket::primary::Kind::Close,
                        liveness: None,
                    })
                })
                .join()
                .unwrap();
            }
            let now = if matches!(input, Input::Canonical(_)) {
                gate.context("live gate")?
                    .with_healthy(p.generation(), || -> Result<i64> {
                        self.guard()?;
                        input.validate(p)?;
                        let now = crate::timing::now_ms();
                        input.advance(p, now)?;
                        Ok(now)
                    })
                    .context("physical fault before native advance")??
            } else {
                let now = crate::timing::now_ms();
                input.advance(p, now)?;
                now
            };
            // Still holding the shared mutex. No SQL/log/model consumer after
            // this native advance; another instrument cannot interleave here.
            Ok(CanonicalPublication {
                label_ms: label,
                source_sha256,
                committed_at_ms: now,
                model_result_at_ms: now,
            })
        })();
        if result.is_err() {
            self.held = true;
            self.stop.stop();
        }
        result
    }
    fn reference(&mut self, i: ShadowInstrument, s: &AdmittedClosedM1Snapshot) -> Result<()> {
        self.guard()?;
        ensure!(s.instrument() == i, "reference instrument");
        self.append("snapshot",json!({"instrument":i,"sha256":s.sha256(),"evidence":s.evidence(),"recorded_at_ms":crate::timing::now_ms()}))
    }
}
/// Only the admitted builder can make a lane; no old IMOEXF model/session.
pub(crate) struct Lane {
    instrument: ShadowInstrument,
    config: String,
    first: i64,
    deadline: i64,
    policy: crate::primary::PrimaryPolicy,
    store: Arc<Mutex<Store>>,
    stop: StopHandle,
    gate: Option<ContinuityGate>,
    finished: bool,
}
impl Lane {
    fn access<T>(&self, work: impl FnOnce(&mut Store) -> Result<T>) -> Result<T, SessionError> {
        let r = self
            .store
            .lock()
            .map_err(|_| SessionError::Evidence)
            .and_then(|mut store| work(&mut store).map_err(|_| SessionError::Evidence));
        if r.is_err() {
            self.stop.stop();
        }
        r
    }
    pub(crate) fn binding(&self) -> (&str, i64, &crate::primary::PrimaryPolicy) {
        (&self.config, self.first, &self.policy)
    }
    pub(crate) fn settings(&self) -> (RuntimeSettings, StopHandle) {
        (
            RuntimeSettings {
                config_sha256: self.config.clone(),
                deadline_ms: self.deadline,
                clock_uncertainty_ms: 100,
                stop_timeout_ms: 1000,
                policy: self.policy.clone(),
            },
            self.stop.clone(),
        )
    }
    pub(crate) fn event(&mut self, event: Value) -> Result<(), SessionError> {
        self.access(|s| {
            s.append(
                "transport",
                json!({"instrument":self.instrument,"event":event,"at_ms":crate::timing::now_ms()}),
            )
        })
    }
    pub(crate) fn reference(
        &mut self,
        snapshot: &AdmittedClosedM1Snapshot,
    ) -> Result<(), SessionError> {
        self.access(|s| s.reference(self.instrument, snapshot))
    }
    pub(crate) fn consume(
        &mut self,
        p: &mut PrimaryProcessor,
        input: Input<'_>,
    ) -> Result<CanonicalPublication, SessionError> {
        self.access(|s| s.publish(self.instrument, p, input, self.gate.as_ref()))
    }
    pub(crate) fn export(&self) -> Result<Value, SessionError> {
        self.access(|s| s.journal.export())
    }
    pub(crate) fn finish(&mut self, reason: &str) -> Result<(), SessionError> {
        if self.finished {
            return Err(SessionError::Input);
        }
        self.event(
            json!({"event":"usd_ri_lane_finalized","reason":reason,"full_session_qualified":false}),
        )?;
        self.finished = true;
        Ok(())
    }
    pub(crate) fn bind_gate(&mut self, gate: ContinuityGate) {
        self.gate = Some(gate);
    }
}
/// Two independent V8 coordinators/calendars; NOT a restart loader.
pub struct Managed {
    sources: Vec<(
        ShadowInstrument,
        Coordinator,
        crate::primary::calendar::AdmittedCalendar,
    )>,
    store: Arc<Mutex<Store>>,
    stop: StopHandle,
    output: PathBuf,
}
pub struct Finished {
    pub output: PathBuf,
    pub evidence: Value,
}
impl Prepared {
    pub fn into_managed(mut self, output: &Path, stop: StopHandle) -> Result<Managed> {
        let now = crate::timing::now_ms();
        self.identities_fresh(now)?;
        ensure!(
            !stop.is_stopped()
                && now >= self.plan.start_not_before_ms
                && now < self.plan.start_before_ms,
            "fresh managed creation"
        );
        ensure!(
            !output.exists(),
            "new managed output required; no automatic restart/adoption"
        );
        let preparation = self.receipt()?;
        let calendars = std::mem::take(&mut self.calendars);
        std::fs::create_dir(output)?;
        let mut log = Log::create(&output.join("managed-control.jsonl"))?;
        log.append(
            "managed_preparation",
            json!({"schema":"finam.shadow.usd-ri.managed.v1","component":preparation,
            "library_composition":true,"operational_activation":false,"orders":false}),
        )?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(output.join("model-manifest.json"))?;
        file.write_all(&serde_json::to_vec(&self.manifest)?)?;
        file.sync_all()?;
        let journal = Journal::create(&output.join("models.sqlite"), self.manifest.clone())?;
        File::open(output)?.sync_all()?;
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        File::open(parent)?.sync_all()?;
        let first = self.manifest.freeze_at_ms;
        let deadline = self.plan.stop_at_ms;
        let configs = self.configs.clone();
        let store = Arc::new(Mutex::new(Store {
            prepared: self,
            journal,
            log,
            stop: stop.clone(),
            held: false,
            closed: false,
            clock: crate::timing::sample_clock(),
            #[cfg(test)]
            stop_after_sql: false,
            #[cfg(test)]
            after_sql_delay_ms: 0,
            #[cfg(test)]
            fault_after_sql: false,
            #[cfg(test)]
            fault_before_advance: false,
        }));
        let mut sources = Vec::new();
        for (i, calendar) in calendars {
            let p = PrimaryProcessor::from_additional_source(
                configs[&i].clone(),
                i,
                calendar.required().to_vec(),
                1,
                now,
                deadline,
            )
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            let lane = Lane {
                instrument: i,
                config: configs[&i].clone(),
                first,
                deadline,
                policy: policy(),
                store: store.clone(),
                stop: stop.clone(),
                gate: None,
                finished: false,
            };
            let owner =
                Coordinator::with_additional(p, lane).map_err(|e| anyhow::anyhow!("{e:?}"))?;
            sources.push((i, owner, calendar));
        }
        Ok(Managed {
            sources,
            store,
            stop,
            output: output.into(),
        })
    }
}
struct StopOnDrop(StopHandle);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop();
    }
}
impl Managed {
    /// Explicit invocation only. Auth is exact-account, readonly and refreshed
    /// by the accepted component on every REST/WS grant. No credentials saved.
    pub async fn run(
        self,
        auth: Arc<AuthLifecycle>,
        clients: [Arc<Stage8bP1fO2GetOnlyClientV1>; 2],
    ) -> Result<Finished> {
        let _guard = StopOnDrop(self.stop.clone());
        let by_instrument: BTreeMap<_, _> = clients
            .into_iter()
            .map(|c| (c.shadow_instrument(), c))
            .collect();
        {
            let s = self
                .store
                .lock()
                .map_err(|_| anyhow::anyhow!("writer poisoned"))?;
            s.guard()?;
            s.prepared.identities_fresh(crate::timing::now_ms())?;
            ensure!(
                by_instrument.len() == 2
                    && INSTRUMENTS.iter().all(|i| by_instrument
                        .get(i)
                        .is_some_and(|c| auth.binds(c.account_id())
                            && crate::seed::digest(c.account_id().as_bytes())
                                == s.prepared.plan.account_sha256)),
                "two exact readonly market-source account bindings"
            );
        }
        let mut sources = self.sources.into_iter();
        let (i, a, ca) = sources.next().context("USD lane")?;
        let (j, b, cb) = sources.next().context("RI lane")?;
        let stop_a = self.stop.clone();
        let stop_b = self.stop.clone();
        // No detached outer tasks: returning from each future stops its peer,
        // whose V8 driver must drain/join its own actual children.
        let a = async {
            let r = live::run(a, ca, auth.clone(), by_instrument[&i].clone()).await;
            stop_a.stop();
            r
        };
        let b = async {
            let r = live::run(b, cb, auth.clone(), by_instrument[&j].clone()).await;
            stop_b.stop();
            r
        };
        let (a, b) = tokio::join!(a, b);
        finalize(&self.store, &self.output, [(i, a), (j, b)])
    }
}
fn finalize(
    store: &Arc<Mutex<Store>>,
    output: &Path,
    reports: [(ShadowInstrument, Result<RunReport, live::Cause>); 2],
) -> Result<Finished> {
    // Never block waiting for a worker that failed its join budget. Absence of
    // a completion is INCOMPLETE, not a fabricated successful drain receipt.
    let mut s = store
        .try_lock()
        .map_err(|_| anyhow::anyhow!("managed writer not joined/poisoned"))?;
    let mut public = Vec::new();
    for (i, r) in reports {
        if !r.as_ref().is_ok_and(|r| {
            r.all_workers_drained
                && r.session_finalized
                && r.completion.as_ref().is_some_and(|c| {
                    c.clock_consistent
                        && c.fault_free
                        && c.continuity_resolved
                        && c.recovery_resolved
                })
                && matches!(
                    r.cause,
                    live::Cause::RequestedStop | live::Cause::RunDeadline
                )
        }) {
            s.held = true;
        }
        public.push(json!({"instrument":i,"result":r}));
    }
    let evidence = json!({"schema":"finam.shadow.usd-ri.managed-finish.v1","reports":public,"held":s.held,
        "joined_at_ms":crate::timing::now_ms(),"scope":"source_only_library","observed_clean_session":false,
        "native_systemd_qualified":false,"full_session_qualified":false,"real_orders":false});
    s.append("managed_finish", evidence.clone())?;
    s.closed = true;
    Ok(Finished {
        output: output.into(),
        evidence,
    })
}
