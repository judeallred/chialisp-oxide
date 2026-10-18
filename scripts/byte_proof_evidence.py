"""Independent byte-policy expectations and declared proof inputs.

The data oracle never calls a compiler, the SMT translator or the Rust policy.
The generic tree codec comes from the separately tested structural proof gate.
"""
import difflib
import hashlib
from pathlib import Path
import runpy

from cargo_authoring import contained_file, load_json, snapshot

ROOT = Path(__file__).resolve().parents[1]
TREE = runpy.run_path(str(ROOT / "scripts/check-tree-proofs.py"))
CODEC = runpy.run_path(str(ROOT / "scripts/check-canonical-factories.py"))
HELPERS = runpy.run_path(str(ROOT / "scripts/check-canonical-helpers.py"))
tree, wire, solution = (TREE[name] for name in ("tree", "wire", "solution"))
ENTRY = "chialisp_oxide_puzzles::atom_not_equal::puzzle"
SOURCE = "puzzles/atom-not-equal/puzzle.rs"
DIRECTORY = "proofs/atom-not-equal"
SPECIFICATION = f"{DIRECTORY}/specification.json"
LINEAGE = "experiments/byte-proofs/src/lineage.json"
CONTROL_IDS = ("operator_eq", "self_inequality", "constant_true", "constant_false", "swap_operands")
POSITIVE = "swap_operands"
SCOPE = {
    "kind": "source_derived_structural_boolean_ir_model",
    "domain": "All finite environment trees with arbitrary finite byte atoms, including malformed strict argument lists.",
    "claim": "The extracted strict two-atom Boolean IR accepts exactly the independent ABI and returns byte inequality in the structural model.",
    "excludes": ["rustc/MIR extraction preservation", "SMT translator correctness proof", "ABI/compiler/CLVM preservation", "Rue optimizer correctness", "termination or resource equivalence", "constant-time behavior", "cryptographic authorization", "consensus admission", "absence of security breaches"],
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def expected_specification():
    env = {"term": "input"}
    first = lambda value: {"term": "first", "value": value}
    rest = lambda value: {"term": "rest", "value": value}
    return {
        "acceptance": {"predicate": "all", "items": [
            {"predicate": "is_pair", "value": env},
            {"predicate": "is_pair", "value": rest(env)},
            {"predicate": "is_atom", "value": first(env)},
            {"predicate": "is_atom", "value": first(rest(env))},
            {"predicate": "byte_equal", "left": rest(rest(env)), "right": {"term": "atom", "hex": ""}},
        ]},
        "output": {"predicate": "not", "value": {
            "predicate": "byte_equal", "left": first(env), "right": first(rest(env)),
        }},
    }


def specification():
    value = load_json(ROOT / SPECIFICATION)
    require(value == expected_specification(), "Independent exact-byte specification changed")
    return value


def arguments(witness):
    value = tree(witness["environment"])
    if not isinstance(value, tuple):
        return None
    left, tail = value
    if not isinstance(left, bytes) or not isinstance(tail, tuple):
        return None
    right, terminator = tail
    return (left, right) if isinstance(right, bytes) and terminator == b"" else None


def boolean_outcome(value):
    require(type(value) is bool, "Expected an actual Boolean")
    return {"status": "accepted", "output_hex": "01" if value else "80"}


def expected(witness):
    values = arguments(witness)
    return {"status": "rejected"} if values is None else boolean_outcome(values[0] != values[1])


def control_expected(control, witness):
    values = arguments(witness)
    require(values is not None, "Source-policy control needs actual typed native arguments")
    left, right = values
    result = {"operator_eq": left == right, "self_inequality": False,
              "constant_true": True, "constant_false": False, "swap_operands": right != left}
    require(control in result, "Unknown reviewed source control")
    return boolean_outcome(result[control])


def lineage():
    value = load_json(ROOT / LINEAGE)
    base = (ROOT / SOURCE).read_text()
    require(value["schema_version"] == 1 and value["base"] == {"source": SOURCE, "sha256": sha(ROOT / SOURCE)},
            "Source-control lineage base changed")
    require(value["abi"] == {"environment": "strict", "parameters": ["&[u8]", "&[u8]"], "result": "bool"},
            "Source-control declared ABI changed")
    require(tuple(row["id"] for row in value["controls"]) == CONTROL_IDS, "Source controls missing/duplicated/reordered")
    replacements = {"operator_eq": "left == right", "self_inequality": "left != left",
                    "constant_true": "true", "constant_false": "false", "swap_operands": "right != left"}
    require(base.count("left != right") == 1, "Unrecognized original byte-comparison source")
    for row in value["controls"]:
        name = row["id"]
        path = f"experiments/byte-proofs/src/{name}.rs"
        entry = f"chialisp_oxide_byte_proofs::{name}::puzzle"
        prefix = "#[allow(unused_variables, clippy::eq_op)]\n" if name == "self_inequality" else "#[allow(unused_variables)]\n" if name.startswith("constant_") else ""
        text = prefix + base.replace("left != right", replacements[name])
        require(row["source"] == path and row["entry"] == entry and (ROOT / path).read_text() == text
                and row["source_sha256"] == sha(ROOT / path), "Unexpected source-control edit or identity")
        patch = "".join(difflib.unified_diff(base.splitlines(True), text.splitlines(True), fromfile=f"a/{SOURCE}", tofile=f"b/{path}"))
        require(row["patch"] == {"format": "unified_diff", "sha256": hashlib.sha256(patch.encode()).hexdigest(), "text": patch},
                "Source-control patch does not describe the exact compiled source")
        role = "positive_equivalence_control" if name == POSITIVE else "negative_mutant"
        require(row["role"] == role, "Source control classification changed")
    for key, path in (("manifest", "experiments/byte-proofs/Cargo.toml"),
                      ("library", "experiments/byte-proofs/src/lib.rs")):
        require(value[key] == {"path": path, "sha256": sha(ROOT / path)}, "Control Cargo source changed")
    native = value["native_cli"]
    require(native["source"] == "experiments/byte-proofs/src/bin/native.rs"
            and native["source_sha256"] == sha(ROOT / native["source"])
            and native["binary"] == "chialisp-oxide-byte-proof-native"
            and native["arguments"] == ["VARIANT", "LEFT_HEX", "RIGHT_HEX"], "Control native dispatcher changed")
    return value


def control_authoring():
    value = lineage()
    paths = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml", LINEAGE}
    paths.update(row["source"] for row in value["controls"])
    paths.update(value[key]["path"] for key in ("manifest", "library"))
    paths.add(value["native_cli"]["source"])
    for package in ("crates/chialisp-oxide", "crates/chialisp-oxide-macros"):
        paths.add(f"{package}/Cargo.toml")
        paths.update(str(path.relative_to(ROOT)) for path in (ROOT / package / "src").rglob("*.rs"))
    return {"mode": "cargo_library", "package": "chialisp-oxide-byte-proofs",
            "manifest": "experiments/byte-proofs/Cargo.toml", "library": "experiments/byte-proofs/src/lib.rs",
            "exports": {row["id"]: {"entry": row["entry"], "source": row["source"]} for row in value["controls"]},
            "sources": [{"path": path, "sha256": sha(contained_file(ROOT, path))} for path in sorted(paths)]}


def authoring():
    value = snapshot(ROOT)
    require(value["exports"]["atom-not-equal"] == {"entry": ENTRY, "source": SOURCE}, "Original Cargo export changed")
    return {**value, "entry": ENTRY}


def checked_corpus(report=None):
    report = load_json(ROOT / "corpus/validation.json") if report is None else report
    current = authoring()
    require(report["authoring"] == {key: value for key, value in current.items() if key != "entry"},
            "Validation report has stale declared Cargo inputs")
    HELPERS["validate"](report)
    rows = [row for row in report["puzzles"] if row["slug"] == "atom-not-equal"]
    require(len(rows) == 1, "Missing/duplicate byte helper corpus row")
    return report, rows[0]


def implementation_sources():
    paths = {row["path"] for row in TREE["implementation_sources"]()}
    paths.update({"scripts/check-byte-proofs.py", "scripts/byte_proof_evidence.py", "scripts/cargo_bundle.py", "scripts/check-canonical-factories.py", "scripts/check-canonical-helpers.py",
                  SPECIFICATION, LINEAGE, "corpus/validation.json", "puzzles/atom-not-equal/puzzle.json",
                  "puzzles/atom-not-equal/reference.clsp", "puzzles/atom-not-equal/puzzle.rue"})
    # checked_corpus executes aggregate helper validation, including the hash
    # gate and its readable/password imports and closed inline/cost profiles.
    # Bind that validation code/data closure as well as the proof translator.
    paths.update({
        "scripts/check-insertion-sort.py",
        "corpus/validate_published_puzzles.py",
        "scripts/check-hashing-puzzles.py",
        "scripts/hashing_provenance.py",
        "scripts/hashing_vectors.py",
        "scripts/ledger_rejections.py",
        "scripts/check-readable-puzzles.py",
        "scripts/runtime_cost_migration.py",
        "scripts/runtime_inline_migration.py",
        "scripts/runtime_refinement_migration.py",
        "scripts/runtime_password_migration.py",
        "scripts/password_evidence.py",
        "scripts/password_ledger.py",
        "scripts/password_provenance.py",
        "scripts/password_vectors.py",
        "corpus/cost-optimization-baseline.json",
        "corpus/helper-inlining-baseline.json",
        "corpus/runtime-refinement-baseline.json",
        "crates/chialisp-oxide-simulator/rejection-protocol.json",
    })
    paths.update(row["path"] for row in control_authoring()["sources"])
    return [{"path": path, "sha256": sha(contained_file(ROOT, path))} for path in sorted(paths)]


def check_program(program, control=None):
    require(set(program) == {"name", "parameters", "result", "body"}, "Expected an actual legacy strict expression export")
    entry = ENTRY if control is None else f"chialisp_oxide_byte_proofs::{control}::puzzle"
    require(program["name"] == entry and program["parameters"] == [
        {"name": "left", "ty": {"kind": "atom"}}, {"name": "right", "ty": {"kind": "atom"}}]
        and program["result"] == {"kind": "bool"}, "Source-derived byte export ABI changed")
    if control in ("constant_true", "constant_false"):
        body = {"op": "bool", "value": control == "constant_true"}
    else:
        left, right = (1, 0) if control == POSITIVE else (0, 0) if control == "self_inequality" else (0, 1)
        body = {"op": "binary", "kind": "eq" if control == "operator_eq" else "ne", "ty": {"kind": "atom"},
                "left": {"op": "arg", "index": left}, "right": {"op": "arg", "index": right}}
    require(program["body"] == body, "Unrecognized actual source-derived policy; refusing guessed mutations")


def check_artifact(artifact):
    require(set(artifact) == {"optimize", "bytes", "clvm_hex", "tree_hash"}
            and type(artifact["optimize"]) is bool and type(artifact["bytes"]) is int,
            "Malformed compiled proof artifact")
    payload = bytes.fromhex(artifact["clvm_hex"])
    require(payload and len(payload) == artifact["bytes"]
            and CODEC["tree_hash"](CODEC["deserialize"](payload)).hex() == artifact["tree_hash"],
            "Compiled proof artifact bytes/size/tree hash disagree")
