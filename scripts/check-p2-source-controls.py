#!/usr/bin/env python3
"""Check supplied, freshly built P2 source-control bundles before Rust replay.

No Cargo extraction, native execution or VM replay occurs here. The caller must
first build into a fresh directory. Current source snapshots and independently
queried compiler/driver identities establish consistency, not authenticated build
provenance, complete environment capture, or compiler/IR/CLVM preservation.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tomllib
import unittest

import cargo_authoring
import cargo_bundle
from cargo_authoring import contained_file, require, snapshot, unique_object
from cargo_bundle import load_declared_export

ROOT = Path(__file__).resolve().parents[1]
CONTROL = "experiments/p2-singleton-controls"
NAMES = ("proper_struct", "reversed_parameters", "wrong_dollar")
PACKAGE = "chialisp-oxide-p2-singleton-controls"
RUE = "245c107a8e795229ea08097142f005a4c0d91075"
BASELINE = "puzzles/p2-singleton/puzzle.rs"
BASELINE_SHA = "1ef0b592106b0d497d21b2f28c8c675e5a714a647142a4bc9c377be5f586d7f9"
RECIPE_SHA = "531223d0ed304ec044df9b3017321244c14c1d30d2c2b06ff97b8358318bb5f7"
PRIVATE_FILES = ("Cargo.toml", "Cargo.lock", "mutations.json", "src/lib.rs",
                 *(f"src/{name}.rs" for name in NAMES))
CARGO_OPTIONS = ["--locked", "--manifest-path", f"{CONTROL}/Cargo.toml"]
MAX_FILE_BYTES = 16 * 1024 * 1024


def digest(payload):
    return hashlib.sha256(payload).hexdigest()


def read(root, path):
    with contained_file(root, path).open("rb") as source:
        payload = source.read(MAX_FILE_BYTES + 1)
    require(len(payload) <= MAX_FILE_BYTES, "Source-control file exceeds read bound: " + path)
    return payload


def private_snapshot(root):
    control = (root / CONTROL).resolve()
    require(control.is_relative_to(root.resolve()), "Control workspace escapes repository")
    return {"mode": "cargo_library", "package": PACKAGE, "manifest": "Cargo.toml",
            "library": "src/lib.rs", "exports": {
                name: {"entry": f"chialisp_oxide_p2_singleton_controls::{name}::puzzle",
                       "source": f"src/{name}.rs"} for name in NAMES},
            "sources": [{"path": path, "sha256": digest(read(control, path))}
                        for path in sorted(PRIVATE_FILES)]}


def recipes(root):
    """Check exact reviewed substitutions, independently of emitted IR bodies."""
    control = (root / CONTROL).resolve()
    payload = read(control, "mutations.json")
    require(digest(payload) == RECIPE_SHA, "Reviewed source-control recipe bytes changed")
    value = json.loads(payload, object_pairs_hook=unique_object)
    require(set(value) == {"baseline_source", "witness", "mutations"}
            and value["baseline_source"] == BASELINE
            and value["witness"] == "puzzles/p2-singleton/vectors.json:named[0]",
            "Changed source-control recipe contract")
    baseline = read(root, BASELINE)
    require(digest(baseline) == BASELINE_SHA, "Reviewed mutation baseline changed")
    text = baseline.decode()
    require([row["name"] for row in value["mutations"]] == list(NAMES),
            "Missing/duplicate/reordered source mutation")
    for row in value["mutations"]:
        require(set(row) == {"name", "source", "original_sha256", "old", "new", "classification"}
                and row["source"] == f"{CONTROL}/src/{row['name']}.rs"
                and row["original_sha256"] == BASELINE_SHA
                and text.count(row["old"]) == 1 and row["old"] != row["new"],
                "Unreviewed source mutation identity or substitution")
        require(read(root, row["source"]).decode() == text.replace(row["old"], row["new"], 1),
                "Source mutation differs from its exact recipe: " + row["name"])
    return value


def validate_bundle(bundle, *, root, compiler, driver_sha256, toolchain):
    """Offline loader checks with expectations supplied independently by caller."""
    root = root.resolve()
    before = snapshot(root)
    control_before = private_snapshot(root)
    recipe = recipes(root)

    def current():
        require(snapshot(root) == before, "Main Cargo/SDK inputs changed during source-control loading")
        recipes(root)
        return private_snapshot(root)

    checked = []
    manifest_path = None
    manifest_sha256 = None
    for name in NAMES:
        export = control_before["exports"][name]
        loaded = load_declared_export(bundle, root=root / CONTROL,
            authoring=control_before, selected_entry=export["entry"], compiler=compiler,
            driver_sha256=driver_sha256, toolchain=toolchain, current_authoring=current,
            optimize=True, rue_revision=RUE, cargo_options=CARGO_OPTIONS)
        # The shared reader checks identity of EVERY IR, not only this export.
        require(manifest_path is None or loaded.manifest_path == manifest_path,
                "Source-control bundle pointer changed between selected exports")
        manifest_path = loaded.manifest_path
        payload = read(manifest_path.parent, manifest_path.name)
        require(json.loads(payload, object_pairs_hook=unique_object) == loaded.manifest,
                "Source-control manifest changed after loading")
        require(manifest_sha256 is None or digest(payload) == manifest_sha256,
                "Source-control manifest changed between selected exports")
        manifest_sha256 = digest(payload)
        checked.append({"name": name, "entry": export["entry"],
                        "source": f"{CONTROL}/{export['source']}",
                        "ir_sha256": loaded.ir_sha256})
    require(current() == control_before, "Private source-control inputs changed during loading")
    require(snapshot(root) == before, "Main Cargo/SDK inputs changed after source-control loading")
    return {"schema_version": 1, "protocol": "chialisp-oxide-p2-source-controls/v1",
            "status": "checked_bundle_identity_and_current_sources", "exports": checked,
            "manifest_path": str(manifest_path), "manifest_sha256": manifest_sha256,
            "compiler": compiler, "driver_sha256": driver_sha256, "toolchain": toolchain,
            "rue_revision": RUE, "optimize_lir": True, "cargo_options": CARGO_OPTIONS,
            "private_authoring": control_before, "main_authoring": before,
            "implementation_sources": [{"path": f"scripts/{path.name}", "sha256": digest(path.read_bytes())}
                for path in (Path(__file__).resolve(), Path(cargo_authoring.__file__).resolve(),
                             Path(cargo_bundle.__file__).resolve())],
            "baseline": {"path": BASELINE, "sha256": BASELINE_SHA},
            "recipes": {"path": f"{CONTROL}/mutations.json", "sha256": RECIPE_SHA,
                        "mutations": [row["name"] for row in recipe["mutations"]]},
            "scope": "Fresh caller-supplied three-export Cargo bundle identity and current source consistency only. Exact reviewed Rust mutations are checked separately from IR identity. The main declared Cargo/SDK and seven private inputs are checked before/after loading. This gate does not extract, validate Program semantics, compile or replay native/CLVM programs, authenticate execution, capture the complete build environment, or prove compiler/IR/CLVM preservation. Existing Rust replay performs Program validation and six finite VM comparisons."}


def captured_identities(root, driver):
    toolchain = tomllib.loads(read(root, "rust-toolchain.toml").decode())["toolchain"]["channel"]
    result = subprocess.run(["rustup", "run", toolchain, "rustc", "-vV"], cwd=root,
                            text=True, capture_output=True, timeout=30)
    require(result.returncode == 0 and not result.stderr, "Cannot independently query pinned compiler")
    return result.stdout, digest(driver.resolve(strict=True).read_bytes()), toolchain


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--driver", type=Path, default=ROOT / "target/debug/chialisp-oxide-driver")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    compiler, driver_sha256, toolchain = captured_identities(ROOT, args.driver)
    result = validate_bundle(args.bundle.resolve(), root=ROOT, compiler=compiler,
                             driver_sha256=driver_sha256, toolchain=toolchain)
    print(json.dumps(result, indent=2, sort_keys=True))
    if args.self_test:
        # Constructed loader controls do not add execution credit to this record.
        suite = unittest.defaultTestLoader.discover(str(ROOT / "scripts"), pattern="test_p2_source_controls.py")
        require(suite.countTestCases() > 0, "Missing source-control identity corruption tests")
        if not unittest.TextTestRunner(stream=sys.stderr).run(suite).wasSuccessful():
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
