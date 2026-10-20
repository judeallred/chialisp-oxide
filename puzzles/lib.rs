//! WIP teaching puzzles. Never use these fixtures with real funds.

mod fixture_keys;

#[path = "boolean-gate/puzzle.rs"]
pub mod boolean_gate;
#[path = "bounded-counter/puzzle.rs"]
pub mod bounded_counter;
#[path = "capped-payment/puzzle.rs"]
pub mod capped_payment;
#[path = "fixed-payment/puzzle.rs"]
pub mod fixed_payment;
#[path = "recovery-payment/puzzle.rs"]
pub mod recovery_payment;
#[path = "signature-payment/puzzle.rs"]
pub mod signature_payment;
#[path = "split-payment/puzzle.rs"]
pub mod split_payment;
#[path = "timelocked-payment/puzzle.rs"]
pub mod timelocked_payment;
#[path = "two-signature-payment/puzzle.rs"]
pub mod two_signature_payment;

#[path = "intro-add/puzzle.rs"]
pub mod intro_add;
#[path = "intro-conditional/puzzle.rs"]
pub mod intro_conditional;
#[path = "intro-list/puzzle.rs"]
pub mod intro_list;

#[path = "delegated-puzzle/puzzle.rs"]
pub mod delegated_puzzle;

#[path = "module-add/puzzle.rs"]
pub mod module_add;
#[path = "module-factorial/puzzle.rs"]
pub mod module_factorial;
#[path = "module-times-ten/puzzle.rs"]
pub mod module_times_ten;

#[path = "p2-conditions/puzzle.rs"]
pub mod p2_conditions;
#[path = "p2-delegated-conditions/puzzle.rs"]
pub mod p2_delegated_conditions;

#[path = "rightmost-tree/puzzle.rs"]
pub mod rightmost_tree;

#[path = "last-list-element/puzzle.rs"]
pub mod last_list_element;

#[path = "prefix-list/puzzle.rs"]
pub mod prefix_list;

#[path = "atom-not-equal/puzzle.rs"]
pub mod atom_not_equal;

#[path = "insertion-sort-by-keys/puzzle.rs"]
pub mod insertion_sort_by_keys;

#[path = "sha256tree/puzzle.rs"]
pub mod sha256tree;

#[path = "singleton-launcher/puzzle.rs"]
pub mod singleton_launcher;

#[path = "password-coin/puzzle.rs"]
pub mod password_coin;

#[path = "p2-singleton/puzzle.rs"]
pub mod p2_singleton;
