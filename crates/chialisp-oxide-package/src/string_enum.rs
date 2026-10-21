//! String-only wire enums. Serde's derived external unit-enum decoder also
//! accepts `{ "variant": null }`, which is outside this package protocol.
use serde::de::{Deserializer, Error, Visitor};
use std::fmt;

pub(crate) fn deserialize<'de, D, T>(
    deserializer: D,
    variants: &'static [&'static str],
    parse: fn(&str) -> Option<T>,
) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringEnum<T> {
        variants: &'static [&'static str],
        parse: fn(&str) -> Option<T>,
    }
    impl<'de, T> Visitor<'de> for StringEnum<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a string naming a supported wire variant")
        }
        fn visit_str<E: Error>(self, value: &str) -> Result<T, E> {
            (self.parse)(value).ok_or_else(|| E::unknown_variant(value, self.variants))
        }
    }
    deserializer.deserialize_str(StringEnum { variants, parse })
}
