//! Bounded routing before a Cargo manifest can allocate a serde_json::Value.
//! The walk retains only routing fields and bounded pointer keys/strings.
//! Complete schema-2 admission remains the package reader's responsibility.
use chialisp_oxide_package::{Error, ErrorKind, Limits, Result};
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{collections::BTreeSet, fmt};

#[derive(Default)]
pub(super) struct Pointer {
    pub manifest: Option<String>,
    pub bundle: Option<String>,
    pub fields: usize,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Root {
    Pointer,
    Manifest,
    Nested,
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Key {
    Manifest,
    Bundle,
    Schema,
    Receipt,
    Other,
}
impl Key {
    fn from_str(value: &str) -> Self {
        match value {
            "manifest" => Self::Manifest,
            "bundle" => Self::Bundle,
            "schema_version" => Self::Schema,
            "source_receipt" => Self::Receipt,
            _ => Self::Other,
        }
    }
}
enum Scalar {
    Unsigned(u64),
    Text(String),
    Other,
}
struct Scan<'a> {
    limits: &'a Limits,
    nodes: usize,
    failure: Option<(ErrorKind, &'static str)>,
    pointer: Pointer,
    pointer_keys: BTreeSet<String>,
    manifest_seen: bool,
    bundle_seen: bool,
    schema_seen: bool,
    schema: Option<u64>,
    receipt: bool,
}
impl Scan<'_> {
    fn fail<E: serde::de::Error>(&mut self, kind: ErrorKind, message: &'static str) -> E {
        self.failure.get_or_insert((kind, message));
        E::custom(message)
    }
    fn charge<E: serde::de::Error>(&mut self, depth: usize) -> std::result::Result<(), E> {
        if depth > self.limits.max_json_depth.min(96) {
            return Err(self.fail(
                ErrorKind::ResourceLimit,
                "Cargo routing JSON depth limit exceeded",
            ));
        }
        if self.nodes >= self.limits.max_json_nodes {
            return Err(self.fail(
                ErrorKind::ResourceLimit,
                "Cargo routing JSON node limit exceeded",
            ));
        }
        self.nodes += 1;
        Ok(())
    }
    fn field<E: serde::de::Error>(&mut self, root: Root, key: Key) -> std::result::Result<(), E> {
        let duplicate = match (root, key) {
            (Root::Pointer, Key::Manifest) => std::mem::replace(&mut self.manifest_seen, true),
            (Root::Pointer, Key::Bundle) => std::mem::replace(&mut self.bundle_seen, true),
            (Root::Manifest, Key::Schema) => std::mem::replace(&mut self.schema_seen, true),
            (Root::Manifest, Key::Receipt) => std::mem::replace(&mut self.receipt, true),
            _ => false,
        };
        if duplicate {
            return Err(self.fail(
                ErrorKind::InvalidManifest,
                "duplicate Cargo routing JSON field",
            ));
        }
        if root == Root::Pointer {
            // Each field was charged as a JSON node before this addition.
            self.pointer.fields += 1;
        }
        Ok(())
    }
}
struct KeySeed<'a, 'b> {
    scan: &'a mut Scan<'b>,
    depth: usize,
    pointer_key: bool,
}
struct KeyVisitor<'a, 'b> {
    scan: &'a mut Scan<'b>,
    pointer_key: bool,
}
impl<'de> Visitor<'de> for KeyVisitor<'_, '_> {
    type Value = Key;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON field name")
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Key, E> {
        if self.pointer_key {
            if value.len() > 4096 {
                return Err(self
                    .scan
                    .fail(ErrorKind::ResourceLimit, "Cargo pointer key limit exceeded"));
            }
            if self.scan.pointer_keys.contains(value) {
                return Err(self.scan.fail(
                    ErrorKind::InvalidManifest,
                    "duplicate Cargo pointer JSON field",
                ));
            }
            let mut key = String::new();
            key.try_reserve_exact(value.len()).map_err(|_| {
                self.scan.fail::<E>(
                    ErrorKind::ResourceLimit,
                    "Cargo pointer key allocation failed",
                )
            })?;
            key.push_str(value);
            self.scan.pointer_keys.insert(key);
        }
        Ok(Key::from_str(value))
    }
}
impl<'de> DeserializeSeed<'de> for KeySeed<'_, '_> {
    type Value = Key;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> std::result::Result<Key, D::Error> {
        self.scan.charge(self.depth)?;
        if self.pointer_key && self.scan.pointer.fields >= self.scan.limits.max_objects {
            return Err(self.scan.fail(
                ErrorKind::ResourceLimit,
                "Cargo pointer field limit exceeded",
            ));
        }
        d.deserialize_str(KeyVisitor {
            scan: self.scan,
            pointer_key: self.pointer_key,
        })
    }
}
struct Seed<'a, 'b> {
    scan: &'a mut Scan<'b>,
    depth: usize,
    root: Root,
    keep_text: bool,
}
impl<'de> DeserializeSeed<'de> for Seed<'_, '_> {
    type Value = Scalar;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        d: D,
    ) -> std::result::Result<Scalar, D::Error> {
        self.scan.charge(self.depth)?;
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_, '_> {
    type Value = Scalar;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded Cargo routing JSON")
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<Scalar, E> {
        Ok(Scalar::Other)
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Scalar, E> {
        Ok(Scalar::Other)
    }
    fn visit_u64<E: serde::de::Error>(self, value: u64) -> std::result::Result<Scalar, E> {
        Ok(Scalar::Unsigned(value))
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Scalar, E> {
        Ok(Scalar::Other)
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Scalar, E> {
        Ok(Scalar::Other)
    }
    fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Scalar, E> {
        Ok(Scalar::Other)
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Scalar, E> {
        if !self.keep_text {
            return Ok(Scalar::Other);
        }
        if value.len() > 4096 {
            return Err(self.scan.fail(
                ErrorKind::ResourceLimit,
                "Cargo pointer string limit exceeded",
            ));
        }
        let mut text = String::new();
        text.try_reserve_exact(value.len()).map_err(|_| {
            self.scan.fail::<E>(
                ErrorKind::ResourceLimit,
                "Cargo pointer string allocation failed",
            )
        })?;
        text.push_str(value);
        Ok(Scalar::Text(text))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> std::result::Result<Scalar, A::Error> {
        while access
            .next_element_seed(Seed {
                scan: self.scan,
                depth: self.depth + 1,
                root: Root::Nested,
                keep_text: false,
            })?
            .is_some()
        {}
        Ok(Scalar::Other)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> std::result::Result<Scalar, A::Error> {
        while let Some(key) = access.next_key_seed(KeySeed {
            scan: self.scan,
            depth: self.depth + 1,
            pointer_key: self.root == Root::Pointer,
        })? {
            self.scan.field(self.root, key)?;
            let keep_text =
                self.root == Root::Pointer && matches!(key, Key::Manifest | Key::Bundle);
            let value = access.next_value_seed(Seed {
                scan: self.scan,
                depth: self.depth + 1,
                root: Root::Nested,
                keep_text,
            })?;
            match (self.root, key, value) {
                (Root::Pointer, Key::Manifest, Scalar::Text(value)) => {
                    self.scan.pointer.manifest = Some(value)
                }
                (Root::Pointer, Key::Bundle, Scalar::Text(value)) => {
                    self.scan.pointer.bundle = Some(value)
                }
                (Root::Manifest, Key::Schema, Scalar::Unsigned(value)) => {
                    self.scan.schema = Some(value)
                }
                _ => (),
            }
        }
        Ok(Scalar::Other)
    }
}

fn scan<'a>(bytes: &[u8], limits: &'a Limits, root: Root) -> Result<Scan<'a>> {
    if bytes.len() > limits.max_envelope_bytes {
        return Err(Error::new(
            ErrorKind::ResourceLimit,
            "Cargo routing JSON byte limit exceeded",
        ));
    }
    let mut scan = Scan {
        limits,
        nodes: 0,
        failure: None,
        pointer: Pointer::default(),
        pointer_keys: BTreeSet::new(),
        manifest_seen: false,
        bundle_seen: false,
        schema_seen: false,
        schema: None,
        receipt: false,
    };
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let parsed = Seed {
        scan: &mut scan,
        depth: 1,
        root,
        keep_text: false,
    }
    .deserialize(&mut decoder);
    if let Err(error) = parsed {
        return Err(match scan.failure {
            Some((kind, message)) => Error::new(kind, message),
            None => Error::new(
                ErrorKind::InvalidManifest,
                format!("invalid Cargo routing JSON: {error}"),
            ),
        });
    }
    decoder.end().map_err(|error| {
        Error::new(
            ErrorKind::InvalidManifest,
            format!("trailing Cargo routing JSON: {error}"),
        )
    })?;
    Ok(scan)
}
pub(super) fn pointer(bytes: &[u8], limits: &Limits) -> Result<Pointer> {
    Ok(scan(bytes, limits, Root::Pointer)?.pointer)
}
pub(super) fn source_aware(bytes: &[u8], limits: &Limits) -> Result<bool> {
    let scan = scan(bytes, limits, Root::Manifest)?;
    Ok(scan.schema == Some(2) || scan.receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_is_top_level_exact_and_receipt_presence_is_never_downgraded() {
        for raw in [
            br#"{"schema_version":2}"#.as_slice(),
            br#"{"schema_version":1,"source_receipt":null}"#,
            br#"{"schema_version":1,"source_\u0072eceipt":{}}"#,
        ] {
            assert!(source_aware(raw, &Limits::default()).unwrap());
        }
        for raw in [
            br#"{"schema_version":1}"#.as_slice(),
            br#"{"schema_version":1,"nested":{"source_receipt":null,"schema_version":2}}"#,
            br#"{"schema_version":1,"note":"source_receipt"}"#,
        ] {
            assert!(!source_aware(raw, &Limits::default()).unwrap());
        }
        let result = pointer(br#"{"manifest":"run/manifest.json"}"#, &Limits::default()).unwrap();
        assert_eq!(result.manifest.as_deref(), Some("run/manifest.json"));
        assert!(result.bundle.is_none());
        assert_eq!(result.fields, 1);
    }

    #[test]
    fn node_and_depth_charges_precede_malformed_key_or_value_decoding() {
        let nodes = Limits {
            max_json_nodes: 1,
            ..Limits::default()
        };
        let depth = Limits {
            max_json_depth: 1,
            ..Limits::default()
        };
        for raw in [
            br#"{"\uD800":null}"#.as_slice(),
            b"{\"\xff\":null}",
            br#"{"\u"#,
        ] {
            assert_eq!(
                source_aware(raw, &Limits::default()).unwrap_err().kind(),
                ErrorKind::InvalidManifest
            );
            for limits in [&nodes, &depth] {
                assert_eq!(
                    source_aware(raw, limits).unwrap_err().kind(),
                    ErrorKind::ResourceLimit
                );
            }
        }
        let limits = Limits {
            max_json_nodes: 2,
            ..Limits::default()
        };
        assert_eq!(
            source_aware(br#"{"schema_version":"\uD800"}"#, &limits)
                .unwrap_err()
                .kind(),
            ErrorKind::ResourceLimit
        );
        let exact = Limits {
            max_json_nodes: 3,
            max_json_depth: 2,
            ..Limits::default()
        };
        assert!(!source_aware(br#"{"a":[]}"#, &exact).unwrap());
        assert_eq!(
            source_aware(br#"{"a":[0]}"#, &exact).unwrap_err().kind(),
            ErrorKind::ResourceLimit
        );
        assert_eq!(
            source_aware(br#"{"a":[[]]}"#, &exact).unwrap_err().kind(),
            ErrorKind::ResourceLimit
        );
    }

    #[test]
    fn duplicates_and_decoded_aliases_cannot_change_routing_or_pointer_identity() {
        for raw in [
            br#"{"schema_version":2,"schema_version":1}"#.as_slice(),
            br#"{"schema_version":2,"schema_\u0076ersion":1}"#,
            br#"{"source_receipt":null,"source_receipt":null}"#,
        ] {
            assert_eq!(
                source_aware(raw, &Limits::default()).unwrap_err().kind(),
                ErrorKind::InvalidManifest
            );
        }
        for raw in [
            br#"{"manifest":"one","manifest":"two"}"#.as_slice(),
            br#"{"manifest":"one","mani\u0066est":"two"}"#,
            br#"{"bundle":"one","bundle":"two"}"#,
            br#"{"extra":0,"extra":1}"#,
        ] {
            assert!(
                matches!(pointer(raw, &Limits::default()), Err(error) if error.kind()==ErrorKind::InvalidManifest)
            );
        }
    }

    #[test]
    fn pointer_key_text_and_field_limits_are_bounded_before_retention() {
        for raw in [
            format!("{{\"manifest\":\"{}\"}}", "x".repeat(4097)),
            format!("{{\"{}\":null}}", "x".repeat(4097)),
        ] {
            assert!(
                matches!(pointer(raw.as_bytes(), &Limits::default()), Err(error) if error.kind()==ErrorKind::ResourceLimit)
            );
        }
        let exact = Limits {
            max_objects: 1,
            ..Limits::default()
        };
        assert!(pointer(br#"{"manifest":"one"}"#, &exact).is_ok());
        assert!(
            matches!(pointer(br#"{"manifest":"one","bundle":"run"}"#, &exact), Err(error) if error.kind()==ErrorKind::ResourceLimit)
        );
        for raw in [b"{}{}".as_slice(), b"{}junk", b"\xff", b""] {
            assert!(source_aware(raw, &Limits::default()).is_err());
        }
    }
}
