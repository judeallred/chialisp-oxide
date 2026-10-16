//! Export annotations only: Rust compilation remains responsible for semantics.
use proc_macro::TokenStream;
use syn::parse::Parser;

/// Mark a Rust function as a CLVM export without changing its function body.
///
/// The default `environment = "strict"` validates the complete typed argument
/// list. A nullary function may explicitly select `environment = "ignored"`
/// to accept every CLVM environment tree without decoding arguments.
#[proc_macro_attribute]
pub fn puzzle(arguments: TokenStream, item: TokenStream) -> TokenStream {
    let function = match syn::parse::<syn::ItemFn>(item.clone()) {
        Ok(function) => function,
        Err(error) => return error.to_compile_error().into(),
    };
    let mut ignored = None;
    let parser = syn::meta::parser(|meta| {
        if !meta.path.is_ident("environment") {
            return Err(meta.error("unknown #[puzzle] option; expected `environment`"));
        }
        if ignored.is_some() {
            return Err(meta.error("duplicate #[puzzle] environment option"));
        }
        let value: syn::LitStr = meta.value()?.parse()?;
        ignored = Some(match value.value().as_str() {
            "strict" => false,
            "ignored" => true,
            _ => {
                return Err(syn::Error::new_spanned(
                    value,
                    "unknown environment policy; expected \"strict\" or \"ignored\"",
                ));
            }
        });
        Ok(())
    });
    if let Err(error) = parser.parse(arguments) {
        return error.to_compile_error().into();
    }
    if ignored == Some(true) && !function.sig.inputs.is_empty() {
        return syn::Error::new_spanned(
            &function.sig.inputs,
            "environment = \"ignored\" requires a function with no parameters",
        )
        .to_compile_error()
        .into();
    }
    let mut exported: TokenStream = if ignored == Some(true) {
        "#[doc = \"__chialisp_oxide_puzzle_v2_environment_ignored\"]"
    } else {
        // Keep the original strict marker so existing metadata stays valid.
        "#[doc = \"__oxide_puzzle_v1\"]"
    }
    .parse()
    .unwrap();
    exported.extend(item);
    exported
}
