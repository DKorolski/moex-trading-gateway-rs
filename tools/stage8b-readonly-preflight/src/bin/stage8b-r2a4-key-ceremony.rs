use stage8b_readonly_preflight::r2a4;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let [output] = arguments.as_slice() else {
        return Err("usage: stage8b-r2a4-key-ceremony NEW_OUTPUT_DIRECTORY".into());
    };
    let values = r2a4::generate_key_ceremony(Path::new(output))?;
    println!("{}", serde_json::to_string_pretty(&values)?);
    Ok(())
}
