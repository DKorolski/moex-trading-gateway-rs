//! Explicit, single-segment read-only source runner. No auth discovery/renewal,
//! automatic retry, order route, recovery or trade switch.
//!
//! Historical-prefix startup requires a source-admitted reconstruction capability.
//! BARS are provisional and never advance the model; independently scheduled
//! REST snapshots supply finality. This slice stops at an explicit EOD check;
//! it does not claim multi-session EOD acceptance. Reconnect is one explicitly
//! planned, healthy-session transition, never recovery from transport failure.
//! Coverage includes the 23:40-labelled M10, closed at 23:50, before the
//! explicit EOD allowance expires (strictly before midnight). Local coverage
//! checks are not actual two-session observed qualification or full S8/S9.
//! Raw quote frames are retained before feedback. This is not proof of a complete
//! restart inventory or collected operational qualification across downtime.
use crate::{
    segment::SeededSegment, session::ObservedSession, transport::ConfiguredTransports,
    ws::Observation, ValidatedConfig,
};
use broker_finam::{
    sparse_m10::{AdmittedClosedM1Snapshot, ClosedM1RequestPlan},
    AccessTokenLease, Stage8bP1fO2GetOnlyClientV1,
};
use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};
use tokio::{sync::mpsc, task::JoinSet};

/// All fields mandatory; input file contains no credential. The read-only-role
/// assertion is operator supplied, not proof of the provider's token permissions.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPlan {
    pub config_sha256: String,
    pub account_id: String,
    pub read_only_credentials: bool,
    pub token_expires_at: DateTime<Utc>,
    pub eod_check_at: DateTime<Utc>,
    pub reconnect_after_data_frames: Option<u64>,
    pub requests: Vec<Request>,
}

/// Separately trusted operator plan, NOT generated from the supplied prefix.
/// Digest pinning prevents a replaced prefix/binding from silently authorizing
/// itself. This is an operator assertion, not provider-authenticated continuity.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrefixRunPlan {
    pub plan: RunPlan,
    pub prefix_binding_sha256: String,
    pub source_coverage: PrefixSourceCoverage,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrefixSourceCoverage {
    Unproven,
    OperatorApprovedPinnedPriorOutputInventory { inventory_sha256: String },
}

impl PrefixRunPlan {
    pub fn validate(
        &self,
        prefix: &crate::prefix::ReconstructedPrefix,
    ) -> Result<(), &'static str> {
        if self.prefix_binding_sha256 != prefix.binding_sha256() {
            return Err("runner_prefix_receipt_binding");
        }
        match &self.source_coverage {
            PrefixSourceCoverage::OperatorApprovedPinnedPriorOutputInventory {
                inventory_sha256,
            } if inventory_sha256 == &prefix.binding().prior_output_inventory_sha256 => {}
            _ => return Err("runner_prefix_coverage_unproven"),
        }
        self.plan.validate_prefix(prefix)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub request_at: DateTime<Utc>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub part_ends: Vec<DateTime<Utc>>,
    pub required: Vec<DateTime<Utc>>,
}

fn next_label(label: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let local = label.checked_add_signed(chrono::Duration::hours(3))?;
    if local.hour() == 23 && local.minute() == 40 {
        let mut date = local.date_naive().succ_opt()?;
        while date.weekday().number_from_monday() > 5 {
            date = date.succ_opt()?;
        }
        date.and_hms_opt(7, 0, 0)?
            .and_utc()
            .checked_sub_signed(chrono::Duration::hours(3))
    } else {
        label.checked_add_signed(chrono::Duration::minutes(10))
    }
}

impl RunPlan {
    pub fn validate(&self, config: &ValidatedConfig) -> Result<(), &'static str> {
        self.validate_from(config, config.config().observation_start_utc_ms)
    }

    pub fn validate_prefix(
        &self,
        prefix: &crate::prefix::ReconstructedPrefix,
    ) -> Result<(), &'static str> {
        self.validate_from(prefix.config(), prefix.next_label_utc_ms())
    }

    fn validate_from(
        &self,
        config: &ValidatedConfig,
        next_label_ms: i64,
    ) -> Result<(), &'static str> {
        let c = config.config();
        let cutoff = self.eod_check_at.timestamp_millis();
        if self.config_sha256 != config.fingerprint()
            || !self.read_only_credentials
            || self.account_id.is_empty()
            || self.requests.is_empty()
            || self.requests.len() as u64 > c.max_cache_bars
            || cutoff > c.run_deadline_utc_ms
            || cutoff <= c.observation_start_utc_ms
            || self.token_expires_at <= self.eod_check_at
        {
            return Err("runner_plan_binding");
        }
        if self.reconnect_after_data_frames == Some(0) {
            return Err("runner_reconnect_plan");
        }
        // The last model label is 23:40, not its 23:50 nominal close. Leave
        // explicit time for final admission/feedback, but never cross midnight.
        let local = self.eod_check_at + chrono::Duration::hours(3);
        let eod = local
            .date_naive()
            .and_hms_opt(23, 50, 0)
            .ok_or("runner_eod")?
            .and_utc()
            - chrono::Duration::hours(3);
        let lag = (self.eod_check_at - eod).num_milliseconds();
        if lag <= 0 || lag >= 600_000 || lag as u64 > c.lmax_ms {
            return Err("runner_eod");
        }
        let mut expected =
            DateTime::from_timestamp_millis(next_label_ms).ok_or("runner_plan_time")?;
        let mut previous_request = None;
        for request in &self.requests {
            let plan =
                ClosedM1RequestPlan::new(request.start, request.end, request.part_ends.clone())
                    .map_err(|_| "runner_closed_range")?;
            // One nominal bucket per request: no historical batch presented as live.
            let close = expected
                .checked_add_signed(chrono::Duration::minutes(10))
                .ok_or("runner_plan_time")?;
            if request.required != [expected]
                || plan.start() != expected
                || plan.end() != close
                || request.request_at < close
                || request.request_at >= self.eod_check_at
                || ((request.request_at - close).num_milliseconds() as u64)
                    .checked_add(c.clock_uncertainty_ms)
                    .map_or(true, |lag| lag >= c.lmax_ms)
                || previous_request.is_some_and(|p| request.request_at <= p)
            {
                return Err("runner_required_schedule");
            }
            previous_request = Some(request.request_at);
            expected = next_label(expected).ok_or("runner_plan_time")?;
        }
        if expected.timestamp_millis() + 600_000 <= cutoff {
            return Err("runner_missing_prefix");
        }
        Ok(())
    }
}

enum Event {
    Snapshot(Box<AdmittedClosedM1Snapshot>, Vec<DateTime<Utc>>),
    Socket(Observation),
    Reconnected(i64, u64),
}

fn count_reconnect_frame(
    planned: Option<u64>,
    frames: &mut u64,
    consumed: &mut bool,
    nonempty: bool,
) -> bool {
    if *consumed || !nonempty {
        return false;
    }
    *frames = frames.saturating_add(1);
    if planned.is_some_and(|count| count > 0 && *frames >= count) {
        *consumed = true;
        return true;
    }
    false
}

async fn wait_until(at: DateTime<Utc>) {
    if let Ok(delay) = (at - Utc::now()).to_std() {
        tokio::time::sleep(delay).await;
    }
}

/// Workers keep pending observe futures alive across ALL coordinator selections.
/// Cancellation occurs only on terminal shutdown. Bounded queue applies pressure;
/// no quote or canonical input is silently dropped.
pub async fn run(
    segment: SeededSegment,
    config: &ValidatedConfig,
    plan: RunPlan,
    lease: AccessTokenLease,
    output: &Path,
) -> Result<(), &'static str> {
    run_start(
        Start::Seed(Box::new(segment)),
        config,
        plan,
        lease,
        Some(output),
        None,
    )
    .await
}

/// Explicit prefix path; no conversion from arbitrary OfflineReplay is possible.
/// Reuses the prefix output so the binding/receipt precede every live row.
pub async fn run_prefix(
    prefix: crate::prefix::ReconstructedPrefix,
    config: &ValidatedConfig,
    plan: PrefixRunPlan,
    lease: AccessTokenLease,
) -> Result<(), &'static str> {
    plan.validate(&prefix)?;
    run_start(
        Start::Prefix(Box::new(prefix)),
        config,
        plan.plan.clone(),
        lease,
        None,
        Some(plan),
    )
    .await
}

enum Start {
    Seed(Box<SeededSegment>),
    Prefix(Box<crate::prefix::ReconstructedPrefix>),
}

async fn run_start(
    start: Start,
    config: &ValidatedConfig,
    plan: RunPlan,
    lease: AccessTokenLease,
    output: Option<&Path>,
    prefix_plan: Option<PrefixRunPlan>,
) -> Result<(), &'static str> {
    let prefix_session = matches!(&start, Start::Prefix(_));
    let fingerprint = match &start {
        Start::Seed(segment) => {
            plan.validate(config)?;
            segment.seed_observation().config_sha256
        }
        Start::Prefix(prefix) => {
            plan.validate_prefix(prefix)?;
            prefix.config().fingerprint().to_owned()
        }
    };
    if fingerprint != config.fingerprint() {
        return Err("runner_segment_binding");
    }
    if lease.token().is_empty() || lease.should_refresh(std::time::Instant::now()) {
        return Err("runner_credential_expired");
    }
    let mut transports =
        ConfiguredTransports::new(config).map_err(|_| "runner_transport_config")?;
    let stop = transports.stop_handle();
    let mut session = match start {
        Start::Seed(segment) => {
            ObservedSession::create(*segment, output.ok_or("runner_output")?, stop.clone())
        }
        Start::Prefix(prefix) => ObservedSession::resume(
            *prefix,
            stop.clone(),
            prefix_plan.as_ref().ok_or("runner_prefix_plan")?,
        ),
    }
    .map_err(|_| "runner_session_create")?;
    session
        .record_run_plan(&plan)
        .map_err(|_| "runner_plan_receipt")?;
    let mut jobs = JoinSet::new();
    let (tx, mut rx) = mpsc::channel(1);
    let end = plan.eod_check_at;
    let reconnect_after = plan.reconnect_after_data_frames;
    let reconnect_budget = config.config().reconnect_budget_ms;
    let mut bars_reconnected = false;
    let work = async {
        let client =
            Stage8bP1fO2GetOnlyClientV1::new(plan.account_id).map_err(|_| "runner_client")?;
        let mut bars = transports
            .connect_bars(lease.clone())
            .await
            .map_err(|_| "runner_bars_connect")?;
        let sender = tx.clone();
        let bars_lease = lease.clone();
        jobs.spawn(async move {
            let (mut frames, mut consumed) = (0, false);
            loop {
                let observation = bars.observe().await.map_err(|_| "runner_bars_observe")?;
                let reconnect = count_reconnect_frame(
                    reconnect_after,
                    &mut frames,
                    &mut consumed,
                    matches!(&observation, Observation::ProvisionalBars(bars) if !bars.is_empty()),
                );
                sender
                    .send(Event::Socket(observation))
                    .await
                    .map_err(|_| "runner_queue")?;
                if reconnect {
                    let started = std::time::Instant::now();
                    bars = bars
                        .reconnect(bars_lease.clone())
                        .await
                        .map_err(|_| "runner_bars_reconnect")?;
                    // Budget includes connection establishment, not just its start.
                    if started.elapsed() > Duration::from_millis(reconnect_budget) {
                        return Err("runner_reconnect_budget");
                    }
                    sender
                        .send(Event::Reconnected(Utc::now().timestamp_millis(), frames))
                        .await
                        .map_err(|_| "runner_queue")?;
                }
            }
            #[allow(unreachable_code)]
            Ok::<(), &'static str>(())
        });
        let mut quotes = transports
            .connect_quotes(lease.clone())
            .await
            .map_err(|_| "runner_quotes_connect")?;
        let sender = tx.clone();
        jobs.spawn(async move {
            loop {
                let observation = quotes
                    .observe()
                    .await
                    .map_err(|_| "runner_quotes_observe")?;
                sender
                    .send(Event::Socket(observation))
                    .await
                    .map_err(|_| "runner_queue")?;
            }
            #[allow(unreachable_code)]
            Ok::<(), &'static str>(())
        });
        jobs.spawn(async move {
            for request in plan.requests {
                wait_until(request.request_at).await;
                if lease.should_refresh(std::time::Instant::now()) {
                    return Err("runner_credential_expired");
                }
                let plan = ClosedM1RequestPlan::new(request.start, request.end, request.part_ends)
                    .map_err(|_| "runner_closed_range")?;
                plan.preflight(Utc::now()).map_err(|_| "runner_preflight")?;
                let snapshot = transports
                    .collect(&client, lease.token(), &plan)
                    .await
                    .map_err(|_| "runner_rest")?;
                tx.send(Event::Snapshot(Box::new(snapshot), request.required))
                    .await
                    .map_err(|_| "runner_queue")?;
            }
            // ConfiguredTransports owns shared stop: retain it until shutdown.
            std::future::pending::<()>().await;
            drop(transports);
            Ok(())
        });
        loop {
            tokio::select! {
                biased;
                result = jobs.join_next() => return Err(match result {
                    Some(Ok(Err(code))) => code, _ => "runner_worker_ended",
                }),
                event = rx.recv() => {
                    if stop.is_stopped() { return Err("runner_stopped"); }
                    match event.ok_or("runner_queue_closed")? {
                        Event::Reconnected(at, frames) => {
                            session.record_transport(&serde_json::json!({
                                "channel":"BARS", "event":"controlled_reconnect_socket_open",
                                "at_ms":at, "after_data_frames":frames,
                                "plan_consumed":true, "subscription_proven":false
                            })).map_err(|_| "runner_transport_receipt")?;
                            bars_reconnected = true;
                        }
                        Event::Snapshot(snapshot, required) => {
                            let received = snapshot.evidence().parts.iter()
                                .map(|p| p.received_at.timestamp_millis()).max().ok_or("runner_receive_time")?;
                            session.snapshot(&snapshot, &required, received).map_err(|_| "runner_snapshot")?;
                        }
                        Event::Socket(Observation::Quotes(frame)) => {
                            session.quote_frame(frame).map_err(|_| "runner_quote")?;
                        },
                        // Provisional WS candles, handshake and heartbeat are not finality/readiness.
                        Event::Socket(_) => {},
                    }
                }
            }
        }
    };
    let mut result = tokio::select! {
        biased;
        _ = wait_until(end) => Ok(()),
        signal = tokio::signal::ctrl_c() => Err(if signal.is_ok() { "runner_interrupted" } else { "runner_signal" }),
        result = work => result,
    };
    if result.is_ok() && reconnect_after.is_some() && !bars_reconnected {
        result = Err("runner_reconnect_incomplete");
    }
    stop.stop();
    jobs.abort_all();
    let drained = tokio::time::timeout(
        Duration::from_millis(config.config().stop_timeout_ms),
        async {
            while let Some(ended) = jobs.join_next().await {
                match ended {
                    Ok(Err(code)) => result = Err(code),
                    Err(error) if !error.is_cancelled() => result = Err("runner_worker_panicked"),
                    _ => {}
                }
            }
        },
    )
    .await
    .is_ok();
    if !drained {
        result = Err("runner_stop_timeout");
    }
    if session
        .record_transport(&serde_json::json!({
            "event":"runner_end", "at_ms":Utc::now().timestamp_millis(),
            "bars_reconnect_planned":reconnect_after.is_some(),
            "bars_reconnect_completion_observed":bars_reconnected,
            "status":result.as_ref().err().copied().unwrap_or("eod_check_end"),
            "coverage":"nominal_tail_checked_not_parity_acceptance",
            "prefix_session_partial_live_coverage":prefix_session
        }))
        .is_err()
        && result.is_ok()
    {
        result = Err("runner_transport_receipt");
    }
    let finished = session.finish(if result.is_ok() {
        "EOD_CHECK_END_NOT_PARITY_ACCEPTANCE"
    } else {
        "INCOMPLETE_runner_failure"
    });
    if !drained {
        return Err("runner_stop_timeout");
    }
    result?;
    finished.map_err(|_| "runner_session_incomplete")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ValidatedConfig, RunPlan) {
        let start = DateTime::parse_from_rfc3339("2026-10-23T07:00:00+03:00")
            .unwrap()
            .with_timezone(&Utc);
        let eod = start + chrono::Duration::hours(16) + chrono::Duration::minutes(50);
        let config = ValidatedConfig::parse(&serde_json::to_vec(&serde_json::json!({
            "schema_version":1, "reference":"alor_freeze_2026_09_27", "restart_mode":"new_segment",
            "model_profile":"imoexf-baseline07-bo-only-no-riskgate",
            "model_sha256":"a".repeat(64), "reference_sha256":"b".repeat(64),
            "feedback_policy_sha256":"c".repeat(64), "instrument_mapping_sha256":"d".repeat(64),
            "calendar_sha256":"e".repeat(64), "seed_sha256":"f".repeat(64),
            "instrument":"IMOEXF@RTSX", "run_id":"test", "segment_id":"s1", "previous_segment_id":null,
            "warmup_session_dates":["2026-10-19","2026-10-20","2026-10-21","2026-10-22"],
            "warmup_range_start_utc_ms":1, "warmup_range_end_utc_ms":2,
            "observation_start_utc_ms":start.timestamp_millis(),
            "run_deadline_utc_ms":(eod + chrono::Duration::seconds(20)).timestamp_millis(),
            "lmax_ms":10000, "clock_uncertainty_ms":10, "max_quote_age_ms":1000,
            "ws_silence_budget_ms":15000, "reconnect_budget_ms":5000, "stop_timeout_ms":1000,
            "max_input_bytes":1048576, "max_cache_bars":4096, "max_output_bytes":1048576
        })).unwrap()).unwrap();
        let mut requests = Vec::new();
        let mut label = start;
        while label < eod {
            let close = label + chrono::Duration::minutes(10);
            requests.push(Request {
                request_at: close + chrono::Duration::milliseconds(100),
                start: label,
                end: close,
                part_ends: vec![close],
                required: vec![label],
            });
            label = close;
        }
        let plan = RunPlan {
            config_sha256: config.fingerprint().into(),
            account_id: "synthetic".into(),
            read_only_credentials: true,
            token_expires_at: eod + chrono::Duration::hours(1),
            eod_check_at: eod + chrono::Duration::seconds(5),
            reconnect_after_data_frames: Some(2),
            requests,
        };
        (config, plan)
    }

    #[test]
    fn plan_binds_config_and_roundtrips_without_credential() {
        let (config, mut plan) = fixture();
        assert_eq!(plan.validate(&config), Ok(()));
        let encoded = serde_json::to_vec(&plan).unwrap();
        let decoded: RunPlan = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.validate(&config), Ok(()));
        plan.config_sha256 = "0".repeat(64);
        assert_eq!(plan.validate(&config), Err("runner_plan_binding"));
    }

    #[test]
    fn missing_prefix_or_due_tail_and_bad_latency_are_refused() {
        let (config, mut plan) = fixture();
        plan.requests.remove(0);
        assert_eq!(plan.validate(&config), Err("runner_required_schedule"));
        let (config, mut plan) = fixture();
        plan.requests.pop();
        assert_eq!(plan.validate(&config), Err("runner_missing_prefix"));
        let (config, mut plan) = fixture();
        plan.requests[0].request_at = plan.requests[0].end + chrono::Duration::milliseconds(9990);
        assert_eq!(plan.validate(&config), Err("runner_required_schedule"));
    }

    #[test]
    fn full_eod_plan_requires_last_2340_label_closed_at_2350() {
        let (config, mut plan) = fixture();
        assert_eq!(plan.requests.len(), 101);
        let last = plan.requests.last().unwrap();
        assert_eq!(last.start.to_rfc3339(), "2026-10-23T20:40:00+00:00");
        assert_eq!(last.end.to_rfc3339(), "2026-10-23T20:50:00+00:00");
        assert!(last.request_at < plan.eod_check_at);
        assert_eq!(plan.validate(&config), Ok(()));
        plan.requests.pop();
        assert_eq!(plan.validate(&config), Err("runner_missing_prefix"));
        // The former cutoff cannot silently accept a missing final label.
        plan.eod_check_at -= chrono::Duration::minutes(10);
        assert_eq!(plan.validate(&config), Err("runner_eod"));
    }

    #[test]
    fn eod_allowance_is_positive_lmax_bounded_and_before_midnight() {
        for millis in [0, 10_001, 600_000] {
            let (config, mut plan) = fixture();
            let close = plan.requests.last().unwrap().end;
            plan.eod_check_at = close + chrono::Duration::milliseconds(millis);
            let mut c = config.config().clone();
            // Isolate EOD validation from the independent run-deadline bound.
            c.run_deadline_utc_ms = (close + chrono::Duration::hours(1)).timestamp_millis();
            let config = ValidatedConfig::validate(c).unwrap();
            plan.config_sha256 = config.fingerprint().into();
            assert_eq!(plan.validate(&config), Err("runner_eod"));
        }
        let (config, mut plan) = fixture();
        plan.eod_check_at =
            plan.requests.last().unwrap().end + chrono::Duration::milliseconds(10_000);
        assert_eq!(plan.validate(&config), Ok(()));
    }

    #[test]
    fn controlled_reconnect_counts_only_data_and_consumes_once() {
        let (mut frames, mut consumed) = (0, false);
        assert!(!count_reconnect_frame(
            Some(2),
            &mut frames,
            &mut consumed,
            false
        ));
        assert_eq!(frames, 0);
        assert!(!count_reconnect_frame(
            Some(2),
            &mut frames,
            &mut consumed,
            true
        ));
        assert!(count_reconnect_frame(
            Some(2),
            &mut frames,
            &mut consumed,
            true
        ));
        assert!(consumed);
        assert!(!count_reconnect_frame(
            Some(2),
            &mut frames,
            &mut consumed,
            true
        ));
        let (mut frames, mut consumed) = (0, false);
        assert!(!count_reconnect_frame(
            None,
            &mut frames,
            &mut consumed,
            true
        ));
        assert!(!consumed);
        let (config, mut plan) = fixture();
        plan.reconnect_after_data_frames = Some(0);
        assert_eq!(plan.validate(&config), Err("runner_reconnect_plan"));
    }
    #[test]
    fn schedule_matches_session_weekend_boundary() {
        let last = DateTime::parse_from_rfc3339("2026-10-23T23:40:00+03:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            next_label(last).unwrap().to_rfc3339(),
            "2026-10-26T04:00:00+00:00"
        );
    }
    #[tokio::test]
    async fn worker_pending_read_survives_coordinator_timer() {
        let (tx, mut rx) = mpsc::channel(1);
        let (release, ready) = tokio::sync::oneshot::channel();
        let worker = tokio::spawn(async move {
            ready.await.unwrap();
            tx.send(7).await.unwrap();
        });
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(1)) => {},
            _ = rx.recv() => panic!("premature event"),
        }
        release.send(()).unwrap();
        assert_eq!(rx.recv().await, Some(7));
        worker.await.unwrap();
    }
}
