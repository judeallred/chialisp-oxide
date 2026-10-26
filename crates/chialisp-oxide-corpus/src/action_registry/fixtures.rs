//! Public deterministic local simulator fixture material, never custody keys.
//! Only public keys/signatures/preimages enter retained protocol records.
pub(super) const OWNER_SEED: [u8; 32] = [0x14; 32];
pub(super) const WRONG_OWNER_SEED: [u8; 32] = [0x15; 32];
pub(super) const OWNER_PUBLIC_KEY_HEX: &str = "b1b01d4389e60a12b1c9312312f706825184320ed91f2a8f16510cbea3ea900f4e6b465a045781841997b209017fb6a4";
