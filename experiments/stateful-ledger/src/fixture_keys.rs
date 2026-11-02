//! Public disposable simulator seed material. Never use these for custody.
use chialisp_oxide_simulator::SecretKey;
pub const HOLDER_SEED: [u8; 32] = [0x51; 32];
pub const NEXT_SEED: [u8; 32] = [0x52; 32];
pub const WRONG_SEED: [u8; 32] = [0x53; 32];
pub fn holder() -> SecretKey {
    SecretKey::from_seed(&HOLDER_SEED)
}
pub fn next() -> SecretKey {
    SecretKey::from_seed(&NEXT_SEED)
}
pub fn wrong() -> SecretKey {
    SecretKey::from_seed(&WRONG_SEED)
}
