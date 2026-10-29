//! Bounded public checkpoint input. Resume reexecutes the exact fixed recipe;
//! no saved event/assertion bypasses SDK validation or accepts signing secrets.
use anyhow::{Context, Result, ensure};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

const INPUTS: &[(&str, &[u8])] = &[
    ("src/main.rs", include_bytes!("main.rs")),
    ("src/launch.rs", include_bytes!("launch.rs")),
    ("src/controls.rs", include_bytes!("controls.rs")),
    ("src/metrics.rs", include_bytes!("metrics.rs")),
    ("src/journal.rs", include_bytes!("journal.rs")),
    ("Cargo.toml", include_bytes!("../Cargo.toml")),
    ("Cargo.lock", include_bytes!("../Cargo.lock")),
    (
        "fixtures/register.rue.hex",
        include_bytes!("../fixtures/register.rue.hex"),
    ),
];
pub fn fingerprint() -> String {
    let mut h = Sha256::new();
    for (name, bytes) in INPUTS {
        h.update((name.len() as u64).to_be_bytes());
        h.update(name.as_bytes());
        h.update((bytes.len() as u64).to_be_bytes());
        h.update(bytes);
    }
    hex::encode(h.finalize())
}
pub fn current_sources() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (name, expected) in INPUTS {
        let actual = bounded(&root.join(name), 1024 * 1024)?;
        ensure!(
            actual.as_slice() == *expected,
            "native binary/source inputs changed; rebuild isolated host"
        );
    }
    Ok(())
}
fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        // Open the actual descriptor without following the leaf or waiting for
        // a FIFO peer; validate that descriptor, never a prior path stat.
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
            .open(path)?
    };
    #[cfg(not(unix))]
    anyhow::bail!("checkpoint reads require reviewed Unix descriptor flags");
    #[cfg(unix)]
    {
        let meta = file.metadata()?;
        ensure!(
            meta.is_file() && meta.len() <= limit as u64,
            "bounded regular public checkpoint/source descriptor"
        );
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= limit && bytes.len() as u64 == meta.len(),
            "checkpoint/source descriptor grew or changed"
        );
        Ok(bytes)
    }
}

struct Seed<'a> {
    remaining: &'a mut usize,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> std::result::Result<Value, D::Error> {
        if self.depth > 96 || *self.remaining == 0 {
            return Err(de::Error::custom("checkpoint JSON work/depth limit"));
        }
        *self.remaining -= 1;
        d.deserialize_any(UniqueVisitor(self))
    }
}
struct UniqueVisitor<'a>(Seed<'a>);
impl<'de> Visitor<'de> for UniqueVisitor<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("bounded JSON with unique decoded object keys")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Value, E> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| de::Error::custom("nonfinite JSON number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> std::result::Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(v) = a.next_element_seed(Seed {
            remaining: self.0.remaining,
            depth: self.0.depth + 1,
        })? {
            values.push(v);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> std::result::Result<Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = a.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate checkpoint object key"));
            }
            if *self.0.remaining == 0 {
                return Err(de::Error::custom("checkpoint JSON work limit"));
            }
            *self.0.remaining -= 1; // Charge every retained key before value/allocation.
            let value = a.next_value_seed(Seed {
                remaining: self.0.remaining,
                depth: self.0.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}
fn unique_json(bytes: &[u8]) -> Result<Value> {
    ensure!(bytes.len() <= 1024 * 1024, "checkpoint JSON byte limit");
    let mut d = serde_json::Deserializer::from_slice(bytes);
    let mut remaining = 65_536;
    let value = Seed {
        remaining: &mut remaining,
        depth: 0,
    }
    .deserialize(&mut d)
    .context("invalid bounded unique-key checkpoint JSON")?;
    d.end().context("trailing checkpoint JSON data")?;
    Ok(value)
}

pub fn read(path: &Path) -> Result<Value> {
    let v = unique_json(&bounded(path, 1024 * 1024)?)?;
    ensure!(
        v["protocol"] == "chialisp-oxide-xchandles-canonical-host-pilot/v1"
            && v["status"] == "paused_after_validated_step"
            && v["recipe_fingerprint"] == fingerprint(),
        "checkpoint protocol/status/current recipe mismatch"
    );
    let n = v["registration_steps"]
        .as_array()
        .context("checkpoint steps")?
        .len();
    ensure!(
        (1..3).contains(&n),
        "checkpoint requires one or two completed outer steps"
    );
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_source_input_window_matches_compiled_recipe() {
        current_sources().unwrap();
        assert_eq!(fingerprint().len(), 64);
    }
    #[test]
    fn absent_checkpoint_is_infrastructure_not_policy_rejection() {
        assert!(read(Path::new("/nonexistent-xchandles-fixture-checkpoint")).is_err());
    }
    #[test]
    fn duplicate_keys_are_rejected_at_every_object_before_prefix_comparison() {
        for bytes in [
            br#"{"x":1,"x":2}"#.as_slice(),
            br#"{"outer":[{"x":1,"x":2}]}"#.as_slice(),
            br#"{"outer":{"x":1,"\u0078":1}}"#.as_slice(),
        ] {
            assert!(unique_json(bytes).is_err());
        }
        assert_eq!(
            unique_json(br#"{"a":{"x":1},"b":{"x":2}}"#).unwrap()["b"]["x"],
            2
        );
    }
    #[test]
    fn json_work_depth_and_trailing_data_are_infrastructure_failures() {
        assert!(unique_json(b"{} {}").is_err());
        let deep = format!("{}0{}", "[".repeat(97), "]".repeat(97));
        assert!(unique_json(deep.as_bytes()).is_err());
        let many = format!("[{}]", vec!["null"; 65_536].join(","));
        assert!(unique_json(many.as_bytes()).is_err());
    }
}
