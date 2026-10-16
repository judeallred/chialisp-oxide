//! Independently compile Rue source through its pinned frontend and backend.
//!
//! This adapter never translates Rust or wraps emitted CLVM. Sources are trusted
//! local fixtures; Rue imports can read files and this is not a sandbox.

use std::path::Path;

use anyhow::{Context, Result, bail};
use clvmr::{Allocator, serde::node_to_bytes};
use rue_compiler::{Compiler, FileTree, normalize_path};
use rue_diagnostic::DiagnosticSeverity;
use rue_options::CompilerOptions;

pub const RUE_REVISION: &str = "245c107a8e795229ea08097142f005a4c0d91075";

/// Compile a Rue file's `main` to serialized CLVM with a controlled LIR toggle.
///
/// `auto_inline` stays enabled; static-pair codegen folding and debug symbols stay
/// disabled in both modes. Thus the boolean controls actual Rue LIR optimization,
/// not a relabeling of one shared compiled artifact. The workspace's documented
/// pinned Rue patches apply to this frontend path as well as Rust LIR emission.
pub fn compile_file(path: &Path, optimize: bool) -> Result<Vec<u8>> {
    let path = path
        .canonicalize()
        .with_context(|| format!("reading Rue source {}", path.display()))?;
    let root = path
        .parent()
        .context("Rue source has no parent directory")?;
    let mut compiler = Compiler::new(CompilerOptions {
        optimize_lir: optimize,
        auto_inline: true,
        optimize_static_pairs: false,
        debug_symbols: false,
        ..CompilerOptions::default()
    });
    let tree = FileTree::compile_file(&mut compiler, &path);
    check_diagnostics(&mut compiler, root)?;
    let tree = tree.with_context(|| format!("compiling Rue source {}", path.display()))?;
    let mut allocator = Allocator::new();
    let program = tree.main(
        &mut compiler,
        &mut allocator,
        &normalize_path(&path)?,
        root.to_owned(),
    );
    // Lowering/code generation can add diagnostics after the initial file pass.
    check_diagnostics(&mut compiler, root)?;
    let program = program?.context("Rue main missing")?;
    node_to_bytes(&allocator, program).context("serializing Rue CLVM")
}

fn check_diagnostics(compiler: &mut Compiler, root: &Path) -> Result<()> {
    let errors = compiler
        .take_diagnostics()
        .into_iter()
        .filter(|diagnostic| diagnostic.kind.severity() == DiagnosticSeverity::Error)
        .map(|diagnostic| diagnostic.message(root))
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        bail!("Rue {RUE_REVISION}: {}", errors.join("\n"));
    }
    Ok(())
}
