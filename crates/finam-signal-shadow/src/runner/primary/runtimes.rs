//! Static, fail-stop consumers of ONE admitted instrument stream. No cache,
//! network client, proof/audit scheduler or independent feed frontier per model.
//! The existing executable still opens one runtime. A multi-consumer cohort is
//! bounded library composition, not aggregate EOD/restart qualification.
use super::*;
use crate::{session::CanonicalPublication, transport::StopHandle, ValidatedConfig};
use serde_json::json;

pub(super) struct Runtimes {
    members: Vec<ObservedSession>,
    #[cfg(test)]
    barrier_fault: Option<(u64, bool)>,
}
impl Runtimes {
    pub fn single(session: ObservedSession) -> Self {
        Self {
            members: vec![session],
            #[cfg(test)]
            barrier_fault: None,
        }
    }
    pub fn group(members: Vec<ObservedSession>) -> Result<Self, CoordinatorError> {
        if members.is_empty() || members.len() > 8 {
            return Err(CoordinatorError::Binding);
        }
        let first = &members[0];
        let (hash, label, _) = first.primary_binding().ok_or(CoordinatorError::Binding)?;
        let (_, root) = first.primary_runtime().ok_or(CoordinatorError::Binding)?;
        if members.iter().any(|s| {
            !s.recoverable_primary()
                || s.primary_binding().map(|(h, t, _)| (h, t)) != Some((hash, label))
                || !s
                    .primary_runtime()
                    .is_some_and(|(_, stop)| root.same_run(&stop))
        }) {
            return Err(CoordinatorError::Binding);
        }
        let mut result = Self {
            members,
            #[cfg(test)]
            barrier_fault: None,
        };
        if result.members.len() > 1 {
            let count = result.members.len();
            for (index, session) in result.members.iter_mut().enumerate() {
                session.record_transport(&json!({"event":"primary_runtime_cohort_bound",
                    "runtime_index":index,"runtime_count":count,
                    "same_config_policy_and_stop":true,"aggregate_eod_and_restart_qualified":false}))
                    .map_err(CoordinatorError::Session)?;
            }
        }
        Ok(result)
    }
    pub fn primary_binding(&self) -> Option<(&str, i64, &crate::primary::PrimaryPolicy)> {
        self.members[0].primary_binding()
    }
    pub fn primary_runtime(&self) -> Option<(&ValidatedConfig, StopHandle)> {
        self.members[0].primary_runtime()
    }
    pub fn recoverable_primary(&self) -> bool {
        self.members
            .iter()
            .all(ObservedSession::recoverable_primary)
    }
    pub fn bind_live_continuity(&mut self, gate: crate::primary::ContinuityGate) {
        for session in &mut self.members {
            session.bind_live_continuity(gate.clone());
        }
    }
    fn stop_group_on_error<T>(&self, result: Result<T, SessionError>) -> Result<T, SessionError> {
        if result.is_err() && self.members.len() > 1 {
            if let Some((_, root)) = self.primary_runtime() {
                root.stop();
            }
        }
        result
    }
    /// Diagnostic writes/finish visit all members even if one journal refuses.
    fn each(
        &mut self,
        mut work: impl FnMut(&mut ObservedSession) -> Result<(), SessionError>,
    ) -> Result<(), SessionError> {
        let mut error = None;
        for session in &mut self.members {
            if let Err(e) = work(session) {
                error.get_or_insert(e);
            }
        }
        self.stop_group_on_error(error.map_or(Ok(()), Err))
    }
    pub fn record_transport(&mut self, event: &impl Serialize) -> Result<(), SessionError> {
        self.each(|s| s.record_transport(event))
    }
    pub fn record_primary_reference(
        &mut self,
        source: &AdmittedClosedM1Snapshot,
    ) -> Result<(), SessionError> {
        self.each(|s| s.record_primary_reference(source))
    }
    fn publish(
        &mut self,
        label: i64,
        close: i64,
        replay_deadline: Option<i64>,
        mut work: impl FnMut(&mut ObservedSession) -> Result<CanonicalPublication, SessionError>,
    ) -> Result<CanonicalPublication, SessionError> {
        let result = (|| {
            let mut publications = Vec::with_capacity(self.members.len());
            for session in &mut self.members {
                publications.push(work(session)?);
            }
            let first = &publications[0];
            if publications
                .iter()
                .any(|p| p.label_ms != label || p.source_sha256 != first.source_sha256)
            {
                return Err(SessionError::Input);
            }
            if self.members.len() > 1 {
                self.record_transport(&json!({"event":"primary_runtime_cohort_barrier",
                    "label_ms":label,"source_sha256":first.source_sha256,"runtime_count":self.members.len(),
                    "consumer_commits":publications.iter().enumerate().map(|(index,p)|json!({
                        "runtime_index":index,"committed_at_ms":p.committed_at_ms})).collect::<Vec<_>>(),
                    "shared_frontier_not_yet_advanced":true,"recovery":replay_deadline.is_some()}))?;
                #[cfg(test)]
                if let Some((delay_ms, stop)) = self.barrier_fault {
                    crate::timing::advance_fixture_clock(delay_ms);
                    if stop {
                        self.primary_runtime().unwrap().1.stop();
                    }
                }
                for session in &mut self.members {
                    session.check_shared_publication(close, replay_deadline)?;
                }
            }
            let mut publication = publications.remove(0);
            if self.members.len() > 1 {
                publication.committed_at_ms = crate::timing::now_ms();
            }
            Ok(publication)
        })();
        self.stop_group_on_error(result)
    }
    pub fn canonical(
        &mut self,
        input: &crate::primary::AdmittedCanonicalM10,
    ) -> Result<CanonicalPublication, SessionError> {
        self.publish(
            input.receipt().label_ms,
            input.receipt().close_ms,
            None,
            |s| s.canonical(input),
        )
    }
    pub fn recovery(
        &mut self,
        input: &crate::primary::RecoveryInput,
        source: &AdmittedClosedM1Snapshot,
    ) -> Result<CanonicalPublication, SessionError> {
        self.publish(
            input.receipt().canonical.label_ms,
            input.receipt().canonical.close_ms,
            Some(input.receipt().replay_deadline_ms),
            |s| s.recovery(input, source),
        )
    }
    pub fn finish(&mut self, reason: &str) -> Result<(), SessionError> {
        self.each(|s| s.finish(reason))
    }
    pub fn ws_recovery(
        &mut self,
        input: &crate::primary::WsRecoveryInput,
    ) -> Result<CanonicalPublication, SessionError> {
        self.publish(
            input.receipt().canonical.label_ms,
            input.receipt().canonical.close_ms,
            Some(input.receipt().replay_deadline_ms),
            |s| s.ws_recovery(input),
        )
    }
    /// A single-journal Completion cannot attest all independent cohort files.
    /// Fail closed rather than present a primary consumer's EOD as cohort PASS.
    pub fn eod_model_ready(&self) -> bool {
        self.members.len() == 1 && self.members[0].eod_model_ready()
    }
    pub fn eod_snapshot(&self) -> serde_json::Value {
        let mut value = self.members[0].eod_snapshot();
        if self.members.len() > 1 {
            value["runtime_cohort"] = json!(self
                .members
                .iter()
                .map(ObservedSession::eod_snapshot)
                .collect::<Vec<_>>());
        }
        value
    }
    pub fn journal_seal(&self) -> Option<crate::primary::eod::JournalSeal> {
        if self.members.len() == 1 {
            self.members[0].journal_seal()
        } else {
            None
        }
    }
    #[cfg(test)]
    pub fn fail_primary_evidence(&mut self, kind: &'static str) {
        self.members[0].fail_primary_evidence(kind);
    }
}

#[cfg(test)]
pub(crate) mod tests;
