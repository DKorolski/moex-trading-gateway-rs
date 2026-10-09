use super::inputs::hash;
use super::*;
use crate::*;
use broker_core::{stage4_bootstrap::*, BrokerMarketSessionState, Stage4BrokerTruthFreshnessProbe};
use broker_finam::sparse_m10::{AdmittedClosedM1Snapshot, ClosedM1RequestPlan};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
};
use strategy_runtime_core::{Stage8bP1eScheduleEvidenceKindV1, Stage8bP1eScheduleStateV1};

pub(crate) fn validate_private_root(path: &Path) -> Result<(), O3Error> {
    let m = fs::symlink_metadata(path).map_err(|_| O3Error::Io)?;
    if !m.is_dir()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o777 != 0o700
        || path.canonicalize().map_err(|_| O3Error::Io)? != path
    {
        return Err(O3Error::Input);
    }
    Ok(())
}

pub(crate) fn producer_lease() -> Result<File, O3Error> {
    let path = Path::new(O3_PRODUCER_ROOT).join("producer.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| O3Error::Io)?;
    let m = file.metadata().map_err(|_| O3Error::Io)?;
    if !m.is_file()
        || m.nlink() != 1
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
        || unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0
    {
        return Err(O3Error::Input);
    }
    Ok(file)
}

fn present_private(path: &Path) -> Result<bool, O3Error> {
    match fs::symlink_metadata(path) {
        Ok(m)
            if m.is_file()
                && m.nlink() == 1
                && m.uid() == unsafe { libc::geteuid() }
                && m.mode() & 0o077 == 0
                && m.len() <= 128 * 1024 =>
        {
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        _ => Err(O3Error::Input),
    }
}

/// Missing state is NOT first-publication authority. The fresh administrative
/// path must retain Prepared before initializing Redis; both this runtime and
/// the initializer read those same authenticated bytes. No reset/re-sign path.
pub(crate) fn load_schedule_checkpoint(
    path: &Path,
    owner: u32,
    plan: &O3CompositionPlan,
) -> Result<Stage8bP1eSchedulePublisherStateV1, O3Error> {
    let bytes = read_private_checkpoint(path, owner)?;
    let state =
        crate::stage8b_p1e_schedule_publisher::parse_stage8b_p1e_schedule_publisher_state(&bytes)
            .map_err(|_| O3Error::Input)?;
    validate_schedule_binding(&state, plan)?;
    Ok(state)
}

fn read_private_checkpoint(path: &Path, owner: u32) -> Result<Vec<u8>, O3Error> {
    const MAX: u64 = 128 * 1024;
    let parent = path.parent().ok_or(O3Error::Input)?;
    let directory = fs::symlink_metadata(parent).map_err(|_| O3Error::Io)?;
    if !directory.is_dir()
        || directory.uid() != owner
        || directory.mode() & 0o777 != 0o700
        || parent.canonicalize().map_err(|_| O3Error::Io)? != parent
    {
        return Err(O3Error::Input);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| O3Error::Io)?;
    let before = file.metadata().map_err(|_| O3Error::Io)?;
    if !before.is_file()
        || before.uid() != owner
        || before.nlink() != 1
        || before.mode() & 0o077 != 0
        || before.len() > MAX
    {
        return Err(O3Error::Input);
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| O3Error::Io)?;
    let after = file.metadata().map_err(|_| O3Error::Io)?;
    if bytes.len() as u64 != before.len()
        || after.uid() != before.uid()
        || after.mode() != before.mode()
        || after.nlink() != 1
        || after.len() != before.len()
        || after.mtime() != before.mtime()
        || after.mtime_nsec() != before.mtime_nsec()
        || after.ctime() != before.ctime()
        || after.ctime_nsec() != before.ctime_nsec()
    {
        return Err(O3Error::Input);
    }
    Ok(bytes)
}

fn validate_schedule_binding(
    state: &Stage8bP1eSchedulePublisherStateV1,
    plan: &O3CompositionPlan,
) -> Result<(), O3Error> {
    let envelope: strategy_runtime_core::Stage8bP1eScheduleEnvelopeV3 =
        serde_json::from_slice(&state.exact_envelope_bytes().map_err(|_| O3Error::Input)?)
            .map_err(|_| O3Error::Input)?;
    if state.schedule_semantic_sha256() != plan.schedule_semantic_sha256
        || envelope.operational_identity_sha256 != plan.operational_identity_sha256
        || envelope.runtime_config_fingerprint_sha256 != plan.runtime_config_fingerprint_sha256
        || envelope.instrument_map_fingerprint_sha256 != plan.instrument_map_fingerprint_sha256
        || envelope.payload.registry.registry_identity_sha256
            != plan.schedule.registry_identity_sha256
        || envelope.payload.registry.registry_version != plan.schedule.registry_version
    {
        return Err(O3Error::Input);
    }
    Ok(())
}

pub(crate) fn initial_schedule_checkpoint(
    path: &Path,
    owner: u32,
    plan: &O3CompositionPlan,
    exact_envelope: &[u8],
) -> Result<(), O3Error> {
    let state = load_schedule_checkpoint(path, owner, plan)?;
    if state.phase() != Stage8bP1eSchedulePublisherPhaseV1::Prepared
        || state.publication_sequence() != 1
        || state.semantic_revision() != 1
        || state.exact_envelope_bytes().map_err(|_| O3Error::Input)? != exact_envelope
    {
        return Err(O3Error::Input);
    }
    Ok(())
}

pub(crate) fn prepare_initial_schedule(inputs: &AdmittedInputs) -> Result<Vec<u8>, O3Error> {
    // A first preparation is allowed only in an empty producer root (apart
    // from the held lock). Lost high-water must never trigger automatic repair.
    for entry in fs::read_dir(O3_PRODUCER_ROOT).map_err(|_| O3Error::Io)? {
        if entry.map_err(|_| O3Error::Io)?.file_name() != "producer.lock" {
            return Err(O3Error::Input);
        }
    }
    let now = now_before(inputs.deadline)?;
    let prepared = prepare_stage8b_p1f_o3_synthetic_schedule(
        Stage8bP1fO3ScheduleInputV1 {
            phase_id: inputs.plan.phase_id.clone(),
            expected_synthetic_fixture_sha256: inputs.plan.schedule_fixture_sha256.clone(),
            publisher_input: schedule_input(&inputs.plan, now)?,
            trusted_now_utc: now,
        },
        Stage8bP1eSchedulePublisherLineage::First(
            authorize_stage8b_p1e_first_publication("AUTHORIZE-STAGE8B-P1E-FIRST-PUBLICATION")
                .map_err(|_| O3Error::Input)?,
        ),
        &crate::stage8b_p1f_o3_signer::O3UnixScheduleSigner,
    )
    .map_err(|_| O3Error::Producer)?;
    validate_schedule_binding(&prepared, &inputs.plan)?;
    persist_stage8b_p1e_schedule_publisher_state(Path::new(SCHEDULE_STATE), &prepared)
        .map_err(|_| O3Error::Producer)?;
    let retained = load_schedule_checkpoint(
        Path::new(SCHEDULE_STATE),
        unsafe { libc::geteuid() },
        &inputs.plan,
    )?;
    if retained != prepared {
        return Err(O3Error::Input);
    }
    retained.exact_envelope_bytes().map_err(|_| O3Error::Input)
}

/// A pure, explicitly synthetic O3 report. It uses the ordinary Stage4
/// validation/report builder and accepted source adapter, not fixture signers
/// or fabricated "passed" report fields. Only observation timestamps refresh.
pub(crate) fn schedule_input(
    plan: &O3CompositionPlan,
    now: DateTime<Utc>,
) -> Result<Stage8bP1eSchedulePublisherInputV1, O3Error> {
    let input = build_schedule_input(plan, now)?;
    if stage8b_p1f_synthetic_schedule_fixture_sha256(&plan.phase_id, &input)
        .map_err(|_| O3Error::Input)?
        != plan.schedule_fixture_sha256
    {
        return Err(O3Error::Input);
    }
    Ok(input)
}

pub(super) fn build_schedule_input(
    plan: &O3CompositionPlan,
    now: DateTime<Utc>,
) -> Result<Stage8bP1eSchedulePublisherInputV1, O3Error> {
    // Sample the NEW synthetic observation at the schedule wire precision.
    // Never round/rewrite an existing broker receipt or retained report: the
    // report and envelope must be built from the same instant. Utc::now()
    // may have nanoseconds, whereas the accepted schedule contract has micros.
    let now = DateTime::from_timestamp_micros(now.timestamp_micros()).ok_or(O3Error::Input)?;
    let source = &plan.schedule;
    let mut truth = source.synthetic_truth.clone();
    truth.received_ts = now;
    let mut freshness = Stage4BrokerTruthFreshnessInput::from_broker_truth_received_ts(now, 60_000);
    freshness.schedule = Stage4BrokerTruthFreshnessProbe::fresh(now, 60_000, true);
    let validated = validate_stage4_broker_truth_bootstrap(Stage4BrokerTruthBootstrapInput {
        broker_truth: &truth,
        broker_truth_source_status: Stage4BrokerTruthSourceStatus::Present,
        target_instrument: broker_core::InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: broker_core::Exchange::Moex,
            market: broker_core::Market::Futures,
        },
        restored_runtime_state: None,
        freshness,
        schedule_state: BrokerMarketSessionState::Open,
        adoption: Stage4AdoptionDisposition::default(),
        external_issues: Vec::new(),
        safety_boundary: Stage4BrokerTruthSafetyBoundary::closed(),
        checked_ts: now,
    });
    let sections = validated
        .freshness
        .sections
        .iter()
        .map(|s| Stage4BootstrapEvidenceSourceStatusSection {
            section: s.section,
            source_status: Stage4BrokerTruthSourceStatus::Present,
            required_for_bootstrap: s.required_for_bootstrap,
        })
        .collect::<Vec<_>>();
    let accepted = build_stage4_accepted_paper_host_evidence(&validated, &sections)
        .map_err(|_| O3Error::Input)?;
    let input = adapt_stage8b_p1e_readonly_schedule(Stage8bP1eReadonlyScheduleAdapterInputV1 {
        exact_finam_response_bytes: source.exact_synthetic_schedule_json.as_bytes().to_vec(),
        schedule: serde_json::from_str(&source.exact_synthetic_schedule_json)
            .map_err(|_| O3Error::Input)?,
        stage4_report: accepted.report().clone(),
        registry_entry: source.registry_entry.clone(),
        registry_version: source.registry_version.clone(),
        registry_identity_sha256: source.registry_identity_sha256.clone(),
        operational_identity_sha256: plan.operational_identity_sha256.clone(),
        runtime_config_fingerprint_sha256: plan.runtime_config_fingerprint_sha256.clone(),
        instrument_map_fingerprint_sha256: plan.instrument_map_fingerprint_sha256.clone(),
        source_observed_at_utc: now,
        published_at_utc: now,
        evidence_kind: Stage8bP1eScheduleEvidenceKindV1::Tradability,
        schedule_state: Stage8bP1eScheduleStateV1::Open,
        boundary_proof: None,
    })
    .map_err(|_| O3Error::Input)?;
    Ok(input)
}

pub(crate) async fn schedule_loop(
    inputs: AdmittedInputs,
    output: watch::Sender<Option<Stage8bP1eSchedulePublisherStateV1>>,
    mut stopped: watch::Receiver<bool>,
) -> Result<(), O3Error> {
    let path = Path::new(SCHEDULE_STATE);
    let mut retained = load_schedule_checkpoint(path, unsafe { libc::geteuid() }, &inputs.plan)?;
    let mut writer =
        Stage8bP1fSchedulePublisherRedisV1::connect(durable::STAGE8B_P1E_REDIS_URL_IPV4)
            .await
            .map_err(|_| O3Error::Producer)?;
    loop {
        if *stopped.borrow() {
            return Ok(());
        }
        let now = now_before(inputs.deadline)?;
        let prepared = match retained {
            s if s.phase() == Stage8bP1eSchedulePublisherPhaseV1::Prepared => {
                // Retained Prepared signature is replayed as-is, never refreshed.
                if s.schedule_semantic_sha256() != inputs.plan.schedule_semantic_sha256 {
                    return Err(O3Error::Input);
                }
                s
            }
            previous => {
                let input = schedule_input(&inputs.plan, now)?;
                prepare_stage8b_p1f_o3_synthetic_schedule(
                    Stage8bP1fO3ScheduleInputV1 {
                        phase_id: inputs.plan.phase_id.clone(),
                        expected_synthetic_fixture_sha256: inputs
                            .plan
                            .schedule_fixture_sha256
                            .clone(),
                        publisher_input: input,
                        trusted_now_utc: now,
                    },
                    Stage8bP1eSchedulePublisherLineage::Resume(&previous),
                    &crate::stage8b_p1f_o3_signer::O3UnixScheduleSigner,
                )
                .map_err(|_| O3Error::Producer)?
            }
        };
        validate_schedule_binding(&prepared, &inputs.plan)?;
        let state = publish_stage8b_p1e_prepared_schedule(path, prepared, &mut writer)
            .await
            .map_err(|_| O3Error::Producer)?;
        output.send_replace(Some(state.clone()));
        retained = state;
        tokio::select! {
            _ = tokio::time::sleep(StdDuration::from_millis(1000)) => {},
            _ = stopped.changed() => return Ok(()),
        }
    }
}

fn expected_bytes(
    source: &O3ObservedInput,
    binding: &durable::Stage8bP1ObservedM10Binding,
) -> Result<Vec<u8>, O3Error> {
    if binding.source().sha256() != source.receipt_sha256
        || binding.source().source_snapshot_sha256() != source.snapshot_sha256
        || binding.source().end() != source.evidence.end
    {
        return Err(O3Error::Input);
    }
    let bytes = durable::build_stage8b_p1_observed_canonical_m10(
        binding.operational_identity_sha256(),
        (source.evidence.end - chrono::Duration::minutes(10)).timestamp_millis(),
        binding.source(),
    )
    .map_err(|_| O3Error::Input)?;
    if hash(&bytes) != source.canonical_m10_sha256 {
        return Err(O3Error::Input);
    }
    Ok(bytes)
}

fn retained_binding(
    inputs: &AdmittedInputs,
    source: &O3ObservedInput,
) -> Result<durable::Stage8bP1ObservedM10Binding, O3Error> {
    let binding = durable::Stage8bP1ObservedM10Binding::restore_retained_receipt(
        Path::new(O3_PRODUCER_ROOT),
        &inputs.plan.operational_identity_sha256,
        &source.receipt_sha256,
    )
    .map_err(|_| O3Error::Input)?;
    expected_bytes(source, &binding)?;
    Ok(binding)
}

fn load_exact(
    path: &Path,
    inputs: &AdmittedInputs,
    source: &O3ObservedInput,
) -> Result<Stage8bP1fObservedM10State, O3Error> {
    if !present_private(path)? {
        return Err(O3Error::Input);
    }
    let binding = retained_binding(inputs, source)?;
    let state = Stage8bP1fObservedM10State::load(path, binding).map_err(|_| O3Error::Input)?;
    if state.phase_id() != inputs.plan.phase_id
        || state.producer_phase() != Stage8bP1fProducerPhaseV1::O3Synthetic
    {
        return Err(O3Error::Input);
    }
    if state.canonical_bytes().map_err(|_| O3Error::Input)?
        != expected_bytes(source, state.binding())?
    {
        return Err(O3Error::Input);
    }
    Ok(state)
}

async fn publish_retained_or_new(
    inputs: &AdmittedInputs,
    source: &O3ObservedInput,
    prior: Option<&Stage8bP1fObservedM10State>,
    retained: Option<Stage8bP1fObservedM10State>,
    schedule: &mut watch::Receiver<Option<Stage8bP1eSchedulePublisherStateV1>>,
) -> Result<Stage8bP1fObservedM10State, O3Error> {
    let state = match retained {
        Some(s) => s, // Exact Prepared recovery has no new-source freshness authority.
        None => {
            loop {
                let now = now_before(inputs.deadline)?;
                if now >= source.evidence.end && schedule.borrow().is_some() {
                    break;
                }
                tokio::time::sleep(StdDuration::from_millis(20)).await;
            }
            let now = Utc::now();
            let plan = ClosedM1RequestPlan::new(
                source.evidence.start,
                source.evidence.end,
                source.evidence.parts.iter().map(|p| p.end).collect(),
            )
            .map_err(|_| O3Error::Input)?;
            let snapshot = AdmittedClosedM1Snapshot::admit(source.evidence.clone(), &plan, now)
                .map_err(|_| O3Error::Input)?;
            if snapshot.sha256() != source.snapshot_sha256
                || snapshot.receipt().sha256() != source.receipt_sha256
            {
                return Err(O3Error::Input);
            }
            let current = schedule.borrow().clone().ok_or(O3Error::Producer)?;
            // A new schedule can arrive while a large snapshot is admitted.
            // Capture its immutable Published state BEFORE the verification
            // clock; otherwise a valid concurrent refresh appears "future".
            let now = now_before(inputs.deadline)?;
            let lineage = match prior {
                Some(s) => Stage8bP1fObservedM10Lineage::Resume(s),
                None => Stage8bP1fObservedM10Lineage::First(
                    authorize_stage8b_p1f_first_m10("AUTHORIZE-STAGE8B-P1F-FIRST-M10")
                        .map_err(|_| O3Error::Input)?,
                ),
            };
            let outcome = prepare_stage8b_p1f_observed_m10(
                Stage8bP1fObservedM10Input {
                    phase: Stage8bP1fProducerPhaseV1::O3Synthetic,
                    phase_id: inputs.plan.phase_id.clone(),
                    nominal_close: source.evidence.end,
                    trusted_now: now,
                    snapshot: &snapshot,
                    expected_receipt_sha256: &source.receipt_sha256,
                    expected_synthetic_snapshot_sha256: Some(&source.snapshot_sha256),
                },
                &inputs.plan.operational_identity_sha256,
                &current,
                lineage,
            )
            .map_err(O3Error::ProducerDetail)?;
            match outcome {
                Stage8bP1fObservedM10Outcome::Prepared(s)
                | Stage8bP1fObservedM10Outcome::ReplayPrepared(s)
                | Stage8bP1fObservedM10Outcome::IdempotentPublished(s) => s,
            }
        }
    };
    expected_bytes(source, state.binding())?;
    // The runtime resolves sealed sources beside its configured durable root,
    // not beside the producer high-water. Retain exact admitted bytes there
    // BEFORE publication; no scan, Redis-derived binding or root creation.
    if state.phase() == Stage8bP1fM10ProducerPhaseV1::Published {
        let retained = durable::Stage8bP1ObservedM10Binding::restore_retained_receipt(
            &inputs.durable_parent,
            &inputs.plan.operational_identity_sha256,
            &source.receipt_sha256,
        )
        .map_err(|_| O3Error::Input)?;
        expected_bytes(source, &retained)?;
        return Ok(state);
    }
    retain_runtime_source(state.binding(), &inputs.durable_parent)?;
    let mut feeder = durable::Stage8bP1fM10FeederRedisV1::connect_observed_synthetic(
        durable::STAGE8B_P1E_REDIS_URL_IPV4,
        durable::Stage8bP1RedisConfig::paper_default_auto(),
        state.binding().clone(),
    )
    .await
    .map_err(|_| O3Error::Producer)?;
    publish_stage8b_p1f_observed_m10(Path::new(HIGH_WATER), state, &mut feeder)
        .await
        .map_err(O3Error::ProducerDetail)
}

pub(crate) fn retain_runtime_source(
    binding: &durable::Stage8bP1ObservedM10Binding,
    parent: &Path,
) -> Result<(), O3Error> {
    validate_private_root(parent)?;
    binding
        .persist_retained_receipt(parent)
        .map_err(|_| O3Error::Input)?;
    let retained = durable::Stage8bP1ObservedM10Binding::restore_retained_receipt(
        parent,
        binding.operational_identity_sha256(),
        binding.source().sha256(),
    )
    .map_err(|_| O3Error::Input)?;
    if retained.source().retained_bytes() != binding.source().retained_bytes() {
        return Err(O3Error::Input);
    }
    Ok(())
}

pub(crate) async fn published_window(
    inputs: &AdmittedInputs,
    mut schedule: watch::Receiver<Option<Stage8bP1eSchedulePublisherStateV1>>,
) -> Result<durable::Stage8bP1ObservedPublishedWindow, O3Error> {
    let checkpoint = Path::new(PREDECESSOR);
    let high_water = Path::new(HIGH_WATER);
    let decision = if present_private(checkpoint)? {
        let state = load_exact(checkpoint, inputs, &inputs.plan.decision)?;
        if state.phase() != Stage8bP1fM10ProducerPhaseV1::Published
            || state.publication_sequence() != 1
        {
            return Err(O3Error::Input);
        }
        // Published restart requires the already-retained runtime receipt;
        // do not silently reconstruct a missing/corrupt recovery input.
        let runtime_binding = durable::Stage8bP1ObservedM10Binding::restore_retained_receipt(
            &inputs.durable_parent,
            &inputs.plan.operational_identity_sha256,
            &inputs.plan.decision.receipt_sha256,
        )
        .map_err(|_| O3Error::Input)?;
        expected_bytes(&inputs.plan.decision, &runtime_binding)?;
        state
    } else {
        let retained = if present_private(high_water)? {
            Some(load_exact(high_water, inputs, &inputs.plan.decision)?)
        } else {
            None
        };
        let state =
            publish_retained_or_new(inputs, &inputs.plan.decision, None, retained, &mut schedule)
                .await?;
        retain_stage8b_p1f_observed_published_predecessor(checkpoint, &state)
            .map_err(|_| O3Error::Producer)?;
        state
    };
    // Only two named possibilities are admissible. Never derive source authority
    // from the high-water file or reset on a missing/corrupt retained binding.
    let retained = if let Ok(s) = load_exact(high_water, inputs, &inputs.plan.decision) {
        if s.phase() != Stage8bP1fM10ProducerPhaseV1::Published || s.publication_sequence() != 1 {
            return Err(O3Error::Input);
        }
        None
    } else {
        let s = load_exact(high_water, inputs, &inputs.plan.successor)?;
        if s.publication_sequence() != 2 {
            return Err(O3Error::Input);
        }
        Some(s)
    };
    let successor = publish_retained_or_new(
        inputs,
        &inputs.plan.successor,
        Some(&decision),
        retained,
        &mut schedule,
    )
    .await?;
    restore_stage8b_p1f_observed_published_window(
        checkpoint,
        high_water,
        retained_binding(inputs, &inputs.plan.decision)?,
        retained_binding(inputs, &inputs.plan.successor)?,
        &expected_bytes(&inputs.plan.decision, decision.binding())?,
        &expected_bytes(&inputs.plan.successor, successor.binding())?,
    )
    .map_err(|_| O3Error::Input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn o3_schedule_checkpoint_custody_rejects_missing_links_modes_fifo_and_oversize() {
        let root = std::env::temp_dir().join(format!("o3-checkpoint-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join("schedule.json");
        let uid = unsafe { libc::geteuid() };
        assert!(read_private_checkpoint(&path, uid).is_err());
        fs::write(&path, b"bounded-private-fixture").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            read_private_checkpoint(&path, uid).unwrap(),
            b"bounded-private-fixture"
        );
        assert!(read_private_checkpoint(&path, uid + 1).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(read_private_checkpoint(&path, uid).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o750)).unwrap();
        assert!(read_private_checkpoint(&path, uid).is_err());
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let alias = root.join("alias");
        fs::hard_link(&path, &alias).unwrap();
        assert!(read_private_checkpoint(&path, uid).is_err());
        fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(read_private_checkpoint(&alias, uid).is_err());
        fs::remove_file(&alias).unwrap();
        fs::write(&path, vec![0; 128 * 1024 + 1]).unwrap();
        assert!(read_private_checkpoint(&path, uid).is_err());
        fs::remove_file(&path).unwrap();
        let fifo = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(read_private_checkpoint(&path, uid).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
