use chialisp_oxide::{Cons, Tree};
type MaybeByte = core::option::Option<u8>;
pub struct State {
    pub count: u8,
    pub amount: u64,
}
#[repr(transparent)]
pub struct Wrapped(Option<u8>);
#[chialisp_oxide::puzzle]
pub fn forward(value: MaybeByte) -> MaybeByte {
    value
}
#[chialisp_oxide::puzzle]
pub fn scalar(value: Option<u8>) -> u8 {
    match value {
        Some(n) => n + 1,
        None => 7,
    }
}
#[chialisp_oxide::puzzle]
pub fn ignored(value: Option<u8>) -> u8 {
    let _ = value;
    7
}
#[chialisp_oxide::puzzle]
pub fn unit(value: Option<()>) -> bool {
    matches!(value, Some(()))
}
#[chialisp_oxide::puzzle]
pub fn nested(value: Option<Option<u8>>) -> (bool, u8) {
    match value {
        None => (false, 0),
        Some(None) => (true, 0),
        Some(Some(v)) => (true, v),
    }
}
#[chialisp_oxide::puzzle]
pub fn tuple(value: (u16, Option<(u8, bool)>)) -> (u16, u8, bool) {
    match value.1 {
        Some((n, b)) => (value.0, n, b),
        None => (value.0, 0, false),
    }
}
#[chialisp_oxide::puzzle]
pub fn pair(value: Cons<u8, Option<u16>>) -> (u8, u16) {
    match value.rest {
        Some(n) => (value.first, n),
        None => (value.first, 0),
    }
}
#[chialisp_oxide::puzzle]
pub fn record(value: Option<State>) -> Option<State> {
    match value {
        Some(mut state) => {
            state.count = state.count + 1;
            Some(state)
        }
        None => None,
    }
}
#[chialisp_oxide::puzzle]
pub fn bytes(value: Option<[u8; 2]>) -> [u8; 2] {
    match value {
        Some(bytes) => bytes,
        None => [0, 0],
    }
}
#[chialisp_oxide::puzzle]
pub fn if_let(value: Option<u8>) -> u8 {
    if let Some(n) = value { n } else { 9 }
}
#[chialisp_oxide::puzzle]
pub fn snapshot(mut value: Option<u8>, replacement: u8) -> (Option<u8>, u8) {
    let old = match value {
        Some(v) => v,
        None => 0,
    };
    value = Some(replacement);
    (value, old)
}
#[chialisp_oxide::puzzle]
pub fn guarded(value: Option<u8>, divisor: u8) -> u8 {
    match value {
        None => 7,
        Some(v) => v / divisor,
    }
}
#[chialisp_oxide::puzzle]
pub fn cast(value: Option<u16>) -> u8 {
    match value {
        Some(v) => v as u8,
        None => 0,
    }
}
fn maybe(choose: bool, n: u8) -> Option<u8> {
    if choose { Some(n) } else { None }
}
#[chialisp_oxide::puzzle]
pub fn from_helper(choose: bool, n: u8) -> u8 {
    match maybe(choose, n) {
        Some(v) => v,
        None => 9,
    }
}
#[chialisp_oxide::puzzle]
pub fn array_input(value: [Option<u8>; 2]) -> u8 {
    let _ = value;
    7
}
#[chialisp_oxide::puzzle]
pub fn wrapped(value: Wrapped) -> u8 {
    match value.0 {
        Some(v) => v,
        None => 9,
    }
}
#[chialisp_oxide::puzzle]
pub fn strict(value: Option<u8>, divisor: u8) -> u8 {
    let _unused = Some(9 / divisor);
    match value {
        None => 7,
        Some(v) => v,
    }
}
#[chialisp_oxide::puzzle]
pub fn raw<'a>(value: Option<&'a Tree<'a>>) -> Option<&'a Tree<'a>> {
    match value {
        None => None,
        Some(v) => Some(v),
    }
}
#[chialisp_oxide::puzzle]
pub fn increment(value: Option<u8>) -> Option<u8> {
    match value {
        Some(n) => Some(n + 1),
        None => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fail<T>(run: impl FnOnce() -> T + std::panic::UnwindSafe) {
        let error = std::panic::catch_unwind(run).err().expect("policy failure");
        assert!(!error.is::<chialisp_oxide::ResourceExhausted>());
        assert!(error.is::<String>() || error.is::<&str>());
    }
    #[test]
    fn actual_native_optional_values() {
        assert_eq!(increment(None), None);
        assert_eq!(increment(Some(7)), Some(8));
        fail(|| increment(Some(255)));
        assert_eq!(forward(None), None);
        assert_eq!(forward(Some(0)), Some(0));
        assert_eq!(scalar(None), 7);
        assert_eq!(scalar(Some(7)), 8);
        fail(|| scalar(Some(255)));
        assert_eq!(ignored(Some(255)), 7);
        assert!(!unit(None));
        assert!(unit(Some(())));
        assert_eq!(nested(None), (false, 0));
        assert_eq!(nested(Some(None)), (true, 0));
        assert_eq!(nested(Some(Some(7))), (true, 7));
        assert_eq!(tuple((513, Some((7, true)))), (513, 7, true));
        assert_eq!(tuple((513, None)), (513, 0, false));
        assert_eq!(
            pair(Cons {
                first: 7,
                rest: Some(513)
            }),
            (7, 513)
        );
        assert_eq!(
            pair(Cons {
                first: 7,
                rest: None
            }),
            (7, 0)
        );
        let value = record(Some(State {
            count: 7,
            amount: 513,
        }))
        .unwrap();
        assert_eq!((value.count, value.amount), (8, 513));
        assert!(record(None).is_none());
        fail(|| {
            record(Some(State {
                count: 255,
                amount: 513,
            }))
        });
        assert_eq!(bytes(Some([0, 1])), [0, 1]);
        assert_eq!(bytes(None), [0, 0]);
        assert_eq!(if_let(Some(7)), 7);
        assert_eq!(if_let(None), 9);
        assert_eq!(snapshot(Some(7), 9), (Some(9), 7));
        assert_eq!(snapshot(None, 9), (Some(9), 0));
        assert_eq!(guarded(None, 0), 7);
        assert_eq!(guarded(Some(9), 3), 3);
        fail(|| guarded(Some(9), 0));
        assert_eq!(cast(Some(513)), 1);
        assert_eq!(cast(None), 0);
        assert_eq!(from_helper(true, 7), 7);
        assert_eq!(from_helper(false, 7), 9);
        assert_eq!(array_input([None, Some(255)]), 7);
        assert_eq!(wrapped(Wrapped(Some(7))), 7);
        assert_eq!(wrapped(Wrapped(None)), 9);
        assert_eq!(strict(None, 3), 7);
        fail(|| strict(None, 0));
        let nil = Tree::Atom(&[]);
        let left = Tree::Atom(&[0, 1]);
        let right = Tree::Atom(&[255]);
        let pair = Tree::Pair(&left, &right);
        assert_eq!(raw(None), None);
        assert_eq!(raw(Some(&nil)), Some(&nil));
        assert_eq!(raw(Some(&pair)), Some(&pair));
    }
}
