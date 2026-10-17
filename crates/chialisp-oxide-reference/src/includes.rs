//! A closed include map for trusted classic reference modules.
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use anyhow::{Result, anyhow, bail, ensure};
use chialisp::{
    classic::clvm_tools::binutils::assemble,
    compiler::{
        compiler::{DefaultCompilerOpts, STANDARD_MACROS},
        comptypes::{CompileErr, CompilerOpts, HasCompilerOptsDelegation},
        dialect::detect_modern,
    },
};
use clvmr_reference::allocator::Allocator;

const MAX_FILES: usize = 64;
const MAX_BYTES: usize = 1_048_576;

struct IncludeOptions {
    inner: Rc<dyn CompilerOpts>,
    files: Rc<BTreeMap<String, Vec<u8>>>,
    missing: Rc<RefCell<Option<String>>>,
}

impl HasCompilerOptsDelegation for IncludeOptions {
    fn compiler_opts(&self) -> Rc<dyn CompilerOpts> {
        self.inner.clone()
    }

    fn update_compiler_opts<F: FnOnce(Rc<dyn CompilerOpts>) -> Rc<dyn CompilerOpts>>(
        &self,
        f: F,
    ) -> Rc<dyn CompilerOpts> {
        Rc::new(Self {
            inner: f(self.inner.clone()),
            files: self.files.clone(),
            missing: self.missing.clone(),
        })
    }

    fn override_read_new_file(
        &self,
        _inc_from: String,
        filename: String,
    ) -> Result<(String, Vec<u8>), CompileErr> {
        let content = match filename.as_str() {
            // These are the pinned compiler's embedded classic facilities,
            // not names accepted from the caller's include map.
            "*macros*" => STANDARD_MACROS.as_bytes().to_vec(),
            "*bitconst*" => b"()".to_vec(),
            _ => match self.files.get(&filename) {
                Some(bytes) => bytes.clone(),
                None => {
                    self.missing.borrow_mut().get_or_insert(filename.clone());
                    // Classic upstream falls back to fs::read_to_string when
                    // CompilerOpts returns Err. Return an empty include instead
                    // and reject after compilation; never invoke that fallback.
                    b"()".to_vec()
                }
            },
        };
        Ok((filename, content))
    }
}

/// Compile a trusted classic module with exact, explicitly supplied includes.
///
/// Include names are flat ASCII basenames; their UTF-8 source bytes are used
/// verbatim. Missing includes fail even when a file exists on disk. The only
/// implicit includes are the pinned compiler's embedded classic macros and
/// bit-constant marker. Modern dialects need a separately reviewed resolver.
///
/// This controls include resolution, not arbitrary compile-time CLVM or the
/// upstream compiler's resources/filesystem operators. In particular,
/// `embed-file` can still read disk directly. Source remains trusted.
pub fn compile_with_includes(source: &str, includes: &[(&str, &str)]) -> Result<Vec<u8>> {
    ensure!(includes.len() <= MAX_FILES, "too many reference includes");
    ensure!(
        source.len() <= MAX_BYTES,
        "reference source exceeds byte limit"
    );
    let mut files = BTreeMap::new();
    let mut bytes = source.len();
    for &(name, content) in includes {
        ensure!(
            !name.is_empty()
                && name.len() <= 128
                && !name.contains("..")
                && name != "."
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte)),
            "invalid reference include name {name:?}: expected a flat ASCII basename"
        );
        bytes = bytes.saturating_add(content.len());
        ensure!(bytes <= MAX_BYTES, "reference includes exceed byte limit");
        ensure!(
            files
                .insert(name.to_owned(), content.as_bytes().to_vec())
                .is_none(),
            "duplicate reference include {name:?}"
        );
    }
    let mut allocator = Allocator::new();
    let parsed = assemble(&mut allocator, source)
        .map_err(|error| anyhow!("{}: assembling module: {error}", super::COMPILER_VERSION))?;
    ensure!(
        detect_modern(&mut allocator, parsed).stepping.is_none(),
        "closed reference include resolver supports classic modules only"
    );
    let missing = Rc::new(RefCell::new(None));
    let options = Rc::new(IncludeOptions {
        inner: Rc::new(DefaultCompilerOpts::new("reference.clsp")),
        files: Rc::new(files),
        missing: missing.clone(),
    });
    let result = super::compile_with_options(source, options);
    if let Some(name) = missing.borrow().as_deref() {
        bail!(
            "{}: reference include {name:?} was not supplied",
            super::COMPILER_VERSION
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> IncludeOptions {
        IncludeOptions {
            inner: Rc::new(DefaultCompilerOpts::new("reference.clsp")),
            files: Rc::new(BTreeMap::new()),
            missing: Rc::new(RefCell::new(None)),
        }
    }

    #[test]
    fn unknown_reads_disable_upstream_filesystem_fallback() {
        let options = options();
        assert_eq!(
            options
                .read_new_file("reference.clsp".into(), "/unknown/file.clib".into())
                .unwrap(),
            ("/unknown/file.clib".into(), b"()".to_vec())
        );
        assert_eq!(
            options.missing.borrow().as_deref(),
            Some("/unknown/file.clib")
        );
        assert!(
            compile_with_includes("(mod () (include missing.clib) 42)", &[])
                .unwrap_err()
                .to_string()
                .contains("was not supplied")
        );
    }

    #[test]
    fn rejects_ambiguous_names_duplicates_and_modern_dialects() {
        for name in [
            "", ".", "..", "../x", "/x", "x/y", "x\\y", "x y", "x\0", "*macros*",
        ] {
            assert!(
                compile_with_includes("(mod () 42)", &[(name, "()")]).is_err(),
                "{name:?}"
            );
        }
        assert!(compile_with_includes("(mod () 42)", &[("x", "()"), ("x", "()")]).is_err());
        assert!(
            compile_with_includes("(mod () (include *standard-cl-21*) 42)", &[])
                .unwrap_err()
                .to_string()
                .contains("classic modules only")
        );
    }

    #[test]
    fn updating_compiler_options_preserves_the_closed_resolver() {
        let options = options().set_optimize(true).set_filename("nested.clsp");
        assert_eq!(
            options
                .read_new_file("nested.clsp".into(), "missing".into())
                .unwrap()
                .1,
            b"()"
        );
        assert_eq!(options.get_search_paths(), vec![".".to_owned()]);
    }
}
