//! Independent Chialisp compilation, before the coordinated extraction window.
fn main() {
    let bytes = chialisp_oxide_reference::compile("(mod (multiplier value) (* multiplier value))")
        .expect("compile independent Chialisp multiplier");
    for byte in bytes {
        print!("{byte:02x}");
    }
    println!();
}
