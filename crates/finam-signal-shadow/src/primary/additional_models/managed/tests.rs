//! Synthetic qualification; never FINAM DATA or native process evidence.
use super::super::tests as f;
use super::*;
mod source_last;
use crate::{
    primary::calendar::tests::Dir,
    runner::primary::http::{HttpOutcome, HttpReceipt},
    runner::primary::{
        live::{
            continuous::{drive, Open},
            Job,
        },
        Tick,
    },
    socket::primary::{Failure, Report as SocketReport, Subscription},
    timing::{now_ms, with_fixture_clock},
};
use broker_finam::sparse_m10::*;
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use std::{collections::VecDeque, future::Future, pin::Pin};
use tokio::{
    sync::{mpsc, watch},
    time::{Duration, Instant},
};
use tokio_tungstenite::{
    tungstenite::{protocol::Role, Message},
    WebSocketStream,
};

fn at(t: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(t).unwrap()
}
fn direct(m: &mut Managed) -> Vec<Subscription> {
    m.sources
        .iter_mut()
        .map(|(i, owner, calendar)| {
            let subscription = crate::socket::primary::fixture_subscription_for(*i, 1, f::T);
            // Fresh native calendars cannot be cloned. Re-admit a separate fixture
            // through the same production builder for this direct synchronous test.
            let cal = std::mem::replace(calendar, replacement_calendar(*i));
            owner.bind_calendar(cal).unwrap();
            owner.fixture_additional_gate(ContinuityGate::new(1));
            owner.bind_subscription(&subscription).unwrap();
            subscription
        })
        .collect()
}
fn replacement_calendar(i: ShadowInstrument) -> crate::primary::calendar::AdmittedCalendar {
    f::prepared().calendars.remove(&i).unwrap()
}
fn feed(m: &mut Managed, subs: &[Subscription], slot: i64) {
    for minute in if slot == 0 { 0 } else { 1 }..=10 {
        let t = f::T + slot * 600000 + minute * 60000 + 100;
        f::go(t);
        for ((i, owner, _), sub) in m.sources.iter_mut().zip(subs) {
            let price = if *i == ShadowInstrument::Riz6 {
                "103000"
            } else {
                "100"
            };
            owner
                .on_frame(f::frame(*i, vec![f::row(t - 100, price)], t, sub))
                .unwrap();
        }
    }
    f::go(f::T + slot * 600000 + 601100);
}
#[test]
fn two_typed_coordinators_share_only_sql_not_source_frontier_or_imoexf_book() {
    with_fixture_clock(f::T, || {
        let d = Dir::new();
        let mut m = f::prepared()
            .into_managed(&d.0.join("managed"), StopHandle::default())
            .unwrap();
        let subs = direct(&mut m);
        feed(&mut m, &subs, 0);
        assert!(matches!(
            m.sources[0].1.tick().unwrap(),
            Tick::Published { .. }
        ));
        assert_eq!(
            m.sources[0].1.processor().next_label_ms(),
            Some(f::T + 600000)
        );
        assert_eq!(m.sources[1].1.processor().next_label_ms(), Some(f::T));
        assert!(matches!(
            m.sources[1].1.tick().unwrap(),
            Tick::Published { .. }
        ));
        let out = m.store.lock().unwrap().journal.export().unwrap();
        assert_eq!(out["inputs"].as_array().unwrap().len(), 2);
        assert_eq!(out["results"].as_array().unwrap().len(), 4);
        assert!(out["states"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["arm"].as_str().unwrap().starts_with("IMOEXF/"))
            .all(|s| s["ready"] == false && s["bars_processed"] == 0));
        assert!(
            reader::inspect(&m.output).is_err(),
            "no worker join means no completion"
        );
    });
}
#[test]
fn stop_between_durable_sql_and_source_advance_latches_both_lanes_and_keeps_ahead_prefix() {
    with_fixture_clock(f::T, || {
        let d = Dir::new();
        let mut m = f::prepared()
            .into_managed(&d.0.join("managed"), StopHandle::default())
            .unwrap();
        let subs = direct(&mut m);
        feed(&mut m, &subs, 0);
        m.store.lock().unwrap().stop_after_sql = true;
        assert!(m.sources[0].1.tick().is_err());
        assert!(m.stop.is_stopped());
        assert_eq!(m.sources[0].1.processor().next_label_ms(), Some(f::T));
        assert!(m.sources[1].1.tick().is_err());
        let store = m.store.lock().unwrap();
        assert!(store.held);
        assert_eq!(
            store.journal.export().unwrap()["inputs"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(reader::inspect(&m.output).is_err());
    });
}
#[test]
fn physical_owner_and_instrument_are_not_interchangeable() {
    with_fixture_clock(f::T, || {
        let d = Dir::new();
        let mut m = f::prepared()
            .into_managed(&d.0.join("managed"), StopHandle::default())
            .unwrap();
        let subs = direct(&mut m);
        let wrong = f::frame(
            ShadowInstrument::Riz6,
            vec![f::row(f::T, "100000")],
            f::T,
            &subs[1],
        );
        assert!(m.sources[0].1.on_frame(wrong).is_err());
        assert_eq!(
            m.store.lock().unwrap().journal.export().unwrap()["inputs"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    });
}
#[test]
fn no_resume_and_exact_account_validation_precede_any_network_work() {
    with_fixture_clock(f::T, || {
        let d = Dir::new();
        let path = d.0.join("managed");
        let root = StopHandle::default();
        let m = f::prepared().into_managed(&path, root.clone()).unwrap();
        assert!(f::prepared()
            .into_managed(&path, StopHandle::default())
            .is_err());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let auth = AuthLifecycle::new(
            broker_finam::SecretToken::new("SYNTHETIC_NON_CREDENTIAL"),
            "WRONG_ACCOUNT".into(),
        )
        .unwrap();
        let clients = INSTRUMENTS.map(|i| {
            Arc::new(Stage8bP1fO2GetOnlyClientV1::for_shadow_instrument(f::ACCOUNT, i).unwrap())
        });
        assert!(rt.block_on(m.run(auth, clients)).is_err());
        assert!(root.is_stopped());
        let text = std::fs::read_to_string(path.join("managed-control.jsonl")).unwrap();
        assert!(!text.contains("SYNTHETIC_NON_CREDENTIAL") && !text.contains(f::ACCOUNT));
    });
}
fn http_source(i: ShadowInstrument, plan: &ClosedM1RequestPlan) -> AdmittedClosedM1Snapshot {
    let price = if i == ShadowInstrument::Riz6 {
        "103000"
    } else {
        "100"
    };
    let body=json!({"symbol":i.venue_symbol(),"bars":(plan.start().timestamp_millis()..plan.end().timestamp_millis()).step_by(60000).map(|t|f::row(t,price)).collect::<Vec<_>>()}).to_string();
    AdmittedClosedM1Snapshot::admit_for(
        i,
        ClosedM1SnapshotEvidenceV1 {
            policy: OBSERVED_M1_POLICY_V1.into(),
            start: plan.start(),
            end: plan.end(),
            parts: vec![ClosedM1RestPartV1 {
                method: "GET".into(),
                endpoint: format!(
                    "https://api.finam.ru/v1/instruments/{}/bars",
                    i.venue_symbol()
                ),
                symbol: i.venue_symbol().into(),
                timeframe: "TIME_FRAME_M1".into(),
                start: plan.start(),
                end: plan.end(),
                requested_at: at(now_ms()),
                received_at: at(now_ms()),
                status: 200,
                transport_complete: true,
                declared_body_bytes: Some(body.len() as u64),
                response_sha256: crate::seed::digest(body.as_bytes()),
                raw_body: body,
            }],
        },
        plan,
        at(now_ms()),
    )
    .unwrap()
}
#[test]
fn typed_rest_recovery_uses_same_sql_barrier_without_resetting_other_source() {
    with_fixture_clock(f::T, || {
        let d = Dir::new();
        let mut m = f::prepared()
            .into_managed(&d.0.join("managed"), StopHandle::default())
            .unwrap();
        let subs = direct(&mut m);
        feed(&mut m, &subs, 0);
        for (_, owner, _) in &mut m.sources {
            assert!(matches!(owner.tick().unwrap(), Tick::Published { .. }));
        }
        f::go(f::T + 1201100);
        let (i, owner, _) = &mut m.sources[0];
        owner.latch_recovery().unwrap();
        let plan = owner
            .issue_recovery_request(
                f::T + 1200000,
                crate::primary::RecoveryRequestScope {
                    episode: 1,
                    ordinal: 1,
                    deadline_ms: now_ms() + 45000,
                },
                None,
            )
            .unwrap();
        let source = http_source(*i, plan.request());
        assert!(matches!(
            owner.complete_recovery_request(&plan, &source).unwrap(),
            crate::runner::primary::RecoveryProgress::Ready(1)
        ));
        assert_eq!(owner.processor().next_label_ms(), Some(f::T + 1200000));
        assert_eq!(
            m.sources[1].1.processor().next_label_ms(),
            Some(f::T + 600000)
        );
        let out = m.store.lock().unwrap().journal.export().unwrap();
        assert_eq!(out["inputs"].as_array().unwrap().len(), 3);
        let row = out["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| {
                row["candle"]["construction"]["provenance"]["canonical_receipt"]["origin"]
                    == "RECOVERY_BACKFILL"
            })
            .unwrap();
        let candle = serde_json::from_value(row["candle"].clone()).unwrap();
        let config = m.store.lock().unwrap().prepared.configs[&ShadowInstrument::Usdrubf].clone();
        let references = BTreeMap::from([(source.sha256().to_owned(), source)]);
        reader::projection(&candle, ShadowInstrument::Usdrubf, &config, 1, &references).unwrap();

        assert_eq!(out["results"].as_array().unwrap().len(), 6);
        assert!(out["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |r| r["candle"]["construction"]["provenance"]["canonical_receipt"]["origin"]
                    == "RECOVERY_BACKFILL"
            ));
    });
}

type Peer = WebSocketStream<tokio::io::DuplexStream>;
#[test]
fn late_or_lost_continuity_after_sql_does_not_advance_either_source() {
    for late in [true, false] {
        with_fixture_clock(f::T, || {
            let d = Dir::new();
            let mut m = f::prepared()
                .into_managed(&d.0.join("managed"), StopHandle::default())
                .unwrap();
            let subs = direct(&mut m);
            feed(&mut m, &subs, 0);
            {
                let mut s = m.store.lock().unwrap();
                s.after_sql_delay_ms = if late { 1000 } else { 0 };
                s.fault_after_sql = !late;
            }
            assert!(m.sources[0].1.tick().is_err());
            assert!(m.stop.is_stopped());
            assert_eq!(m.sources[0].1.processor().next_label_ms(), Some(f::T));
            assert_eq!(m.sources[1].1.processor().next_label_ms(), Some(f::T));
            assert_eq!(
                m.store.lock().unwrap().journal.export().unwrap()["inputs"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
        });
    }
}
#[test]
fn typed_ws_historical_recovery_preserves_the_second_source_and_is_not_live() {
    with_fixture_clock(f::T, || {
        let d = Dir::new();
        let mut m = f::prepared()
            .into_managed(&d.0.join("managed"), StopHandle::default())
            .unwrap();
        let subs = direct(&mut m);
        feed(&mut m, &subs, 0);
        for (_, owner, _) in &mut m.sources {
            assert!(matches!(owner.tick().unwrap(), Tick::Published { .. }));
        }
        feed(&mut m, &subs, 1);
        f::go(f::T + 1202100);
        let gate = ContinuityGate::new(1);
        assert!(gate.data_gap(1));
        let owner = &mut m.sources[0].1;
        owner.fixture_additional_gate(gate.clone());
        assert!(matches!(owner.tick().unwrap(), Tick::Degraded));
        assert!(owner
            .fixture_additional_ws_recovery(now_ms() + 45000, gate.snapshot().revision)
            .unwrap());
        assert_eq!(owner.processor().next_label_ms(), Some(f::T + 1200000));
        assert_eq!(
            m.sources[1].1.processor().next_label_ms(),
            Some(f::T + 600000)
        );
        let out = m.store.lock().unwrap().journal.export().unwrap();
        assert_eq!(out["inputs"].as_array().unwrap().len(), 3);
        let row = out["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| {
                row["candle"]["construction"]["provenance"]["canonical_receipt"]["origin"]
                    == "WS_RECOVERY_BACKFILL"
            })
            .unwrap();
        let candle = serde_json::from_value(row["candle"].clone()).unwrap();
        let config = m.store.lock().unwrap().prepared.configs[&ShadowInstrument::Usdrubf].clone();
        let references = BTreeMap::new();
        reader::projection(&candle, ShadowInstrument::Usdrubf, &config, 1, &references).unwrap();

        assert!(out["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |r| r["candle"]["construction"]["provenance"]["canonical_receipt"]["origin"]
                    == "WS_RECOVERY_BACKFILL"
            ));
    });
}
type Factory = Arc<
    dyn Fn(Open) -> Pin<Box<dyn Future<Output = Result<SocketReport, Failure>> + Send>>
        + Send
        + Sync,
>;
async fn setup(
    i: ShadowInstrument,
) -> (
    Factory,
    Vec<Peer>,
    mpsc::Receiver<watch::Receiver<Option<crate::primary::LiveDataStamp>>>,
) {
    let clients = Arc::new(Mutex::new(VecDeque::new()));
    let mut peers = Vec::new();
    for _ in 0..3 {
        let (a, b) = tokio::io::duplex(512 * 1024);
        let client = WebSocketStream::from_raw_socket(a, Role::Client, None).await;
        clients.lock().unwrap().push_back(client);
        peers.push(WebSocketStream::from_raw_socket(b, Role::Server, None).await);
    }
    let (send, receive) = mpsc::channel(4);
    let factory: Factory = Arc::new(move |request: Open| {
        let socket = clients.lock().unwrap().pop_front().unwrap();
        send.try_send(request.feedback.clone()).unwrap();
        Box::pin(crate::socket::primary::fixture_pump_for_until(
            i,
            socket,
            request.generation,
            request.stop,
            request.slot,
            request.tx,
            request.feedback,
            request.started,
            at(request.run_deadline_ms),
        ))
    });
    (factory, peers, receive)
}
async fn handshake(peer: &mut Peer, i: ShadowInstrument) {
    peer.send(Message::Text(
        r#"{"type":"EVENT","eventInfo":{"event":"HANDSHAKE_SUCCESS"}}"#.into(),
    ))
    .await
    .unwrap();
    let Some(Ok(Message::Text(text))) = peer.next().await else {
        panic!("subscribe")
    };
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["action"], "SUBSCRIBE");
    assert!(text.contains(i.venue_symbol()));
}
async fn accepted(progress: &mut watch::Receiver<Option<crate::primary::LiveDataStamp>>, t: i64) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if progress
                .borrow_and_update()
                .as_ref()
                .is_some_and(|s| s.received_at_ms >= t)
            {
                break;
            }
            progress.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}
fn control(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
async fn until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < deadline, "managed fixture timed out");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
fn rehash(rows: &mut [Value]) -> String {
    let mut head = "0".repeat(64);
    let mut text = String::new();
    for (n, r) in rows.iter_mut().enumerate() {
        r["payload"]["index"] = json!(n);
        r["payload"]["previous_sha256"] = json!(head);
        head = super::super::plan::hash(&r["payload"]).unwrap();
        r["sha256"] = json!(head);
        text.push_str(&r.to_string());
        text.push('\n');
    }
    text
}
#[test]
fn two_actual_v8_drivers_join_bounded_workers_and_matching_reader_refuses_semantic_mutations() {
    observed_pair(false);
}
#[test]
fn physical_reconnect_resubscribes_only_usd_and_continues_without_resetting_models() {
    observed_pair(true);
}
fn observed_pair(reconnect: bool) -> Dir {
    observed_pair_case(reconnect, false)
}
fn observed_pair_case(reconnect: bool, stop_after_sql: bool) -> Dir {
    observed_pair_inner(reconnect, stop_after_sql, None).0
}
// Test-only seam: exercise a real executable-opened store with the existing
// physical WS fixtures. No transport factory is exposed by production glue.
pub(in crate::primary::additional_models) fn executable_fixture(
    managed: Managed,
    directory: Dir,
) -> (Dir, Finished) {
    observed_pair_inner(true, false, Some((managed, directory)))
}
fn observed_pair_inner(
    reconnect: bool,
    stop_after_sql: bool,
    external: Option<(Managed, Dir)>,
) -> (Dir, Finished) {
    let external_run = external.is_some();
    let work = || {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async{
            let (mut managed,d)=external.unwrap_or_else(||{let d=Dir::new();let m=f::prepared().into_managed(&d.0.join("managed"),StopHandle::default()).unwrap();(m,d)});
            managed.store.lock().unwrap().stop_after_sql=stop_after_sql;
            let path=managed.output.join("managed-control.jsonl");
            let mut tasks=Vec::new();let mut feeds=Vec::new();
            for (i,mut owner,calendar) in std::mem::take(&mut managed.sources){
                let reference=calendar.reference_start_ms();owner.bind_calendar(calendar).unwrap();
                let (factory,mut peers,mut progress)=setup(i).await;
                let fetch=move|job:Job,_stop:StopHandle|async move{
                    let source=http_source(i,&job.plan().unwrap());
                    HttpOutcome{result:Ok(source),receipt:HttpReceipt{started_at_ms:now_ms(),finished_at_ms:now_ms(),deadline_ms:job.deadline_ms(),
                        get_attempts:1,auth_generation:Some(1),body:None,executor_completed:true}}
                };
                tasks.push((i,tokio::spawn(drive(owner,at(reference),move|r|factory(r),fetch))));
                let ack=progress.recv().await.unwrap();handshake(&mut peers[0],i).await;
                feeds.push((i,peers,ack,progress));
            }
            until(||control(&path).iter().filter(|r|r["payload"]["data"]["event"]["event"]=="primary_subscription_sent").count()==2).await;
            for minute in 0..=10{
                let t=f::T+minute*60000+100;f::go(t);
                for (i,peers,ack,_) in &mut feeds{
                    let peer=&mut peers[0];
                    let price=if *i==ShadowInstrument::Riz6{"103000"}else{"100"};
                    peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture",
                        "payload":{"symbol":i.venue_symbol(),"timeframe":"TIME_FRAME_M1","bars":[f::row(t-100,price)]}}).to_string())).await.unwrap();
                    accepted(ack,t).await;
                }
            }
            f::go(f::T+601100);
            if !stop_after_sql {
            for (i,peers,_ack,_) in &mut feeds{
                let peer=&mut peers[0];
                let price=if *i==ShadowInstrument::Riz6{"103000"}else{"100"};
                peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture",
                    "payload":{"symbol":i.venue_symbol(),"bars":[f::row(f::T+600000,price)]}}).to_string())).await.unwrap();
            }
            until(||control(&path).iter().filter(|r|r["payload"]["data"]["event"]["event"]=="primary_models_source_advanced").count()==2).await;
            if reconnect {
                let (i,peers,ack,progress)=&mut feeds[0];
                peers[0].close(None).await.unwrap();
                let mut next=tokio::time::timeout(Duration::from_secs(5),progress.recv()).await.unwrap().unwrap();
                handshake(&mut peers[1],*i).await;
                until(||control(&path).iter().any(|r|r["payload"]["data"]["event"]["event"]=="primary_subscription_sent"
                    &&r["payload"]["data"]["event"]["generation"]==2)).await;
                f::go(f::T+601200);
                peers[1].send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture",
                    "payload":{"symbol":i.venue_symbol(),"bars":[f::row(f::T+600000,"100")]}}).to_string())).await.unwrap();
                accepted(&mut next,f::T+601200).await;
                until(||control(&path).iter().any(|r|r["payload"]["data"]["event"]["event"]=="primary_recovery_episode_ready")).await;
                *ack=next;
                for minute in 11..=20 {
                    let t=f::T+minute*60000+100;f::go(t);
                    for (i,peers,ack,_) in &mut feeds {
                        let peer=&mut peers[if *i==ShadowInstrument::Usdrubf{1}else{0}];
                        let price=if *i==ShadowInstrument::Riz6{"103000"}else{"100"};
                        peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture",
                            "payload":{"symbol":i.venue_symbol(),"bars":[f::row(t-100,price)]}}).to_string())).await.unwrap();
                        accepted(ack,t).await;
                    }
                }
                f::go(f::T+1201100);
                for (i,peers,_,_) in &mut feeds {
                    let peer=&mut peers[if *i==ShadowInstrument::Usdrubf{1}else{0}];
                    let price=if *i==ShadowInstrument::Riz6{"103000"}else{"100"};
                    peer.send(Message::Text(json!({"type":"DATA","subscriptionType":"BARS","subscriptionKey":"fixture",
                        "payload":{"symbol":i.venue_symbol(),"bars":[f::row(f::T+1200000,price)]}}).to_string())).await.unwrap();
                }
                until(||control(&path).iter().filter(|r|r["payload"]["data"]["event"]["event"]=="primary_models_source_advanced").count()==4).await;
            }
            } else { until(||tasks.iter().all(|(_,task)|task.is_finished())).await; }
            managed.stop.stop();
            let mut reports=Vec::new();
            for (i,task) in tasks{
                let report=tokio::time::timeout(Duration::from_secs(3),task).await.unwrap().unwrap().unwrap();
                assert!(report.all_workers_drained&&report.session_finalized,"{report:?}");
                reports.push((i,Ok(report)));
            }
            let finished=finalize(&managed.store,&managed.output,reports.try_into().unwrap()).unwrap();
            let before=std::fs::read(managed.output.join("models.sqlite")).unwrap();
            let verified=reader::inspect(&managed.output).unwrap();
            if stop_after_sql {
                assert_eq!(verified["status"],"HELD","{verified}");
                assert_eq!(verified["inputs"],1);
                assert_eq!(verified["checked_inputs"],0);
                return (d,finished);
            }
            assert_eq!(verified["status"],"MANAGED_PREFIX_VERIFIED","{}\n{}",verified,control(&path).last().unwrap());assert_eq!(verified["inputs"],if reconnect{4}else{2});
            assert_eq!(before,std::fs::read(managed.output.join("models.sqlite")).unwrap());
            let original=control(&path);
            for mutation in 0..6{
                let mut rows=original.clone();
                match mutation{
                    0=>{rows.last_mut().unwrap()["payload"]["data"]["reports"][0]["result"]["Ok"]["writer_drained"]=json!(false);}
                    1=>{let r=rows.iter_mut().find(|r|r["payload"]["kind"]=="checked_readback").unwrap();r["payload"]["data"]["revision"]=json!(999);}
                    2=>{let r=rows.iter_mut().find(|r|r["payload"]["data"]["event"]["event"]=="primary_models_final_frontier").unwrap();r["payload"]["data"]["event"]["next_label_ms"]=json!(f::T);}
                    3=>{rows.last_mut().unwrap()["payload"]["data"]["reports"].as_array_mut().unwrap().pop();}
                    4=>{let r=rows.iter_mut().find(|r|r["payload"]["kind"]=="publication_intent").unwrap();r["payload"]["data"]["instrument"]=json!("imoexf");}
                    _=>{let r=rows.iter_mut().find(|r|r["payload"]["data"]["event"]["event"]=="primary_subscription_sent").unwrap();r["payload"]["data"]["event"]["generation"]=json!(2);}
                }
                // Test evidence mutation only, never production artifact edits.
                std::fs::write(&path,rehash(&mut rows)).unwrap();
                let error=reader::inspect(&managed.output).unwrap_err();
                if let Ok(retain)=std::env::var(if reconnect{"FINAM_USD_RI_RECONNECT_FIXTURE"}else{"FINAM_USD_RI_MANAGED_FIXTURE"}){
                    let root=PathBuf::from(format!("{retain}-mutants")).join(format!("case-{mutation}"));std::fs::create_dir_all(&root).unwrap();
                    for name in ["managed-control.jsonl","model-manifest.json","models.sqlite"]{std::fs::copy(managed.output.join(name),root.join(name)).unwrap();}
                    std::fs::write(root.join("expected-refusal.txt"),error.to_string()).unwrap();
                }
            }
            std::fs::write(&path,rehash(&mut original.clone())).unwrap();
            if let Ok(retain)=std::env::var(if external_run{"FINAM_USD_RI_EXECUTABLE_CORE_FIXTURE"}else if reconnect{"FINAM_USD_RI_RECONNECT_FIXTURE"}else{"FINAM_USD_RI_MANAGED_FIXTURE"}){
                let retained=PathBuf::from(retain);std::fs::create_dir(&retained).unwrap();
                for name in ["managed-control.jsonl","model-manifest.json","models.sqlite"]{std::fs::copy(managed.output.join(name),retained.join(name)).unwrap();}
            }
            (d,finished)
        })
    };
    if external_run {
        work()
    } else {
        with_fixture_clock(f::T, work)
    }
}
