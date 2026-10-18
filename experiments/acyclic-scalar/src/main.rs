//! The oracle calls the SAME Cargo library that the driver extracts.
use chialisp_oxide_acyclic_scalar_source::*;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [name] = args.as_slice() else {
        panic!("exactly one declared export required")
    };
    let function: fn(u8) -> u8 = match name.as_str() {
        "base::guarded" => base::guarded,
        "base::unused_argument" => base::unused_argument,
        "base::unused_initializer" => base::unused_initializer,
        "base::copied_result" => base::copied_result,
        "guard_mutant::guarded" => guard_mutant::guarded,
        "guard_mutant::unused_argument" => guard_mutant::unused_argument,
        "guard_mutant::unused_initializer" => guard_mutant::unused_initializer,
        "guard_mutant::copied_result" => guard_mutant::copied_result,
        "output_mutant::guarded" => output_mutant::guarded,
        "output_mutant::unused_argument" => output_mutant::unused_argument,
        "output_mutant::unused_initializer" => output_mutant::unused_initializer,
        "output_mutant::copied_result" => output_mutant::copied_result,
        "strict_mutant::guarded" => strict_mutant::guarded,
        "strict_mutant::unused_argument" => strict_mutant::unused_argument,
        "strict_mutant::unused_initializer" => strict_mutant::unused_initializer,
        "strict_mutant::copied_result" => strict_mutant::copied_result,
        _ => panic!("unknown export"),
    };
    std::panic::set_hook(Box::new(|_| {}));
    print!("[");
    for input in 0..=u8::MAX {
        if input != 0 {
            print!(",");
        }
        match std::panic::catch_unwind(|| function(input)) {
            Ok(value) => print!("{{\"input\":{input},\"status\":\"accepted\",\"value\":{value}}}"),
            Err(_) => print!("{{\"input\":{input},\"status\":\"rejected\"}}"),
        }
    }
    println!("]");
}
