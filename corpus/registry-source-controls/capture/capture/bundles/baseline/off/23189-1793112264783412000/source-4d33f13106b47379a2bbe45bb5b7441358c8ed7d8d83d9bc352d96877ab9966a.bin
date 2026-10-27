//! Exact raw contract of the tutorial module. Public passwords are not secure authorization.
use chialisp_oxide::{Tree, sha256};

#[chialisp_oxide::puzzle(environment = "raw")]
pub fn puzzle<'a>(environment: &'a Tree<'a>) -> &'a Tree<'a> {
    match environment {
        Tree::Pair(expected_hash, remainder) => match remainder {
            Tree::Pair(password, after_password) => {
                let password_bytes = match password {
                    Tree::Atom(bytes) => *bytes,
                    Tree::Pair(_, _) => panic!(),
                };
                let digest = sha256(password_bytes);
                let expected_bytes = match expected_hash {
                    Tree::Atom(bytes) => *bytes,
                    Tree::Pair(_, _) => panic!(),
                };
                let digest_bytes: &[u8] = &digest;
                assert!(digest_bytes == expected_bytes);
                match after_password {
                    Tree::Pair(conditions, _) => conditions,
                    Tree::Atom(_) => panic!(),
                }
            }
            Tree::Atom(_) => panic!(),
        },
        Tree::Atom(_) => panic!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_native_bytes_and_subtree_passthrough() {
        let digest = sha256(b"abc");
        let hash = Tree::Atom(&digest);
        let password = Tree::Atom(b"abc");
        let tail = Tree::Atom(&[0, 0]);
        let condition_head = Tree::Atom(&[8]);
        let conditions = Tree::Pair(&condition_head, &tail);
        let third = Tree::Pair(&conditions, &tail);
        let second = Tree::Pair(&password, &third);
        let environment = Tree::Pair(&hash, &second);
        assert!(std::ptr::eq(puzzle(&environment), &conditions));
        let wrong = Tree::Atom(b"xyz");
        let bad_second = Tree::Pair(&wrong, &third);
        let bad_environment = Tree::Pair(&hash, &bad_second);
        assert!(std::panic::catch_unwind(|| puzzle(&bad_environment)).is_err());
    }
}
