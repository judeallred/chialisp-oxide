//! Location policy for the installed compiler and standard library we trust.
//! This does not authenticate their bytes or defend concurrent installation changes.

use std::path::{Path, PathBuf};

use rustc_hir::def_id::{CrateNum, LOCAL_CRATE};
use rustc_interface::interface::Config;
use rustc_middle::ty::TyCtxt;
use rustc_session::{EarlyDiagCtxt, config::host_tuple};
use rustc_span::{Symbol, sym};
use rustc_target::spec::TargetTuple;

const MAX_LOADED_CRATES: usize = 1024;

/// Identity observed when routing Cargo's expanded argument copy. The original
/// arguments, rather than that copy, are passed to rustc for compilation.
pub struct Route {
    pub target: String,
    pub crate_name: Option<String>,
}

fn unsupported(message: impl std::fmt::Display) -> String {
    format!("unsupported toolchain configuration: {message}")
}

fn canonical(path: &Path, context: &str) -> Result<PathBuf, String> {
    path.canonicalize().map_err(|error| {
        unsupported(format!(
            "{context} {} cannot be resolved: {error}",
            path.display()
        ))
    })
}

fn pinned_root() -> Result<PathBuf, String> {
    canonical(Path::new(env!("CHIALISP_OXIDE_SYSROOT")), "pinned sysroot")
}

fn checked_root(explicit: Option<&Path>) -> Result<PathBuf, String> {
    let root = pinned_root()?;
    if let Some(explicit) = explicit
        && canonical(explicit, "sysroot")? != root
    {
        return Err(unsupported(format!(
            "sysroot must resolve to the pinned installation {}",
            root.display()
        )));
    }
    Ok(root)
}

/// The native driver can load a backend before Config for verbose-version and
/// passes-list probes. Constrain their target/sysroot before delegating them.
pub fn early_target(target: &str, sysroot: Option<&str>) -> Result<(), String> {
    if target != host_tuple() {
        return Err(unsupported(
            "target routing requires the pinned compiler's built-in host target; custom JSON and alternate targets are unsupported",
        ));
    }
    checked_root(sysroot.map(Path::new))?;
    Ok(())
}

fn target_library(root: &Path) -> Result<PathBuf, String> {
    canonical(
        &root.join("lib/rustlib").join(host_tuple()).join("lib"),
        "pinned standard library directory",
    )
}

fn installed_file(path: &Path, directory: &Path, context: &str) -> Result<(), String> {
    let file = canonical(path, context)?;
    if !file.is_file() || file.parent() != Some(directory) {
        return Err(unsupported(format!(
            "{context} {} must be an installed pinned standard library file in {}",
            path.display(),
            directory.display()
        )));
    }
    Ok(())
}

fn reserved(name: &str) -> bool {
    matches!(name, "core" | "alloc" | "std")
}

/// Run before session/backend construction, on rustc's actual parsed options.
pub fn configure(config: &mut Config, route: Option<&Route>) {
    if let Err(error) = check_config(config, route) {
        EarlyDiagCtxt::new(config.opts.error_format).early_fatal(error);
    }
}

fn check_config(config: &mut Config, route: Option<&Route>) -> Result<(), String> {
    let root = checked_root(config.opts.sysroot.explicit.as_deref())?;
    // The driver executable's default discovery is not the supported sysroot.
    // Apply the pinned installation only after parsing any explicit option.
    config.opts.sysroot.explicit = Some(root.clone());
    if config.opts.unstable_opts.codegen_backend.is_some() {
        return Err(unsupported(
            "explicit codegen backend overrides are not supported",
        ));
    }
    match &config.opts.target_triple {
        TargetTuple::TargetTuple(target) if target == host_tuple() => {}
        _ => {
            return Err(unsupported(
                "target analysis requires the pinned compiler's built-in host target; custom JSON and alternate targets are unsupported",
            ));
        }
    }
    if let Some(route) = route
        && (config.opts.crate_name != route.crate_name
            || !matches!(&config.opts.target_triple, TargetTuple::TargetTuple(target) if target == &route.target))
    {
        return Err(unsupported(
            "routing identity changed: parsed target/crate name does not match the routed Cargo unit",
        ));
    }
    if config.opts.crate_name.as_deref().is_some_and(reserved) {
        return Err(unsupported(
            "reserved crate name core/alloc/std cannot be compiled as a target unit",
        ));
    }
    let directory = target_library(&root)?;
    for (name, entry) in config.opts.externs.iter() {
        if reserved(name)
            && let Some(files) = entry.files()
        {
            for path in files {
                // Re-resolve the original spelling; a cached unsuccessful
                // CanonicalizedPath is not evidence that a file exists.
                installed_file(
                    path.original(),
                    &directory,
                    "reserved standard library extern",
                )?;
            }
        }
    }
    Ok(())
}

/// Check the loaded graph even for target dependencies which only emit metadata.
/// `tcx.crates` freezes rustc's crate store; bound the borrowed snapshot before
/// collecting identities or inspecting user-controlled dependency metadata.
pub fn loaded(tcx: TyCtxt<'_>) -> Result<(), String> {
    let crates = tcx.crates(());
    if crates.len().saturating_add(1) > MAX_LOADED_CRATES {
        return Err(unsupported(format!(
            "loaded crate graph exceeds {MAX_LOADED_CRATES} crates"
        )));
    }
    if reserved(tcx.crate_name(LOCAL_CRATE).as_str()) {
        return Err(unsupported(
            "reserved crate name core/alloc/std cannot be compiled as a target unit",
        ));
    }
    let root = pinned_root()?;
    let directory = target_library(&root)?;
    if canonical(
        &tcx.sess.target_tlib_path.dir,
        "selected standard library directory",
    )? != directory
    {
        return Err(unsupported(
            "selected standard library directory differs from the pinned target library",
        ));
    }
    let language = tcx.lang_items();
    // These are the language items actually used by the frontend's primitive
    // bridges. User-provided unrelated language items are not core identities.
    let owners = [
        language.eq_trait(),
        language.partial_ord_trait(),
        language.panic_fn(),
        language.panic_fmt(),
        language.drop_trait(),
        language.align_const(),
        language.size_const(),
        language.owned_box(),
        language.global_alloc_ty(),
    ];
    if owners.iter().flatten().any(|id| id.is_local()) {
        return Err(unsupported(
            "primitive language item owner is a local replacement standard library",
        ));
    }
    for &crate_num in crates {
        let name = tcx.crate_name(crate_num);
        let diagnostics = tcx.diagnostic_items(crate_num);
        // is_diagnostic_item consults each crate's own map. Check all loaded
        // maps, including non-user-visible transitive dependencies and aliases.
        let primitive_diagnostic = [
            sym::cmp_partialeq_eq,
            sym::cmp_partialeq_ne,
            sym::cmp_partialord_lt,
            sym::cmp_partialord_le,
            sym::cmp_partialord_gt,
            sym::cmp_partialord_ge,
            sym::box_new,
            Symbol::intern("panic_explicit"),
        ]
        .iter()
        .any(|item| diagnostics.name_to_id.contains_key(item));
        if reserved(name.as_str())
            || primitive_diagnostic
            || owners.iter().flatten().any(|id| id.krate == crate_num)
        {
            installed_crate(tcx, crate_num, &directory)?;
        }
    }
    Ok(())
}

fn installed_crate(tcx: TyCtxt<'_>, crate_num: CrateNum, directory: &Path) -> Result<(), String> {
    let source = tcx.used_crate_source(crate_num);
    let mut count = 0;
    // CrateSource::paths omits sdylib_interface; do not silently omit it here.
    for path in [
        &source.rmeta,
        &source.rlib,
        &source.dylib,
        &source.sdylib_interface,
    ]
    .into_iter()
    .flatten()
    {
        count += 1;
        installed_file(
            path,
            directory,
            &format!("loaded standard library `{}`", tcx.crate_name(crate_num)),
        )?;
    }
    if count == 0 {
        return Err(unsupported(
            "loaded standard library has no attestable metadata/library source",
        ));
    }
    Ok(())
}
