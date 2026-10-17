//! Host-only witness replay. Payload parsing is separate from policy behavior.
use chialisp_oxide_byte_proofs::{
    constant_false, constant_true, operator_eq, self_inequality, swap_operands,
};
use std::process::ExitCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    OperatorEq,
    SelfInequality,
    ConstantTrue,
    ConstantFalse,
    SwapOperands,
}

fn parse_variant(value: &str) -> Result<Variant, String> {
    match value {
        "operator_eq" => Ok(Variant::OperatorEq),
        "self_inequality" => Ok(Variant::SelfInequality),
        "constant_true" => Ok(Variant::ConstantTrue),
        "constant_false" => Ok(Variant::ConstantFalse),
        "swap_operands" => Ok(Variant::SwapOperands),
        _ => Err(format!("unknown source-control variant: {value}")),
    }
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if !value.len().is_multiple_of(2) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("payload must contain an even number of hexadecimal digits".into());
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let text = std::str::from_utf8(pair).map_err(|error| error.to_string())?;
            u8::from_str_radix(text, 16).map_err(|error| error.to_string())
        })
        .collect()
}

struct Invocation {
    variant: Variant,
    left: Vec<u8>,
    right: Vec<u8>,
}

fn parse_invocation(arguments: &[String]) -> Result<Invocation, String> {
    let [variant, left, right] = arguments else {
        return Err("usage: chialisp-oxide-byte-proof-native VARIANT LEFT_HEX RIGHT_HEX".into());
    };
    // Complete validation before invoking any exported policy function.
    let variant = parse_variant(variant)?;
    let left = decode_hex(left)?;
    let right = decode_hex(right)?;
    Ok(Invocation {
        variant,
        left,
        right,
    })
}

fn evaluate(input: &Invocation) -> bool {
    let (left, right) = (input.left.as_slice(), input.right.as_slice());
    match input.variant {
        Variant::OperatorEq => operator_eq::puzzle(left, right),
        Variant::SelfInequality => self_inequality::puzzle(left, right),
        Variant::ConstantTrue => constant_true::puzzle(left, right),
        Variant::ConstantFalse => constant_false::puzzle(left, right),
        Variant::SwapOperands => swap_operands::puzzle(left, right),
    }
}

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match parse_invocation(&arguments) {
        Ok(input) => {
            println!("{}", evaluate(&input));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_payload_or_variant_never_becomes_a_policy_result() {
        for arguments in [
            vec!["unknown", "", ""],
            vec!["operator_eq", "0", "00"],
            vec!["constant_false", "00", "0g"],
            vec!["constant_true", "é", "00"],
            vec!["swap_operands", "00"],
        ] {
            let arguments = arguments.into_iter().map(String::from).collect::<Vec<_>>();
            assert!(parse_invocation(&arguments).is_err());
        }
    }

    #[test]
    fn empty_and_prefixed_payloads_reach_actual_exported_functions() {
        for (variant, left, right, expected) in [
            ("operator_eq", "", "0x", true),
            ("self_inequality", "00", "0000", false),
            ("constant_true", "00", "00", true),
            ("constant_false", "00", "0000", false),
            ("swap_operands", "0xFFFE", "0xffff", true),
        ] {
            let arguments = [variant, left, right].map(String::from);
            assert_eq!(evaluate(&parse_invocation(&arguments).unwrap()), expected);
        }
    }
}
