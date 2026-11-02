//! Compile the independent trusted fixture source; this performs no VM replay.
use std::fmt::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os().nth(1).ok_or("output path required")?;
    let program = chialisp_oxide_reference::compile(include_str!("../reference.clsp"))?;
    let mut hex = String::new();
    for byte in &program {
        write!(hex, "{byte:02x}")?;
    }
    hex.push('\n');
    std::fs::write(output, hex)?;
    println!("compiled reference: {} serialized bytes", program.len());
    Ok(())
}
