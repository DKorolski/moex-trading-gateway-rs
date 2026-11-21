//! Model-free auth/WS qualification. No seed, feedback, orders or automatic retry.
use crate::{
    auth::AuthLifecycle,
    s5::{Channel, Witness},
    socket::{BarsSocket, QuotesSocket},
    transport::ReadOnlyCollector,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    #[serde(default)]
    pub initial_quote_discard: bool,
    pub schema_version: u32,
    pub run_id: String,
    pub account_id: String,
    pub instrument: String,
    pub start_at: DateTime<Utc>,
    pub latest_start_at: DateTime<Utc>,
    pub finish_at: DateTime<Utc>,
    pub request_budget_ms: u64,
    pub silence_ms: u64,
    pub reconnect_ms: u64,
    pub quote_age_ms: u64,
    pub dedup_capacity: usize,
    pub stop_timeout_ms: u64,
    pub max_output_bytes: u64,
}
impl Plan {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.run_id.is_empty()
            || self.run_id.len() > 96
            || !self
                .run_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || self.account_id.is_empty()
            || self.account_id.len() > 256
            || self.instrument != "IMOEXF@RTSX"
            || now < self.start_at
            || now > self.latest_start_at
            || self.latest_start_at < self.start_at
            || self.latest_start_at - self.start_at > chrono::Duration::seconds(30)
            || self.finish_at - self.start_at > chrono::Duration::seconds(1500)
            || self.finish_at - self.latest_start_at < chrono::Duration::seconds(60)
            || self.request_budget_ms != 15000
            || self.silence_ms != 60000
            || self.reconnect_ms != 5000
            || self.quote_age_ms != 5000
            || self.dedup_capacity != 4096
            || self.stop_timeout_ms != 1000
            || !(16384..=1048576).contains(&self.max_output_bytes)
        {
            return Err("diagnostic_plan");
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct GrantReceipt {
    event: &'static str,
    channel: Channel,
    generation: u64,
    expires_at: DateTime<Utc>,
    readonly: bool,
    selected_account_matches: bool,
    rtsx_zero_delay: bool,
}
enum Event {
    Grant(GrantReceipt),
    Witness(crate::s5::Receipt),
}
struct Writer {
    file: File,
    left: u64,
}
impl Writer {
    fn new(path: &Path, left: u64) -> Result<Self, &'static str> {
        let mut o = OpenOptions::new();
        o.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        Ok(Self {
            file: o.open(path).map_err(|_| "diagnostic_output")?,
            left,
        })
    }
    fn put(&mut self, value: &impl Serialize) -> Result<(), &'static str> {
        let mut b = serde_json::to_vec(value).map_err(|_| "diagnostic_output")?;
        b.push(b'\n');
        if b.len() as u64 > self.left {
            return Err("diagnostic_output_budget");
        }
        self.file
            .write_all(&b)
            .and_then(|_| self.file.sync_data())
            .map_err(|_| "diagnostic_output")?;
        self.left -= b.len() as u64;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
enum ErrorCode {
    Bounds,
    Stopped,
    Deadline,
    Held,
    Fetch,
    Admission,
    Clock,
    Socket(crate::socket::SocketCode),
}
#[derive(Debug, Clone, Serialize)]
struct ErrorReceipt {
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp_bounds: Option<crate::socket::TimestampBoundsDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quote_conflict: Option<Box<crate::quote::QuoteConflictDetail>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quote_time: Option<Box<crate::quote::QuoteTimeDetail>>,
    channel: Channel,
    stage: crate::socket::SocketStage,
    code: ErrorCode,
    auth_generation: Option<u64>,
    connection_generation: Option<u64>,
}
#[derive(Debug, Clone)]
struct Failure {
    reason: &'static str,
    detail: Option<ErrorReceipt>,
}
impl From<&'static str> for Failure {
    fn from(reason: &'static str) -> Self {
        Self {
            reason,
            detail: None,
        }
    }
}
impl Failure {
    fn ws(
        reason: &'static str,
        channel: Channel,
        stage: crate::socket::SocketStage,
        auth_generation: Option<u64>,
        connection_generation: Option<u64>,
        error: crate::transport::TransportError,
    ) -> Self {
        use crate::transport::TransportError as E;
        let timestamp_bounds = match &error {
            E::Socket(e) => e.timestamp_bounds,
            _ => None,
        };
        let quote_conflict = match &error {
            E::Socket(e) => e.quote_conflict.clone(),
            _ => None,
        };
        let quote_time = match &error {
            E::Socket(e) => e.quote_time.clone(),
            _ => None,
        };
        let (stage, code) = match error {
            E::Socket(e) => (e.stage, ErrorCode::Socket(e.code)),
            E::Bounds => (stage, ErrorCode::Bounds),
            E::Stopped => (stage, ErrorCode::Stopped),
            E::Deadline => (stage, ErrorCode::Deadline),
            E::Held => (stage, ErrorCode::Held),
            E::Fetch => (stage, ErrorCode::Fetch),
            E::Admission => (stage, ErrorCode::Admission),
            E::Clock => (stage, ErrorCode::Clock),
        };
        Self {
            reason,
            detail: Some(ErrorReceipt {
                timestamp_bounds,
                quote_conflict,
                quote_time,
                channel,
                stage,
                code,
                auth_generation,
                connection_generation,
            }),
        }
    }
}

enum Socket {
    Bars(BarsSocket),
    Quotes(QuotesSocket),
}
impl Socket {
    async fn observe(
        &mut self,
        at: Instant,
    ) -> Result<Option<crate::ws::Observation>, crate::transport::TransportError> {
        match self {
            Self::Bars(s) => s.observe_until(at).await,
            Self::Quotes(s) => s.observe_until(at).await,
        }
    }
    async fn rotate(
        self,
        lease: broker_finam::AccessTokenLease,
    ) -> Result<Self, crate::transport::TransportError> {
        match self {
            Self::Bars(s) => s.rotate_auth(lease).await.map(Self::Bars),
            Self::Quotes(s) => s.rotate_auth(lease).await.map(Self::Quotes),
        }
    }
}
struct Active(Arc<AtomicBool>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

async fn cancelled(rx: &mut watch::Receiver<bool>) {
    if *rx.borrow() {
        return;
    }
    let _ = rx.changed().await;
}
async fn guarded<E, F: std::future::Future<Output = Result<(), E>>>(
    work: F,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), E> {
    tokio::select! { biased; result=work=>result, _=cancelled(&mut cancel)=>Ok(()) }
}

async fn worker(
    channel: Channel,
    p: Arc<Plan>,
    auth: Arc<AuthLifecycle>,
    budget: ReadOnlyCollector,
    tx: mpsc::Sender<Event>,
    active: Arc<AtomicBool>,
) -> Result<(), Failure> {
    let _active = Active(active.clone());
    let mut grant = auth.grant().await.map_err(|_| "diagnostic_auth")?;
    let send_grant = |g: &crate::auth::Grant| {
        Event::Grant(GrantReceipt {
            event: "auth_validated",
            channel,
            generation: g.generation,
            expires_at: g.expires_at,
            readonly: true,
            selected_account_matches: true,
            rtsx_zero_delay: true,
        })
    };
    tx.send(send_grant(&grant))
        .await
        .map_err(|_| "diagnostic_queue")?;
    let mut socket = match channel {
        Channel::Bars => Socket::Bars(
            BarsSocket::connect(grant.lease.clone(), budget, p.silence_ms, p.reconnect_ms)
                .await
                .map_err(|e| {
                    Failure::ws(
                        "diagnostic_ws_connect",
                        channel,
                        crate::socket::SocketStage::Connect,
                        Some(grant.generation),
                        None,
                        e,
                    )
                })?,
        ),
        Channel::Quotes => Socket::Quotes(
            QuotesSocket::connect_diagnostic_initial(
                grant.lease.clone(),
                budget,
                p.silence_ms,
                p.reconnect_ms,
                p.quote_age_ms,
                p.dedup_capacity,
                p.initial_quote_discard,
            )
            .await
            .map_err(|e| {
                Failure::ws(
                    "diagnostic_ws_connect",
                    channel,
                    crate::socket::SocketStage::Connect,
                    Some(grant.generation),
                    None,
                    e,
                )
            })?,
        ),
    };
    active.store(true, Ordering::SeqCst);
    let mut connection = 1;
    let mut witness = Witness::new_auth(channel, connection, grant.generation);
    loop {
        let current = auth.grant().await.map_err(|_| "diagnostic_auth")?;
        if current.generation != grant.generation {
            active.store(false, Ordering::SeqCst);
            let began = Instant::now();
            socket = socket.rotate(current.lease.clone()).await.map_err(|e| {
                Failure::ws(
                    "diagnostic_ws_rotate",
                    channel,
                    crate::socket::SocketStage::Reconnect,
                    Some(current.generation),
                    None,
                    e,
                )
            })?;
            if began.elapsed() > Duration::from_millis(p.reconnect_ms) {
                return Err("diagnostic_reconnect_budget".into());
            }
            active.store(true, Ordering::SeqCst);
            connection += 1;
            grant = current;
            witness = Witness::new_auth(channel, connection, grant.generation);
            tx.send(send_grant(&grant))
                .await
                .map_err(|_| "diagnostic_queue")?;
        }
        let Some(observation) = socket.observe(grant.renew_at).await.map_err(|e| {
            Failure::ws(
                "diagnostic_ws_observe",
                channel,
                crate::socket::SocketStage::Receive,
                Some(grant.generation),
                Some(connection),
                e,
            )
        })?
        else {
            continue;
        };
        if !grant.valid(Utc::now(), Instant::now()) {
            return Err("diagnostic_auth_expired".into());
        }
        for r in witness.receipts(&observation, Utc::now().timestamp_millis()) {
            tx.send(Event::Witness(r))
                .await
                .map_err(|_| "diagnostic_queue")?;
        }
    }
}

#[derive(Default)]
struct Coverage {
    grants: BTreeMap<(String, u64), DateTime<Utc>>,
    sent: BTreeMap<(String, u64, u64), i64>,
    data: BTreeMap<(String, u64, u64), i64>,
}
impl Coverage {
    fn record(&mut self, e: &Event, w: &mut Writer) -> Result<(), &'static str> {
        match e {
            Event::Grant(g) => {
                self.grants
                    .insert((format!("{:?}", g.channel), g.generation), g.expires_at);
                w.put(g)
            }
            Event::Witness(r) => {
                let v = serde_json::to_value(r).map_err(|_| "diagnostic_receipt")?;
                let ch = match v["channel"].as_str() {
                    Some("BARS") => "Bars",
                    Some("QUOTES") => "Quotes",
                    _ => return Err("diagnostic_receipt"),
                };
                let key: (String, u64, u64) = (
                    ch.into(),
                    v["auth_generation"].as_u64().ok_or("diagnostic_receipt")?,
                    v["connection_generation"]
                        .as_u64()
                        .ok_or("diagnostic_receipt")?,
                );
                let at = v["at_ms"].as_i64().ok_or("diagnostic_receipt")?;
                if !self.grants.contains_key(&(key.0.clone(), key.1)) {
                    return Err("diagnostic_receipt_generation");
                }
                match v["event"].as_str() {
                    Some("discarded_initial_stale_candidate")
                        if ch == "Quotes"
                            && key.1 == 1
                            && key.2 == 1
                            && v["data_items"].as_u64() == Some(0)
                            && v["initial_discard"]["rows"]
                                .as_u64()
                                .is_some_and(|n| n > 0 && n <= 512)
                            && v["initial_discard"]["cumulative_rows"]
                                .as_u64()
                                .is_some_and(|n| n > 0 && n <= 512)
                            && v["initial_discard"]["cumulative_bytes"]
                                .as_u64()
                                .is_some_and(|n| n > 0 && n <= 262144) =>
                    {
                        if !self.sent.get(&key).is_some_and(|sent| at >= *sent)
                            || self.data.contains_key(&key)
                        {
                            return Err("diagnostic_receipt_order");
                        }
                    }
                    Some("auxiliary_updates_observed")
                        if ch == "Quotes"
                            && v["data_items"].as_u64() == Some(0)
                            && v["auxiliary_total"].as_u64().is_some_and(|n| n > 0) =>
                    {
                        // Cumulative across reconnects, not fresh DATA in this generation.
                        // Allowed before or after DATA, but never before subscription.
                        if !self.sent.get(&key).is_some_and(|sent| at >= *sent) {
                            return Err("diagnostic_receipt_order");
                        }
                    }
                    Some("snapshot_discarded") if ch == "Bars" => {
                        if !self.sent.get(&key).is_some_and(|sent| at >= *sent)
                            || self.data.contains_key(&key)
                        {
                            return Err("diagnostic_receipt_order");
                        }
                    }
                    Some("handshake_and_subscription_sent") => {
                        self.sent.insert(key, at);
                    }
                    Some("first_validated_data") if v["data_items"].as_u64().unwrap_or(0) > 0 => {
                        if !self.sent.get(&key).is_some_and(|sent| at >= *sent) {
                            return Err("diagnostic_receipt_order");
                        }
                        self.data.insert(key, at);
                    }
                    _ => return Err("diagnostic_receipt"),
                }
                w.put(r)
            }
        }
    }
    fn complete(&self) -> bool {
        ["Bars", "Quotes"].into_iter().all(|ch| {
            let first = self.grants.get(&(ch.into(), 1));
            let second = self.grants.get(&(ch.into(), 2));
            matches!((first,second),(Some(a),Some(b)) if b>a)
                && [1, 2]
                    .into_iter()
                    .all(|g| self.data.keys().any(|k| k.0 == ch && k.1 == g))
        }) && self.grants.get(&("Bars".into(), 2)) == self.grants.get(&("Quotes".into(), 2))
    }
}

/// Caller supplies SIGINT future; tests can inject cancellation without live FINAM.
/// The plan/account is never serialized into public output.
pub async fn run(
    plan: Plan,
    auth: Arc<AuthLifecycle>,
    output: &Path,
    interrupt: impl std::future::Future<Output = std::io::Result<()>>,
) -> Result<u8, &'static str> {
    plan.validate(Utc::now())?;
    if !auth.binds(&plan.account_id) {
        return Err("diagnostic_account_binding");
    }
    let mut writer = Writer::new(output, plan.max_output_bytes)?;
    use sha2::{Digest, Sha256};
    let plan_sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&plan).map_err(|_| "diagnostic_plan")?)
    );
    writer.put(
        &serde_json::json!({"event":"diagnostic_start","schema":1,"run_id":plan.run_id,
            "initial_quote_discard":plan.initial_quote_discard,
            "initial_quote_policy":"diagnostic.first-handshake.pre-fresh.512rows.262144bytes.v1",
        "start_at":plan.start_at,"latest_start_at":plan.latest_start_at,"finish_at":plan.finish_at,
        "model_invoked":false,"controlled_reconnect":false,"canonical_private_plan_sha256":plan_sha256,
        "workers":["BARS","QUOTES"],"request_budget_ms":plan.request_budget_ms,"silence_ms":plan.silence_ms,
        "reconnect_ms":plan.reconnect_ms,"quote_age_ms":plan.quote_age_ms,"dedup_capacity":plan.dedup_capacity,
        "stop_timeout_ms":plan.stop_timeout_ms,"max_output_bytes":plan.max_output_bytes}),
    )?;
    let (stop, bars) = ReadOnlyCollector::new(
        plan.finish_at,
        Duration::from_millis(plan.request_budget_ms),
    )
    .map_err(|_| "diagnostic_budget")?;
    let quotes = bars.sibling();
    let p = Arc::new(plan);
    let (cancel, rx_cancel) = watch::channel(false);
    let (tx, rx) = mpsc::channel(8);
    let active = [
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    ];
    let mut jobs = JoinSet::new();
    for (i, (channel, budget)) in [(Channel::Bars, bars), (Channel::Quotes, quotes)]
        .into_iter()
        .enumerate()
    {
        jobs.spawn(guarded(
            worker(
                channel,
                p.clone(),
                auth.clone(),
                budget,
                tx.clone(),
                active[i].clone(),
            ),
            rx_cancel.clone(),
        ));
    }
    drop(tx);
    coordinate(
        p,
        writer,
        Fleet {
            jobs,
            rx,
            active,
            cancel,
            stop,
        },
        interrupt,
    )
    .await
}

struct Fleet {
    jobs: JoinSet<Result<(), Failure>>,
    rx: mpsc::Receiver<Event>,
    active: [Arc<AtomicBool>; 2],
    cancel: watch::Sender<bool>,
    stop: crate::transport::StopHandle,
}

async fn coordinate(
    p: Arc<Plan>,
    writer: Writer,
    fleet: Fleet,
    interrupt: impl std::future::Future<Output = std::io::Result<()>>,
) -> Result<u8, &'static str> {
    coordinate_with_post_flush_clock(p, writer, fleet, interrupt, || {
        (Utc::now(), tokio::time::Instant::now())
    })
    .await
}

// Private seam: the clock is sampled only AFTER the final real write + sync_data.
async fn coordinate_with_post_flush_clock(
    p: Arc<Plan>,
    mut writer: Writer,
    fleet: Fleet,
    interrupt: impl std::future::Future<Output = std::io::Result<()>>,
    post_flush_clock: impl FnOnce() -> (DateTime<Utc>, tokio::time::Instant),
) -> Result<u8, &'static str> {
    let Fleet {
        mut jobs,
        mut rx,
        active,
        cancel,
        stop,
    } = fleet;
    let remaining = (p.finish_at - Utc::now()).to_std().unwrap_or_default();
    let deadline = tokio::time::Instant::now() + remaining;
    let mut coverage = Coverage::default();
    let mut failure = None;
    let mut joined = 0;
    let mut expected_stop = false;
    tokio::pin!(interrupt);
    loop {
        tokio::select! { biased;
            result=jobs.join_next()=>{joined+=1;failure=Some(match result {Some(Ok(Err(e)))=>e,_=>"diagnostic_worker_exit".into()});break;},
            signal=&mut interrupt=>{
                if signal.is_err(){failure=Some("diagnostic_signal".into());}
                let left=(p.finish_at-Utc::now()).num_milliseconds();
                expected_stop=(5000..=20000).contains(&left);
                break;
            },
            _=tokio::time::sleep_until(deadline)=>{failure=Some("diagnostic_guard_missing".into());break;},
            event=rx.recv()=>{
                match event {
                    Some(e)=>if let Err(e)=coverage.record(&e,&mut writer){failure=Some(e.into());break;},
                    None=>{failure=Some("diagnostic_queue_closed".into());break;}
                }
            }
        }
    }
    let owners_active = active.iter().all(|a| a.load(Ordering::SeqCst));
    cancel.send_replace(true);
    stop.stop();
    let drain = async {
        while let Some(result) = jobs.join_next().await {
            joined += 1;
            if let Err(e) = result.unwrap_or(Err("diagnostic_worker_panic".into())) {
                failure.get_or_insert(e);
            }
        }
    };
    if tokio::time::timeout(Duration::from_millis(p.stop_timeout_ms), drain)
        .await
        .is_err()
    {
        failure.get_or_insert("diagnostic_stop_timeout".into());
        jobs.abort_all();
        while jobs.join_next().await.is_some() {
            joined += 1;
        }
    }
    while let Ok(e) = rx.try_recv() {
        if let Err(e) = coverage.record(&e, &mut writer) {
            failure.get_or_insert(e.into());
        }
    }
    if Utc::now() >= p.finish_at || tokio::time::Instant::now() >= deadline {
        failure.get_or_insert("diagnostic_deadline_overrun".into());
    }
    if joined != 2 || active.iter().any(|a| a.load(Ordering::SeqCst)) {
        failure.get_or_insert("diagnostic_jobs_unaccounted".into());
    }
    let code = if failure.is_some() {
        1
    } else if expected_stop && owners_active && coverage.complete() {
        0
    } else {
        2
    };
    // No durable PASS before the last flush: qualification is a two-part receipt.
    // Pending output alone (including a crash or missing exit status) never passes.
    writer.put(&serde_json::json!({"event":"diagnostic_terminal","result":match code{0=>"DIAGNOSTIC_PENDING_EXIT",1=>"DIAGNOSTIC_FAILED",_=>"DIAGNOSTIC_INCOMPLETE"},
        "requires_successful_process_exit":true,
        "reason":failure.as_ref().map(|e|e.reason).unwrap_or(if expected_stop{"bounded_stop"}else{"early_interrupt"}),
        "primary_error":failure.as_ref().and_then(|e|e.detail.as_ref()),
        "both_socket_owners_active_before_stop":owners_active,"joined_jobs":joined,"expected_guard_window":expected_stop,
        "record_prepared_at":Utc::now(),"model_invoked":false}))?;
    let (wall, monotonic) = post_flush_clock();
    if wall >= p.finish_at || monotonic >= deadline {
        // CLI maps this fixed error to FAILED/exit 1, overriding pending evidence.
        // Do not require another synchronous write to report a failed final flush.
        return Err("diagnostic_final_flush_deadline");
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ws::{Observation, QuoteFrame};
    use std::sync::atomic::AtomicUsize;
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    #[test]
    fn initial_protocol_witness_coverage_and_output_budget() {
        let file = path();
        let mut w = Writer::new(&file, 1048576).unwrap();
        let mut c = Coverage::default();
        for e in events()
            .into_iter()
            .filter(|e| matches!(e, Event::Grant(_)))
        {
            c.record(&e, &mut w).unwrap();
        }
        let mut witness = Witness::new_auth(Channel::Quotes, 1, 1);
        for (i, o) in crate::ws::initial_quote_tests::observations()
            .iter()
            .enumerate()
        {
            for r in witness.receipts(o, i as i64 + 1) {
                c.record(&Event::Witness(r), &mut w).unwrap();
            }
            if i < 2 {
                assert!(c.data.is_empty());
            }
            assert!(!c.complete());
        }
        assert_eq!(c.data.len(), 1);
        drop(w);
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(text.matches("discarded_initial_stale_candidate").count(), 1);
        assert_eq!(text.matches("first_validated_data").count(), 1);
        std::fs::remove_file(file).unwrap();
        let file = path();
        let mut w = Writer::new(&file, 1).unwrap();
        let mut witness = Witness::new_auth(Channel::Quotes, 1, 1);
        let obs = crate::ws::initial_quote_tests::observations();
        witness.receipts(&obs[0], 1);
        let r = witness.receipts(&obs[1], 2).pop().unwrap();
        assert!(w.put(&r).is_err());
        drop(w);
        std::fs::remove_file(file).unwrap();
    }
    #[test]
    fn initial_plan_is_explicit_opt_in() {
        let p = plan();
        let mut v = serde_json::to_value(&p).unwrap();
        v.as_object_mut().unwrap().remove("initial_quote_discard");
        assert!(
            !serde_json::from_value::<Plan>(v.clone())
                .unwrap()
                .initial_quote_discard
        );
        v["initial_quote_discard"] = serde_json::json!(true);
        assert!(
            serde_json::from_value::<Plan>(v.clone())
                .unwrap()
                .initial_quote_discard
        );
        v["initial_quote_discard"] = serde_json::json!("true");
        assert!(serde_json::from_value::<Plan>(v).is_err());
    }
    #[test]
    fn last_projection_protocol_witness_coverage_writer_chain() {
        let file = path();
        let mut writer = Writer::new(&file, 1048576).unwrap();
        let mut c = Coverage::default();
        for e in events()
            .into_iter()
            .filter(|e| matches!(e, Event::Grant(_)))
        {
            c.record(&e, &mut writer).unwrap();
        }
        let mut witness = Witness::new_auth(Channel::Quotes, 1, 1);
        for (i, o) in crate::ws::snapshot_tests::last_projection_observations()
            .into_iter()
            .enumerate()
        {
            if i == 3 {
                witness = Witness::new_auth(Channel::Quotes, 2, 2);
            }
            let before = c.data.clone();
            let sent = c.sent.clone();
            for receipt in witness.receipts(&o, i as i64 + 1) {
                c.record(&Event::Witness(receipt), &mut writer).unwrap();
            }
            if i == 2 || i == 4 {
                assert_eq!(c.data, before);
                assert_eq!(c.sent, sent);
            }
            if i == 4 {
                assert!(!c.data.contains_key(&("Quotes".into(), 2, 2)));
            }
            assert!(!c.complete());
        }
        assert!(c.data.contains_key(&("Quotes".into(), 2, 2)));
        drop(writer);
        let raw = std::fs::read_to_string(&file).unwrap();
        assert_eq!(raw.matches("auxiliary_updates_observed").count(), 2);
        assert_eq!(raw.matches("first_validated_data").count(), 2);
        assert!(!raw.contains("SECRET"));
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn mixed_aux_and_data_reaches_writer_and_aux_is_budgeted() {
        let mixed = crate::ws::snapshot_tests::last_projection_observations()
            .pop()
            .unwrap();
        for budget in [1, 1048576] {
            let file = path();
            let mut writer = Writer::new(&file, 1048576).unwrap();
            let mut c = Coverage::default();
            for e in events()
                .into_iter()
                .filter(|e| matches!(e, Event::Grant(_)))
            {
                c.record(&e, &mut writer).unwrap();
            }
            let mut w = Witness::new_auth(Channel::Quotes, 2, 2);
            for r in w.receipts(&Observation::Handshake, 1) {
                c.record(&Event::Witness(r), &mut writer).unwrap();
            }
            writer.left = budget;
            let receipts = w.receipts(&mixed, 2);
            assert_eq!(receipts.len(), 2);
            let mut results = receipts
                .into_iter()
                .map(|r| c.record(&Event::Witness(r), &mut writer));
            if budget == 1 {
                assert_eq!(results.next().unwrap(), Err("diagnostic_output_budget"));
                drop(results);
                assert!(c.data.is_empty());
            } else {
                assert!(results.all(|r| r.is_ok()));
                assert_eq!(c.data.len(), 1);
            }
            std::fs::remove_file(file).unwrap();
        }
    }

    #[test]
    fn auxiliary_receipt_order_generation_and_shape_fail_closed() {
        for case in 0..8 {
            let file = path();
            let mut writer = Writer::new(&file, 1048576).unwrap();
            let mut c = Coverage::default();
            let mut w = Witness::new_auth(Channel::Quotes, 2, 2);
            for e in events()
                .into_iter()
                .filter(|e| matches!(e, Event::Grant(g) if case != 1 || g.generation != 2))
            {
                c.record(&e, &mut writer).unwrap();
            }
            let handshake = w.receipts(&Observation::Handshake, 2).pop().unwrap();
            if case != 0 && case != 1 {
                c.record(&Event::Witness(handshake), &mut writer).unwrap();
            }
            let o = crate::ws::snapshot_tests::last_projection_observations().remove(2);
            let r = w
                .receipts(&o, if case == 7 { 1 } else { 3 })
                .pop()
                .unwrap()
                .test_variant(
                    if case == 2 {
                        "unknown_event"
                    } else {
                        "auxiliary_updates_observed"
                    },
                    if case == 3 {
                        Channel::Bars
                    } else {
                        Channel::Quotes
                    },
                    usize::from(case == 4),
                    if case == 5 {
                        None
                    } else {
                        Some(if case == 6 { 0 } else { 1 })
                    },
                );
            let left = writer.left;
            assert!(
                c.record(&Event::Witness(r), &mut writer).is_err(),
                "case {case}"
            );
            assert!(c.data.is_empty());
            assert_eq!(writer.left, left);
            std::fs::remove_file(file).unwrap();
        }
    }
    fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "shadow-diagnostic-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ))
    }
    fn plan() -> Plan {
        let now = Utc::now();
        Plan {
            initial_quote_discard: false,
            schema_version: 1,
            run_id: "diagnostic-test".into(),
            account_id: "PRIVATE_ACCOUNT_SENTINEL".into(),
            instrument: "IMOEXF@RTSX".into(),
            start_at: now - chrono::Duration::seconds(1),
            latest_start_at: now + chrono::Duration::seconds(20),
            finish_at: now + chrono::Duration::seconds(1490),
            request_budget_ms: 15000,
            silence_ms: 60000,
            reconnect_ms: 5000,
            quote_age_ms: 5000,
            dedup_capacity: 4096,
            stop_timeout_ms: 1000,
            max_output_bytes: 1048576,
        }
    }
    fn observation(channel: Channel) -> Observation {
        match channel {
            Channel::Quotes => Observation::Quotes(QuoteFrame {
                initial_discard: None,
                auxiliary_total: 0,
                raw_frame: "SECRET_FRAME".into(),
                received_at_ms: 3,
                quotes: vec![crate::feedback::Quote {
                    instrument: "IMOEXF@RTSX".into(),
                    source_id: "SECRET_ID".into(),
                    price_ticks: 2,
                    event_at_ms: 2,
                    received_at_ms: 3,
                }],
            }),
            Channel::Bars => Observation::ProvisionalBars(vec![broker_core::event::Bar {
                instrument: broker_core::InstrumentId {
                    symbol: "IMOEXF".into(),
                    venue_symbol: Some("IMOEXF@RTSX".into()),
                    exchange: broker_core::Exchange::Moex,
                    market: broker_core::Market::Futures,
                },
                source_kind: broker_core::event::MarketDataSourceKind::LiveStream,
                timeframe_sec: 60,
                open_ts: Utc::now(),
                close_ts: Utc::now(),
                open: 1.into(),
                high: 1.into(),
                low: 1.into(),
                close: 1.into(),
                volume: 1.into(),
                is_final: false,
            }]),
        }
    }
    fn events() -> Vec<Event> {
        let mut out = Vec::new();
        let expiry = Utc::now() + chrono::Duration::minutes(15);
        for ch in [Channel::Bars, Channel::Quotes] {
            for generation in [1, 2] {
                out.push(Event::Grant(GrantReceipt {
                    event: "auth_validated",
                    channel: ch,
                    generation,
                    expires_at: expiry + chrono::Duration::minutes(generation as i64),
                    readonly: true,
                    selected_account_matches: true,
                    rtsx_zero_delay: true,
                }));
                let mut w = Witness::new_auth(ch, generation, generation);
                out.push(Event::Witness(
                    w.observe(&Observation::Handshake, 1).unwrap(),
                ));
                out.push(Event::Witness(w.observe(&observation(ch), 3).unwrap()));
            }
        }
        out
    }
    #[test]
    fn exact_start_and_budgets_fail_closed() {
        let mut p = plan();
        assert!(p.validate(Utc::now()).is_ok());
        assert!(p
            .validate(p.latest_start_at + chrono::Duration::milliseconds(1))
            .is_err());
        assert!(p
            .validate(p.start_at - chrono::Duration::milliseconds(1))
            .is_err());
        p.silence_ms += 1;
        assert!(p.validate(Utc::now()).is_err());
    }
    #[test]
    fn duration_limits_are_exact_not_truncated_seconds() {
        let mut p = plan();
        p.latest_start_at = p.start_at + chrono::Duration::seconds(30);
        p.finish_at = p.start_at + chrono::Duration::seconds(1500);
        assert!(p.validate(p.start_at).is_ok());
        p.latest_start_at += chrono::Duration::nanoseconds(1);
        assert!(p.validate(p.start_at).is_err());
        p.latest_start_at -= chrono::Duration::nanoseconds(1);
        p.finish_at += chrono::Duration::nanoseconds(1);
        assert!(p.validate(p.start_at).is_err());
        p.finish_at = p.latest_start_at + chrono::Duration::seconds(60);
        assert!(p.validate(p.start_at).is_ok());
        p.finish_at -= chrono::Duration::nanoseconds(1);
        assert!(p.validate(p.start_at).is_err());
    }
    #[test]
    fn history_receipts_never_complete_either_auth_generation() {
        let file = path();
        let mut writer = Writer::new(&file, 1048576).unwrap();
        let mut coverage = Coverage::default();
        for event in events() {
            if let Event::Witness(ref receipt) = event {
                let value = serde_json::to_value(receipt).unwrap();
                if value["event"] == "first_validated_data" && value["channel"] == "BARS" {
                    let g = value["auth_generation"].as_u64().unwrap();
                    let mut witness = Witness::new_auth(Channel::Bars, g, g);
                    witness.receipts(&Observation::Handshake, 1);
                    for r in witness.receipts(
                        &Observation::InitialHistory {
                            live: vec![],
                            history: crate::ws::SnapshotReceipt {
                                rows: 300,
                                min_timestamp_ms: 0,
                                max_timestamp_ms: 60_000,
                            },
                        },
                        2,
                    ) {
                        coverage.record(&Event::Witness(r), &mut writer).unwrap();
                    }
                    continue;
                }
            }
            coverage.record(&event, &mut writer).unwrap();
        }
        assert!(!coverage.complete());
        assert!(coverage.data.keys().all(|key| key.0 == "Quotes"));
        drop(writer);
        let raw = std::fs::read_to_string(&file).unwrap();
        assert!(raw.contains("snapshot_discarded"));
        assert!(!raw.contains("SECRET"));
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn both_generations_and_channels_required_no_secrets() {
        let file = path();
        let mut writer = Writer::new(&file, 1048576).unwrap();
        let mut c = Coverage::default();
        let ev = events();
        for (i, e) in ev.iter().enumerate() {
            c.record(e, &mut writer).unwrap();
            if i + 1 < ev.len() {
                assert!(!c.complete());
            }
        }
        assert!(c.complete());
        drop(writer);
        let raw = std::fs::read_to_string(&file).unwrap();
        assert!(!raw.contains("SECRET"));
        assert!(!raw.contains("PRIVATE_ACCOUNT"));
        std::fs::remove_file(file).unwrap();
    }
    #[test]
    fn heartbeat_duplicate_only_and_old_generation_do_not_qualify() {
        let mut w = Witness::new_auth(Channel::Quotes, 2, 2);
        assert!(w.observe(&Observation::Ignore, 1).is_none());
        w.observe(&Observation::Handshake, 1);
        assert!(w
            .observe(
                &Observation::Quotes(QuoteFrame {
                    initial_discard: None,
                    auxiliary_total: 0,
                    raw_frame: "duplicate".into(),
                    received_at_ms: 3,
                    quotes: vec![]
                }),
                3
            )
            .is_none());
        let file = path();
        let mut writer = Writer::new(&file, 1048576).unwrap();
        let mut c = Coverage::default();
        for e in events()
            .into_iter()
            .filter(|e| !matches!(e,Event::Grant(g) if g.generation==2))
        {
            if c.record(&e, &mut writer).is_err() {
                break;
            }
        }
        assert!(!c.complete());
        std::fs::remove_file(file).unwrap();
    }
    #[tokio::test]
    async fn cancellation_drops_pending_auth_connect_and_idle_owners() {
        for _phase in [
            "initial_auth",
            "renewal",
            "connect",
            "subscribe",
            "idle_read",
        ] {
            let flag = Arc::new(AtomicBool::new(true));
            let guard = Active(flag.clone());
            let (tx, rx) = watch::channel(false);
            let task = tokio::spawn(guarded(
                async move {
                    let _guard = guard;
                    std::future::pending::<Result<(), &'static str>>().await
                },
                rx,
            ));
            tokio::task::yield_now().await;
            tx.send_replace(true);
            assert!(tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .is_ok());
            assert!(!flag.load(Ordering::SeqCst));
        }
    }
    async fn coordinate_fixture(
        complete: bool,
        fault: Option<&'static str>,
        early: bool,
    ) -> (u8, String) {
        coordinate_flush_fixture(complete, fault, early, false, false).await
    }
    async fn coordinate_flush_fixture(
        complete: bool,
        fault: Option<&'static str>,
        early: bool,
        wall_overrun: bool,
        monotonic_overrun: bool,
    ) -> (u8, String) {
        coordinate_error_fixture(
            complete,
            fault.map(Failure::from),
            early,
            wall_overrun,
            monotonic_overrun,
        )
        .await
    }
    async fn coordinate_error_fixture(
        complete: bool,
        fault: Option<Failure>,
        early: bool,
        wall_overrun: bool,
        monotonic_overrun: bool,
    ) -> (u8, String) {
        let mut p = plan();
        p.finish_at = Utc::now() + chrono::Duration::seconds(if early { 60 } else { 19 });
        let file = path();
        let writer = Writer::new(&file, 1048576).unwrap();
        let (stop, _) = ReadOnlyCollector::new(p.finish_at, Duration::from_secs(1)).unwrap();
        let (cancel, rx_cancel) = watch::channel(false);
        let (tx, rx) = mpsc::channel(32);
        if complete {
            for e in events() {
                tx.send(e).await.unwrap();
            }
        }
        let active = [
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicBool::new(true)),
        ];
        let mut jobs = JoinSet::new();
        for (i, a) in active.iter().enumerate() {
            let guard = Active(a.clone());
            let sender = tx.clone();
            let fault = fault.clone();
            jobs.spawn(guarded(
                async move {
                    let _guard = guard;
                    let _sender = sender;
                    if i == 0 {
                        if let Some(e) = fault {
                            return Err(e);
                        }
                    }
                    std::future::pending::<Result<(), Failure>>().await
                },
                rx_cancel.clone(),
            ));
        }
        drop(tx);
        let finish = p.finish_at;
        let code = coordinate_with_post_flush_clock(
            Arc::new(p),
            writer,
            Fleet {
                jobs,
                rx,
                active: active.clone(),
                cancel,
                stop,
            },
            async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                Ok(())
            },
            || {
                // Simulate final flush reaching D on either clock independently.
                let wall = if wall_overrun { finish } else { Utc::now() };
                let monotonic = tokio::time::Instant::now()
                    + Duration::from_secs(if monotonic_overrun { 3600 } else { 0 });
                (wall, monotonic)
            },
        )
        .await;
        let code = if wall_overrun || monotonic_overrun {
            assert_eq!(code, Err("diagnostic_final_flush_deadline"));
            1 // Same error-to-exit mapping as the diagnostic CLI.
        } else {
            code.unwrap()
        };
        assert!(active.iter().all(|v| !v.load(Ordering::SeqCst)));
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::remove_file(file).unwrap();
        (code, text)
    }
    #[tokio::test]
    async fn history_cap_and_stop_fail_without_false_pass_and_join_both_jobs() {
        for cap in [false, true] {
            let error = crate::socket::tests::history_error_fixture(cap).await;
            let failure = Failure::ws(
                "diagnostic_ws_observe",
                Channel::Bars,
                crate::socket::SocketStage::Receive,
                Some(2),
                Some(2),
                error,
            );
            let (exit, text) =
                coordinate_error_fixture(false, Some(failure), false, false, false).await;
            assert_eq!(exit, 1);
            assert!(!text.contains("SECRET"));
            let terminal: serde_json::Value =
                serde_json::from_str(text.lines().last().unwrap()).unwrap();
            assert_eq!(terminal["joined_jobs"], 2);
            assert_eq!(
                terminal["primary_error"]["code"],
                if cap {
                    serde_json::json!({"Socket":{"Protocol":"Budget"}})
                } else {
                    serde_json::json!("Stopped")
                }
            );
        }
    }

    #[tokio::test]
    async fn provider_validation_and_budget_failures_survive_terminal_redacted() {
        for channel in [Channel::Bars, Channel::Quotes] {
            for case in 1..=3 {
                let error =
                    crate::socket::tests::error_fixture(matches!(channel, Channel::Quotes), case)
                        .await;
                let expected = match case {
                    1 => serde_json::json!({"Socket":{"Protocol":"Provider"}}),
                    2 if matches!(channel, Channel::Quotes) => {
                        serde_json::json!({"Socket":{"Protocol":{"Quote":"Price"}}})
                    }
                    2 => serde_json::json!({"Socket":{"Protocol":{"FrameAt":"PayloadIdentity"}}}),
                    _ => serde_json::json!("Stopped"),
                };
                let failure = Failure::ws(
                    "diagnostic_ws_observe",
                    channel,
                    crate::socket::SocketStage::Receive,
                    Some(1),
                    Some(1),
                    error,
                );
                let (exit, text) =
                    coordinate_error_fixture(false, Some(failure), false, false, false).await;
                assert_eq!(exit, 1);
                assert!(!text.contains("SECRET_RAW_ACCOUNT_TOKEN"));
                let terminal: serde_json::Value =
                    serde_json::from_str(text.lines().last().unwrap()).unwrap();
                assert_eq!(terminal["joined_jobs"], 2);
                assert_eq!(terminal["primary_error"]["code"], expected);
                assert_eq!(
                    terminal["primary_error"]["channel"],
                    serde_json::to_value(channel).unwrap()
                );
            }
        }
    }
    #[tokio::test]
    async fn actual_protocol_failures_reach_terminal_with_both_workers_joined() {
        for channel in [Channel::Bars, Channel::Quotes] {
            let error =
                crate::socket::tests::protocol_error_fixture(matches!(channel, Channel::Quotes))
                    .await;
            let failure = Failure::ws(
                "diagnostic_ws_observe",
                channel,
                crate::socket::SocketStage::Receive,
                Some(1),
                Some(1),
                error,
            );
            let (code, text) =
                coordinate_error_fixture(true, Some(failure), false, false, false).await;
            assert_eq!(code, 1);
            assert!(!text.contains("SECRET_RAW_ACCOUNT_TOKEN"));
            let terminal: serde_json::Value =
                serde_json::from_str(text.lines().last().unwrap()).unwrap();
            assert_eq!(terminal["joined_jobs"], 2);
            let primary = &terminal["primary_error"];
            assert_eq!(primary["stage"], "ProtocolReceive");
            assert_eq!(primary["auth_generation"], 1);
            assert_eq!(primary["connection_generation"], 1);
            assert_eq!(primary["channel"], serde_json::to_value(channel).unwrap());
            assert_eq!(
                primary["code"]["Socket"]["Protocol"]["FrameAt"],
                "EnvelopeJson"
            );
        }
    }
    #[tokio::test]
    async fn actual_bars_timestamp_bounds_survives_terminal_redacted() {
        let error = crate::socket::tests::error_fixture(false, 4).await;
        let failure = Failure::ws(
            "diagnostic_ws_observe",
            Channel::Bars,
            crate::socket::SocketStage::Receive,
            Some(2),
            Some(3),
            error,
        );
        let (code, text) = coordinate_error_fixture(true, Some(failure), false, false, false).await;
        assert_eq!(code, 1);
        assert!(!text.contains("SECRET_RAW_ACCOUNT_TOKEN"));
        let terminal: serde_json::Value =
            serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(terminal["joined_jobs"], 2);
        let p = &terminal["primary_error"];
        assert_eq!(p["stage"], "ProtocolReceive");
        assert_eq!(p["auth_generation"], 2);
        assert_eq!(p["connection_generation"], 3);
        assert_eq!(
            p["code"]["Socket"]["Protocol"]["FrameAt"],
            "TimestampBounds"
        );
        let d = &p["timestamp_bounds"];
        assert_eq!(d["row"], 0);
        assert_eq!(d["future"], true);
        assert_eq!(d["snapshot"], true);
        assert_eq!(d["silence_ms"], 5000);
        assert!(d["timestamp_ms"].as_i64().unwrap() > d["received_at_ms"].as_i64().unwrap());
    }
    #[tokio::test]
    async fn actual_quote_conflict_pair_survives_terminal_redacted() {
        let error = crate::socket::tests::error_fixture(true, 5).await;
        let failure = Failure::ws(
            "diagnostic_ws_observe",
            Channel::Quotes,
            crate::socket::SocketStage::Receive,
            Some(2),
            Some(3),
            error,
        );
        let (code, text) = coordinate_error_fixture(true, Some(failure), false, false, false).await;
        assert_eq!(code, 1);
        assert!(!text.contains("SECRET_RAW_ACCOUNT_TOKEN"));
        let terminal: serde_json::Value =
            serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(terminal["joined_jobs"], 2);
        let p = &terminal["primary_error"];
        assert_eq!(p["code"]["Socket"]["Protocol"]["Quote"], "Conflict");
        assert_eq!(p["channel"], "QUOTES");
        assert_eq!(p["stage"], "ProtocolReceive");
        assert_eq!(p["auth_generation"], 2);
        assert_eq!(p["connection_generation"], 3);
        assert!(p.get("timestamp_bounds").is_none());
        let d = &p["quote_conflict"];
        assert_eq!(d["row"], 0);
        assert_eq!(d["previous_from_same_frame"], false);
        assert_ne!(d["previous"]["last"], d["current"]["last"]);
        assert_eq!(d["previous"]["bid"], d["current"]["bid"]);
        assert_eq!(d["previous"]["ask"], d["current"]["ask"]);
        assert!(d.to_string().len() < 1200);
    }
    #[tokio::test]
    async fn actual_quote_time_survives_socket_and_terminal_redacted() {
        let error = crate::socket::tests::error_fixture(true, 6).await;
        let failure = Failure::ws(
            "diagnostic_ws_observe",
            Channel::Quotes,
            crate::socket::SocketStage::Receive,
            Some(1),
            Some(1),
            error,
        );
        let (code, text) = coordinate_error_fixture(true, Some(failure), false, false, false).await;
        assert_eq!(code, 1);
        assert!(!text.contains("SECRET_RAW_ACCOUNT_TOKEN"));
        let terminal: serde_json::Value =
            serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(terminal["joined_jobs"], 2);
        let p = &terminal["primary_error"];
        assert_eq!(p["code"]["Socket"]["Protocol"]["Quote"], "Time");
        assert_eq!(p["channel"], "QUOTES");
        assert_eq!(p["stage"], "ProtocolReceive");
        assert!(p.get("timestamp_bounds").is_none());
        assert!(p.get("quote_conflict").is_none());
        assert_eq!(p["quote_time"]["reason"], "StaleSource");
        assert_eq!(p["quote_time"]["row"], 0);
        assert!(p["quote_time"]["age_ms"].as_i64().unwrap() >= 30000);
        assert_eq!(p["quote_time"]["max_age_ms"], 5000);
        assert!(p["quote_time"].to_string().len() < 1200);
    }
    #[tokio::test]
    async fn coordinator_stop_accounts_for_both_jobs_and_complete_evidence() {
        let (code, text) = coordinate_fixture(true, None, false).await;
        assert_eq!(code, 0);
        assert!(text.contains("\"joined_jobs\":2"));
        assert_eq!(coordinate_fixture(false, None, false).await.0, 2);
        assert_eq!(coordinate_fixture(true, None, true).await.0, 2);
    }
    #[tokio::test]
    async fn final_flush_overrun_on_either_clock_cannot_qualify() {
        for (wall, monotonic) in [(false, false), (true, false), (false, true), (true, true)] {
            let (code, text) = coordinate_flush_fixture(true, None, false, wall, monotonic).await;
            assert_eq!(code, if wall || monotonic { 1 } else { 0 });
            assert!(text.contains("\"joined_jobs\":2"));
            assert!(text.contains("\"both_socket_owners_active_before_stop\":true"));
            assert!(text.contains("\"expected_guard_window\":true"));
            assert!(text.contains("DIAGNOSTIC_PENDING_EXIT"));
            assert!(text.contains("\"requires_successful_process_exit\":true"));
            assert!(!text.contains("DIAGNOSTIC_PASS"));
            assert!(!text.contains("finished_at"));
        }
    }
    #[tokio::test]
    async fn failures_win_even_after_rotation_and_stop_sibling() {
        for reason in [
            "diagnostic_auth",
            "diagnostic_ws_connect",
            "diagnostic_ws_observe",
        ] {
            let (code, text) = coordinate_fixture(true, Some(reason), false).await;
            assert_eq!(code, 1);
            assert!(text.contains(reason));
        }
    }
    #[test]
    fn bounded_writer_refuses_existing_file_and_overflow() {
        let file = path();
        let mut w = Writer::new(&file, 1).unwrap();
        assert!(w.put(&serde_json::json!({"event":"test"})).is_err());
        assert!(Writer::new(&file, 4096).is_err());
        std::fs::remove_file(file).unwrap();
    }
}
