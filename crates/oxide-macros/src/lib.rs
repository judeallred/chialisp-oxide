//! Export annotations only: Rust compilation remains responsible for semantics.
use proc_macro::TokenStream;

/// Mark a Rust function as a CLVM export without changing its function body.
#[proc_macro_attribute]
pub fn puzzle(arguments: TokenStream, item: TokenStream) -> TokenStream {
    if !arguments.is_empty() {
        return "compile_error!(\"#[puzzle] does not accept arguments\");"
            .parse()
            .unwrap();
    }
    if let Err(error) = syn::parse::<syn::ItemFn>(item.clone()) {
        return error.to_compile_error().into();
    }
    let mut exported: TokenStream = "#[doc = \"__oxide_puzzle_v1\"]".parse().unwrap();
    exported.extend(item);
    exported
}
