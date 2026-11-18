//! Synthetic preparation only: clocks, atomicity and causal checkpoint restore.
use super::*;

fn inputs() -> (ContextIdentity, Vec<ContextInput>, i64) {
    let mut old = context(day(), ContextPolicy::RuntimeAcceptedOnly);
    let at = old.identity().cutoff(day());
    freeze(&mut old, day(), at);
    let mut rows: Vec<_> = old
        .frozen_anchor_inputs(day())
        .unwrap()
        .into_iter()
        .cloned()
        .collect();
    for r in &mut rows {
        r.received_at_utc = Some(at - 3);
        r.durable_at_utc = Some(at - 2);
    }
    (old.identity().clone(), rows, at - 1)
}
fn ready(s: &mut RiContextStore) {
    let at = s.identity().cutoff(day());
    freeze(s, day(), at);
    assert!(s.frozen(day()).unwrap().ready());
}

#[test]
fn ri_first_bootstrap_preserves_actual_late_receipts_and_restores_exactly() {
    let (id, rows, now) = inputs();
    let original = rows.clone();
    let mut s = RiContextStore::new(id.clone()).unwrap();
    s.bootstrap(&rows, day(), now).unwrap();
    ready(&mut s);
    assert_eq!(rows, original);
    assert_eq!(
        s.frozen(day())
            .unwrap()
            .previous
            .as_ref()
            .unwrap()
            .latest_durable_at_utc,
        now - 1
    );
    assert_eq!(
        s.frozen_anchor_inputs(day())
            .unwrap()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>(),
        rows
    );
    let bytes = s.checkpoint_bytes().unwrap();
    assert_eq!(
        RiContextStore::restore(&id, &bytes, id.cutoff(day()))
            .unwrap()
            .checkpoint_bytes()
            .unwrap(),
        bytes
    );
}

#[test]
fn ri_first_bootstrap_keeps_92_bar_anchor_rule_not_an_imoexf_grid() {
    let (id, rows, now) = inputs();
    for count in [91, 92] {
        let mut slots = std::collections::BTreeMap::<NaiveDate, usize>::new();
        let mut selected: Vec<_> = rows
            .iter()
            .filter(|r| {
                let date = crate::ri::local(r.bar.close_time_utc).unwrap().date();
                let i = slots.entry(date).or_default();
                let slot = *i;
                *i += 1;
                slot < 2 || slot >= 100 - count + 2
            })
            .cloned()
            .collect();
        for (i, r) in selected.iter_mut().enumerate() {
            r.sequence = i as u64 + 1;
        }
        let mut s = RiContextStore::new(id.clone()).unwrap();
        let result = s.bootstrap(&selected, day(), now);
        if count == 91 {
            assert!(result.is_err());
            assert_eq!(s.last_sequence(), 0);
        } else {
            result.unwrap();
            ready(&mut s);
            assert_eq!(
                s.frozen(day())
                    .unwrap()
                    .previous
                    .as_ref()
                    .unwrap()
                    .distinct_bars,
                92
            );
        }
    }
}

#[test]
fn ri_first_bootstrap_rejects_cutoff_repeat_and_bad_clocks_atomically() {
    let (id, rows, now) = inputs();
    let mut s = RiContextStore::new(id.clone()).unwrap();
    let before = s.checkpoint_bytes().unwrap();
    assert!(s.bootstrap(&rows, day(), id.cutoff(day())).is_err());
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    let mut bad = rows.clone();
    bad[0].durable_at_utc = Some(now + 1);
    assert!(s.bootstrap(&bad, day(), now).is_err());
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    s.bootstrap(&rows, day(), now).unwrap();
    let before = s.checkpoint_bytes().unwrap();
    assert!(s.bootstrap(&rows, day(), now).is_err());
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
}

#[test]
fn ri_first_bootstrap_rejects_foreign_grid_sequence_and_nonfinite_input() {
    let (id, rows, now) = inputs();
    for which in 0..6 {
        let mut bad = rows.clone();
        match which {
            0 => bad[0].bar.symbol = "RIH7".into(),
            1 => bad[0].bar.close_time_utc += 60,
            2 => bad[1].sequence += 1,
            3 => bad[0].bar.h = f64::INFINITY,
            4 => bad[0].bar.origin = DataOrigin::Replay,
            _ => bad[0].received_at_utc = None,
        }
        let mut s = RiContextStore::new(id.clone()).unwrap();
        let before = s.checkpoint_bytes().unwrap();
        assert!(s.bootstrap(&bad, day(), now).is_err(), "mutation{which}");
        assert_eq!(s.checkpoint_bytes().unwrap(), before);
    }
}

#[test]
fn ri_bootstrap_does_not_relax_later_ordinary_ingest_cutoff() {
    let (id, rows, now) = inputs();
    let mut s = RiContextStore::new(id.clone()).unwrap();
    s.bootstrap(&rows, day(), now).unwrap();
    ready(&mut s);
    let mut late = rows[0].clone();
    late.sequence = s.last_sequence() + 1;
    let at = id.cutoff(day()) + 600;
    late.received_at_utc = Some(at);
    late.durable_at_utc = Some(at);
    assert_eq!(
        s.ingest(late, at).unwrap(),
        ContextRoute::Quarantined(QuarantineReason::AtOrAfterCutoff)
    );
    assert!(RiContextStore::restore(&id, &s.checkpoint_bytes().unwrap(), at).is_ok());
}

#[test]
fn ri_recomputed_checksum_cannot_change_bootstrap_receipts_without_prefix() {
    let (id, rows, now) = inputs();
    let mut s = RiContextStore::new(id.clone()).unwrap();
    s.bootstrap(&rows, day(), now).unwrap();
    ready(&mut s);
    let original = s.checkpoint_bytes().unwrap();
    let mut v: Value = serde_json::from_slice(&original).unwrap();
    v["payload"]["bootstrap"]["inputs"][0]["received_at_utc"] = json!(now - 4);
    v["payload_sha256"] = json!(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&v["payload"]).unwrap())
    ));
    assert!(
        RiContextStore::restore(&id, &serde_json::to_vec(&v).unwrap(), id.cutoff(day())).is_err()
    );
    assert_eq!(s.checkpoint_bytes().unwrap(), original);
}
