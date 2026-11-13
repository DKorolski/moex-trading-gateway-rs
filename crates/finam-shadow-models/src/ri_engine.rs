//! RI Author41/42 prospective numerical port, not the IMO/USD reaction engine.
//! Extracted from the pinned RI prospective methods listed in provenance.json.
//! Broker/lifecycle IO is replaced by local model marks and the same end-of-step
//! assumed-fill transitions. There is no order client or outbound command type.
use crate::ri_context::{FrozenContext, SessionAnchor};
use crate::BarEvent;
use anyhow::{ensure, Result};
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Side {
    Long,
    Short,
}
impl Side {
    fn sign(self) -> f64 {
        if self == Self::Long {
            1.0
        } else {
            -1.0
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum Phase {
    #[serde(rename = "flat")]
    Flat,
    #[serde(rename = "live_pending_entry")]
    PendingEntry,
    #[serde(rename = "live_pending_exit")]
    PendingExit,
    #[serde(rename = "live_in_position")]
    InPosition,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Owner {
    Author41Mr,
    Author42Bo,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Role {
    Entry,
    Exit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct MrPosition {
    side: Side,
    entry_ts: NaiveDateTime,
    entry_price: f64,
    prev_close: f64,
    prev_range: f64,
    bars_held: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct BoPosition {
    side: Side,
    entry_ts: NaiveDateTime,
    entry_price: f64,
    bars_held: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Pending {
    Entry {
        side: Side,
        signal: NaiveDateTime,
    },
    Exit {
        reason: String,
        signal: NaiveDateTime,
    },
}

/// A model mark, NOT a FINAM fill. Price/entry/cost audit extends the official
/// projection, which intentionally contains only action identity and net points.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MarkAction {
    component: Owner,
    role: Role,
    side: Side,
    reason: String,
    scheduled_ts: String,
    pnl: Option<f64>,
    pub signal_bar_start_utc: i64,
    pub action_bar_start_utc: i64,
    pub entry_price: f64,
    pub model_mark_price: f64,
    pub roundtrip_cost_points: f64,
}
impl MarkAction {
    fn projection(&self) -> Value {
        json!({"component":self.component,"role":self.role,"side":self.side,
            "reason":self.reason,"scheduled_ts":self.scheduled_ts,"pnl":self.pnl})
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Engine {
    day: NaiveDate,
    previous: Option<SessionAnchor>,
    previous2: Option<SessionAnchor>,
    phase: Phase,
    mr: Option<MrPosition>,
    bo: Option<BoPosition>,
    pending: Option<Pending>,
    mr_total: u32,
    bo_total: u32,
    count: u32,
    first: Option<NaiveDateTime>,
    first_high: Option<f64>,
    first_low: Option<f64>,
    high: Option<f64>,
    low: Option<f64>,
    long_level: Option<f64>,
    short_level: Option<f64>,
    allowed: bool,
    used_long: bool,
    used_short: bool,
    branches: Vec<String>,
    actions: Vec<MarkAction>,
}

fn time(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap()
}
fn label_epoch(dt: NaiveDateTime) -> i64 {
    (dt - Duration::hours(3)).and_utc().timestamp()
}

impl Engine {
    pub(crate) fn new(frozen: &FrozenContext) -> Result<Self> {
        use chrono::Datelike;
        for a in [&frozen.previous, &frozen.previous2].into_iter().flatten() {
            ensure!(
                [a.high, a.low, a.close, a.range]
                    .iter()
                    .all(|v| v.is_finite())
                    && a.low > 0.
                    && a.range >= 0.,
                "invalid frozen RI anchor"
            );
        }
        let previous = frozen.ready().then(|| frozen.previous.clone()).flatten();
        let previous2 = frozen.ready().then(|| frozen.previous2.clone()).flatten();
        let d = frozen.session_date;
        let month_day = (d.month(), d.day());
        let allowed = previous
            .as_ref()
            .zip(previous2.as_ref())
            .is_some_and(|(p, p2)| {
                p.range > 0.
                    && p.close > 0.
                    && (p.close / p2.close - 1.).is_finite()
                    && d.weekday().number_from_monday() != 5
                    && !((5, 21)..=(7, 1)).contains(&month_day)
            });
        Ok(Self {
            day: d,
            previous,
            previous2,
            phase: Phase::Flat,
            mr: None,
            bo: None,
            pending: None,
            mr_total: 0,
            bo_total: 0,
            count: 0,
            first: None,
            first_high: None,
            first_low: None,
            high: None,
            low: None,
            long_level: None,
            short_level: None,
            allowed,
            used_long: false,
            used_short: false,
            branches: vec![],
            actions: vec![],
        })
    }

    fn branch(&mut self, value: &str) {
        self.branches.push(value.to_owned());
    }

    fn action(
        &mut self,
        owner: Owner,
        role: Role,
        side: Side,
        prices: (f64, f64),
        timing: (NaiveDateTime, NaiveDateTime),
        reason: &str,
    ) -> Result<()> {
        let (entry, mark) = prices;
        let cost = if owner == Owner::Author41Mr { 2. } else { 0. };
        let pnl = (mark - entry) * side.sign() - cost;
        ensure!(
            entry.is_finite() && entry > 0. && mark.is_finite() && mark > 0. && pnl.is_finite(),
            "nonfinite RI mark/economics"
        );
        self.actions.push(MarkAction {
            component: owner,
            role,
            side,
            reason: reason.into(),
            scheduled_ts: timing.1.format("%Y-%m-%d %H:%M:%S").to_string(),
            pnl: (role == Role::Exit).then_some(pnl),
            signal_bar_start_utc: label_epoch(timing.0),
            action_bar_start_utc: label_epoch(timing.1),
            entry_price: entry,
            model_mark_price: mark,
            roundtrip_cost_points: cost,
        });
        self.phase = if role == Role::Entry {
            Phase::PendingEntry
        } else {
            Phase::PendingExit
        };
        Ok(())
    }

    fn mr_step(&mut self, b: &BarEvent, dt: NaiveDateTime) -> Result<()> {
        if let Some(mut p) = self.mr.clone() {
            p.bars_held = p.bars_held.saturating_add(1);
            // Canonical RI source split: short stop .75R/take .020R,
            // long stop 1.00R/take .005R. No reaction-next-open conversion.
            let (stop_k, take_k) = if p.side == Side::Long {
                (1.0, 0.005)
            } else {
                (0.75, 0.020)
            };
            let stop = p.prev_close - p.side.sign() * stop_k * p.prev_range;
            let take = p.prev_close + p.side.sign() * take_k * p.prev_range;
            let exit = if (p.side == Side::Long && b.l <= stop)
                || (p.side == Side::Short && b.h >= stop)
            {
                Some((stop, "stop"))
            } else if (p.side == Side::Long && b.close > take)
                || (p.side == Side::Short && b.close < take)
            {
                Some((b.close, "take_author_close"))
            } else if dt.time() >= time(20, 0) {
                Some((b.close, "time_exit"))
            } else if p.bars_held > 20
                && ((p.side == Side::Long && b.h >= p.entry_price)
                    || (p.side == Side::Short && b.l <= p.entry_price))
            {
                Some((p.entry_price, "breakeven_limit"))
            } else {
                None
            };
            self.mr = Some(p.clone());
            if let Some((mark, reason)) = exit {
                self.branch(&format!("mr:exit:{reason}"));
                self.action(
                    Owner::Author41Mr,
                    Role::Exit,
                    p.side,
                    (p.entry_price, mark),
                    (dt, dt),
                    reason,
                )?;
            } else {
                self.branch("mr:hold");
            }
            return Ok(());
        }
        if self.mr_total >= 2 {
            self.branch("mr:entry_quota");
            return Ok(());
        }
        if self.bo.is_some() {
            self.branch("mr:bo_owns_slot");
            return Ok(());
        }
        let mut side = None;
        if let Some(p) = &self.previous {
            let ratio = p.range / p.low;
            if dt.time() <= time(10, 0) && ratio.is_finite() && 0.005 < ratio && ratio < 0.100 {
                if b.close > p.close && b.close < p.close + 0.20 * p.range {
                    side = Some(Side::Short);
                } else if b.close < p.close && b.close > p.close - 0.11 * p.range {
                    side = Some(Side::Long);
                }
            }
        }
        if let Some(side) = side {
            let p = self.previous.as_ref().unwrap();
            self.mr = Some(MrPosition {
                side,
                entry_ts: dt,
                entry_price: b.close,
                prev_close: p.close,
                prev_range: p.range,
                bars_held: 0,
            });
            self.mr_total = self.mr_total.saturating_add(1);
            self.branch("mr:entry");
            self.action(
                Owner::Author41Mr,
                Role::Entry,
                side,
                (b.close, b.close),
                (dt, dt),
                "live_entry",
            )?;
        } else {
            self.branch("mr:no_entry_signal");
        }
        Ok(())
    }

    fn hour_check(&self, dt: NaiveDateTime) -> bool {
        dt.minute() == 50
            && self
                .first
                .is_some_and(|first| dt > first + Duration::minutes(50))
    }

    fn observe_bo(&mut self, b: &BarEvent, dt: NaiveDateTime) {
        if self.first.is_none() {
            self.first = Some(dt);
            self.first_high = Some(b.h);
            self.first_low = Some(b.l);
        }
        self.high = Some(self.high.map_or(b.h, |h| h.max(b.h)));
        self.low = Some(self.low.map_or(b.l, |l| l.min(b.l)));
        if self.count == 5 {
            if let Some(p) = &self.previous {
                self.long_level = Some(
                    (p.close + 0.42 * p.range)
                        .max(b.close)
                        .max(self.first_high.unwrap()),
                );
                self.short_level = Some(
                    (p.close - 0.42 * p.range)
                        .min(b.close)
                        .min(self.first_low.unwrap()),
                );
                if (b.close - p.close).abs() > 1.50 * p.range {
                    self.allowed = false;
                }
            }
        }
        self.count = self.count.saturating_add(1);
        self.branch("bo:observed");
    }

    fn schedule_bo(&mut self, b: &BarEvent, dt: NaiveDateTime) {
        let reason = if self.previous.is_none() {
            Some("context_unavailable")
        } else if self.pending.is_some() {
            Some("pending_exists")
        } else if self.bo.is_some() {
            Some("bo_owns_slot")
        } else if self.mr.is_some() {
            Some("mr_owns_slot")
        } else if !self.allowed {
            Some("session_disallowed")
        } else if self.long_level.is_none() || self.short_level.is_none() {
            Some("levels_unavailable")
        } else if dt.time() >= time(23, 0) {
            Some("terminal_time")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.branch(&format!("bo_entry:{reason}"));
            return;
        }
        let p = self.previous.as_ref().unwrap();
        let ret = p.close / self.previous2.as_ref().unwrap().close - 1.;
        let range_ok = p.high / p.low > 1.01;
        let selected = if self.used_long && b.h >= self.high.unwrap() {
            Some((Side::Long, "reentry_long"))
        } else if self.used_short && b.l <= self.low.unwrap() {
            Some((Side::Short, "reentry_short"))
        } else if self.hour_check(dt)
            && range_ok
            && ret > -0.025
            && b.close > self.long_level.unwrap()
        {
            Some((Side::Long, "hour_long"))
        } else if self.hour_check(dt)
            && range_ok
            && ret < 0.025
            && b.close < self.short_level.unwrap()
        {
            Some((Side::Short, "hour_short"))
        } else {
            None
        };
        if let Some((side, reason)) = selected {
            self.pending = Some(Pending::Entry { side, signal: dt });
            self.branch(&format!("bo_entry:{reason}"));
        } else {
            self.branch("bo_entry:no_entry_signal");
        }
    }

    fn bo_step(&mut self, b: &BarEvent, dt: NaiveDateTime) -> Result<()> {
        // Observation precedes every lifecycle early return, as in upstream.
        self.observe_bo(b, dt);
        if self.phase == Phase::PendingEntry {
            self.branch("bo:pending_entry");
            return Ok(());
        }
        if self.phase == Phase::PendingExit {
            self.branch("bo:pending_exit");
            return Ok(());
        }
        if let Some(pending) = self.pending.take() {
            match pending {
                Pending::Entry { side, signal } => {
                    if dt.time() >= time(23, 0) {
                        self.branch("bo:pending_entry_terminal");
                    } else if self.mr.is_some() || self.bo.is_some() {
                        self.branch("bo:pending_entry_occupied");
                    } else {
                        self.bo = Some(BoPosition {
                            side,
                            entry_ts: dt,
                            entry_price: b.o,
                            bars_held: 0,
                        });
                        self.bo_total = self.bo_total.saturating_add(1);
                        if side == Side::Long {
                            self.used_long = true;
                        } else {
                            self.used_short = true;
                        }
                        self.branch("bo:entry");
                        self.action(
                            Owner::Author42Bo,
                            Role::Entry,
                            side,
                            (b.o, b.o),
                            (signal, dt),
                            "live_entry_next_bar",
                        )?;
                    }
                }
                Pending::Exit { reason, signal } => {
                    let p = self
                        .bo
                        .as_ref()
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("RI pending exit without position"))?;
                    self.branch("bo:exit_pending");
                    self.action(
                        Owner::Author42Bo,
                        Role::Exit,
                        p.side,
                        (p.entry_price, b.o),
                        (signal, dt),
                        &reason,
                    )?;
                    return Ok(());
                }
            }
        }
        if let Some(mut p) = self.bo.clone() {
            p.bars_held = p.bars_held.saturating_add(1);
            self.bo = Some(p.clone());
            if dt.time() >= time(23, 0) {
                self.branch("bo:exit_eod");
                self.action(
                    Owner::Author42Bo,
                    Role::Exit,
                    p.side,
                    (p.entry_price, b.close),
                    (dt, dt),
                    "time_exit_same_bar_close",
                )?;
            } else {
                self.branch("bo:hold");
                let context = self.previous.as_ref().unwrap();
                let emergency = if p.side == Side::Long {
                    b.close < context.close + 0.18 * context.range
                } else {
                    b.close > context.close - 0.18 * context.range
                };
                let hourly = self.hour_check(dt)
                    && if p.side == Side::Long {
                        b.close < context.close + 0.50 * context.range
                    } else {
                        b.close > context.close - 0.50 * context.range
                    };
                if emergency || hourly {
                    let kind = if emergency { "emergency" } else { "hour" };
                    let side = if p.side == Side::Long {
                        "long"
                    } else {
                        "short"
                    };
                    self.pending = Some(Pending::Exit {
                        reason: format!("stop_{kind}_{side}_next_open"),
                        signal: dt,
                    });
                }
            }
        }
        self.schedule_bo(b, dt);
        Ok(())
    }

    pub(crate) fn step(&mut self, b: &BarEvent, dt: NaiveDateTime) -> Result<()> {
        ensure!(
            dt.date() == self.day,
            "RI engine cannot roll sessions implicitly"
        );
        self.clear_step();
        self.mr_step(b, dt)?;
        self.bo_step(b, dt)?;
        // Both components have observed the same closed bar before assumed fill.
        match self.phase {
            Phase::PendingEntry => self.phase = Phase::InPosition,
            Phase::PendingExit => {
                self.mr = None;
                self.bo = None;
                self.pending = None;
                self.phase = Phase::Flat;
            }
            _ => {}
        }
        ensure!(
            !(self.mr.is_some() && self.bo.is_some()),
            "overlapping RI ownership"
        );
        Ok(())
    }

    pub(crate) fn clear_step(&mut self) {
        self.branches.clear();
        self.actions.clear();
    }
    pub(crate) fn actions(&self) -> &[MarkAction] {
        &self.actions
    }
    pub(crate) fn is_flat(&self) -> bool {
        self.mr.is_none()
            && self.bo.is_none()
            && self.pending.is_none()
            && self.phase == Phase::Flat
    }
    pub(crate) fn projection(&self, frozen: &FrozenContext) -> Value {
        let anchor = |a: &Option<SessionAnchor>| {
            a.as_ref().map(|a| {
                json!({"date":a.session_date,
            "high":a.high,"low":a.low,"close":a.close,"range":a.range,"bars":a.distinct_bars})
            })
        };
        let (owner, side, entry) = if let Some(p) = &self.mr {
            (Some(Owner::Author41Mr), Some(p.side), Some(p.entry_ts))
        } else if let Some(p) = &self.bo {
            (Some(Owner::Author42Bo), Some(p.side), Some(p.entry_ts))
        } else {
            (None, None, None)
        };
        let pending = self.pending.as_ref().map(|p| match p {
            Pending::Entry { side, .. } => json!({"kind":"entry","side":side,"reason":null}),
            Pending::Exit { reason, .. } => json!({"kind":"exit","side":null,"reason":reason}),
        });
        json!({"context":{"previous":anchor(&frozen.previous),"previous2":anchor(&frozen.previous2)},
            "phase":self.phase,"mr_total":self.mr_total,"bo_total":self.bo_total,
            "lifecycle_owner":owner,"lifecycle_side":side,
            "lifecycle_entry_ts":entry.map(|t|t.format("%Y-%m-%d %H:%M:%S").to_string()),
            "mr":self.mr,"bo":self.bo,"pending":pending,"observation_count":self.count,
            "first_bar":self.first,"long_level":self.long_level,"short_level":self.short_level,
            "day_high":self.high,"day_low":self.low,"trade_allowed":self.allowed,
            "long_used":self.used_long,"short_used":self.used_short,
            "branches":self.branches,"actions":self.actions.iter().map(MarkAction::projection).collect::<Vec<_>>()})
    }
}
