use chialisp_oxide_acyclic_source::*;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [name, number] = args.as_slice() else {
        panic!("entry and u8 argument required")
    };
    let value: u8 = number.parse().unwrap();
    let function = match name.as_str() {
        "helper_guard" => helper_guard,
        "missing_guard" => missing_guard,
        "unused_argument" => unused_argument,
        "missing_argument_check" => missing_argument_check,
        "unused_initializer" => unused_initializer,
        "missing_initializer" => missing_initializer,
        _ => panic!("unknown source fixture"),
    };
    std::panic::set_hook(Box::new(|_| {}));
    match std::panic::catch_unwind(|| function(value)) {
        Ok(output) => println!("{{\"status\":\"accepted\",\"value\":{output}}}"),
        Err(_) => println!("{{\"status\":\"rejected\"}}"),
    }
}
