//! Versioned export markers are transport metadata, not a trust boundary.
use chialisp_oxide_ir::Environment;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_middle::ty::TyCtxt;

const STRICT_MARKER: &str = "__oxide_puzzle_v1";
const IGNORED_MARKER: &str = "__chialisp_oxide_puzzle_v2_environment_ignored";
const RAW_MARKER: &str = "__chialisp_oxide_puzzle_v3_environment_raw";

pub fn collect(tcx: TyCtxt<'_>) -> Result<Vec<(DefId, Environment)>, String> {
    let mut exports = Vec::new();
    // Include non-body items: a forged marker on a struct must not disappear
    // merely because a separate, valid function also produces an export.
    for owner in tcx.hir_crate_items(()).definitions() {
        if let Some(environment) = environment(tcx, owner)? {
            exports.push((owner.to_def_id(), environment));
        }
    }
    Ok(exports)
}

pub fn environment(tcx: TyCtxt<'_>, owner: LocalDefId) -> Result<Option<Environment>, String> {
    let mut found = None;
    for attribute in tcx.hir_attrs(tcx.local_def_id_to_hir_id(owner)) {
        let Some(text) = attribute.doc_str() else {
            continue;
        };
        let policy = match text.as_str() {
            STRICT_MARKER => Environment::Strict,
            IGNORED_MARKER => Environment::Ignored,
            RAW_MARKER => Environment::Raw,
            marker
                if marker.starts_with("__oxide_puzzle_")
                    || marker.starts_with("__chialisp_oxide_puzzle_") =>
            {
                return Err(error(tcx, owner, "unknown #[puzzle] export marker"));
            }
            _ => continue,
        };
        if found.replace(policy).is_some() {
            return Err(error(
                tcx,
                owner,
                "#[puzzle] requires one annotation with one environment policy",
            ));
        }
    }
    if found.is_some() && tcx.def_kind(owner) != rustc_hir::def::DefKind::Fn {
        return Err(error(
            tcx,
            owner,
            "#[puzzle] requires one annotation on a free function",
        ));
    }
    Ok(found)
}

fn error(tcx: TyCtxt<'_>, owner: LocalDefId, message: &str) -> String {
    format!(
        "{}: {message}",
        tcx.sess
            .source_map()
            .span_to_diagnostic_string(tcx.def_span(owner))
    )
}
