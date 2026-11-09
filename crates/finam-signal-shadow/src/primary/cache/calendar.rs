use super::*;
use crate::primary::calendar::{AdmittedCalendar, CalendarError};
impl PrimaryProcessor {
    pub(crate) fn eod_complete(&self, now: i64) -> bool {
        self.calendar
            .as_ref()
            .is_some_and(|c| now >= c.eod_check_at_ms())
            && self.next == self.required.len()
            && self.session_end_committed
            && !self.gap_unresolved
            && self.in_flight.is_none()
            && self.state != PipelineState::Hold
            && self.physical_owner.is_some()
            && self.fresh_seen
            && self.last_live_data.as_ref().is_some_and(|d| {
                d.generation == self.generation
                    && now
                        .checked_sub(d.received_at_ms)
                        .is_some_and(|age| (0..=1200000).contains(&age))
            })
    }
    pub fn calendar(&self) -> Option<&AdmittedCalendar> {
        self.calendar.as_ref()
    }
    pub(crate) fn bind_calendar(
        &mut self,
        calendar: AdmittedCalendar,
    ) -> Result<(), CalendarError> {
        if self.calendar.is_some()
            || self.physical_owner.is_some()
            || self.fresh_seen
            || !self.cache.is_empty()
            || self.in_flight.is_some()
            || self.gap_unresolved
            || self.recovery_pending.is_some()
            || !calendar.matches(
                &self.config_sha256,
                self.next_label_ms().ok_or(CalendarError::Binding)?,
            )
            || self.required != calendar.required()
        {
            return Err(CalendarError::Binding);
        }
        calendar.validate_entry(crate::timing::now_ms())?;
        self.calendar = Some(calendar);
        Ok(())
    }
    pub(super) fn session_end_label(&self) -> Option<i64> {
        self.calendar.as_ref().map(|c| c.last_label_ms())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        primary::calendar::tests::{at, Fixture},
        timing::{advance_fixture_clock, now_ms, with_fixture_clock},
    };
    fn processor(f: &Fixture) -> PrimaryProcessor {
        PrimaryProcessor::from_fixture(
            &f.config,
            broker_core::InstrumentId {
                symbol: "IMOEXF".into(),
                venue_symbol: Some("IMOEXF@RTSX".into()),
                exchange: broker_core::Exchange::Moex,
                market: broker_core::Market::Futures,
            },
            f.model.required_m10_labels_ms.clone(),
            1,
            now_ms(),
        )
        .unwrap()
    }
    fn go(t: i64) {
        advance_fixture_clock((t - now_ms()).try_into().unwrap());
    }
    #[test]
    fn first_subscribe_is_window_fenced_but_later_generations_do_not_restart_the_run() {
        let f = Fixture::new();
        with_fixture_clock(f.open(), || {
            let mut p = processor(&f);
            p.bind_calendar(f.cap(now_ms()).unwrap()).unwrap();
            go(f.binding.start_before_ms);
            let sub = crate::socket::primary::fixture_subscription(1, now_ms());
            assert_eq!(
                p.bind_subscription(&sub, now_ms()),
                Err(PrimaryError::EntryWindow)
            );
            assert!(!p.subscription_data_ready());
        });
        with_fixture_clock(f.open(), || {
            let mut p = processor(&f);
            p.bind_calendar(f.cap(now_ms()).unwrap()).unwrap();
            p.bind_subscription(
                &crate::socket::primary::fixture_subscription(1, now_ms()),
                now_ms(),
            )
            .unwrap();
            go(f.open() + 1200000);
            p.break_continuity();
            p.begin_generation(2, now_ms()).unwrap();
            p.bind_subscription(
                &crate::socket::primary::fixture_subscription(2, now_ms()),
                now_ms(),
            )
            .unwrap();
            assert!(p.gap_unresolved());
            assert!(!p.subscription_data_ready());
        });
    }
    #[test]
    fn session_end_rejects_early_wrong_unbound_stale_and_conflicting_proofs() {
        let f = Fixture::new();
        for case in [
            "early",
            "wrong_reason",
            "unbound",
            "late",
            "no_data",
            "conflict",
        ] {
            with_fixture_clock(f.open(), || {
                let mut p = processor(&f);
                if case != "unbound" {
                    p.bind_calendar(f.cap(now_ms()).unwrap()).unwrap();
                }
                // Numerical data-capability test only. The full 101 actual-model
                // flow is checked separately; this does not import model state.
                p.next = 100;
                let last = f.close() - 600000;
                if case != "no_data" {
                    go(last + 540100);
                    let body=serde_json::json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture","payload":{"symbol":"IMOEXF@RTSX","bars":[f.row(last+540000)]}}).to_string();
                    p.ingest(
                        WireFrame::normalize(&body, 1, "fixture", at(now_ms())).unwrap(),
                        now_ms(),
                    )
                    .unwrap();
                }
                go(f.close()
                    + if case == "early" {
                        2999
                    } else if case == "late" {
                        9900
                    } else {
                        3000
                    });
                let plan = broker_finam::sparse_m10::ClosedM1RequestPlan::new(
                    at(last),
                    at(f.close()),
                    vec![at(f.close())],
                )
                .unwrap();
                let mut source = f.rest(&plan);
                if case == "conflict" {
                    let mut e = source.evidence().clone();
                    let mut raw: serde_json::Value =
                        serde_json::from_str(&e.parts[0].raw_body).unwrap();
                    raw["bars"][9]["volume"] = serde_json::json!({"value":"1"});
                    e.parts[0].raw_body = raw.to_string();
                    e.parts[0].response_sha256 =
                        crate::seed::digest(e.parts[0].raw_body.as_bytes());
                    e.parts[0].declared_body_bytes = Some(e.parts[0].raw_body.len() as u64);
                    source = broker_finam::sparse_m10::AdmittedClosedM1Snapshot::admit(
                        e,
                        &plan,
                        at(now_ms()),
                    )
                    .unwrap();
                }
                let reason = if case == "wrong_reason" {
                    ExceptionReason::SparseUnproven
                } else {
                    ExceptionReason::SessionEnd
                };
                assert!(
                    p.prepare_rest_exception(&source, reason, now_ms()).is_err(),
                    "{case}"
                );
                assert_eq!(p.counters().committed, 0);
                assert!(p.in_flight.is_none());
            });
        }
    }
}
