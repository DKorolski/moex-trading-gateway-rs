use super::*;

#[test]
fn legitimate_historical_readback_is_bound_to_the_real_latest_input() {
    let t = Temp::new();
    let m = manifest();
    let first = event(&m, 0, 0);
    let second = event(&m, 0, 1);
    let mut journal = Journal::create(&t.db(), m.clone()).unwrap();
    journal.apply(first.clone()).unwrap();
    let old = journal
        .committed_input("IMOEXF", first.source_sequence)
        .unwrap()
        .unwrap();
    journal.apply(second.clone()).unwrap();
    let reread = journal
        .committed_input("IMOEXF", first.source_sequence)
        .unwrap()
        .unwrap();
    assert_eq!(reread.candle, first);
    assert_eq!(reread.revision, 1);
    assert_eq!(reread.result_ids, old.result_ids);
    assert_eq!(
        journal
            .committed_input("IMOEXF", second.source_sequence)
            .unwrap()
            .unwrap()
            .revision,
        2
    );
    drop(journal);
    let reopened = Journal::open(&t.db(), m).unwrap();
    assert_eq!(
        reopened
            .committed_input("IMOEXF", first.source_sequence)
            .unwrap()
            .unwrap()
            .result_ids,
        old.result_ids
    );
}

#[test]
fn reviewer_latest_readback_rejects_checkpoint_frontier_mutation() {
    let t = Temp::new();
    let m = manifest();
    let c = event(&m, 0, 0);
    let mut journal = Journal::create(&t.db(), m.clone()).unwrap();
    journal.apply(c.clone()).unwrap();
    assert!(journal
        .committed_input("IMOEXF", c.source_sequence)
        .unwrap()
        .is_some());
    let changed = rusqlite::Connection::open(t.db())
        .unwrap()
        .execute(
            "UPDATE arms SET source_sequence=source_sequence+1 WHERE instrument='IMOEXF'",
            [],
        )
        .unwrap();
    assert_eq!(changed, 2);
    let readback = journal.committed_input("IMOEXF", c.source_sequence);
    let reopened = Journal::open(&t.db(), m);
    println!("REVIEWER checkpoint_sequence_increment=1 changed_arms={changed} readback_accepted={} reopen_rejected={}", readback.is_ok(), reopened.is_err());
    assert!(
        reopened.is_err(),
        "control: reopen must reject inconsistent checkpoint frontier"
    );
    assert!(
        readback.is_err(),
        "checked postcommit readback accepted wrong latest checkpoint source_sequence"
    );
}

// Adopted second independent BR-1 case: one-column checkpoint identity mutation.
#[test]
fn reviewer_latest_readback_rejects_checkpoint_identity_mutation() {
    let t = Temp::new();
    let m = manifest();
    let c = event(&m, 0, 0);
    let mut journal = Journal::create(&t.db(), m.clone()).unwrap();
    journal.apply(c.clone()).unwrap();
    assert!(journal
        .committed_input("IMOEXF", c.source_sequence)
        .unwrap()
        .is_some());
    assert!(Journal::open(&t.db(), m.clone()).is_ok());
    let changed = rusqlite::Connection::open(t.db())
        .unwrap()
        .execute(
            "UPDATE arms SET instrument='RIZ6' WHERE instrument='IMOEXF'",
            [],
        )
        .unwrap();
    assert_eq!(changed, 2);
    let readback = journal.committed_input("IMOEXF", c.source_sequence);
    let reopened = Journal::open(&t.db(), m);
    println!("REVIEWER checkpoint_identity_changed_to=RIZ6 changed_arms={changed} readback_accepted={} reopen_rejected={}", readback.is_ok(), reopened.is_err());
    assert!(
        reopened.is_err(),
        "control: reopen must reject inconsistent checkpoint identity"
    );
    assert!(
        readback.is_err(),
        "checked postcommit readback accepted wrong current checkpoint instrument"
    );
}
