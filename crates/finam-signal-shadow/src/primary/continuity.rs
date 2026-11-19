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
    #[test]
    fn v7_r1_atomic_data_wait_returns_the_physical_fault_that_won_the_race() {
        let gate = ContinuityGate::new(1);
        assert!(gate.healthy()); // an obsolete caller observation
        gate.fault(failure(1));
        let before = gate.snapshot();
        let state = gate.data_gap_state(1).unwrap();
        assert_eq!(state.revision, before.revision);
        assert!(state.lost);
        assert!(matches!(state.failure.unwrap().kind, Kind::Close));
        assert!(gate.data_gap_state(2).is_none());
        let clean = ContinuityGate::new(1);
        let entered = clean.data_gap_state(1).unwrap();
        assert!(entered.lost && entered.failure.is_none());
        assert_eq!(clean.data_gap_state(1).unwrap().revision, entered.revision);
    }
    #[cfg(feature = "models-bridge")]
    #[test]
    fn live_advance_is_linearized_before_or_after_the_generation_fault() {
        let lost = ContinuityGate::new(1);
        lost.fault(failure(1));
        assert!(lost
            .with_healthy(1, || panic!("must not advance"))
            .is_none());
        let gate = ContinuityGate::new(1);
        assert!(gate
            .with_healthy(2, || panic!("foreign generation"))
            .is_none());
        let (send, receive) = std::sync::mpsc::channel();
        let peer = gate.clone();
        let fault = std::thread::spawn(move || {
            receive.recv().unwrap();
            peer.fault(failure(1));
        });
        assert_eq!(
            gate.with_healthy(1, || {
                send.send(()).unwrap();
                42
            }),
            Some(42)
        );
        fault.join().unwrap();
        assert!(gate
            .with_healthy(1, || panic!("fault was recorded"))
            .is_none());
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
    #[cfg(test)]
    pub(crate) fn data_gap(&self, generation: u64) -> bool {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if s.generation != generation || s.failure.is_some() || s.lost {
            return false;
        }
        s.lost = true;
        s.revision += 1;
        true
    }
    /// Enter canonical wait OR return the physical fault that won the race,
    /// under the same lock. No healthy()/transition check-then-act window.
    pub(crate) fn data_gap_state(&self, generation: u64) -> Option<Snapshot> {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if s.generation != generation {
            return None;
        }
        if !s.lost && s.failure.is_none() {
            s.lost = true;
            s.revision += 1;
        }
        Some(Snapshot {
            generation: s.generation,
            revision: s.revision,
            lost: s.lost,
            failure: s.failure.clone(),
        })
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
    /// Linearize the short native live advance with a physical generation fault.
    /// No SQL, log IO, network or model callback belongs inside this fence.
    #[cfg(feature = "models-bridge")]
    pub(crate) fn with_healthy<T>(
        &self,
        generation: u64,
        advance: impl FnOnce() -> T,
    ) -> Option<T> {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != generation || state.lost || state.failure.is_some() {
            return None;
        }
        let result = advance();
        drop(state);
        Some(result)
    }
}
