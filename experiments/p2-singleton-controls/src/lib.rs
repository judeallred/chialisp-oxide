//! Deliberate policy errors: the compiler cannot infer the intended protocol.
pub mod proper_struct;
pub mod reversed_parameters;
pub mod wrong_dollar;

#[cfg(test)]
mod tests {
    use chialisp_oxide::Tree;
    type Conditions<'a> = ((u64, [u8; 32]), (u64, [u8; 1]), (u64, &'a Tree<'a>));
    const EXPECTED_DIGEST: &str =
        "e2c454394ddedbf9675e1bc1afc7bd6febb16553e4219058070dfd07c1c6b483";
    fn observe(puzzle: for<'a> fn(&'a Tree<'a>) -> Conditions<'a>) -> ([u8; 32], [u8; 1]) {
        let nil = Tree::Atom(&[]);
        let module = Tree::Atom(&[0x11; 32]);
        let launcher = Tree::Atom(&[0x22; 32]);
        let launcher_hash = Tree::Atom(&[0x33; 32]);
        let inner = Tree::Atom(&[0x44; 32]);
        let id = Tree::Atom(&[0x55; 32]);
        let fifth = Tree::Pair(&id, &nil);
        let fourth = Tree::Pair(&inner, &fifth);
        let third = Tree::Pair(&launcher_hash, &fourth);
        let second = Tree::Pair(&launcher, &third);
        let environment = Tree::Pair(&module, &second);
        let (assertion, announcement, coin_id) = puzzle(&environment);
        assert_eq!(assertion.0, 63);
        assert_eq!(announcement.0, 60);
        assert_eq!(coin_id.0, 70);
        assert_eq!(*coin_id.1, id);
        (assertion.1, announcement.1)
    }
    #[test]
    fn proper_instead_of_dotted_struct_is_a_detected_policy_error() {
        let (digest, dollar) = observe(super::proper_struct::puzzle);
        assert_ne!(hex::encode(digest), EXPECTED_DIGEST);
        assert_eq!(dollar, *b"$");
        println!(
            "proper_struct digest={} dollar={}",
            hex::encode(digest),
            hex::encode(dollar)
        );
    }
    #[test]
    fn wrong_curry_order_is_a_detected_policy_error() {
        let (digest, dollar) = observe(super::reversed_parameters::puzzle);
        assert_ne!(hex::encode(digest), EXPECTED_DIGEST);
        assert_eq!(dollar, *b"$");
        println!(
            "reversed_parameters digest={} dollar={}",
            hex::encode(digest),
            hex::encode(dollar)
        );
    }
    #[test]
    fn wrong_dollar_is_a_detected_policy_error() {
        let (digest, dollar) = observe(super::wrong_dollar::puzzle);
        assert_eq!(hex::encode(digest), EXPECTED_DIGEST);
        assert_ne!(dollar, *b"$");
        println!(
            "wrong_dollar digest={} dollar={}",
            hex::encode(digest),
            hex::encode(dollar)
        );
    }
}
