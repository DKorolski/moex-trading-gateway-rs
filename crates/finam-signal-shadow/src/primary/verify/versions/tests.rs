use super::*;
use crate::primary::calendar::tests::{at, Fixture};
use broker_finam::sparse_m10::{AdmittedClosedM1Snapshot, ClosedM1RequestPlan};
use serde_json::json;
#[test]
fn consistent_v5_reference_reader_accepts_price_version_but_not_inventory_change() {
    let f = Fixture::new();
    crate::timing::with_fixture_clock(f.open(), || {
        let plan =
            ClosedM1RequestPlan::new(at(f.open() - 120000), at(f.open()), vec![at(f.open())])
                .unwrap();
        let base = f.rest(&plan);
        let version = |mode: &str| {
            let mut e = base.evidence().clone();
            let p = &mut e.parts[0];
            let mut v: Value = serde_json::from_str(&p.raw_body).unwrap();
            v["bars"][0]["close"] = json!({"value":if mode=="old" {"2200.5"}else{"2200"}});
            if mode != "extra" {
                v["bars"].as_array_mut().unwrap().remove(1);
            }
            if mode == "missing" {
                v["bars"] = json!([f.row(f.open() - 60000)]);
            }
            p.raw_body = v.to_string();
            p.declared_body_bytes = Some(p.raw_body.len() as u64);
            p.response_sha256 = digest(p.raw_body.as_bytes());
            AdmittedClosedM1Snapshot::admit(e, &plan, at(f.open())).unwrap()
        };
        for mode in ["price", "missing", "extra"] {
            let mut d = Diagnostics::default();
            d.bind_reference(&version("old")).unwrap();
            assert_eq!(d.bind_reference(&version(mode)).is_ok(), mode == "price");
        }
    });
}
