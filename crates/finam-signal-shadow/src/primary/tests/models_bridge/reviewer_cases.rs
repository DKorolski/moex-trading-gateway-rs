use super::*;

// Adopted independent R0 review regressions, extended for R1 mutation coverage.
fn reviewer_rest_capability(
    source: &mut PrimaryProcessor,
    minutes: &[i64],
) -> Box<AdmittedCanonicalM10> {
    for &minute in minutes {
        let ms = T + minute * 60_000;
        source
            .ingest(frame(vec![source_row(ms, "100.5")], ms + 100), ms + 100)
            .unwrap();
    }
    let CanonicalStatus::PendingProof {
        reason,
        may_request,
        ..
    } = source.prepare(C + 1000).unwrap()
    else {
        panic!("expected pending REST source without WS boundary");
    };
    assert!(may_request);
    let rest = reference(
        minutes
            .iter()
            .map(|&minute| source_row(T + minute * 60_000, "100.5"))
            .collect(),
        T,
        C,
        C + 1050,
    );
    let input = source
        .prepare_rest_exception(&rest, reason, C + 1060)
        .unwrap()
        .unwrap();
    assert_eq!(input.receipt().origin, "REST_EXCEPTION");
    assert_eq!(input.receipt().proof_received_at_ms, Some(C + 1050));
    assert_eq!(input.receipt().canonical_ready_at_ms, C + 1060);
    assert!(input.rest_evidence().is_some());
    input
}

#[test]
fn reviewer_native_rest_bridge_retains_evidence_clocks_and_exact_retry() {
    let t = Temp::new();
    let m = manifest();
    let c = cfg();
    let mut b = ImoexfModelBridge::create(&t.db(), m.clone(), &c).unwrap();
    let mut source = p(&c);
    let minutes: Vec<_> = (0..10).collect();
    let input = reviewer_rest_capability(&mut source, &minutes);
    let expected_receipt = serde_json::to_value(input.receipt()).unwrap();
    let expected_rest = serde_json::to_value(input.rest_evidence()).unwrap();
    let expected_receipt_sha = input.provenance_sha256().unwrap();
    let first =
        crate::timing::with_fixture_clock(C + 1200, || b.consume(&mut source, *input)).unwrap();
    assert!(matches!(first, Commit::Committed { revision: 1, .. }));
    assert_eq!(source.counters().committed, 1);
    let before = b.export().unwrap();
    let candle = &before["inputs"][0]["candle"];
    let provenance = &candle["construction"]["provenance"];
    assert_eq!(provenance["canonical_receipt"], expected_receipt);
    assert_eq!(provenance["rest_evidence"], expected_rest);
    assert_eq!(provenance["canonical_receipt_sha256"], expected_receipt_sha);
    assert_eq!(candle["received_ms"], C + 1050);
    assert_eq!(candle["durable_ms"], C + 1060);
    assert_eq!(candle["source_processed_ms"], C + 1060);
    assert_eq!(candle["data"]["volume"], "20");
    drop(b);
    let mut b = open(&t.db(), &m, &c);
    let mut source = p(&c);
    let input = reviewer_rest_capability(&mut source, &minutes);
    let retry =
        crate::timing::with_fixture_clock(C + 1200, || b.consume(&mut source, *input)).unwrap();
    assert!(matches!(
        retry,
        Commit::AlreadyCommitted { revision: 1, .. }
    ));
    assert_eq!(source.counters().committed, 1);
    assert_eq!(b.export().unwrap(), before);
}

#[test]
fn reviewer_native_sparse_rest_bridge_does_not_invent_minutes() {
    let t = Temp::new();
    let m = manifest();
    let c = cfg();
    let mut b = ImoexfModelBridge::create(&t.db(), m, &c).unwrap();
    let mut source = p(&c);
    let input = reviewer_rest_capability(&mut source, &[0, 4, 9]);
    assert_eq!(
        input.receipt().minute_bitmap,
        (1 << 0) | (1 << 4) | (1 << 9)
    );
    let result =
        crate::timing::with_fixture_clock(C + 1200, || b.consume(&mut source, *input)).unwrap();
    assert!(matches!(result, Commit::Committed { revision: 1, .. }));
    let exported = b.export().unwrap();
    let candle = &exported["inputs"][0]["candle"];
    assert_eq!(candle["data"]["volume"], "6");
    assert_eq!(
        candle["construction"]["provenance"]["canonical_receipt"]["actual_m1"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(exported["results"].as_array().unwrap().len(), 2);
    assert_eq!(source.counters().committed, 1);
}

#[test]
fn reviewer_native_checked_readback_rejects_foreign_frontier_mutation() {
    for mutation in ["frontier", "identity"] {
        let t = Temp::new();
        let m = manifest();
        let c = cfg();
        let sequence = m
            .seeds
            .iter()
            .find(|s| s.binding.instrument == "IMOEXF")
            .unwrap()
            .history
            .len() as u64
            + 1;
        let mut b = ImoexfModelBridge::create(&t.db(), m.clone(), &c).unwrap();
        // Synthetic foreign SQL corruption only: ordinary production writer does
        // not create this trigger. No change to production bridge or its hooks.
        let install = std::process::Command::new("python3")
        .arg("-c")
        .arg(r#"import sqlite3,sys
db=sqlite3.connect(sys.argv[1])
seq=int(sys.argv[2])
change="source_sequence=NEW.source_sequence+1" if sys.argv[3]=="frontier" else "instrument='RIZ6'"
db.execute("CREATE TRIGGER reviewer_foreign_frontier AFTER UPDATE OF source_sequence ON arms WHEN NEW.instrument='IMOEXF' AND NEW.source_sequence=%d BEGIN UPDATE arms SET %s WHERE id=NEW.id; END" % (seq,change))
db.commit()
"#)
        .arg(t.db())
        .arg(sequence.to_string())
        .arg(mutation)
        .output()
        .unwrap();
        assert!(
            install.status.success(),
            "{}",
            String::from_utf8_lossy(&install.stderr)
        );
        let mut source = p(&c);
        let input = admitted(&mut source, 0);
        let result = crate::timing::with_fixture_clock(C + 1200, || b.consume(&mut source, *input));
        let committed = source.counters().committed;
        let read = std::process::Command::new("python3")
        .arg("-c")
        .arg(r#"import sqlite3,sys,json
db=sqlite3.connect(sys.argv[1])
print(json.dumps({'arms':db.execute("SELECT id,instrument,source_sequence FROM arms WHERE id LIKE 'IMOEXF/%' ORDER BY id").fetchall(),'inputs':db.execute("SELECT instrument,source_sequence,revision FROM inputs ORDER BY revision").fetchall()}))
"#)
        .arg(t.db())
        .output()
        .unwrap();
        assert!(
            read.status.success(),
            "{}",
            String::from_utf8_lossy(&read.stderr)
        );
        let sql: Value = serde_json::from_slice(&read.stdout).unwrap();
        assert_eq!(b.export().unwrap()["revision"], 1);
        let mut second_source = p(&c);
        let second_input = admitted(&mut second_source, 0);
        assert!(crate::timing::with_fixture_clock(C + 1200, || b
            .consume(&mut second_source, *second_input))
        .unwrap_err()
        .to_string()
        .contains("bridge held"));
        assert_eq!(second_source.counters().committed, 0);
        drop(b);
        let reopen = ImoexfModelBridge::open(&t.db(), m, &c);
        let reopen_error = reopen.err().map(|e| e.to_string());
        println!(
            "REVIEWER_NATIVE_FRONTIER_MUTANT {}",
            json!({
                "injection":"synthetic foreign SQL trigger; not an ordinary writer/crash state",
                "mutation":mutation,
                "requested_sequence":sequence,
                "consume_ok":result.is_ok(),
                "consume_error":result.as_ref().err().map(|e|e.to_string()),
                "producer_committed":committed,
                "sql":sql,
                "reopen_rejected":reopen_error.is_some(),
                "reopen_error":reopen_error,
            })
        );
        assert!(
            result.is_err(),
            "BR-1: foreign checkpoint frontier corruption was acknowledged by native bridge"
        );
        assert_eq!(
            committed, 0,
            "Producer must not advance after failed checked readback"
        );
    }
}
