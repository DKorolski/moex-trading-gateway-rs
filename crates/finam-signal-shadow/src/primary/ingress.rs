//! Nonblocking socket-to-processor handoff. Overflow is a latched continuity
//! fault, not a silent drop or an await that stops polling Ping/Pong.
use super::{PrimaryError, PrimaryPolicy, WireFrame};
use crate::transport::StopHandle;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct IngressSender {
    tx: mpsc::Sender<WireFrame>,
    lost: Arc<AtomicBool>,
    generation: u64,
}

pub struct IngressReceiver {
    rx: mpsc::Receiver<WireFrame>,
    lost: Arc<AtomicBool>,
    max_age_ms: i64,
}

pub fn bounded_ingress(
    policy: &PrimaryPolicy,
    generation: u64,
) -> Result<(IngressSender, IngressReceiver), PrimaryError> {
    policy.validate()?;
    if generation == 0 {
        return Err(PrimaryError::Generation);
    }
    let (tx, rx) = mpsc::channel(policy.ingress_capacity);
    let lost = Arc::new(AtomicBool::new(false));
    Ok((
        IngressSender {
            tx,
            lost: lost.clone(),
            generation,
        },
        IngressReceiver {
            rx,
            lost,
            max_age_ms: policy.max_ingress_age_ms as i64,
        },
    ))
}

impl IngressSender {
    pub fn continuity_lost(&self) -> bool {
        self.lost.load(Ordering::SeqCst)
    }

    pub fn offer(&self, frame: WireFrame) -> Result<(), PrimaryError> {
        if frame.generation != self.generation {
            return Err(PrimaryError::Generation);
        }
        if self.continuity_lost() {
            return Err(PrimaryError::Continuity);
        }
        self.tx.try_send(frame).map_err(|_| {
            // Synchronous: owner sees this before any reconnect/proof await.
            self.lost.store(true, Ordering::SeqCst);
            PrimaryError::Continuity
        })
    }
}

impl IngressReceiver {
    pub fn continuity_lost(&self) -> bool {
        self.lost.load(Ordering::SeqCst)
    }

    pub async fn receive(&mut self, stop: &StopHandle) -> Result<Option<WireFrame>, PrimaryError> {
        self.receive_at(stop, crate::timing::now_ms).await
    }

    async fn receive_at(
        &mut self,
        stop: &StopHandle,
        clock: impl FnOnce() -> i64,
    ) -> Result<Option<WireFrame>, PrimaryError> {
        if self.continuity_lost() {
            return Err(PrimaryError::Continuity);
        }
        let frame = tokio::select! {
            biased;
            _ = stop.cancelled() => return Ok(None),
            frame = self.rx.recv() => frame,
        };
        let Some(frame) = frame else {
            self.lost.store(true, Ordering::SeqCst);
            return Err(PrimaryError::Continuity);
        };
        let now = clock();
        if self.continuity_lost()
            || now < frame.received_at_ms
            || now - frame.received_at_ms > self.max_age_ms
        {
            self.lost.store(true, Ordering::SeqCst);
            return Err(PrimaryError::Continuity);
        }
        Ok(Some(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::ReadOnlyCollector;
    use std::time::Duration;

    fn stop() -> StopHandle {
        ReadOnlyCollector::new(
            chrono::Utc::now() + chrono::Duration::seconds(20),
            Duration::from_secs(1),
        )
        .unwrap()
        .0
    }

    fn frame(generation: u64) -> WireFrame {
        WireFrame {
            rows: vec![],
            generation,
            received_at_ms: 10000,
            wire_bytes: 0,
            sha256: "a".repeat(64),
            received_mono: tokio::time::Instant::now(),
            owner: None,
        }
    }

    #[test]
    fn full_queue_latches_without_waiting_for_reader() {
        let (tx, _rx) = bounded_ingress(&PrimaryPolicy::default(), 1).unwrap();
        for _ in 0..128 {
            tx.offer(frame(1)).unwrap();
        }
        assert_eq!(tx.offer(frame(1)), Err(PrimaryError::Continuity));
        assert!(tx.continuity_lost());
    }

    #[test]
    fn old_generation_does_not_poison_new_queue() {
        let (tx, _rx) = bounded_ingress(&PrimaryPolicy::default(), 2).unwrap();
        assert_eq!(tx.offer(frame(1)), Err(PrimaryError::Generation));
        assert!(!tx.continuity_lost());
        tx.offer(frame(2)).unwrap();
    }

    #[tokio::test]
    async fn queue_age_edges_are_enforced() {
        for age in [1999, 2000, 2001] {
            let (tx, mut rx) = bounded_ingress(&PrimaryPolicy::default(), 1).unwrap();
            tx.offer(frame(1)).unwrap();
            let result = rx.receive_at(&stop(), || 10000 + age).await;
            assert_eq!(result.is_ok(), age <= 2000);
            assert_eq!(rx.continuity_lost(), age > 2000);
        }
    }

    #[tokio::test]
    async fn stop_cancels_pending_queue_wait() {
        let (_tx, mut rx) = bounded_ingress(&PrimaryPolicy::default(), 1).unwrap();
        let stop = stop();
        stop.stop();
        assert!(rx.receive(&stop).await.unwrap().is_none());
        assert!(!rx.continuity_lost());
    }

    #[tokio::test]
    async fn writer_close_is_not_clean_eod() {
        let (tx, mut rx) = bounded_ingress(&PrimaryPolicy::default(), 1).unwrap();
        drop(tx);
        assert!(matches!(
            rx.receive(&stop()).await,
            Err(PrimaryError::Continuity)
        ));
        assert!(rx.continuity_lost());
    }
}
