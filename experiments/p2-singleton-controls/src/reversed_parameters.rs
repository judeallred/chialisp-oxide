//! WIP raw-domain port of the pinned canonical p2_singleton module.
//! Its announcement binding requires a committed singleton policy; not production-ready.
use chialisp_oxide::{Tree, sha256_parts};

const ATOM_DOMAIN: [u8; 1] = [1];
const PAIR_DOMAIN: [u8; 1] = [2];
const APPLY_OPCODE: [u8; 1] = [2];
const QUOTE_OPCODE: [u8; 1] = [1];
const CONS_OPCODE: [u8; 1] = [4];
const ASSERT_PUZZLE_ANNOUNCEMENT: u64 = 63;
const CREATE_COIN_ANNOUNCEMENT: u64 = 60;
const ASSERT_MY_COIN_ID: u64 = 70;
const REWARD_ANNOUNCEMENT: [u8; 1] = *b"$";

type PuzzleAnnouncement = (u64, [u8; 32]);
type CoinAnnouncement = (u64, [u8; 1]);
type RawCoinIdAssertion<'a> = (u64, &'a Tree<'a>);
type RewardConditions<'a> = (PuzzleAnnouncement, CoinAnnouncement, RawCoinIdAssertion<'a>);

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &'a Tree<'a>) -> RewardConditions<'a> {
    // Original five projections; the tail after my_id remains uninspected.
    match environment {
        Tree::Pair(singleton_module_hash, rest) => match rest {
            Tree::Pair(launcher_id, rest) => match rest {
                Tree::Pair(launcher_puzzle_hash, rest) => match rest {
                    Tree::Pair(inner_puzzle_hash, rest) => match rest {
                        Tree::Pair(my_id, _) => claim_rewards(
                            singleton_module_hash,
                            launcher_id,
                            launcher_puzzle_hash,
                            inner_puzzle_hash,
                            my_id,
                        ),
                        Tree::Atom(_) => panic!(),
                    },
                    Tree::Atom(_) => panic!(),
                },
                Tree::Atom(_) => panic!(),
            },
            Tree::Atom(_) => panic!(),
        },
        Tree::Atom(_) => panic!(),
    }
}

fn claim_rewards<'a>(
    singleton_module_hash: &'a Tree<'a>,
    launcher_id: &'a Tree<'a>,
    launcher_puzzle_hash: &'a Tree<'a>,
    inner_puzzle_hash: &'a Tree<'a>,
    my_id: &'a Tree<'a>,
) -> RewardConditions<'a> {
    // LAUNCHER_ID and LAUNCHER_PUZZLE_HASH remain arbitrary trees, not Bytes32.
    let launcher = Tree::Pair(launcher_id, launcher_puzzle_hash);
    let singleton_struct = Tree::Pair(singleton_module_hash, &launcher);
    let struct_hash = tree_hash(&singleton_struct);
    let path_one_hash = atom_hash(&QUOTE_OPCODE);

    // The legacy helper consumes REVERSED parameter hashes: inner first,
    // singleton_struct second. These supplied "hash" atoms need not be 32 bytes.
    let after_struct = quoted_call_hash(&CONS_OPCODE, &struct_hash, &path_one_hash);
    let after_inner = quoted_call_hash(&CONS_OPCODE, atom_bytes(inner_puzzle_hash), &after_struct);
    let full_puzzle_hash = quoted_call_hash(
        &APPLY_OPCODE,
        atom_bytes(singleton_module_hash),
        &after_inner,
    );
    let message = sha256_parts(&full_puzzle_hash, atom_bytes(my_id), &[]);
    (
        assert_puzzle_announcement(message),
        create_coin_announcement(REWARD_ANNOUNCEMENT),
        assert_my_coin_id_tree(my_id),
    )
}

fn atom_bytes<'a>(tree: &'a Tree<'a>) -> &'a [u8] {
    match tree {
        Tree::Atom(bytes) => bytes,
        Tree::Pair(_, _) => panic!(),
    }
}

fn atom_hash(payload: &[u8]) -> [u8; 32] {
    sha256_parts(&ATOM_DOMAIN, payload, &[])
}

// Inputs are exact unvalidated prehash bytes, intentionally not nominal Hash32.
fn pair_hash(first: &[u8], rest: &[u8]) -> [u8; 32] {
    sha256_parts(&PAIR_DOMAIN, first, rest)
}

// Both operations hash (operation (quote . first_argument) environment).
// The named CONS/APPLY operation atoms select the two constructions above.
fn quoted_call_hash(
    operation: &[u8],
    first_argument_hash: &[u8],
    environment_hash: &[u8],
) -> [u8; 32] {
    let operation_hash = atom_hash(operation);
    let quote_hash = atom_hash(&QUOTE_OPCODE);
    let nil_hash = atom_hash(&[]);
    let quoted_first = pair_hash(&quote_hash, first_argument_hash);
    let environment_tail = pair_hash(environment_hash, &nil_hash);
    let arguments = pair_hash(&quoted_first, &environment_tail);
    pair_hash(&operation_hash, &arguments)
}

fn tree_hash<'a>(tree: &'a Tree<'a>) -> [u8; 32] {
    match tree {
        Tree::Atom(payload) => atom_hash(payload),
        Tree::Pair(first, rest) => {
            let first_hash = tree_hash(first);
            let rest_hash = tree_hash(rest);
            pair_hash(&first_hash, &rest_hash)
        }
    }
}

fn assert_puzzle_announcement(message: [u8; 32]) -> PuzzleAnnouncement {
    (ASSERT_PUZZLE_ANNOUNCEMENT, message)
}

fn create_coin_announcement(message: [u8; 1]) -> CoinAnnouncement {
    (CREATE_COIN_ANNOUNCEMENT, message)
}

fn assert_my_coin_id_tree<'a>(my_id: &'a Tree<'a>) -> RawCoinIdAssertion<'a> {
    (ASSERT_MY_COIN_ID, my_id)
}
