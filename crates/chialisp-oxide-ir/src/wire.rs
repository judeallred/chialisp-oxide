//! Preserve the strict v1 wire format; opt-in ABI changes need a versioned envelope.
use crate::{Environment, Expr, Parameter, Program, Type};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _, ser::Error as _};

#[derive(Serialize)]
struct FieldsRef<'a> {
    name: &'a str,
    parameters: &'a [Parameter],
    result: &'a Type,
    body: &'a Expr,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fields {
    name: String,
    parameters: Vec<Parameter>,
    result: Type,
    body: Expr,
}
#[derive(Serialize)]
struct EnvelopeRef<'a> {
    ir_version: u32,
    environment: Environment,
    program: FieldsRef<'a>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    ir_version: u32,
    environment: Environment,
    program: Fields,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Wire {
    Versioned(Envelope),
    Strict(Fields),
}
impl Serialize for Program {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let fields = FieldsRef {
            name: &self.name,
            parameters: &self.parameters,
            result: &self.result,
            body: &self.body,
        };
        match self.environment {
            Environment::Strict => fields.serialize(serializer),
            Environment::Ignored => {
                if !self.parameters.is_empty() {
                    return Err(S::Error::custom(
                        "ignored environment requires zero parameters",
                    ));
                }
                EnvelopeRef {
                    ir_version: 2,
                    environment: self.environment,
                    program: fields,
                }
                .serialize(serializer)
            }
        }
    }
}
impl<'de> Deserialize<'de> for Program {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (fields, environment) = match Wire::deserialize(deserializer)? {
            Wire::Strict(fields) => (fields, Environment::Strict),
            Wire::Versioned(envelope) => {
                if envelope.ir_version != 2 || envelope.environment != Environment::Ignored {
                    return Err(D::Error::custom(
                        "unsupported IR version or environment policy",
                    ));
                }
                if !envelope.program.parameters.is_empty() {
                    return Err(D::Error::custom(
                        "ignored environment requires zero parameters",
                    ));
                }
                (envelope.program, Environment::Ignored)
            }
        };
        Ok(Self {
            name: fields.name,
            parameters: fields.parameters,
            result: fields.result,
            body: fields.body,
            environment,
        })
    }
}
