//! Host artifact/shape/group-decoding controls, not VM or chain evidence.
use super::*;
use serde_json::json;

mod bounds;
mod curry;
mod standard;

const STANDARD_HEX: &str = concat!(
    "ff02ffff01ff02ffff03ff0bffff01ff02ffff03ffff09ff05ffff1dff0bffff1effff0bff0bffff02ff06ffff04ff02ffff04ff17ff8080808080808080",
    "ffff01ff02ff17ff2f80ffff01ff088080ff0180ffff01ff04ffff04ff04ffff04ff05ffff04ffff02ff06ffff04ff02ffff04ff17ff80808080ff80808080",
    "ffff02ff17ff2f808080ff0180ffff04ffff01ff32ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080",
    "ffff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080"
);
const KEY_HEX: &str = "9415e2b4b30977380e167096714abfdd5a594f6326dc278aa895e6bd6901953b9d8d37126e5411027eda81be8b16b97f";
const OBSERVED_REVEAL_HEX: &str = concat!(
    "ff02ffff01ff02ffff01ff02ffff03ff0bffff01ff02ffff03ffff09ff05ffff1dff0bffff1effff0bff0bffff02ff06ffff04ff02ffff04ff17ff8080808080808080",
    "ffff01ff02ff17ff2f80ffff01ff088080ff0180ffff01ff04ffff04ff04ffff04ff05ffff04ffff02ff06ffff04ff02ffff04ff17ff80808080ff80808080",
    "ffff02ff17ff2f808080ff0180ffff04ffff01ff32ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080",
    "ffff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080ffff04ffff01b0",
    "9415e2b4b30977380e167096714abfdd5a594f6326dc278aa895e6bd6901953b9d8d37126e5411027eda81be8b16b97fff018080"
);
fn bytes(value: &str) -> Vec<u8> {
    hex::decode(value).unwrap()
}
fn pair(first: &[u8], rest: &[u8]) -> Vec<u8> {
    let mut result = vec![0xff];
    result.extend_from_slice(first);
    result.extend_from_slice(rest);
    result
}
fn list(items: &[&[u8]]) -> Vec<u8> {
    items
        .iter()
        .rev()
        .fold(vec![0x80], |tail, first| pair(first, &tail))
}
fn atom(value: &[u8]) -> Vec<u8> {
    let mut result = if value.is_empty() {
        vec![0x80]
    } else if value.len() == 1 && value[0] < 0x80 {
        return value.to_vec();
    } else if value.len() < 64 {
        vec![0x80 | value.len() as u8]
    } else if value.len() < 8192 {
        vec![0xc0 | ((value.len() >> 8) as u8), value.len() as u8]
    } else {
        panic!("test atom exceeds bounded fixture domain")
    };
    result.extend_from_slice(value);
    result
}
fn curried(module: &[u8], arguments: &[Vec<u8>]) -> Vec<u8> {
    let mut environment = vec![1];
    for arg in arguments.iter().rev() {
        environment = list(&[&[4], &pair(&[1], arg), &environment]);
    }
    list(&[&[2], &pair(&[1], module), &environment])
}
fn inspect(bytes: &[u8]) -> Inspection {
    inspect_reveal(bytes, &Limits::default(), None).unwrap()
}
fn kind<T>(value: Result<T>, expected: ErrorKind) {
    match value {
        Ok(_) => panic!("expected {expected:?}"),
        Err(error) => assert_eq!(error.kind(), expected, "{error}"),
    }
}
