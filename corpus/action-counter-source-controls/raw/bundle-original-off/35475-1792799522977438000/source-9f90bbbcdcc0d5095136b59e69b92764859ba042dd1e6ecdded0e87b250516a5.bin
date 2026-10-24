//! Exact raw-field semantics of the canonical singleton launcher.
use chialisp_oxide::{Tree, sha256_parts};

const ATOM_DOMAIN: [u8; 1] = [1];
const PAIR_DOMAIN: [u8; 1] = [2];
const CREATE_COIN: u64 = 51;
const CREATE_COIN_ANNOUNCEMENT: u64 = 60;

type RawCreateCoin<'a> = (u64, &'a Tree<'a>, &'a Tree<'a>);
type CoinAnnouncement = (u64, [u8; 32]);
type LauncherConditions<'a> = (RawCreateCoin<'a>, CoinAnnouncement);

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &'a Tree<'a>) -> LauncherConditions<'a> {
    // The original selects three fields and leaves the final tail uninspected.
    match environment {
        Tree::Pair(puzzle_hash, rest) => match rest {
            Tree::Pair(amount, rest) => match rest {
                Tree::Pair(key_values, _) => {
                    let nil = Tree::Atom(&[]);
                    let keys = Tree::Pair(key_values, &nil);
                    let amounts = Tree::Pair(amount, &keys);
                    let values = Tree::Pair(puzzle_hash, &amounts);
                    (
                        create_coin_tree(puzzle_hash, amount),
                        announce_tree(sha256tree(&values)),
                    )
                }
                Tree::Atom(_) => panic!(),
            },
            Tree::Atom(_) => panic!(),
        },
        Tree::Atom(_) => panic!(),
    }
}

// Raw field constructors are deliberate: the module does not validate these
// trees as a typed destination/amount. Consensus validates complete spends.
fn create_coin_tree<'a>(puzzle_hash: &'a Tree<'a>, amount: &'a Tree<'a>) -> RawCreateCoin<'a> {
    (CREATE_COIN, puzzle_hash, amount)
}

fn announce_tree(message: [u8; 32]) -> CoinAnnouncement {
    (CREATE_COIN_ANNOUNCEMENT, message)
}

fn sha256tree<'a>(tree: &'a Tree<'a>) -> [u8; 32] {
    match tree {
        Tree::Atom(payload) => sha256_parts(&ATOM_DOMAIN, payload, &[]),
        Tree::Pair(first, rest) => {
            let first_hash = sha256tree(first);
            let rest_hash = sha256tree(rest);
            sha256_parts(&PAIR_DOMAIN, &first_hash, &rest_hash)
        }
    }
}
