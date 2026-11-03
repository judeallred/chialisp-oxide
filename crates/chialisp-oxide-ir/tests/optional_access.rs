//! Constructed v8 admission controls; source and VM execution live in separate tests.
use chialisp_oxide_ir::{Environment, Expr, Function, OptionalAccess, Parameter, Program, Type};
use serde_json::json;
fn byte() -> Type {
    Type::Int {
        bits: 8,
        signed: false,
    }
}
fn optional(item: Type) -> Type {
    Type::Optional {
        item: Box::new(item),
    }
}
fn inspect(value: Expr) -> Expr {
    Expr::OptionalIsSome(OptionalAccess {
        value: Box::new(value),
    })
}
fn payload(value: Expr) -> Expr {
    Expr::OptionalValue(OptionalAccess {
        value: Box::new(value),
    })
}
fn source() -> Program {
    Program {
        name: "option-access".into(),
        parameters: vec![Parameter {
            name: "input".into(),
            ty: optional(byte()),
        }],
        result: Type::Bool,
        body: inspect(Expr::Arg { index: 0 }),
        environment: Environment::Strict,
        functions: vec![],
    }
}
#[test]
fn v8_is_mandatory_for_entry_optional_or_inspection_and_keeps_exact_transport() {
    for body in [inspect(Expr::Arg { index: 0 }), Expr::Bool { value: true }] {
        let mut p = source();
        p.body = body;
        p.validate().unwrap();
        assert!(p.uses_optional_access() && p.uses_optional() && p.uses_runtime());
        let wire = serde_json::to_value(&p).unwrap();
        assert_eq!(wire["ir_version"], 8);
        let p: Program = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(p).unwrap(), wire);
        for version in 4..=7 {
            let mut old = wire.clone();
            old["ir_version"] = version.into();
            assert!(serde_json::from_value::<Program>(old).is_err());
        }
        let mut old = wire["program"].clone();
        old.as_object_mut().unwrap().remove("functions");
        assert!(serde_json::from_value::<Program>(old).is_err());
    }
    // The historical strict form has no envelope; a fabricated v8 declaration
    // cannot be used merely as a newer spelling for an older program.
    let wire = json!({"ir_version":8,"environment":"strict","program":{"name":"none","parameters":[],"result":{"kind":"bool"},"body":{"op":"bool","value":true},"functions":[]}});
    assert!(serde_json::from_value::<Program>(wire).is_err());
}
#[test]
fn opaque_internal_optional_parameters_stay_v6_and_dead_inspection_selects_v8() {
    let mut p = source();
    p.parameters.clear();
    p.body = Expr::Bool { value: true };
    p.functions.push(Function {
        id: 0,
        name: "opaque".into(),
        parameters: vec![Parameter {
            name: "input".into(),
            ty: optional(byte()),
        }],
        result: Type::Unit,
        body: Expr::Unit,
    });
    assert!(!p.uses_optional_access());
    assert_eq!(serde_json::to_value(&p).unwrap()["ir_version"], 6);
    p.functions[0].result = Type::Bool;
    p.functions[0].body = inspect(Expr::Arg { index: 0 });
    assert!(p.uses_optional_access());
    assert_eq!(serde_json::to_value(&p).unwrap()["ir_version"], 8);
    p.functions.clear();
    p.body = Expr::If {
        condition: Box::new(Expr::Bool { value: true }),
        then_value: Box::new(Expr::Bool { value: true }),
        else_value: Box::new(inspect(Expr::Reject {
            message: "never".into(),
        })),
    };
    assert!(p.uses_optional_access() && p.uses_optional());
    p.validate().unwrap();
    assert_eq!(serde_json::to_value(&p).unwrap()["ir_version"], 8);
}
#[test]
fn access_operands_and_new_payload_fields_are_closed_in_every_body() {
    let mut p = source();
    p.result = byte();
    p.body = payload(Expr::Arg { index: 0 });
    p.validate().unwrap();
    for value in [Expr::Unit, Expr::Bool { value: true }] {
        for body in [inspect(value.clone()), payload(value)] {
            let mut bad = source();
            bad.body = body.clone();
            assert!(bad.validate().is_err());
            bad = source();
            bad.functions.push(Function {
                id: 0,
                name: "unused".into(),
                parameters: vec![],
                result: Type::Bool,
                body,
            });
            assert!(bad.validate().is_err());
        }
    }
    for op in ["optional_is_some", "optional_value"] {
        let mut wire = serde_json::to_value(&p).unwrap();
        wire["program"]["body"] = json!({"op":op,"value":{"op":"arg","index":0},"invented":true});
        if op == "optional_is_some" {
            wire["program"]["result"] = json!({"kind":"bool"});
        }
        assert!(serde_json::from_value::<Program>(wire).is_err());
    }
    let mut bad = p.clone();
    bad.parameters[0].ty = optional(Type::Int {
        bits: 7,
        signed: false,
    });
    assert!(bad.validate().is_err());
    let mut item = byte();
    for _ in 0..32 {
        item = optional(item);
    }
    bad = p;
    bad.parameters[0].ty = item;
    assert!(bad.validate().is_err());
}
