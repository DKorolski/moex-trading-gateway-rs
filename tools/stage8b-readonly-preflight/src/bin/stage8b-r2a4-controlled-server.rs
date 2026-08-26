use stage8b_readonly_preflight::{r2a4, Operation};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let operation = match arguments.as_slice() {
        [operation] if operation == "PLACE" => Operation::Place,
        [operation] if operation == "CANCEL" => Operation::Cancel,
        _ => return Err("usage: stage8b-r2a4-controlled-server PLACE|CANCEL".into()),
    };
    r2a4::serve_controlled_tls_once(operation).await?;
    Ok(())
}
