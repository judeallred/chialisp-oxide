use chialisp_oxide_ir::{Environment, Expr, Parameter, Program, Type};
use serde::Deserialize;
use serde_json::{Value, json};

fn strict() -> Program {
    Program {
        name: "closed".into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Unit,
        environment: Environment::Strict,
    }
}
#[test]
fn strict_wire_stays_legacy_and_ignored_wire_cannot_be_read_as_strict() {
    let mut p = strict();
    let legacy =
        r#"{"name":"closed","parameters":[],"result":{"kind":"unit"},"body":{"op":"unit"}}"#;
    assert_eq!(serde_json::to_string(&p).unwrap(), legacy);
    assert_eq!(
        serde_json::from_str::<Program>(legacy).unwrap().environment,
        Environment::Strict
    );
    // Model the old reader, including its permissive unknown-field behavior.
    #[derive(Deserialize)]
    struct Legacy {
        #[serde(rename = "name")]
        _name: String,
        #[serde(rename = "parameters")]
        _parameters: Vec<Parameter>,
        #[serde(rename = "result")]
        _result: Type,
        #[serde(rename = "body")]
        _body: Expr,
    }
    assert!(serde_json::from_str::<Legacy>(legacy).is_ok());
    p.environment = Environment::Ignored;
    let wire = serde_json::to_value(&p).unwrap();
    assert_eq!(
        wire,
        json!({"ir_version":2,"environment":"ignored","program":serde_json::from_str::<Value>(legacy).unwrap()})
    );
    assert!(serde_json::from_value::<Legacy>(wire.clone()).is_err());
    assert_eq!(
        serde_json::from_value::<Program>(wire).unwrap().environment,
        Environment::Ignored
    );
}
#[test]
fn ambiguous_or_unknown_abi_metadata_fails_closed() {
    let mut p = strict();
    p.environment = Environment::Ignored;
    let valid = serde_json::to_value(&p).unwrap();
    for (key, value) in [
        ("ir_version", json!(1)),
        ("ir_version", json!(3)),
        ("environment", json!("strict")),
        ("environment", json!("anything")),
        ("extra", json!(true)),
    ] {
        let mut bad = valid.clone();
        bad[key] = value;
        assert!(serde_json::from_value::<Program>(bad).is_err(), "{key}");
    }
    let mut legacy = serde_json::to_value(strict()).unwrap();
    legacy["environment"] = json!("ignored");
    assert!(serde_json::from_value::<Program>(legacy).is_err());
    let mut bad = valid;
    bad["program"]["parameters"] = json!([{"name":"_","ty":{"kind":"unit"}}]);
    assert!(serde_json::from_value::<Program>(bad).is_err());
    p.parameters.push(Parameter {
        name: "_".into(),
        ty: Type::Unit,
    });
    assert!(serde_json::to_value(&p).is_err());
}
