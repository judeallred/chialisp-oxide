"""Checked actual-source recipes and finite boomerang control inputs.

Importing this module executes no Cargo, compiler, VM, SDK or formatter process.
The private generated package invokes the current annotated source itself and
three deliberately wrong policies plus a source-equivalent positive control.
These are policy errors Rust permits, not compiler-enforced security properties.
"""
from pathlib import Path
import hashlib
import json
import os
import re
import stat
import tomllib
from source_walk import WalkLimits, source_files

import cargo_authoring
import cargo_bundle
import raw_tree

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = "experiments/boomerang-controls"
BASE = "puzzles/boomerang/puzzle.rs"
RECIPES = DIRECTORY + "/mutations.json"
PACKAGE = "chialisp-oxide-boomerang-controls"
PROTOCOL = "chialisp-oxide-boomerang-source-controls/v1"
LEDGER_PROTOCOL = "chialisp-oxide-boomerang-source-controls-ledger/v1"
NAMES = ("original", "equivalent", "drop_claim_signature", "late_claim", "early_refund")
TOOLS = ("cargo-chialisp-oxide", "chialisp-oxide-driver", "chialisp-oxide", "chialisp-oxide-verifier")
KEY = bytes.fromhex("8bc61e93380f3427ed2ace813fdff67504f55268a99a72cea57133c2b32a1d836fd52381ae9ca10957557b80064059e5")
INTENT = b"chialisp-oxide/boomerang/claim:1"
TERMS = "(0x" + KEY.hex() + " 0x" + "42" * 32 + " 0x" + "43" * 32 + " 1000 100)"
FILES = sorted(["Cargo.toml", "src/lib.rs", "tests/native.rs", "examples/ledger.rs", "witnesses.json", "mutation-recipes.json"] + ["src/" + name + ".rs" for name in NAMES])
SCOPE = "WIP prototype, not ready for real funds. Five actual source-derived Rust exports in both compiler modes, one native test group with nine output observations, eighteen retained primary VM observations and eighteen separate SDK consensus attempts. Each mutant is funded under its own compiled curried reveal. No Rue or Chialisp mutant execution, generic ledger group, exhaustive security, authenticated execution, BLS/Optional/ledger model proof or compiler-preservation credit."
require = cargo_authoring.require


def closed(value, fields, label):
    require(type(value) is dict and set(value) == set(fields), label + ": closed fields required")


def digest(value):
    return hashlib.sha256(value).hexdigest()


def read(path, limit=64 * 1024 * 1024, *, allow_empty=False):
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    try:
        metadata = os.fstat(descriptor)
        require(stat.S_ISREG(metadata.st_mode) and (0 if allow_empty else 1) <= metadata.st_size <= limit, "bounded regular input required")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            payload = stream.read(limit + 1)
        require((0 if allow_empty else 1) <= len(payload) <= limit, "input byte bound")
        return payload
    finally:
        os.close(descriptor)


def sha(path, limit=2 * 1024 * 1024 * 1024):
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    try:
        require(stat.S_ISREG(os.fstat(descriptor).st_mode), "regular hash input required")
        result, total = hashlib.sha256(), 0
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            while chunk := stream.read(min(65536, limit - total + 1)):
                total += len(chunk)
                require(total <= limit, "hash byte bound")
                result.update(chunk)
    finally:
        os.close(descriptor)
    return result.hexdigest()


def load(path):
    return json.loads(read(path), object_pairs_hook=cargo_authoring.unique_object)


def absolute(value):
    require(type(value) is str and value.startswith("/") and not value.startswith("//"), "absolute captured path required")
    path = Path(value)
    require(path.as_posix() == value and ".." not in path.parts, "normalized captured path required")
    return path


def relative(base, value):
    path = cargo_bundle._relative(value)
    original = Path(base).resolve().joinpath(*path.parts)
    require(all(not current.is_symlink() for current in [original, *list(original.parents)[:len(path.parts)-1]]), "retained symlink not admitted")
    return cargo_bundle._contained(base, value)


def list_tree(items):
    result = b""
    for item in reversed(items):
        result = item, result
    return result


def integer(value):
    return raw_tree.parse(str(value))


def expected_output(name, claim):
    require(name in NAMES and type(claim) is bool, "closed output witness")
    before = claim and name != "late_claim"
    deadline = 0 if (claim and name == "late_claim") or (not claim and name == "early_refund") else 100
    conditions = [
        list_tree([integer(85 if before else 81), integer(deadline)]),
        list_tree([integer(73), integer(1000)]),
        list_tree([integer(51), bytes([0x42 if claim else 0x43]) * 32, integer(1000)]),
    ]
    if claim and name != "drop_claim_signature":
        conditions.append(list_tree([integer(50), KEY, INTENT]))
    return {"status": "accepted", "output_hex": raw_tree.serialize(list_tree(conditions)).hex()}


def witnesses():
    scenarios = [
        ("timely_unsigned_claim", True, 99, "none", "BadAggregateSignature"),
        ("signed_late_claim", True, 101, "recipient", "AssertBeforeSecondsAbsoluteFailed"),
        ("early_unsigned_refund", False, 99, "none", "AssertSecondsAbsoluteFailed"),
    ]
    result = []
    for name in NAMES:
        selected = scenarios if name in ("original", "equivalent") else [scenarios[NAMES.index(name) - 2]]
        for label, claim, timestamp, signing, code in selected:
            result.append(dict(name=name, witness=label, claim=claim, timestamp=timestamp, signing=signing,
                solution="(" + TERMS + (" 1)" if claim else " ())"),
                instance_solution="(1)" if claim else "(())",
                expected=expected_output(name, claim),
                ledger_expected={"status": "consensus_rejected", "code": code} if name in ("original", "equivalent") else {"status": "accepted"}))
    require(len(result) == 9, "nine finite output/ledger witnesses")
    return result


def inputs(root=ROOT):
    baseline = read(Path(root) / BASE)
    recipe_bytes = read(Path(root) / RECIPES)
    recipes = json.loads(recipe_bytes, object_pairs_hook=cargo_authoring.unique_object)
    closed(recipes, ["schema_version", "source", "classification", "controls", "scope"], "source recipes")
    require(type(recipes["schema_version"]) is int and recipes["schema_version"] == 1 and recipes["source"] == BASE
        and recipes["classification"] == "actual_source_policy_controls" and type(recipes["scope"]) is str, "recipe identity")
    require(type(recipes["controls"]) is list and [row.get("name") for row in recipes["controls"]] == list(NAMES), "closed five source controls")
    sources, controls = {}, []
    for index, recipe in enumerate(recipes["controls"]):
        closed(recipe, ["name", "role", "edits"], "source recipe")
        require(type(recipe["role"]) is str and recipe["role"] and type(recipe["edits"]) is list and len(recipe["edits"]) == (0 if index == 0 else 1), "one exact mutation per control")
        source = baseline.decode()
        for edit in recipe["edits"]:
            closed(edit, ["before", "after"], "source substitution")
            require(all(type(edit[key]) is str and 0 < len(edit[key]) <= 1024 for key in ("before", "after"))
                and edit["before"] != edit["after"] and source.count(edit["before"]) == 1, "unique exact source mutation anchor")
            source = source.replace(edit["before"], edit["after"], 1)
        payload = source.encode()
        sources["src/" + recipe["name"] + ".rs"] = payload
        controls.append(dict(name=recipe["name"], role=recipe["role"], source_sha256=digest(payload), edits=recipe["edits"]))
    require(sources["src/original.rs"] == baseline, "original must remain byte-identical")
    instance = load(Path(root) / "puzzles/boomerang/puzzle.json")["coin_instances"]
    require(len(instance) == 1 and instance[0]["label"] == "public_demo_payment"
        and raw_tree.parse(instance[0]["argument"]) == raw_tree.parse(TERMS), "base demonstration terms changed")
    return sources, dict(source=BASE, source_sha256=digest(baseline), recipe=RECIPES,
        recipe_sha256=digest(recipe_bytes), controls=controls), recipe_bytes


def package_files(source_root, root=ROOT):
    source_root = absolute(str(source_root))
    files, recipe, recipe_bytes = inputs(root)
    files["Cargo.toml"] = ("[package]\nname = \"" + PACKAGE + "\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\n\n[dependencies]\nchialisp-oxide = { path = " + json.dumps(str(source_root / "crates/chialisp-oxide")) + " }\n\n[dev-dependencies]\nanyhow = \"1\"\nhex = \"0.4\"\nserde = { version = \"1\", features = [\"derive\"] }\nserde_json = \"=1.0.154\"\nchialisp-oxide-simulator = { path = " + json.dumps(str(source_root / "crates/chialisp-oxide-simulator")) + " }\n").encode()
    files["src/lib.rs"] = "".join("pub mod " + name + ";\n" for name in sorted(NAMES)).encode()
    files["tests/native.rs"] = read(Path(root) / DIRECTORY / "native.rs")
    files["examples/ledger.rs"] = read(Path(root) / DIRECTORY / "ledger.rs")
    files["witnesses.json"] = (json.dumps(witnesses(), indent=2) + "\n").encode()
    files["mutation-recipes.json"] = recipe_bytes
    require(sorted(files) == FILES, "closed generated package inputs")
    return files, recipe


def private_snapshot(package, root=ROOT, *, locked=True):
    package = Path(package).resolve()
    files, _ = package_files(root, root)
    for path, payload in files.items():
        require(read(package / path) == payload, "private source is not exact base/recipe/template: " + path)
    paths = sorted(FILES + (["Cargo.lock"] if locked else []))
    require(sorted(path.relative_to(package).as_posix() for path in package.rglob("*") if path.is_file()) == paths, "closed private source file set")
    if locked:
        tomllib.loads(read(package / "Cargo.lock").decode())
    return dict(mode="cargo_library", package=PACKAGE, manifest="Cargo.toml", library="src/lib.rs",
        exports={name: dict(entry="chialisp_oxide_boomerang_controls::" + name + "::puzzle", source="src/" + name + ".rs") for name in NAMES},
        sources=[dict(path=path, sha256=sha(package / path)) for path in paths])


def source_window(root=ROOT):
    root = Path(root)
    authoring = cargo_authoring.snapshot(root)
    paths = {row["path"] for row in authoring["sources"]}
    paths.update(["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml", BASE,
        "puzzles/boomerang/puzzle.json", "crates/chialisp-oxide-vm/failure-protocol.json",
        "crates/chialisp-oxide-simulator/rejection-protocol.json", RECIPES,
        DIRECTORY + "/native.rs", DIRECTORY + "/ledger.rs"])
    paths.update(["scripts/source_walk.py", "scripts/test_source_walk.py"])
    # Every root Python reader/producer is a declared implementation input. Never
    # recurse through target, virtual environments or a private prepared package.
    script_paths = source_files(root, "scripts", suffixes=(".py",), recursive=False,
        limits=WalkLimits(directories=1, entries=4096))
    paths.update(script_paths)
    members = tomllib.loads(read(root / "Cargo.toml").decode())["workspace"]["members"]
    require(type(members) is list and 0 < len(members) <= 64, "bounded workspace source roots")
    directories = []
    for member in members:
        require(type(member) is str and not Path(member).is_absolute() and ".." not in Path(member).parts and Path(member).as_posix() == member, "confined workspace member")
        manifest = member + "/Cargo.toml"
        paths.add(manifest)
        metadata = tomllib.loads(read(root / manifest).decode())
        require(metadata["package"].get("build") in (None, False, "build.rs"), "reviewed conventional build script required")
        for kind, directory in [("lib", "src"), ("bin", "src"), ("example", "examples"), ("test", "tests"), ("bench", "benches")]:
            declarations = [metadata[kind]] if kind == "lib" and kind in metadata else metadata.get(kind, [])
            require(type(declarations) is list and len(declarations) <= 64, "bounded declared targets")
            for target in declarations:
                if "path" not in target:
                    continue
                path = target["path"]
                require(type(path) is str and not Path(path).is_absolute() and ".." not in Path(path).parts
                    and Path(path).as_posix() == path and Path(path).suffix == ".rs"
                    and ((member == "puzzles" and kind == "lib" and path == "lib.rs") or path.startswith(directory + "/")), "reviewed conventional source layout required")
        if (root / member / "build.rs").is_file():
            paths.add(member + "/build.rs")
        for directory in ("src", "examples", "tests", "benches"):
            directories.append(member + "/" + directory)
    # Reserve the shallow script pass's entire entry/directory budget so both
    # passes together retain the same 16,384-entry / 4,096-directory ceilings.
    paths.update(source_files(root, directories, limits=WalkLimits(
        directories=4095, entries=12288, sources=4096-len(script_paths))))
    require(len(paths) <= 4096, "source count bound")
    total, rows = 0, []
    for path in sorted(paths):
        payload = read(cargo_authoring.contained_file(root, path))
        total += len(payload)
        require(total <= 64 * 1024 * 1024, "source byte bound")
        rows.append(dict(path=path, sha256=digest(payload)))
    return dict(authoring=authoring, sources=rows)


def native_rows(text):
    require(type(text) is str and len(text.encode()) <= 2 * 1024 * 1024, "native output bound")
    rows = [json.loads(value, object_pairs_hook=cargo_authoring.unique_object) for value in re.findall(r"BOOMERANG_MUTATION_NATIVE (\{[^\n]+\})", text)]
    expected = [dict(name=row["name"], witness=row["witness"], actual=row["expected"]) for row in witnesses()]
    require(rows == expected and text.count("test result: ok. 1 passed; 0 failed;") == 1, "one actual native test group/nine exact output observations required")
    return rows


def options(package):
    return ["--manifest-path", str(package / "Cargo.toml"), "--package", PACKAGE, "--locked", "--offline"]


def strict_abi(envelope, entry):
    closed(envelope, ["ir_version", "environment", "program"], "optional IR envelope")
    require(type(envelope["ir_version"]) is int and envelope["ir_version"] == 6 and envelope["environment"] == "strict", "exact v6 strict optional-output ABI")
    body = envelope["program"]
    integer_type = dict(kind="int", bits=64, signed=False)
    byte_type = lambda length: dict(kind="bytes", len=length)
    tuple_type = lambda *items: dict(kind="tuple", items=list(items))
    signature = tuple_type(integer_type, byte_type(48), byte_type(32))
    result = dict(kind="pair", first=tuple_type(integer_type, integer_type), rest=dict(kind="pair",
        first=tuple_type(integer_type, integer_type), rest=dict(kind="pair",
        first=tuple_type(integer_type, byte_type(32), integer_type), rest=dict(kind="optional", item=signature))))
    require(type(body) is dict and body.get("name") == entry and body.get("result") == result
        and [row["ty"] for row in body["parameters"]] == [tuple_type(byte_type(48), byte_type(32), byte_type(32), integer_type, integer_type), dict(kind="bool")], "exact exported Terms/bool/result shapes")


def fresh_bundle(directory, package, *, compiler, driver_sha256, toolchain, optimize):
    before = source_window()
    private = private_snapshot(package)
    def current():
        require(source_window() == before, "main sources changed during bundle loading")
        return private_snapshot(package)
    result = None
    for name in NAMES:
        loaded = cargo_bundle.load_declared_export(directory, root=package, authoring=private,
            selected_entry=private["exports"][name]["entry"], compiler=compiler, driver_sha256=driver_sha256,
            toolchain=toolchain, current_authoring=current, optimize=optimize,
            rue_revision="245c107a8e795229ea08097142f005a4c0d91075", cargo_options=options(package))
        require(len(loaded.manifest["entries"]) == 5, "complete five-export bundle")
        require(result is None or result.manifest_path == loaded.manifest_path, "bundle pointer changed")
        result = loaded
    require(current() == private, "private input window changed")
    return result


def curry(module):
    module_tree = raw_tree.deserialize(module)
    one = integer(1)
    return raw_tree.serialize(list_tree([integer(2), (one, module_tree),
        list_tree([integer(4), (one, raw_tree.parse(TERMS)), one])]))
