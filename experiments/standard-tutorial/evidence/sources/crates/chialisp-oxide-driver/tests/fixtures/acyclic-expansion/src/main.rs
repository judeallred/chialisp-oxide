use chialisp_oxide_acyclic_expansion::*;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [name, number, rest @ ..] = args.as_slice() else {
        panic!("entry, u8 input, and optional bool required")
    };
    let value: u8 = number.parse().unwrap();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(|| match name.as_str() {
        "dag_guard" => dag_guard(value),
        "copied_result" => copied_result(value),
        "unused_argument" => unused_argument(value),
        "unused_initializer" => unused_initializer(value),
        "unused_overflow" => unused_overflow(value),
        "lazy" => lazy(value, rest[0].parse().unwrap()),
        _ => panic!("unknown source fixture"),
    });
    match result {
        Ok(output) => println!("{{\"status\":\"accepted\",\"value\":{output}}}"),
        Err(_) => println!("{{\"status\":\"rejected\"}}"),
    }
}
