use chialisp_oxide_ir::{Environment, Expr, Parameter, Program, Type};
use serde::Deserialize;
use serde_json::{Value, json};

fn strict() -> Program {
    Program {
        functions: vec![],
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

#[test]
fn legacy_ignored_and_raw_serialization_remains_exact() {
    let mut p = strict();
    p.environment = Environment::Ignored;
    assert_eq!(
        serde_json::to_string(&p).unwrap(),
        r#"{"ir_version":2,"environment":"ignored","program":{"name":"closed","parameters":[],"result":{"kind":"unit"},"body":{"op":"unit"}}}"#
    );
    p.environment = Environment::Raw;
    p.parameters = vec![Parameter {
        name: "environment".into(),
        ty: Type::Tree,
    }];
    assert_eq!(
        serde_json::to_string(&p).unwrap(),
        r#"{"ir_version":3,"environment":"raw","program":{"name":"closed","parameters":[{"name":"environment","ty":{"kind":"tree"}}],"result":{"kind":"unit"},"body":{"op":"unit"}}}"#
    );
}

#[test]
fn runtime_nodes_and_even_unused_function_tables_require_v4_for_every_environment() {
    for environment in [Environment::Strict, Environment::Ignored, Environment::Raw] {
        let mut p = strict();
        p.environment = environment;
        if environment == Environment::Raw {
            p.parameters = vec![Parameter {
                name: "environment".into(),
                ty: Type::Tree,
            }];
        }
        for body in [
            Expr::Let {
                local_id: 0,
                ty: Type::Unit,
                value: Box::new(Expr::Unit),
                body: Box::new(Expr::Local { local_id: 0 }),
            },
            Expr::If {
                condition: Box::new(Expr::Bool { value: true }),
                then_value: Box::new(Expr::Unit),
                else_value: Box::new(Expr::Let {
                    local_id: 7,
                    ty: Type::Unit,
                    value: Box::new(Expr::Unit),
                    body: Box::new(Expr::Unit),
                }),
            },
        ] {
            p.body = body;
            p.validate().unwrap();
            let wire = serde_json::to_value(&p).unwrap();
            assert_eq!(wire["ir_version"], 4);
            assert_eq!(wire["program"]["functions"], json!([]));
            let decoded = serde_json::from_value::<Program>(wire.clone()).unwrap();
            assert!(decoded.uses_runtime());
            assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
        }
        p.body = Expr::Unit;
        p.functions = vec![chialisp_oxide_ir::Function {
            id: 0,
            name: "unused".into(),
            parameters: vec![],
            result: Type::Unit,
            body: Expr::Unit,
        }];
        p.validate().unwrap();
        let wire = serde_json::to_value(&p).unwrap();
        assert_eq!(wire["ir_version"], 4);
        assert_eq!(wire["program"]["functions"].as_array().unwrap().len(), 1);
        assert!(
            serde_json::from_value::<Program>(wire)
                .unwrap()
                .uses_runtime()
        );
    }
}

#[test]
fn every_legacy_format_rejects_runtime_nodes_even_in_unreachable_descendants() {
    let new_nodes = [
        json!({"op":"call","function_id":0,"arguments":[]}),
        json!({"op":"let","local_id":0,"ty":{"kind":"unit"},"value":{"op":"unit"},"body":{"op":"unit"}}),
        json!({"op":"local","local_id":0}),
    ];
    for environment in [Environment::Strict, Environment::Ignored, Environment::Raw] {
        let mut p = strict();
        p.environment = environment;
        if environment == Environment::Raw {
            p.parameters = vec![Parameter {
                name: "environment".into(),
                ty: Type::Tree,
            }];
        }
        let original = serde_json::to_value(&p).unwrap();
        for node in &new_nodes {
            for nested in [false, true] {
                let mut wire = original.clone();
                let fields = if environment == Environment::Strict {
                    &mut wire
                } else {
                    &mut wire["program"]
                };
                fields["body"] = if nested {
                    json!({"op":"if","condition":{"op":"bool","value":true},"then_value":{"op":"unit"},"else_value":node})
                } else {
                    node.clone()
                };
                assert!(
                    serde_json::from_value::<Program>(wire).is_err(),
                    "{environment:?}, {node}, nested={nested}"
                );
            }
        }
        let mut wire = original;
        let fields = if environment == Environment::Strict {
            &mut wire
        } else {
            &mut wire["program"]
        };
        fields["functions"] = json!([]);
        assert!(serde_json::from_value::<Program>(wire).is_err());
    }
}

#[test]
fn runtime_envelope_and_standalone_function_metadata_fail_closed() {
    let mut p = strict();
    p.functions = vec![chialisp_oxide_ir::Function {
        id: 0,
        name: "helper".into(),
        parameters: vec![],
        result: Type::Unit,
        body: Expr::Unit,
    }];
    p.body = Expr::Call {
        function_id: 0,
        arguments: vec![],
    };
    let original = serde_json::to_value(p).unwrap();
    for version in [0, 1, 2, 3, 5] {
        let mut bad = original.clone();
        bad["ir_version"] = json!(version);
        assert!(serde_json::from_value::<Program>(bad).is_err());
    }
    for field in ["ir_version", "environment"] {
        let mut bad = original.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Program>(bad).is_err());
    }
    let mut bad = original.clone();
    bad["program"].as_object_mut().unwrap().remove("functions");
    assert!(serde_json::from_value::<Program>(bad).is_err());
    let mut bad = original.clone();
    bad["program"]["extra"] = json!(true);
    assert!(serde_json::from_value::<Program>(bad).is_err());
    let mut bad = original.clone();
    bad["program"]["functions"][0]["extra"] = json!(true);
    assert!(serde_json::from_value::<Program>(bad).is_err());
    let definition = original["program"]["functions"][0].clone();
    assert!(serde_json::from_value::<chialisp_oxide_ir::Function>(definition.clone()).is_ok());
    let mut bad = definition.clone();
    bad["extra"] = json!(true);
    assert!(serde_json::from_value::<chialisp_oxide_ir::Function>(bad).is_err());
    let mut bad = definition;
    bad.as_object_mut().unwrap().remove("id");
    assert!(serde_json::from_value::<chialisp_oxide_ir::Function>(bad).is_err());
}

#[test]
fn runtime_free_v4_envelopes_cannot_silently_downgrade_to_legacy_semantics() {
    for environment in [Environment::Strict, Environment::Ignored, Environment::Raw] {
        let mut p = strict();
        p.environment = environment;
        if environment == Environment::Raw {
            p.parameters = vec![Parameter {
                name: "environment".into(),
                ty: Type::Tree,
            }];
        }
        for body in [
            Expr::Unit,
            Expr::Sequence {
                first: Box::new(Expr::Unit),
                then_value: Box::new(Expr::Unit),
            },
        ] {
            p.body = body;
            p.validate().unwrap();
            assert!(!p.uses_runtime());
            let legacy = serde_json::to_value(&p).unwrap();
            let mut fields = if environment == Environment::Strict {
                legacy
            } else {
                legacy["program"].clone()
            };
            fields["functions"] = json!([]);
            let wire = json!({"ir_version":4,"environment":environment,"program":fields});
            let error = serde_json::from_value::<Program>(wire).unwrap_err();
            assert!(
                error.to_string().contains("requires runtime expressions"),
                "{environment:?}: {error}"
            );
        }
    }
}
