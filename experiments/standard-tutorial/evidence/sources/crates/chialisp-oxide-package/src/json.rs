//! Bounded JSON with recursive duplicate-key rejection before typed decoding.
use crate::{Error, ErrorKind, Limits, Result};
use serde::{
    Deserialize,
    de::{DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value};
use std::fmt;

struct Budget<'a> {
    limits: &'a Limits,
    nodes: usize,
    failure: Option<(ErrorKind, String)>,
}
impl Budget<'_> {
    fn fail<E: serde::de::Error>(&mut self, kind: ErrorKind, message: &str) -> E {
        if self.failure.is_none() {
            self.failure = Some((kind, message.into()));
        }
        E::custom(message)
    }
    fn charge<E: serde::de::Error>(&mut self, depth: usize) -> std::result::Result<(), E> {
        // Stay below serde_json's own recursive parser bound even if a host
        // caller requests a larger tree limit. This is a package JSON profile.
        if depth > self.limits.max_json_depth.min(96) {
            return Err(self.fail(
                ErrorKind::ResourceLimit,
                "package JSON depth limit exceeded",
            ));
        }
        if self.nodes >= self.limits.max_json_nodes {
            return Err(self.fail(ErrorKind::ResourceLimit, "package JSON node limit exceeded"));
        }
        self.nodes += 1;
        Ok(())
    }
}
struct Seed<'a, 'b> {
    budget: &'a mut Budget<'b>,
    depth: usize,
}
/// MapAccess invokes this seed only when a key exists. Charge before the key
/// deserializer scans escapes or allocates its String, without charging an
/// absent key when an empty or completed object reaches its closing delimiter.
struct KeySeed<'a, 'b> {
    budget: &'a mut Budget<'b>,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for KeySeed<'_, '_> {
    type Value = String;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<String, D::Error> {
        self.budget.charge(self.depth)?;
        String::deserialize(deserializer)
    }
}
impl<'de> DeserializeSeed<'de> for Seed<'_, '_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        d: D,
    ) -> std::result::Result<Value, D::Error> {
        self.budget.charge(self.depth)?;
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_, '_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded JSON")
    }
    fn visit_bool<E: serde::de::Error>(self, v: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite JSON number"))
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(v.into()))
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> std::result::Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> std::result::Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(v) = a.next_element_seed(Seed {
            budget: self.budget,
            depth: self.depth + 1,
        })? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> std::result::Result<Value, A::Error> {
        let mut out = Map::new();
        while let Some(key) = a.next_key_seed(KeySeed {
            budget: self.budget,
            depth: self.depth + 1,
        })? {
            if out.contains_key(&key) {
                return Err(self
                    .budget
                    .fail(ErrorKind::InvalidManifest, "duplicate JSON object key"));
            }
            let value = a.next_value_seed(Seed {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
            out.insert(key, value);
        }
        Ok(Value::Object(out))
    }
}
pub(crate) fn decode_value(bytes: &[u8], limits: &Limits) -> Result<Value> {
    if bytes.len() > limits.max_envelope_bytes {
        return Err(Error::new(
            ErrorKind::ResourceLimit,
            "package JSON byte limit exceeded",
        ));
    }
    let mut budget = Budget {
        limits,
        nodes: 0,
        failure: None,
    };
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let parsed = Seed {
        budget: &mut budget,
        depth: 1,
    }
    .deserialize(&mut decoder);
    let value = match parsed {
        Ok(value) => value,
        Err(error) => {
            return Err(match budget.failure {
                Some((kind, message)) => Error::new(kind, message),
                None => Error::new(
                    ErrorKind::InvalidManifest,
                    format!("invalid package JSON: {error}"),
                ),
            });
        }
    };
    decoder.end().map_err(|e| {
        Error::new(
            ErrorKind::InvalidManifest,
            format!("trailing package JSON: {e}"),
        )
    })?;
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicates_nested_and_escaped_aliases_fail() {
        for input in [
            r#"{"x":1,"x":2}"#,
            r#"{"x":{"a":1,"\u0061":2}}"#,
            r#"[{"x":1,"x":1}]"#,
        ] {
            assert_eq!(
                decode_value(input.as_bytes(), &Limits::default())
                    .unwrap_err()
                    .kind(),
                ErrorKind::InvalidManifest
            );
        }
    }
    #[test]
    fn explicit_budgets_and_complete_consumption() {
        let limits = Limits {
            max_json_depth: 3,
            max_json_nodes: 7,
            ..Limits::default()
        };
        assert!(decode_value(br#"{"a":[1,2]}"#, &limits).is_ok());
        assert_eq!(
            decode_value(br#"{"a":[[1]]}"#, &limits).unwrap_err().kind(),
            ErrorKind::ResourceLimit
        );
        let limits = Limits {
            max_json_nodes: 1,
            ..limits
        };
        assert!(decode_value(b"{}", &limits).is_ok());
        assert_eq!(
            decode_value(br#"{"a":1}"#, &limits).unwrap_err().kind(),
            ErrorKind::ResourceLimit
        );
        for bytes in [b"{}{}".as_slice(), b"{}junk", b"\xff", b""] {
            assert!(decode_value(bytes, &Limits::default()).is_err());
        }
    }
    #[test]
    fn object_key_budget_precedes_key_decoding_and_allocation() {
        let nodes = Limits {
            max_json_nodes: 1,
            ..Limits::default()
        };
        let depth = Limits {
            max_json_depth: 1,
            ..Limits::default()
        };
        // A malformed key distinguishes charging before String deserialization
        // from reading/allocating the key and only then checking its budget.
        for input in [
            br#"{"\uD800":null}"#.as_slice(),
            b"{\"\xff\":null}",
            br#"{"\u"#,
        ] {
            assert_eq!(
                decode_value(input, &Limits::default()).unwrap_err().kind(),
                ErrorKind::InvalidManifest
            );
            for limits in [&nodes, &depth] {
                assert_eq!(
                    decode_value(input, limits).unwrap_err().kind(),
                    ErrorKind::ResourceLimit
                );
            }
        }
        let large_key = format!(r#"{{"{}":null}}"#, "x".repeat(65_536));
        assert_eq!(
            decode_value(large_key.as_bytes(), &nodes)
                .unwrap_err()
                .kind(),
            ErrorKind::ResourceLimit
        );
    }
    #[test]
    fn absent_keys_and_escaped_keys_keep_exact_node_and_depth_boundaries() {
        let empty = Limits {
            max_json_nodes: 1,
            max_json_depth: 1,
            ..Limits::default()
        };
        assert_eq!(decode_value(b"{}", &empty).unwrap(), serde_json::json!({}));
        let exact = Limits {
            max_json_nodes: 3,
            max_json_depth: 2,
            ..Limits::default()
        };
        for input in [br#"{"a":{}}"#.as_slice(), br#"{"\u0061":null}"#] {
            assert!(decode_value(input, &exact).is_ok());
            let fewer = Limits {
                max_json_nodes: 2,
                ..exact.clone()
            };
            assert_eq!(
                decode_value(input, &fewer).unwrap_err().kind(),
                ErrorKind::ResourceLimit
            );
        }
        let duplicate = br#"{"a":null,"\u0061":null}"#;
        assert_eq!(
            decode_value(duplicate, &Limits::default())
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidManifest
        );
        assert_eq!(
            decode_value(duplicate, &exact).unwrap_err().kind(),
            ErrorKind::ResourceLimit
        );
    }
}
