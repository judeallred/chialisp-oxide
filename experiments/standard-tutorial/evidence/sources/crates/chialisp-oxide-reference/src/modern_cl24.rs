//! Narrow trusted-fixture route for the pinned compiler's embedded cl24 provider.
//!
//! This is not general modern include support, a source sandbox, or compiler
//! execution authentication. Only the two reviewed embedded reads are admitted;
//! source is still trusted and upstream compilation remains an in-process task.
use std::rc::Rc;

use anyhow::{Result, ensure};
use chialisp::{
    classic::clvm_tools::binutils::assemble,
    compiler::{
        BasicCompileContext, CompileContextWrapper,
        clvm::NewStyleIntConversion,
        compiler::{ADVANCED_MACROS, DefaultCompilerOpts, compile_pre_forms},
        comptypes::{CompileErr, CompilerOpts, CompilerOutput, HasCompilerOptsDelegation},
        dialect::{AcceptedDialect, KNOWN_DIALECTS, detect_modern},
        optimize::get_optimizer,
        sexp::SExp,
        srcloc::Srcloc,
    },
};
use clvmr_reference::allocator::Allocator;

pub const MODERN_CL24_PROTOCOL: &str = "chialisp-oxide-reference-modern-cl24/v1";
pub const MODERN_CL24_NAME: &str = "*standard-cl-24*";
pub const MODERN_CL24_CRATE_CHECKSUM: &str =
    "c991ea839bb6e2299bd3600b786381c4c1b08f6780800719a24c5d33157cb3e2";
pub const MODERN_CL24_VCS_REVISION: &str = "32254c8402a3b277b43b1bdad743781f15433b80";

// Reviewed bytes exposed by KNOWN_DIALECTS / ADVANCED_MACROS in chialisp 0.4.6.
// These literal pins do not authenticate installed crate or executable bytes.
const DIALECT_SOURCE: &str = r#"(
    (defconstant *chialisp-version* 24)
)"#;
const MACRO_SOURCE: &str = r#"(
(defmac __chia__primitive__if (A B C)
  (qq (a (i (unquote A) (com (unquote B)) (com (unquote C))) @))
  )

(defun __chia__if (ARGS)
  (__chia__primitive__if (r (r (r ARGS)))
    (qq (a (i (unquote (f ARGS)) (com (unquote (f (r ARGS)))) (com (unquote (__chia__if (r (r ARGS)))))) @))
    (qq (a (i (unquote (f ARGS)) (com (unquote (f (r ARGS)))) (com (unquote (f (r (r ARGS)))))) @))
    )
  )

(defmac if ARGS (__chia__if ARGS))

(defun __chia__compile-list (args)
  (if args
    (c 4 (c (f args) (c (__chia__compile-list (r args)) ())))
    ()
    )
  )

(defmac list ARGS (__chia__compile-list ARGS))

(defun-inline / (A B) (f (divmod A B)))
)
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModernCl24Provider {
    pub protocol: &'static str,
    pub compiler: &'static str,
    pub crate_checksum: &'static str,
    pub vcs_revision: &'static str,
    pub license: &'static str,
    pub compiler_source_sha256: &'static str,
    pub dialect_source_sha256: &'static str,
    pub preprocessor_source_sha256: &'static str,
    pub license_sha256: &'static str,
    pub dialect_name: &'static str,
    pub dialect_content: &'static str,
    pub dialect_content_sha256: &'static str,
    pub macro_name: &'static str,
    pub macro_content: &'static str,
    pub macro_content_sha256: &'static str,
    pub accepted: AcceptedDialect,
}

fn expected_dialect() -> AcceptedDialect {
    AcceptedDialect {
        stepping: Some(24),
        strict: true,
        int_fix: true,
        extra_numeric_constants: false,
    }
}

fn validate_provider(dialect: &AcceptedDialect, dialect_source: &str, macros: &str) -> Result<()> {
    ensure!(
        *dialect == expected_dialect(),
        "embedded cl24 dialect flags changed"
    );
    ensure!(
        dialect_source == DIALECT_SOURCE,
        "embedded cl24 source bytes changed"
    );
    ensure!(
        macros == MACRO_SOURCE,
        "embedded cl24 strict macro bytes changed"
    );
    Ok(())
}

/// Return actual embedded API contents after comparing their flags/bytes with
/// fixed reviewed pins. Source-file/package hashes describe provenance; they do
/// not attest installed source or executable identity. Callers can hash the
/// returned actual contents independently when serializing an evidence record.
pub fn modern_cl24_provider() -> Result<ModernCl24Provider> {
    let dialect = KNOWN_DIALECTS
        .get(MODERN_CL24_NAME)
        .ok_or_else(|| anyhow::anyhow!("embedded cl24 provider is missing"))?;
    validate_provider(&dialect.accepted, &dialect.content, &ADVANCED_MACROS)?;
    Ok(ModernCl24Provider {
        protocol: MODERN_CL24_PROTOCOL,
        compiler: super::COMPILER_VERSION,
        crate_checksum: MODERN_CL24_CRATE_CHECKSUM,
        vcs_revision: MODERN_CL24_VCS_REVISION,
        license: "Apache-2.0",
        compiler_source_sha256: "0f33b01d833654f9efcd63e54feb088e3922811d2d76dbff7787d00884e9a441",
        dialect_source_sha256: "447e99fa538fd5d9a30a12c610e713740e0343592bbf7f20595a539a9c99c01f",
        preprocessor_source_sha256: "5ec1f0e836b25da5e52fb2da045b5babe3ac033b6799564dc3991675ee77f050",
        license_sha256: "7db0cf1060026bf2a544ea29cf9a96fba7d3781f93a1d06d6fee18251f1c207a",
        dialect_name: MODERN_CL24_NAME,
        dialect_content: &dialect.content,
        dialect_content_sha256: "d8c0eca00f44247f73ec6f129d15ee115514e08a443344747fb09f16e9ca68bb",
        macro_name: "*macros*",
        macro_content: &ADVANCED_MACROS,
        macro_content_sha256: "4dcba426c34e872616c7aa3d05eab95714b6b09d6ef4a01c67ae7ca66eb7c339",
        accepted: dialect.accepted.clone(),
    })
}

struct ClosedCl24Options {
    inner: Rc<dyn CompilerOpts>,
    provider: ModernCl24Provider,
}

impl HasCompilerOptsDelegation for ClosedCl24Options {
    fn compiler_opts(&self) -> Rc<dyn CompilerOpts> {
        self.inner.clone()
    }

    fn update_compiler_opts<F: FnOnce(Rc<dyn CompilerOpts>) -> Rc<dyn CompilerOpts>>(
        &self,
        f: F,
    ) -> Rc<dyn CompilerOpts> {
        Rc::new(Self {
            inner: f(self.inner.clone()),
            provider: self.provider.clone(),
        })
    }

    fn override_read_new_file(
        &self,
        inc_from: String,
        filename: String,
    ) -> Result<(String, Vec<u8>), CompileErr> {
        let bytes = match filename.as_str() {
            MODERN_CL24_NAME => self.provider.dialect_content,
            "*macros*" => self.provider.macro_content,
            _ => {
                return Err(CompileErr(
                    Srcloc::start(&inc_from),
                    format!("closed cl24 provider rejects include {filename:?}"),
                ));
            }
        };
        Ok((filename, bytes.as_bytes().to_vec()))
    }

    fn override_compile_program(
        &self,
        context: &mut BasicCompileContext,
        sexp: Rc<SExp>,
    ) -> Result<CompilerOutput, CompileErr> {
        // DefaultCompilerOpts::compile_program clones itself, which would drop
        // this resolver for macro/closure/nested-module subcompilations. Mirror
        // its public upstream entry sequence while preserving the wrapper.
        let _int_conversion = NewStyleIntConversion::new(self.inner.dialect().int_fix);
        let me: Rc<dyn CompilerOpts> = Rc::new(Self {
            inner: self.inner.clone(),
            provider: self.provider.clone(),
        });
        let mut wrapper = CompileContextWrapper::new(
            context.runner.clone(),
            &mut context.symbols,
            get_optimizer(&sexp.loc(), me.clone())?,
        );
        compile_pre_forms(wrapper.context(), me, &[sexp])
    }

    fn override_get_file_mod_date(&self, loc: &Srcloc, filename: &str) -> Result<u64, CompileErr> {
        Err(CompileErr(
            loc.clone(),
            format!("closed cl24 provider rejects file metadata {filename:?}"),
        ))
    }

    fn override_write_new_file(&self, filename: &str, _content: &[u8]) -> Result<(), CompileErr> {
        Err(CompileErr(
            Srcloc::start(filename),
            "closed cl24 provider rejects compiler file writes".into(),
        ))
    }
}

/// Compile a trusted cl24 fixture using only the actual pinned embedded cl24
/// dialect and strict macro prelude. No caller include map or filesystem
/// include fallback is admitted. Other dialects remain outside this route.
pub fn compile_modern_cl24(source: &str) -> Result<Vec<u8>> {
    ensure!(
        source.len() <= 1_048_576,
        "cl24 fixture source exceeds byte limit"
    );
    let provider = modern_cl24_provider()?;
    let mut allocator = Allocator::new();
    let parsed = assemble(&mut allocator, source).map_err(|error| {
        anyhow::anyhow!(
            "{}: assembling cl24 fixture: {error}",
            super::COMPILER_VERSION
        )
    })?;
    ensure!(
        detect_modern(&mut allocator, parsed) == expected_dialect(),
        "closed cl24 route requires the exact embedded cl24 dialect"
    );
    let options = Rc::new(ClosedCl24Options {
        inner: Rc::new(DefaultCompilerOpts::new("reference.clsp")),
        provider,
    });
    super::compile_with_options(source, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chialisp_oxide_vm::classify_failure;
    use clvmr::{
        ChiaDialect, MEMPOOL_MODE,
        allocator::Allocator,
        run_program,
        serde::{node_from_bytes, node_to_bytes},
    };

    fn execute(program: &[u8], environment: &[u8]) -> Option<Vec<u8>> {
        let mut allocator = Allocator::new();
        let program = node_from_bytes(&mut allocator, program).unwrap();
        let environment = node_from_bytes(&mut allocator, environment).unwrap();
        match run_program(
            &mut allocator,
            &ChiaDialect::new(MEMPOOL_MODE),
            program,
            environment,
            1_000_000,
        ) {
            Ok(result) => Some(node_to_bytes(&allocator, result.1).unwrap()),
            Err(error) => {
                assert!(
                    classify_failure(&error).is_program_rejection(),
                    "resource/infrastructure is inconclusive: {error:?}"
                );
                None
            }
        }
    }

    #[test]
    fn actual_provider_bytes_and_flags_match_fixed_reviewed_pins() {
        let provider = modern_cl24_provider().unwrap();
        assert_eq!(provider.protocol, MODERN_CL24_PROTOCOL);
        assert_eq!(provider.dialect_content, DIALECT_SOURCE);
        assert_eq!(provider.macro_content, MACRO_SOURCE);
        assert_eq!(provider.accepted, expected_dialect());
        let mut changed = provider.accepted.clone();
        changed.int_fix = false;
        assert!(
            validate_provider(&changed, provider.dialect_content, provider.macro_content).is_err()
        );
        assert!(validate_provider(&provider.accepted, "()", provider.macro_content).is_err());
        assert!(validate_provider(&provider.accepted, provider.dialect_content, "()").is_err());
    }

    #[test]
    fn let_module_uses_actual_modern_provider_and_preserves_raw_numbers() {
        let source = "(mod (Z) (include *standard-cl-24*) (let ((X (+ Z 1)) (Y (- Z 1))) (* X Y)))";
        let program = compile_modern_cl24(source).unwrap();
        assert_eq!(execute(&program, &[0xff, 5, 0x80]), Some(vec![24]));
        assert_eq!(execute(&program, &[0xff, 0x82, 0, 5, 0x80]), Some(vec![24]));
        assert_eq!(
            execute(&program, &[0xff, 0x80, 0x80]),
            Some(vec![0x81, 0xff])
        );
        assert_eq!(execute(&program, &[0x80]), None);
        assert_eq!(execute(&program, &[0xff, 0xff, 1, 0x80, 0x80]), None);
    }

    #[test]
    fn named_and_lambda_maps_preserve_numeric_lists_and_reject_truthy_atom_tails() {
        for source in [
            "(mod (Z) (include *standard-cl-24*) (defun add-one (X) (+ X 1)) (defun map (F L) (if L (c (a F (list (f L))) (map F (r L))) ())) (map add-one Z))",
            "(mod (Z) (include *standard-cl-24*) (defun map (F L) (if L (c (a F (list (f L))) (map F (r L))) ())) (map (lambda (Y) (+ Y 1)) Z))",
        ] {
            let program = compile_modern_cl24(source).unwrap();
            assert_eq!(
                execute(&program, &[0xff, 0xff, 1, 0xff, 2, 0xff, 3, 0x80, 0x80]),
                Some(vec![0xff, 2, 0xff, 3, 0xff, 4, 0x80])
            );
            assert_eq!(execute(&program, &[0xff, 0x80, 0x80]), Some(vec![0x80]));
            assert_eq!(execute(&program, &[0xff, 0, 0x80]), None);
            assert_eq!(execute(&program, &[0xff, 0xff, 1, 7, 0x80]), None);
            assert_eq!(
                execute(&program, &[0xff, 0xff, 0xff, 1, 0x80, 0x80, 0x80]),
                None
            );
        }
    }

    #[test]
    fn no_classic_or_alternative_modern_dialect_is_admitted() {
        for source in [
            "(mod (Z) Z)",
            "(mod (Z) (include *standard-cl-23*) Z)",
            "(mod (Z) (include *standard-cl-24*) (include *standard-cl-25*) Z)",
            "(mod (Z) (include *standard-cl-24*) (include ../not-embedded.clib) Z)",
        ] {
            assert!(
                compile_modern_cl24(source).is_err(),
                "unreviewed provider accepted: {source}"
            );
        }
    }

    #[test]
    fn unknown_include_is_rejected_even_when_a_real_file_exists() {
        let path = std::env::temp_dir().join(format!(
            "chialisp-oxide-cl24-{}-{}.clib",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, "((defconstant VALUE 7))").unwrap();
        let sources = [
            format!(
                "(mod () (include *standard-cl-24*) (include {}) VALUE)",
                path.display()
            ),
            format!(
                "(mod () (include *standard-cl-24*) (mod () (include *standard-cl-24*) (include {}) VALUE))",
                path.display()
            ),
        ];
        let results: Vec<_> = sources
            .iter()
            .map(|source| (super::super::compile(source), compile_modern_cl24(source)))
            .collect();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            execute(results[0].0.as_ref().unwrap(), &[0x80]),
            Some(vec![7])
        );
        for (ordinary, closed) in results {
            assert!(
                ordinary.is_ok(),
                "on-disk fixture must load under the ordinary compiler: {ordinary:?}"
            );
            assert!(
                closed
                    .unwrap_err()
                    .to_string()
                    .contains("closed cl24 provider rejects include")
            );
        }
    }
}
