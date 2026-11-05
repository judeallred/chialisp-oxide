// Previous v9 source capability, with exactly the snapshot export's public ABI.
#[chialisp_oxide::puzzle]
pub fn same_public_abi(base: u64) -> (u64, u64, u64) {
    let step: fn(u64) -> u64 = |value| value;
    (step(base + 1), step(base + 2), step(base + 100))
}
