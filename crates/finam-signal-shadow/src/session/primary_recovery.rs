//! Continue the SAME owned model/book through source-bound historical inputs.
//! Real IO/replay times remain separate from the declared virtual close clock.
use super::*;
#[cfg(test)]
mod tail_tests;
impl ObservedSession {
    #[cfg(test)]
    pub(crate) fn fail_primary_evidence(&mut self, kind: &'static str) {
        self.observer.fail_on_kind(kind);
    }
    pub(crate) fn recoverable_primary(&self) -> bool {
        !self.closed
            && !self.stop.is_stopped()
            && !self.model.held()
            && self.model.pending().is_none()
            && self.config.primary_policy().is_some()
            && self.config.candle_only()
    }
    pub(crate) fn recovery(
        &mut self,
        input: &crate::primary::RecoveryInput,
        source: &AdmittedClosedM1Snapshot,
    ) -> Result<CanonicalPublication, SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        match self.recovery_inner(input, source) {
            Ok(result) => Ok(result),
            Err(error) => self.fail(error),
        }
    }
    fn recovery_inner(
        &mut self,
        input: &crate::primary::RecoveryInput,
        source: &AdmittedClosedM1Snapshot,
    ) -> Result<CanonicalPublication, SessionError> {
        self.check()?;
        self.fence.check_now().map_err(|_| SessionError::Timing)?;
        let receipt = input.receipt();
        let canonical = &receipt.canonical;
        let policy = self.config.primary_policy().ok_or(SessionError::Input)?;
        let check = |now: i64| -> Result<(), SessionError> {
            if now < canonical.canonical_ready_at_ms || now > receipt.replay_deadline_ms {
                return Err(SessionError::Timing);
            }
            Ok(())
        };
        check(crate::timing::now_ms())?;
        if !self.recoverable_primary()
            || canonical.config_sha256 != self.config.fingerprint()
            || canonical.policy_sha256 != policy.fingerprint().map_err(|_| SessionError::Input)?
            || canonical.origin != "RECOVERY_BACKFILL"
            || canonical.label_ms != self.next_label_ms
            || input.model_bar().origin != HybridRuntimeBarOrigin::Replay
            || canonical.canonical_digest
                != crate::input::canonical_digest(input.model_bar())
                    .map_err(|_| SessionError::Input)?
            || canonical.rest_snapshot_sha256.as_deref() != Some(source.sha256())
            || receipt.virtual_callback_at_ms != canonical.close_ms
            || receipt.actual_received_at_ms > canonical.canonical_ready_at_ms
        {
            return Err(SessionError::Input);
        }
        let hash = self
            .observer
            .record_recovery_source(input, source)
            .map_err(|_| SessionError::Evidence)?;
        let actual_start = crate::timing::now_ms();
        let before = self.model.observation();
        self.model.select_backfill();
        let intent = self
            .model
            .on_bar(input.model_bar().clone(), receipt.virtual_callback_at_ms)
            .map_err(|_| SessionError::Model)?;
        let after = self.model.observation();
        let decision = Decision::from_model_observation(
            intent.as_ref().map(|r| r.action),
            intent.as_ref().map_or(0, |r| r.quantity_lots),
            &after,
        )
        .map_err(|_| SessionError::Model)?;
        self.observer
            .record_step(&Step {
                mode: ObservationMode::BackfillReplay,
                instrument: input.model_bar().instrument.symbol.clone(),
                model_label_utc_ms: canonical.label_ms,
                nominal_close_utc_ms: canonical.close_ms,
                provider_at_ms: None,
                received_at_ms: canonical.close_ms,
                canonical_ready_at_ms: canonical.close_ms,
                model_start_at_ms: canonical.close_ms,
                result_at_ms: canonical.close_ms,
                canonical_input_sha256: canonical.canonical_digest.clone(),
                raw_provenance_sha256: hash.clone(),
                before,
                after,
                decision,
            })
            .map_err(|_| SessionError::Evidence)?;
        if let Some(intent) = intent {
            self.fence.check_now().map_err(|_| SessionError::Timing)?;
            let fill = self
                .model
                .on_candle_close(&intent, canonical.close_ms, canonical.close_ms, &hash)
                .map_err(|_| SessionError::Model)?;
            self.observer
                .record_candle_feedback(
                    ObservationMode::BackfillReplay,
                    &fill,
                    &self.model.observation(),
                )
                .map_err(|_| SessionError::Evidence)?;
        }
        if self.model.pending().is_some() {
            return Err(SessionError::Incomplete);
        }
        self.fence.check_now().map_err(|_| SessionError::Timing)?;
        let actual_result = crate::timing::now_ms();
        check(actual_result)?;
        self.observer.record_transport(&serde_json::json!({"event":"recovery_step_durable_publication","origin":"RECOVERY_BACKFILL",
            "mode":ObservationMode::BackfillReplay,"model_label_utc_ms":canonical.label_ms,"virtual_callback_at_ms":canonical.close_ms,
            "actual_model_start_at_ms":actual_start,"actual_model_result_at_ms":actual_result,
            "actual_source_received_at_ms":receipt.actual_received_at_ms,"source_sha256":hash,"history_proves_live_readiness":false})).map_err(|_|SessionError::Evidence)?;
        let published = crate::timing::now_ms();
        self.observer
            .record_canonical_commit(canonical.label_ms, &hash, actual_result, published)
            .map_err(|_| SessionError::Evidence)?;
        self.fence.check_now().map_err(|_| SessionError::Timing)?;
        let committed = crate::timing::now_ms();
        check(committed)?;
        self.model.select_observed();
        self.next_label_ms = next_label(canonical.label_ms).ok_or(SessionError::Input)?;
        Ok(CanonicalPublication {
            label_ms: canonical.label_ms,
            source_sha256: hash,
            committed_at_ms: committed,
            model_result_at_ms: actual_result,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        primary::{CanonicalStatus, PrimaryProcessor},
        timing::{advance_fixture_clock, with_fixture_clock},
    };
    use broker_finam::sparse_m10::{
        ClosedM1RequestPlan, ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1, OBSERVED_M1_POLICY_V1,
    };
    #[test]
    fn historical_repair_preserves_real_long_and_short_books_without_reentry() {
        for side in [-1, 1] {
            let (model, last) = crate::model::candle_tests::nonflat(side);
            let first = (last.close_time_utc + 600) * 1000;
            let now = first + 2 * 600000 + 100;
            with_fixture_clock(now, || {
                let (mut session, config, _, path) =
                    crate::session::tests::primary_fixture_mode(first, false);
                session.model = model;
                let live_gate = crate::primary::ContinuityGate::new(1);
                live_gate.fault(crate::socket::primary::Failure {
                    generation: 1,
                    kind: crate::socket::primary::Kind::Close,
                    liveness: None,
                });
                session.bind_live_continuity(live_gate.clone());
                let before = session.model.observation();
                assert_eq!(session.model.position_lots(), side);
                let mut processor = PrimaryProcessor::from_fixture(
                    &config,
                    last.instrument.clone(),
                    vec![first, first + 600000, first + 1200000],
                    1,
                    now,
                )
                .unwrap();
                processor.break_continuity();
                let plan = processor.plan_recovery(first + 1200000, now).unwrap();
                let rows:Vec<_>=(0..20).map(|i|serde_json::json!({"timestamp":DateTime::<Utc>::from_timestamp_millis(first+i*60000).unwrap().to_rfc3339(),
                    "open":{"value":last.close.to_string()},"high":{"value":last.close.to_string()},"low":{"value":last.close.to_string()},"close":{"value":last.close.to_string()},"volume":{"value":"2"}})).collect();
                let request = plan.request();
                let start = request.start();
                let end = request.end();
                let stamp = DateTime::<Utc>::from_timestamp_millis(now).unwrap();
                let body = serde_json::json!({"symbol":"IMOEXF@RTSX","bars":rows}).to_string();
                let source = AdmittedClosedM1Snapshot::admit(
                    ClosedM1SnapshotEvidenceV1 {
                        policy: OBSERVED_M1_POLICY_V1.into(),
                        start,
                        end,
                        parts: vec![ClosedM1RestPartV1 {
                            method: "GET".into(),
                            endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
                            symbol: "IMOEXF@RTSX".into(),
                            timeframe: "TIME_FRAME_M1".into(),
                            start,
                            end,
                            requested_at: stamp,
                            received_at: stamp,
                            status: 200,
                            transport_complete: true,
                            declared_body_bytes: Some(body.len() as u64),
                            response_sha256: crate::seed::digest(body.as_bytes()),
                            raw_body: body,
                        }],
                    },
                    &ClosedM1RequestPlan::new(start, end, vec![end]).unwrap(),
                    stamp,
                )
                .unwrap();
                let admitted = processor.admit_recovery(&plan, &source, now).unwrap();
                for input in admitted.inputs {
                    let publication = session.recovery(&input, &source).unwrap();
                    processor
                        .recovered(input, publication.committed_at_ms)
                        .unwrap();
                    advance_fixture_clock(1);
                }
                assert_eq!(session.model.position_lots(), side);
                assert!(session.model.pending().is_none());
                let text = std::fs::read_to_string(path).unwrap();
                let events: Vec<serde_json::Value> = text
                    .lines()
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect();
                let steps: Vec<_> = events
                    .iter()
                    .filter(|r| r["kind"] == "model_step")
                    .collect();
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0]["event"]["before"], before);
                assert!(steps
                    .iter()
                    .all(|r| r["event"]["decision"]["action"].is_null()));
                assert!(!events
                    .iter()
                    .any(|r| r["kind"] == "hypothetical_candle_feedback"));
                assert!(processor.gap_unresolved());
                assert!(!live_gate.healthy());
                assert!(matches!(
                    processor.prepare(crate::timing::now_ms()).unwrap(),
                    CanonicalStatus::Waiting | CanonicalStatus::PendingProof { .. }
                ));
            });
        }
    }
}
