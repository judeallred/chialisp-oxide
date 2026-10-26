//! WIP original teaching policy; not ready for use: original bounded teaching registry, not an upstream port.
//! Shared two-name action contract. No production readiness claim.
use chialisp_oxide::{
    AggSigMe, Cons, CreateCoinWithHint, Message32, PuzzleHash, SendPuzzleMessage, Tree,
    curry::{atom_hash, curried_puzzle_hash, pair_hash},
    sha256_parts,
};

/// A nominal opaque entry commitment; finite checks do not prove hash security.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct EntryCommitment(pub [u8; 32]);

pub type Entries = Cons<EntryCommitment, EntryCommitment>;
pub type RegistryState = Cons<bool, Entries>;
pub type RegistryTruth<'a> = Cons<Tree<'a>, RegistryState>;
pub type InitializeOutput<'a> = Cons<RegistryTruth<'a>, (CreateCoinWithHint, CreateCoinWithHint)>;
pub type SetOutput<'a> = Cons<RegistryTruth<'a>, (SendPuzzleMessage, CreateCoinWithHint, AggSigMe)>;

pub const INITIAL_VALUE: [u8; 32] = [0; 32];
pub const SLOT_AMOUNT: chialisp_oxide::Amount = chialisp_oxide::Amount::from_mojos(0);
pub const REGISTRY_SLOT_NONCE: u64 = 0x434f5852454731;
pub const REGISTRY_SLOT_NONCE_ATOM: [u8; 7] = [0x43, 0x4f, 0x58, 0x52, 0x45, 0x47, 0x31];
const ACTION_USED: [u8; 1] = [1];

// SHA256 of exact UTF-8 domains, independently recomputed by draft_spec.py.
pub const ALPHA_NAME: [u8; 32] = [
    0x94, 0x89, 0xdb, 0x9a, 0x06, 0x4d, 0x17, 0x29, 0x82, 0x73, 0x49, 0x19, 0x52, 0x8b, 0x3a, 0x7f,
    0x5e, 0x20, 0x4b, 0x93, 0xe5, 0x5f, 0x27, 0x78, 0x66, 0xc5, 0x1e, 0xe5, 0x0c, 0x6c, 0xe5, 0x00,
];
pub const BETA_NAME: [u8; 32] = [
    0x41, 0x89, 0xb7, 0x3b, 0x23, 0x7d, 0x35, 0x7f, 0x20, 0x7c, 0xda, 0x7a, 0xcb, 0x86, 0xb7, 0xe5,
    0xfb, 0xff, 0x7a, 0xe2, 0x98, 0x50, 0xdd, 0x03, 0x6f, 0x35, 0xf2, 0xf2, 0x4a, 0x26, 0xd0, 0xe0,
];
pub const SET_ALPHA_DOMAIN: [u8; 32] = [
    0xc7, 0xe4, 0x22, 0x47, 0xb3, 0x0f, 0x28, 0x0c, 0x30, 0xdc, 0x30, 0x3d, 0xa3, 0xaa, 0x4f, 0x0d,
    0x9b, 0xb2, 0x6c, 0xa0, 0x53, 0x5b, 0xd0, 0xb8, 0xcc, 0x36, 0x84, 0x35, 0x5e, 0x8d, 0x69, 0xae,
];
pub const SET_BETA_DOMAIN: [u8; 32] = [
    0xbf, 0xf7, 0xc4, 0xe8, 0x91, 0x53, 0xf4, 0x93, 0x82, 0xb6, 0x54, 0xdf, 0xb2, 0x8c, 0x90, 0xc9,
    0xca, 0xbb, 0xc1, 0x3c, 0x4c, 0x02, 0x14, 0x11, 0xe0, 0x21, 0xe2, 0x3a, 0xb8, 0x15, 0x84, 0xf6,
];

pub fn entry_commitment(name: &[u8; 32], value: &[u8; 32]) -> EntryCommitment {
    EntryCommitment(pair_hash(&atom_hash(name), &atom_hash(value)))
}

/// first_curry is the hash of actual Curry(SLOT,[controller_info,nonce]).
/// entry.0 is an atom's payload; hash that atom before curry hash arithmetic.
pub fn slot_puzzle_hash(first_curry: PuzzleHash, entry: EntryCommitment) -> PuzzleHash {
    PuzzleHash(curried_puzzle_hash(&first_curry.0, &atom_hash(&entry.0)))
}

pub fn update_intent(domain: &[u8; 32], old: &[u8; 32], new: &[u8; 32]) -> Message32 {
    Message32(sha256_parts(domain, old, new))
}

pub fn require_unused_ephemeral(ephemeral: Tree<'_>) {
    match ephemeral {
        Tree::Atom([]) => (),
        _ => panic!(),
    }
}

pub fn returned_truth<'a>(state: RegistryState) -> RegistryTruth<'a> {
    Cons {
        first: Tree::Atom(&ACTION_USED),
        rest: state,
    }
}
