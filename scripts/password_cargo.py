"""Sequential fresh-bundle retention checks using prebuilt Cargo/driver tools.

No workspace tool rebuild occurs here. Run after the coordinated Rust build and
tests have finished. Compiler/source identity is checked with cargo_bundle's
existing loader; this is not binary authentication or compiler preservation.
"""
import hashlib
from pathlib import Path
import subprocess
import tempfile
import tomllib

from cargo_authoring import snapshot
from cargo_bundle import load_declared_export
from password_provenance import ROOT
from password_vectors import require
from runtime_cost_migration import verify_ir_bundle


def file_hash(path):
    require(path.is_file(), "Missing prebuilt Cargo/driver executable: " + str(path))
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(arguments, timeout):
    result = subprocess.run([str(value) for value in arguments], cwd=ROOT,
                            capture_output=True, text=True, timeout=timeout)
    require(result.returncode == 0, "Fresh password IR extraction command failed: " + result.stderr[:3000])
    return result.stdout


def extract(report, *, cargo=None, driver=None):
    cargo = ROOT / "target/debug/cargo-chialisp-oxide" if cargo is None else Path(cargo)
    driver = ROOT / "target/debug/chialisp-oxide-driver" if driver is None else Path(driver)
    declared = snapshot(ROOT)
    require(declared == report["authoring"], "Report Cargo inputs differ before fresh extraction")
    identities = {cargo: file_hash(cargo), driver: file_hash(driver)}
    compiler = command(["rustc", "-vV"], 30)
    toolchain = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    options = ["--manifest-path", declared["manifest"], "--locked", "--offline"]
    summaries = []
    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-password-cargo-") as directory:
        for optimize in (True, False):
            output = Path(directory) / ("on" if optimize else "off")
            arguments = [cargo, "build", *options, "--output-dir", output]
            if not optimize:
                arguments.append("--no-optimize")
            command(arguments, 300)
            # The loader checks every declared entry/artifact, actual requested
            # mode/options, compiler/driver/toolchain, source fingerprints and
            # callback freshness before selecting the new export.
            export = load_declared_export(output, root=ROOT, authoring=declared,
                selected_entry=declared["exports"]["password-coin"]["entry"],
                compiler=compiler, driver_sha256=identities[driver], toolchain=toolchain,
                current_authoring=lambda: snapshot(ROOT), optimize=optimize,
                rue_revision=report["rue_revision"], cargo_options=options)
            summary = verify_ir_bundle(export.manifest_path, report)
            require(summary["optimize_lir"] is optimize, "Fresh bundle mode differs from requested extraction")
            summaries.append({**summary, "compiler_driver_source_identity_checked": True})
            require(snapshot(ROOT) == declared, "Cargo inputs changed during extraction")
            require(all(file_hash(path) == value for path, value in identities.items()),
                    "Prebuilt Cargo/driver changed during extraction; coordinate build phases")
    return summaries
