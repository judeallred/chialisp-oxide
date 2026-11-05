#[repr(transparent)]
pub struct PuzzleHash(pub [u8; 32]);
#[repr(transparent)]
pub struct CoinId(pub [u8; 32]);

fn rewrap(hash: PuzzleHash) -> PuzzleHash {
    PuzzleHash(hash.0)
}
pub fn puzzle(hash: PuzzleHash, coin: CoinId) -> (PuzzleHash, CoinId) {
    (rewrap(hash), CoinId(coin.0))
}
