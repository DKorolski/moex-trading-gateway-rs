//! Synchronous cross-task fence. A generation fault is visible to the canonical
//! writer/callback fence before reconnect can await. Old sockets cannot poison
//! a newer generation; replacing one never resets the operator STOP latch.
use crate::socket::primary::Failure;
use std::sync::{Arc, Mutex};
#[derive(Clone)]
pub(crate) struct ContinuityGate(Arc<Mutex<State>>);
struct State {
    generation: u64,
    revision: u64,
    lost: bool,
    failure: Option<Failure>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::socket::primary::Kind;
    fn failure(generation: u64) -> Failure {
        Failure {
            generation,
            kind: Kind::Close,
            liveness: None,
        }
    }
    #[test]
    fn a_new_failure_invalidates_resolution_but_old_generation_cannot_poison_new() {
        let gate = ContinuityGate::new(1);
        gate.fault(failure(1));
        assert!(!gate.healthy());
        let revision = gate.begin_generation(2).unwrap();
        assert!(!gate.resolve(2, revision - 1));
        gate.fault(failure(1));
        assert_eq!(gate.snapshot().revision, revision);
        assert!(gate.resolve(2, revision));
        gate.fault(failure(1));
        assert!(gate.healthy());
        gate.fault(failure(2));
        assert!(!gate.healthy());
        assert!(!gate.resolve(2, revision));
    }
    #[test]
    fn opening_a_new_generation_is_not_itself_continuity_resolution() {
        let gate = ContinuityGate::new(1);
        assert!(gate.begin_generation(2).is_none());
        gate.fault(failure(1));
        let revision = gate.begin_generation(2).unwrap();
        assert!(!gate.healthy());
        assert!(gate.snapshot().failure.is_none());
        assert!(!gate.resolve(1, revision));
        assert!(gate.resolve(2, revision));
        assert!(!gate.resolve(2, revision));
    }
    #[test]
    fn data_gap_fences_callbacks_without_hiding_a_later_physical_fault() {
        let gate = ContinuityGate::new(1);
        assert!(!gate.data_gap(2));
        assert!(gate.data_gap(1));
        let revision = gate.snapshot().revision;
        assert!(!gate.healthy());
        assert!(gate.snapshot().failure.is_none());
        assert!(!gate.data_gap(1));
        gate.fault(failure(1));
        assert!(gate.snapshot().failure.is_some());
        assert!(gate.snapshot().revision > revision);
        assert!(!gate.resolve(1, revision));
        assert!(!gate.resolve(1, gate.snapshot().revision));
        assert!(!gate.data_gap(1));
    }
}
#[derive(Clone)]
pub(crate) struct Snapshot {
    pub generation: u64,
    pub revision: u64,
    pub lost: bool,
    pub failure: Option<Failure>,
}
impl ContinuityGate {
    pub fn new(generation: u64) -> Self {
        Self(Arc::new(Mutex::new(State {
            generation,
            revision: 0,
            lost: false,
            failure: None,
        })))
    }
    pub fn snapshot(&self) -> Snapshot {
        let s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        Snapshot {
            generation: s.generation,
            revision: s.revision,
            lost: s.lost,
            failure: s.failure.clone(),
        }
    }
    pub fn fault(&self, failure: Failure) {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if failure.generation == s.generation && s.failure.is_none() {
            s.revision += 1;
            s.lost = true;
            s.failure = Some(failure);
        }
    }
    /// Canonical uncertainty fences callbacks, but does NOT occupy the
    /// transport failure slot. A real network fault can still latch atomically.
    pub(crate) fn data_gap(&self, generation: u64) -> bool {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if s.generation != generation || s.failure.is_some() || s.lost {
            return false;
        }
        s.lost = true;
        s.revision += 1;
        true
    }
    pub fn begin_generation(&self, generation: u64) -> Option<u64> {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !s.lost || generation <= s.generation {
            return None;
        }
        s.generation = generation;
        s.revision += 1;
        s.failure = None;
        Some(s.revision)
    }
    /// Only the writer calls this after data-source commits and a current
    /// physical subscription have both been checked. There is no public reset.
    pub fn resolve(&self, generation: u64, revision: u64) -> bool {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if s.generation != generation || s.revision != revision || !s.lost || s.failure.is_some() {
            return false;
        }
        s.lost = false;
        true
    }
    pub fn healthy(&self) -> bool {
        !self.snapshot().lost
    }
}
