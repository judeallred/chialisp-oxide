"""Generic actual-source VM controls for paired-corpus exports.

`experiments/corpus-controls/mutations.json` declares exact source edits and a
distinguishing witness per control. Capture writes an isolated Cargo package of
original and mutant modules, compiles it with the real `cargo chialisp-oxide` in
both modes, and executes each witness through the primary VM CLI. Each original
module must compile to the corpus artifact's exact tree hash and match the
independent port oracle; each mutant's witness outcome must differ.

These are primary-VM observations only: no native, SDK or consensus credit, no
authenticated execution and no compiler-preservation proof.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

import corpus_report
import port_oracles

ROOT = Path(__file__).resolve().parents[1]
RECIPE = "experiments/corpus-controls/mutations.json"
PACKET = "corpus/corpus-source-controls"
PACKAGE = "chialisp-oxide-corpus-controls"
CRATE = PACKAGE.replace("-", "_")
PROTOCOL = "chialisp-oxide-corpus-source-controls/v1"
MODES = (("on", True), ("off", False))
MAX_FILE = 4 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def read(path, limit=MAX_FILE):
    path = Path(path)
    require(path.is_file() and not path.is_symlink(), f"regular input required: {path}")
    raw = path.read_bytes()
    require(len(raw) <= limit, f"bounded input: {path}")
    return raw


def recipe(root=ROOT):
    data = json.loads(read(root / RECIPE))
    require(data.get("schema_version") == 1 and data.get("classification") == "actual_source_vm_controls", "recipe header")
    modules = {m["module"]: m for m in data["modules"]}
    require(len(modules) == len(data["modules"]), "unique control modules")
    for control in data["controls"]:
        require(control["original"] in modules and control["mutant"] in modules, "controls name declared modules")
        require(not modules[control["original"]]["edits"] and modules[control["mutant"]]["edits"], "original unedited, mutant edited")
        require(modules[control["original"]]["slug"] == modules[control["mutant"]]["slug"], "one slug per control")
    return data, modules


def module_source(module, root=ROOT):
    text = read(root / module["source"]).decode()
    for edit in module["edits"]:
        require(text.count(edit["before"]) == 1, f"{module['module']}: edit anchor must occur exactly once")
        text = text.replace(edit["before"], edit["after"])
    return text


def source_window(root=ROOT):
    """Hashes of the recipe and every declared source; a capture is fresh only if these match."""
    data, modules = recipe(root)
    paths = sorted({RECIPE, *(m["source"] for m in modules.values())})
    return {path: sha(read(root / path)) for path in paths}


def write_package(directory, root=ROOT):
    data, modules = recipe(root)
    (directory / "src").mkdir(parents=True)
    facade = json.dumps(str(root / "crates/chialisp-oxide"))
    (directory / "Cargo.toml").write_text(
        f'[package]\nname = "{PACKAGE}"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n'
        f'[lib]\npath = "src/lib.rs"\n\n[dependencies]\nchialisp-oxide = {{ path = {facade} }}\n\n[workspace]\n')
    shutil.copyfile(root / "Cargo.lock", directory / "Cargo.lock")
    shutil.copyfile(root / "rust-toolchain.toml", directory / "rust-toolchain.toml")
    lib = ["//! Generated corpus source controls; WIP prototype, not for real funds.\n"]
    for name, module in modules.items():
        (directory / "src" / f"{name}.rs").write_text(module_source(module, root))
        lib.append(f"pub mod {name};\n")
    (directory / "src/lib.rs").write_text("".join(lib))


def run_witness(cli, program_hex_path, witness):
    result = subprocess.run([str(cli), "run", str(program_hex_path), "--solution", witness],
                            capture_output=True, text=True, timeout=120)
    require(result.returncode in (0, 2), f"CLI run failed: {result.stderr[-400:]}")
    record = json.loads(result.stdout)
    outcome = record["outcome"]
    if outcome.get("status") == "rejected":
        # Only a typed program failure is a rejection; resource or infrastructure
        # failures are inconclusive and must not count as policy evidence.
        require(result.returncode == 2 and (record.get("failure") or {}).get("kind") == "program",
                "inconclusive VM failure is not a rejection")
    else:
        require(result.returncode == 0 and outcome.get("status") == "accepted", "typed witness outcome")
    return outcome


def corpus_tree_hashes(root=ROOT):
    report = corpus_report.decode(read(root / "corpus/validation.json", corpus_report.REPORT_LIMIT))
    return {(p["slug"], a["optimize_lir"]): a["tree_hash"] for p in report["puzzles"]
            for a in p["artifacts"] if a["backend"] == "rust_mir"}


def capture(output, *, tools, root=ROOT):
    """Build every control module in both modes and record witness outcomes."""
    output = Path(output).resolve()
    require(not output.exists(), "new capture directory required")
    data, modules = recipe(root)
    before = source_window(root)
    package = output / "package"
    write_package(package, root)
    programs = output / "programs"
    programs.mkdir(parents=True)
    artifacts = {}
    for mode, optimize in MODES:
        argv = [str(tools / "cargo-chialisp-oxide"), "build", "--manifest-path", str(package / "Cargo.toml"),
                "--offline", "--output-dir", str(output / f"bundle-{mode}")]
        if not optimize:
            argv.append("--no-optimize")
        result = subprocess.run(argv, capture_output=True, text=True, timeout=1800)
        require(result.returncode == 0, f"control build ({mode}) failed: {result.stderr[-800:]}")
        manifests = sorted((output / f"bundle-{mode}").glob("*/manifest.json"))
        require(len(manifests) == 1, "one bundle manifest per mode")
        manifest = json.loads(read(manifests[0]))
        for entry in manifest["entries"]:
            crate, name, function = entry["entry"].split("::")
            require(crate == CRATE and function == "puzzle" and name in modules, "only declared control exports")
            artifact = entry["artifact"]
            path = programs / f"{name}-{mode}.clvm.hex"
            path.write_text(artifact["clvm_hex"])
            artifacts[(name, mode)] = dict(bytes=artifact["bytes"], tree_hash=artifact["tree_hash"],
                                           clvm_sha256=sha(path.read_bytes()), path=path.relative_to(output).as_posix())
    require(set(artifacts) == {(name, mode) for name in modules for mode, _ in MODES}, "every module in both modes")
    controls = []
    for control in data["controls"]:
        row = dict(name=control["name"], original=control["original"], mutant=control["mutant"], modes={})
        for mode, _ in MODES:
            row["modes"][mode] = {
                role: run_witness(tools / "chialisp-oxide", output / artifacts[(control[role], mode)]["path"], control["witness"])
                for role in ("original", "mutant")}
        controls.append(row)
    require(source_window(root) == before, "sources changed during capture")
    report = dict(protocol=PROTOCOL, scope=data["scope"], source_window=before,
                  modules={f"{name}:{mode}": value for (name, mode), value in sorted(artifacts.items())},
                  controls=controls)
    validate(report, output, root=root)
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    shutil.rmtree(package)
    for mode, _ in MODES:
        shutil.rmtree(output / f"bundle-{mode}")
    return report


def validate(report, directory, *, root=ROOT, cli=None):
    """Check freshness, corpus identity, oracle agreement and mutant divergence; optionally replay."""
    data, modules = recipe(root)
    require(report.get("protocol") == PROTOCOL and report.get("scope") == data["scope"], "protocol and scope")
    require(report.get("source_window") == source_window(root), "stale recipe or control source; recapture required")
    expected_keys = {f"{name}:{mode}" for name in modules for mode, _ in MODES}
    require(set(report["modules"]) == expected_keys, "complete module inventory")
    corpus = corpus_tree_hashes(root)
    for key, row in report["modules"].items():
        name, mode = key.rsplit(":", 1)
        raw = read(Path(directory) / row["path"])
        require(sha(raw) == row["clvm_sha256"], f"retained program bytes: {key}")
        if not modules[name]["edits"]:
            require(corpus[(modules[name]["slug"], mode == "on")] == row["tree_hash"],
                    f"original control compiles to the exact corpus artifact: {key}")
    require([c["name"] for c in report["controls"]] == [c["name"] for c in data["controls"]], "complete ordered controls")
    for declared, row in zip(data["controls"], report["controls"]):
        oracle = port_oracles.PORTS[modules[declared["original"]]["slug"]]["expected"](declared["witness"])
        for mode, _ in MODES:
            observed = row["modes"][mode]
            require(observed["original"] == oracle, f"{declared['name']}/{mode}: original matches the independent oracle")
            require(observed["mutant"] != observed["original"], f"{declared['name']}/{mode}: mutant witness must diverge")
            if cli:
                for role in ("original", "mutant"):
                    path = Path(directory) / report["modules"][f"{declared[role]}:{mode}"]["path"]
                    require(run_witness(cli, path, declared["witness"]) == observed[role], f"{declared['name']}/{mode}/{role}: replay")
    return dict(controls=len(report["controls"]), modules=len(report["modules"]), replayed=bool(cli))
