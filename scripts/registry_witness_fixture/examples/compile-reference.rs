//! Independent reference compilation runs before driver extraction.
fn main() {
    let multiplier = chialisp_oxide_reference::compile("(mod (FIRST SECOND) (* FIRST SECOND))")
        .expect("independent Chialisp multiply");
    print!("multiply:");
    for byte in multiplier { print!("{byte:02x}"); }
    println!();
    for size in [257, 8192] {
        let source = format!("(mod ENV (q . 0x{}))", "a5".repeat(size));
        let program = chialisp_oxide_reference::compile(&source)
            .expect("independent Chialisp quoted atom");
        print!("quote{size}:");
        for byte in program { print!("{byte:02x}"); }
        println!();
    }
}
