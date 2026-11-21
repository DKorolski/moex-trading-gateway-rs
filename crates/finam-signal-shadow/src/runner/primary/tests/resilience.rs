use super::*;

fn observed(work: impl FnOnce(Coordinator, PathBuf)) {
    with_fixture_clock(T - 100, || {
        let (session, config, _, path) = crate::session::tests::primary_fixture_delivery(
            T,
            true,
            crate::primary::DeliveryPolicy::ObservedBo1sVolumeAuditV1,
        );
        let instrument = broker_core::InstrumentId {
            symbol: "IMOEXF".into(),
            venue_symbol: Some("IMOEXF@RTSX".into()),
            exchange: broker_core::Exchange::Moex,
            market: broker_core::Market::Futures,
        };
        let p = PrimaryProcessor::from_fixture(
            &config,
            instrument,
            (0..12).map(|i| T + i * 600000).collect(),
            1,
            T - 100,
        )
        .unwrap();
        work(Coordinator::new(p, session).unwrap(), path);
    });
}

#[test]
fn observed_real_model_dense_path_is_fast_and_needs_no_get() {
    observed(|mut o, path| {
        assert!(matches!(
            window(&mut o, T),
            Tick::Published {
                origin: "WS_LIVE",
                ..
            }
        ));
        assert_eq!(o.role_counts().proof_scheduled, 0);
        let rows = read(&path);
        let step = rows.iter().find(|r| r["kind"] == "model_step").unwrap();
        assert!(step["event"]["result_at_ms"].as_i64().unwrap() <= T + 602000);
        assert_eq!(o.processor().counters().committed, 1);
    });
}

#[test]
fn observed_startup_volume_race_1548_vs_1543_does_not_stop_or_recheck() {
    observed(|mut o, path| {
        // A missing first minute demands independently admitted REST, not a
        // synthetic dense WS capability. The numeric race is a regression,
        // not a claim that the original rejecting morning WS row was retained.
        for i in 1..10 {
            let t = T + i * 60000;
            go(t + 100);
            let mut r = raw_row(t);
            if i == 9 {
                r["volume"] = json!({"value":"1548"});
            }
            o.on_frame(crate::primary::WireFrame::normalize(&json!({"type":"DATA","subscription_type":"BARS","subscription_key":"test-key","payload":{"symbol":"IMOEXF@RTSX","bars":[r]}}).to_string(),1,"test-key",at(t+100)).unwrap()).unwrap();
        }
        go(T + 601000);
        let Tick::ProofRequired(ticket) = o.tick().unwrap() else {
            panic!("one proof")
        };
        let duplicate = ticket.clone();
        assert_eq!(ticket.deadline_ms(), T + 601900);
        go(T + 601050);
        let rows = (0..10)
            .map(|i| {
                let mut r = raw_row(T + i * 60000);
                if i == 9 {
                    r["volume"] = json!({"value":"1543"});
                }
                r
            })
            .collect();
        let response = source(&ticket, rows);
        assert!(matches!(
            o.complete_proof(ticket, Ok(response)).unwrap(),
            Tick::Published {
                origin: "REST_EXCEPTION",
                ..
            }
        ));
        assert!(o
            .complete_proof(duplicate, Err(RestFailure::Timeout))
            .is_err());
        assert_eq!(o.role_counts().proof_scheduled, 1);
        assert_eq!(o.processor().counters().committed, 1);
        let trace = read(&path);
        assert_eq!(
            trace.iter().filter(|r| r["kind"] == "model_step").count(),
            1
        );
        assert!(trace
            .iter()
            .any(|r| r["event"]["event"] == "primary_source_change"
                && r["event"]["change"]["kind"] == "VOLUME_ONLY"));
    });
}

#[test]
fn observed_volume_audit_is_not_a_false_match_or_duplicate_model_step() {
    observed(|mut o, path| {
        for i in 0..6 {
            window(&mut o, T + i * 600000);
        }
        let ticket = o.take_audit().unwrap().unwrap();
        go(now_ms() + 10);
        let rows = (0..10)
            .map(|i| {
                let mut r = raw_row(ticket.label_ms() + i * 60000);
                if i == 9 {
                    r["volume"] = json!({"value":"1548"});
                }
                r
            })
            .collect();
        let response = source(&ticket, rows);
        assert_eq!(
            o.complete_audit(ticket, Ok(response)).unwrap(),
            AuditStatus::VolumeRevision
        );
        window(&mut o, T + 6 * 600000);
        assert_eq!(o.processor().counters().committed, 7);
        assert_eq!(
            read(&path)
                .iter()
                .filter(|r| r["kind"] == "model_step")
                .count(),
            7
        );
    });
}

#[test]
fn observed_late_proof_does_not_reach_actual_model() {
    observed(|mut o, path| {
        go(T + 100);
        o.on_frame(wire(T, T + 100)).unwrap();
        go(T + 601000);
        let Tick::ProofRequired(ticket) = o.tick().unwrap() else {
            panic!("proof")
        };
        go(T + 601901);
        let rows = (0..10).map(|i| raw_row(T + i * 60000)).collect();
        let response = source(&ticket, rows);
        assert!(o.complete_proof(ticket, Ok(response)).is_err());
        assert!(!read(&path).iter().any(|r| r["kind"] == "model_step"));
    });
}
