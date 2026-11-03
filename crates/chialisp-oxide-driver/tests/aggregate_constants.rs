//! Rustc-evaluated data preserves native layouts and existing CLVM data shapes.
mod support;
#[path = "support/vm.rs"]
mod vm;
use chialisp_oxide_backend_experiment::Outcome;
use std::process::Command;
use support::Fixture;

const SOURCE: &str = r#"
use chialisp_oxide::Cons;
#[repr(transparent)] pub struct Policy((i16, [u8; 3], bool));
const fn terms() -> (u8,u128,u16,bool) { (7, u128::MAX, 513, true) }
const TERMS:(u8,u128,u16,bool)=terms();
const NESTED:((i16,[u8;3],bool),())=((-129,[0,128,255],false),());
const WRAPPED:Policy=Policy((-129,[0,128,255],false));
const PAIR:Cons<u8,u16>=Cons{first:7,rest:513};
const EMPTY:Cons<(),()>=Cons{first:(),rest:()};
const SINGLE:(i128,)=(i128::MIN,);
pub struct Record { pub small:u8, pub wide:u64, pub flag:bool }
pub struct Zero { pub value:() }
const RECORD:Record=Record{flag:true,wide:513,small:7};
const ZERO:Zero=Zero{value:()};
const VALUES:[u16;3]=[0,128,65535];
const PRODUCTS:[(u8,[u8;2]);2]=[(7,[0,128]),(9,[255,0])];
const RECORDS:[Record;2]=[RECORD,Record{small:9,wide:1,flag:false}];
const NO_VALUES:[u16;0]=[];
mod impostor {
    pub struct Cons { pub first:u8, pub rest:u16 }
    pub const VALUE:Cons=Cons{rest:513,first:7};
}
#[chialisp_oxide::puzzle] pub fn padded(_:u8)->(u8,u128,u16,bool){TERMS}
#[chialisp_oxide::puzzle] pub fn nested(_:u8)->((i16,[u8;3],bool),()){NESTED}
#[chialisp_oxide::puzzle] pub fn wrapped(_:u8)->Policy{WRAPPED}
#[chialisp_oxide::puzzle] pub fn pair(_:u8)->Cons<u8,u16>{PAIR}
#[chialisp_oxide::puzzle] pub fn empty(_:u8)->Cons<(),()>{EMPTY}
#[chialisp_oxide::puzzle] pub fn single(_:u8)->(i128,){SINGLE}
#[chialisp_oxide::puzzle] pub fn strict(v:u8)->(u8,u128,u16,bool){let _checked=1/v;TERMS}
#[chialisp_oxide::puzzle] pub fn branch(v:bool)->(u8,u128,u16,bool){if v {TERMS}else{(0,1,2,false)}}
#[chialisp_oxide::puzzle] pub fn record(_:u8)->Record{RECORD}
#[chialisp_oxide::puzzle] pub fn zero_record(_:u8)->Zero{ZERO}
#[chialisp_oxide::puzzle] pub fn local_cons(_:u8)->impostor::Cons{impostor::VALUE}
#[chialisp_oxide::puzzle] pub fn record_in_tuple(_:u8)->(Record,u8){(RECORD,9)}
#[chialisp_oxide::puzzle] pub fn values(_:u8)->[u16;3]{VALUES}
#[chialisp_oxide::puzzle] pub fn products(_:u8)->[(u8,[u8;2]);2]{PRODUCTS}
#[chialisp_oxide::puzzle] pub fn records(_:u8)->[Record;2]{RECORDS}
#[chialisp_oxide::puzzle] pub fn no_values(_:u8)->[u16;0]{NO_VALUES}
#[test] fn native_values(){
assert_eq!(padded(1),(7,u128::MAX,513,true));assert_eq!(nested(1),((-129,[0,128,255],false),()));
assert_eq!(wrapped(1).0,(-129,[0,128,255],false));assert_eq!((pair(1).first,pair(1).rest),(7,513));
assert_eq!((empty(1).first,empty(1).rest),((),()));assert_eq!(single(1),(i128::MIN,));
assert!(std::panic::catch_unwind(||strict(0)).is_err());assert_eq!(strict(1),TERMS);
assert_eq!(branch(true),TERMS);assert_eq!(branch(false),(0,1,2,false));
let r=record(1);assert_eq!((r.small,r.wide,r.flag),(7,513,true));assert_eq!(zero_record(1).value,());
let c=local_cons(1);assert_eq!((c.first,c.rest),(7,513));let(r,n)=record_in_tuple(1);assert_eq!((r.small,r.wide,r.flag,n),(7,513,true,9));
assert_eq!(values(1),[0,128,65535]);assert_eq!(products(1),[(7,[0,128]),(9,[255,0])]);
let [a,b]=records(1);assert_eq!((a.small,a.wide,a.flag,b.small,b.wide,b.flag),(7,513,true,9,1,false));assert!(no_values(1).is_empty());
}
"#;
fn native(f: &Fixture) {
    let out = Command::new(f.compiler().parent().unwrap().join("cargo"))
        .current_dir(&f.0)
        .env("RUSTC", f.compiler())
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CHIALISP_OXIDE_CARGO_MODE")
        .args(["test", "--lib", "--offline", "--target-dir"])
        .arg(f.0.join("native-build"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
#[test]
fn actual_aggregate_constants_match_native_and_independent_chialisp_in_both_modes() {
    let f = Fixture::with_sdk_alias("chialisp_oxide");
    f.source(SOURCE);
    native(&f);
    let out = f.cargo(true);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let programs = f.exports();
    assert_eq!(programs.len(), 16);
    for program in programs {
        let name = program.name.rsplit("::").next().unwrap();
        let value = match name {
            "padded" | "strict" => "(7 340282366920938463463374607431768211455 513 1)",
            "nested" => "((-129 0x0080ff ()) ())",
            "wrapped" => "(-129 0x0080ff ())",
            "pair" => "(7 . 513)",
            "empty" => "(() . ())",
            "single" => "(-170141183460469231731687303715884105728)",
            "branch" => "(7 340282366920938463463374607431768211455 513 1)",
            "record" => "(7 513 1)",
            "zero_record" => "(())",
            "local_cons" => "(7 513)",
            "record_in_tuple" => "((7 513 1) 9)",
            "values" => "(() 128 65535)",
            "products" => "((7 0x0080) (9 0xff00))",
            "records" => "((7 513 1) (9 1 ()))",
            "no_values" => "()",
            _ => panic!("{name}"),
        };
        let reference =
            chialisp_oxide_reference::compile(&format!("(mod (VALUE) (q . {value}))")).unwrap();
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(&program, optimize).unwrap();
            assert_eq!(
                vm::execute(&bytes, "(1)", 20_000_000).unwrap().outcome,
                vm::execute(&reference, "(1)", 20_000_000).unwrap().outcome,
                "{name}/{optimize}"
            );
            for malformed in ["()", "(1 2)", "(1 . 2)", "((1))", "(0x0001)", "(256)"] {
                assert!(
                    matches!(
                        vm::execute(&bytes, malformed, 20_000_000).unwrap().outcome,
                        Outcome::Rejected
                    ),
                    "{name}/{optimize}/{malformed}"
                );
            }
            if name == "strict" {
                assert!(matches!(
                    vm::execute(&bytes, "(0)", 20_000_000).unwrap().outcome,
                    Outcome::Rejected
                ));
            }
            if name == "branch" {
                let other = chialisp_oxide_reference::compile("(mod () (q . (0 1 2 ())))").unwrap();
                assert_eq!(
                    vm::execute(&bytes, "(0)", 20_000_000).unwrap().outcome,
                    vm::execute(&other, "()", 20_000_000).unwrap().outcome
                );
            }
        }
    }
}

#[test]
fn aggregate_constants_do_not_erase_nested_unsupported_types() {
    for (name, source) in [
        (
            "references",
            "const DATA:(u8,&[u8;1])=(1,&[2]); #[chialisp_oxide::puzzle] pub fn p()->(u8,&'static[u8;1]){DATA}",
        ),
        (
            "option",
            "const DATA:(u8,Option<u8>)=(1,None); #[chialisp_oxide::puzzle] pub fn p()->(u8,Option<u8>){DATA}",
        ),
        (
            "array",
            "const DATA:([char;2],u8)=(['a','b'],3); #[chialisp_oxide::puzzle] pub fn p()->([char;2],u8){DATA}",
        ),
        (
            "same-name",
            "struct Cons(u8,u8); const DATA:(Cons,u8)=(Cons(1,2),3); #[chialisp_oxide::puzzle] pub fn p()->(Cons,u8){DATA}",
        ),
        (
            "zero-ref",
            "const DATA:([&u8;0],u8)=([],1); #[chialisp_oxide::puzzle] pub fn p()->([&'static u8;0],u8){DATA}",
        ),
        (
            "zero-option",
            "const DATA:[Option<u8>;0]=[]; #[chialisp_oxide::puzzle] pub fn p()->[Option<u8>;0]{DATA}",
        ),
        (
            "zero-drop",
            "pub struct Bad{value:u8} impl Drop for Bad{fn drop(&mut self){}} const DATA:[Bad;0]=[]; #[chialisp_oxide::puzzle] pub fn p()->[Bad;0]{DATA}",
        ),
    ] {
        let f = Fixture::with_sdk_alias("chialisp_oxide");
        f.source(source);
        assert!(f.cargo(false).status.success(), "native {name}");
        let out = f.cargo(true);
        assert!(!out.status.success(), "accepted {name}");
        assert!(f.exports().is_empty(), "partial export {name}");
        let errors = String::from_utf8_lossy(&out.stderr);
        assert!(
            !errors.contains("internal compiler error"),
            "{name}: {errors}"
        );
    }
}

#[test]
fn aggregate_constant_shape_and_storage_limits_reject_native_valid_values() {
    for (name, base, levels, expected) in [
        (
            "storage",
            "type T0=[u8;256]; const C0:T0=[0;256];",
            9,
            "aggregate constant storage limit",
        ),
        (
            "nodes",
            "type T0=(u8,u8); const C0:T0=(1,2);",
            11,
            "type expansion limit",
        ),
        (
            "depth",
            "type T0=u8; const C0:T0=1;",
            32,
            "type expansion limit",
        ),
    ] {
        let mut source = String::from(base);
        for level in 1..=levels {
            let previous = level - 1;
            if name == "depth" {
                source.push_str(&format!(
                    "type T{level}=(T{previous},); const C{level}:T{level}=(C{previous},);"
                ));
            } else {
                source.push_str(&format!("type T{level}=(T{previous},T{previous}); const C{level}:T{level}=(C{previous},C{previous});"));
            }
        }
        source.push_str(&format!(
            "#[chialisp_oxide::puzzle] pub fn entry()->T{levels}{{C{levels}}}"
        ));
        let fixture = Fixture::with_sdk_alias("chialisp_oxide");
        fixture.source(&source);
        assert!(fixture.cargo(false).status.success(), "native {name}");
        let result = fixture.cargo(true);
        assert!(!result.status.success(), "accepted {name}");
        let errors = String::from_utf8_lossy(&result.stderr);
        assert!(errors.contains(expected), "{name}: {errors}");
        assert!(
            !errors.contains("internal compiler error"),
            "{name}: {errors}"
        );
        assert!(fixture.exports().is_empty(), "partial export {name}");
    }
}
