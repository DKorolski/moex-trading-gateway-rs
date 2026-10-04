// Offline artifact probe only. Linked to the exact built release rlib.
use runtime_durable_service::{Stage8bP1RuntimeProfileV1, Stage8bP1RuntimeProfileV2};

fn main() {
    let (_, legacy) = Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
    let (_, current) = Stage8bP1RuntimeProfileV2::build_hybrid_runtime().unwrap();
    assert_ne!(legacy, current);
    println!("legacy={legacy}\nno_riskgate={current}");
}
