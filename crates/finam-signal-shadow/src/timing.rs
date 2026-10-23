//! Timing fence for real callbacks. No replay clock or LIVE promotion API.
use crate::{transport::StopHandle, ValidatedConfig};
use chrono::Utc;
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, PartialEq, Eq)]
pub enum TimingError {
    Bounds,
    Held,
    Stopped,
    Clock,
    Window,
    Late,
    Callback,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallbackTiming {
    pub received_at_ms: i64,
    pub canonical_ready_at_ms: i64,
    pub model_start_at_ms: i64,
    pub result_at_ms: i64,
    pub callback_elapsed_ns: u128,
}

pub struct CallbackFence {
    config_sha256: String,
    start_ms: i64,
    deadline_ms: i64,
    lmax_ms: u64,
    uncertainty_ms: u64,
    anchor_wall: i64,
    anchor_mono: Instant,
    last_wall: i64,
    held: bool,
    stop: StopHandle,
}

impl CallbackFence {
    pub fn new(config: &ValidatedConfig, stop: StopHandle) -> Result<Self, TimingError> {
        Self::at(config, stop, Utc::now().timestamp_millis(), Instant::now())
    }

    fn at(
        config: &ValidatedConfig,
        stop: StopHandle,
        now: i64,
        mono: Instant,
    ) -> Result<Self, TimingError> {
        let c = config.config();
        if c.run_deadline_utc_ms <= now || c.lmax_ms > i64::MAX as u64 {
            return Err(TimingError::Bounds);
        }
        Ok(Self {
            config_sha256: config.fingerprint().into(),
            start_ms: c.observation_start_utc_ms,
            deadline_ms: c.run_deadline_utc_ms,
            lmax_ms: c.lmax_ms,
            uncertainty_ms: c.clock_uncertainty_ms,
            anchor_wall: now,
            anchor_mono: mono,
            last_wall: now,
            held: false,
            stop,
        })
    }

    pub fn config_sha256(&self) -> &str {
        &self.config_sha256
    }
    pub fn held(&self) -> bool {
        self.held
    }

    /// Shared monotonic/wall-clock fence for feedback and publication boundaries.
    pub(crate) fn check_now(&mut self) -> Result<(), TimingError> {
        if self.held {
            return Err(TimingError::Held);
        }
        self.held = true;
        self.check_clock(Utc::now().timestamp_millis(), Instant::now())?;
        self.held = false;
        Ok(())
    }

    /// Caller binds these timestamps to admitted input; labels alone prove no source.
    /// Closure may mutate the model. A late/error result is terminal, not rolled back.
    /// This synchronous callback cannot be preempted; elapsed violations are caught
    /// after return. Process-level hard stop remains a separate obligation.
    pub fn execute<T, E>(
        &mut self,
        nominal_close_ms: i64,
        received_ms: i64,
        ready_ms: i64,
        callback: impl FnOnce(i64) -> Result<T, E>,
    ) -> Result<(T, CallbackTiming), TimingError> {
        self.execute_clock(nominal_close_ms, received_ms, ready_ms, callback, || {
            (Utc::now().timestamp_millis(), Instant::now())
        })
    }

    fn execute_clock<T, E>(
        &mut self,
        close: i64,
        received: i64,
        ready: i64,
        callback: impl FnOnce(i64) -> Result<T, E>,
        mut clock: impl FnMut() -> (i64, Instant),
    ) -> Result<(T, CallbackTiming), TimingError> {
        if self.held {
            return Err(TimingError::Held);
        }
        // Includes panic unwinding: a failed callback cannot be silently retried.
        self.held = true;
        let (start, start_mono) = clock();
        self.check_clock(start, start_mono)?;
        if close <= self.start_ms || close > received || received > ready || ready > start {
            return Err(TimingError::Window);
        }
        self.check_latency(close, start)?;
        let value = callback(start).map_err(|_| TimingError::Callback)?;
        let (end, end_mono) = clock();
        self.check_clock(end, end_mono)?;
        self.check_latency(close, end)?;
        let elapsed = end_mono
            .checked_duration_since(start_mono)
            .ok_or(TimingError::Clock)?;
        if elapsed.as_millis() > u128::from(self.lmax_ms - self.uncertainty_ms) {
            return Err(TimingError::Late);
        }
        self.held = false;
        Ok((
            value,
            CallbackTiming {
                received_at_ms: received,
                canonical_ready_at_ms: ready,
                model_start_at_ms: start,
                result_at_ms: end,
                callback_elapsed_ns: elapsed.as_nanos(),
            },
        ))
    }

    fn check_latency(&self, close: i64, now: i64) -> Result<(), TimingError> {
        let elapsed = now
            .checked_sub(close)
            .and_then(|v| u64::try_from(v).ok())
            .ok_or(TimingError::Clock)?;
        if elapsed
            .checked_add(self.uncertainty_ms)
            .map_or(true, |v| v > self.lmax_ms)
        {
            return Err(TimingError::Late);
        }
        Ok(())
    }

    fn check_clock(&mut self, wall: i64, mono: Instant) -> Result<(), TimingError> {
        if self.stop.is_stopped() {
            return Err(TimingError::Stopped);
        }
        let elapsed = mono
            .checked_duration_since(self.anchor_mono)
            .ok_or(TimingError::Clock)?;
        let expected = i128::from(self.anchor_wall) + elapsed.as_millis() as i128;
        if wall < self.last_wall
            || (i128::from(wall) - expected).unsigned_abs() > u128::from(self.uncertainty_ms)
        {
            return Err(TimingError::Clock);
        }
        if wall >= self.deadline_ms || expected >= i128::from(self.deadline_ms) {
            return Err(TimingError::Window);
        }
        self.last_wall = wall;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::ReadOnlyCollector;
    use std::{cell::Cell, time::Duration};

    fn fence() -> (CallbackFence, StopHandle, Instant) {
        let (stop, _) = ReadOnlyCollector::new(
            Utc::now() + chrono::Duration::seconds(10),
            Duration::from_secs(1),
        )
        .unwrap();
        let mono = Instant::now();
        (
            CallbackFence {
                config_sha256: "test".into(),
                start_ms: 0,
                deadline_ms: 5000,
                lmax_ms: 100,
                uncertainty_ms: 5,
                anchor_wall: 1000,
                anchor_mono: mono,
                last_wall: 1000,
                held: false,
                stop: stop.clone(),
            },
            stop,
            mono,
        )
    }

    #[test]
    fn measures_callback_and_retains_all_time_frontiers() {
        let (mut f, _, mono) = fence();
        let mut times = [(1000, mono), (1007, mono + Duration::from_millis(7))].into_iter();
        let (value, t) = f
            .execute_clock(950, 960, 980, Ok::<_, ()>, || times.next().unwrap())
            .unwrap();
        assert_eq!(value, 1000);
        assert_eq!(
            (
                t.received_at_ms,
                t.canonical_ready_at_ms,
                t.model_start_at_ms,
                t.result_at_ms
            ),
            (960, 980, 1000, 1007)
        );
        assert_eq!(t.callback_elapsed_ns, 7_000_000);
        assert!(!f.held());
    }

    #[test]
    fn late_history_and_bad_order_never_call_model() {
        for (close, receive, ready) in [(800, 900, 990), (950, 1001, 1002), (950, 940, 990)] {
            let (mut f, _, mono) = fence();
            let called = Cell::new(false);
            assert!(f
                .execute_clock(
                    close,
                    receive,
                    ready,
                    |_| {
                        called.set(true);
                        Ok::<_, ()>(())
                    },
                    || (1000, mono)
                )
                .is_err());
            assert!(!called.get());
            assert!(f.held());
        }
    }

    #[test]
    fn late_callback_is_terminal_without_rollback_or_repeat() {
        let (mut f, _, mono) = fence();
        let calls = Cell::new(0);
        let mut times = [(1000, mono), (1100, mono + Duration::from_millis(100))].into_iter();
        assert_eq!(
            f.execute_clock(
                950,
                960,
                980,
                |_| {
                    calls.set(calls.get() + 1);
                    Ok::<_, ()>(())
                },
                || times.next().unwrap()
            )
            .unwrap_err(),
            TimingError::Late
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(
            f.execute_clock(
                950,
                960,
                980,
                |_| {
                    calls.set(calls.get() + 1);
                    Ok::<_, ()>(())
                },
                || (1100, mono)
            )
            .unwrap_err(),
            TimingError::Held
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn wall_jump_or_stop_during_callback_refuses_result() {
        let (mut f, stop, mono) = fence();
        let mut times = [(1000, mono), (1001, mono + Duration::from_millis(1))].into_iter();
        assert_eq!(
            f.execute_clock(
                950,
                960,
                980,
                |_| {
                    stop.stop();
                    Ok::<_, ()>(())
                },
                || times.next().unwrap()
            )
            .unwrap_err(),
            TimingError::Stopped
        );
        let (mut f, _, mono) = fence();
        let mut times = [(1000, mono), (1020, mono + Duration::from_millis(1))].into_iter();
        assert_eq!(
            f.execute_clock(950, 960, 980, |_| Ok::<_, ()>(()), || times.next().unwrap())
                .unwrap_err(),
            TimingError::Clock
        );
    }
}
