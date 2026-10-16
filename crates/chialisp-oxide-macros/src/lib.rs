//! Export annotations only: Rust compilation remains responsible for semantics.
use proc_macro::TokenStream;
use syn::parse::Parser;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Environment {
    Strict,
    Ignored,
    Raw,
}

/// Mark a Rust function as a CLVM export without changing its function body.
///
/// The default `environment = "strict"` validates the complete typed argument
/// list. A nullary function may explicitly select `environment = "ignored"`
/// to accept every CLVM environment tree without decoding arguments.
/// A function with one supported borrowed-tree argument may instead select
/// `environment = "raw"` to receive the entire environment as immutable data.
#[proc_macro_attribute]
pub fn puzzle(arguments: TokenStream, item: TokenStream) -> TokenStream {
    let function = match syn::parse::<syn::ItemFn>(item.clone()) {
        Ok(function) => function,
        Err(error) => return error.to_compile_error().into(),
    };
    let mut environment = None;
    let parser = syn::meta::parser(|meta| {
        if !meta.path.is_ident("environment") {
            return Err(meta.error("unknown #[puzzle] option; expected `environment`"));
        }
        if environment.is_some() {
            return Err(meta.error("duplicate #[puzzle] environment option"));
        }
        let value: syn::LitStr = meta.value()?.parse()?;
        environment = Some(match value.value().as_str() {
            "strict" => Environment::Strict,
            "ignored" => Environment::Ignored,
            "raw" => Environment::Raw,
            _ => {
                return Err(syn::Error::new_spanned(
                    value,
                    "unknown environment policy; expected \"strict\", \"ignored\", or \"raw\"",
                ));
            }
        });
        Ok(())
    });
    if let Err(error) = parser.parse(arguments) {
        return error.to_compile_error().into();
    }
    if environment == Some(Environment::Ignored) && !function.sig.inputs.is_empty() {
        return syn::Error::new_spanned(
            &function.sig.inputs,
            "environment = \"ignored\" requires a function with no parameters",
        )
        .to_compile_error()
        .into();
    }
    if environment == Some(Environment::Raw) && function.sig.inputs.len() != 1 {
        return syn::Error::new_spanned(
            &function.sig,
            "environment = \"raw\" requires exactly one tree parameter",
        )
        .to_compile_error()
        .into();
    }
    let mut exported: TokenStream = match environment.unwrap_or(Environment::Strict) {
        Environment::Ignored => "#[doc = \"__chialisp_oxide_puzzle_v2_environment_ignored\"]",
        Environment::Raw => "#[doc = \"__chialisp_oxide_puzzle_v3_environment_raw\"]",
        // Keep the original strict marker so existing metadata stays valid.
        Environment::Strict => "#[doc = \"__oxide_puzzle_v1\"]",
    }
    .parse()
    .unwrap();
    exported.extend(item);
    exported
}
