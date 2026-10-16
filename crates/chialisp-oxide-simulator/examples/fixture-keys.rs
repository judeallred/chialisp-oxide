//! Reproduce public constants for the deterministic, public test keys.
//! These seeds are public fixtures and must never control real funds.

use chialisp_oxide_simulator::SecretKey;

fn main() {
    for byte in [0x17, 0x18] {
        let key = SecretKey::from_seed(&[byte; 32]);
        println!(
            "seed 0x{byte:02x}: {}",
            hex::encode(key.public_key().to_bytes())
        );
    }
}
