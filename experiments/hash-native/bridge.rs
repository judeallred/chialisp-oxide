//! Line protocol for the native-only experiment; not the puzzle compiler.
#[cfg(not(native_sha_mutant))]
#[path = "../../crates/chialisp-oxide/src/hashing.rs"]
mod hashing;
#[cfg(native_sha_mutant)]
extern crate mutated_hashing as hashing;

use std::io::{self, BufRead};

fn decode(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected even ASCII hex".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|error| error.to_string()))
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let line = line?;
        let fields: Vec<_> = line.split(':').collect();
        if fields.len() != 3 {
            return Err("expected exactly three colon-separated hex fields".into());
        }
        let a = decode(fields[0])?;
        let b = decode(fields[1])?;
        let c = decode(fields[2])?;
        let digest = hashing::sha256_parts(&a, &b, &c);
        if b.is_empty() && c.is_empty() {
            assert_eq!(hashing::sha256(&a), digest, "single-part wrapper disagrees");
        }
        for byte in digest {
            print!("{byte:02x}");
        }
        println!();
    }
    Ok(())
}
