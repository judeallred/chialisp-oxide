"""Finite actual-source precommit controls; importing this module executes nothing.

The source window and retained files establish consistency, not authenticated
execution, a complete build graph, compiler preservation or protocol security.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import tomllib

import cargo_authoring
import cargo_bundle
from cargo_source_receipt import decode_json
import raw_tree
import precommit_oracle as oracle
from source_walk import WalkLimits, source_files
from vm_failures import check_execution

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = "experiments/precommit-controls"
PACKAGE = "chialisp-oxide-precommit-controls"
PROTOCOL = "chialisp-oxide-precommit-source-controls/v1"
NAMES = ("claim_original", "claim_drop_age", "registry_original", "registry_drop_signature")
CONTROLS = ("minimum_age_original", "minimum_age_removed", "owner_signature_original", "owner_signature_removed")
TOOLS = ("cargo-chialisp-oxide", "chialisp-oxide-driver", "chialisp-oxide", "chialisp-oxide-verifier")
SCOPE = "Four actual Rust source exports: four native output observations, eight primary-VM output observations and eight separate SDK submissions in two compiler modes. Each SDK mutant is funded under its own compiled reveal and composes with an independently compiled Chialisp counterpart. Rejected SDK submissions require exact consensus codes and full in-memory simulator equality. Recorded readers do not authenticate execution or prove BLS, SHA, fairness, liveness, absence of attacks or compiler preservation."
require = cargo_authoring.require


def closed(value, keys, label):
    require(type(value) is dict and set(value) == set(keys), label + ": exact fields required")


def read(path, maximum=16 * 1024 * 1024, *, allow_empty=False):
    path = Path(path)
    require(not path.is_symlink(), "symlink input")
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        metadata = os.fstat(descriptor)
        require(stat.S_ISREG(metadata.st_mode) and (0 if allow_empty else 1) <= metadata.st_size <= maximum, "bounded regular input")
        payload = stream.read(maximum + 1)
    require((0 if allow_empty else 1) <= len(payload) <= maximum, "input byte bound")
    return payload


def digest(payload):
    return hashlib.sha256(payload).hexdigest()


def sha(path, maximum=2 * 1024**3):
    path = Path(path)
    require(type(maximum) is int and 0 < maximum <= 2 * 1024**3, "hash byte bound")
    require(not path.is_symlink(), "symlink hash input")
    result = hashlib.sha256()
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        metadata = os.fstat(descriptor)
        require(stat.S_ISREG(metadata.st_mode) and 0 <= metadata.st_size <= maximum, "bounded regular hash input")
        remaining = metadata.st_size
        while remaining:
            chunk = stream.read(min(65536, remaining))
            require(chunk, "hash input truncated")
            remaining -= len(chunk)
            result.update(chunk)
        require(not stream.read(1), "hash input grew")
    return result.hexdigest()


def load(path):
    return decode_json(read(path))


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n")


def recipe_sources():
    recipes = load(ROOT / DIRECTORY / "mutations.json")
    closed(recipes, ("schema_version", "classification", "controls", "scope"), "recipes")
    require(type(recipes["schema_version"]) is int and recipes["schema_version"] == 1
            and recipes["classification"] == "actual_source_policy_controls", "recipe protocol")
    require([row.get("name") for row in recipes["controls"]] == list(NAMES), "closed four source controls")
    sources = {}
    for index, row in enumerate(recipes["controls"]):
        closed(row, ("name", "source", "role", "edits"), "recipe")
        expected = "puzzles/precommit-" + ("claim" if index < 2 else "registry") + "/puzzle.rs"
        require(row["source"] == expected and type(row["role"]) is str and row["role"], "base source identity")
        require(type(row["edits"]) is list and len(row["edits"]) == (0, 2, 0, 4)[index], "exact edit count")
        source = read(ROOT / expected).decode()
        for edit in row["edits"]:
            closed(edit, ("before", "after"), "edit")
            require(all(type(edit[key]) is str and len(edit[key]) <= 2048 for key in edit)
                    and edit["before"] and edit["before"] != edit["after"] and source.count(edit["before"]) == 1,
                    "unique exact source edit")
            source = source.replace(edit["before"], edit["after"], 1)
        sources["src/" + row["name"] + ".rs"] = source.encode()
    return sources


def witnesses():
    def atom(value, width=32):
        return bytes([value]) * width
    claim = oracle.proper(oracle.proper(atom(0x11), atom(0x22), atom(0x33)), b"\1", atom(0x44))
    registry = oracle.proper(atom(0x55), atom(0x11), atom(0x66), atom(0x33),
                             oracle.proper(atom(0x77, 48), atom(0x88), atom(0x99)))
    claim_output = raw_tree.deserialize(bytes.fromhex(oracle.claim_relation(claim)["output_hex"]))
    registry_output = raw_tree.deserialize(bytes.fromhex(oracle.registry_relation(registry)["output_hex"]))
    claim_fields = oracle.fields(claim_output, 6)
    registry_fields = oracle.fields(registry_output, 6)
    changed_age = list(claim_fields)
    changed_age[2] = oracle.proper(oracle.integer(82), b"")
    outputs = (claim_output, oracle.proper(*changed_age), registry_output,
               oracle.proper(*registry_fields[:4], registry_fields[5]))
    return [dict(name=name, solution=expression(claim if index < 2 else registry),
                 expected=dict(status="accepted", output_hex=raw_tree.serialize(outputs[index]).hex()))
            for index, name in enumerate(NAMES)]


def expression(tree):
    return "(" + expression(tree[0]) + " . " + expression(tree[1]) + ")" if type(tree) is tuple else ("0x" + tree.hex() if tree else "()")


def package_files(source_root=ROOT):
    files = recipe_sources()
    path = json.dumps(str(Path(source_root) / "crates/chialisp-oxide"))
    files["Cargo.toml"] = (f'[package]\nname = "{PACKAGE}"\nversion = "0.1.0"\nedition = "2024"\npublish = false\n\n[workspace]\n\n[dependencies]\nchialisp-oxide = {{ path = {path} }}\n\n[dev-dependencies]\nchialisp-oxide = {{ path = {path}, features = ["vm"] }}\nhex = "0.4"\nserde_json = "=1.0.154"\n').encode()
    files["src/lib.rs"] = "".join(f"pub mod {name};\n" for name in sorted((*NAMES, "precommit_recipe"))).encode()
    files["src/precommit_recipe.rs"] = read(ROOT / "puzzles/precommit_recipe.rs")
    files["tests/native.rs"] = read(ROOT / DIRECTORY / "native.rs")
    files["witnesses.json"] = (json.dumps([dict(name=row["name"], expected=row["expected"]) for row in witnesses()], indent=2) + "\n").encode()
    return files


def private_snapshot(package):
    files = package_files()
    for path, payload in files.items():
        require(read(cargo_bundle._contained(package, path)) == payload, "prepared source changed: " + path)
    paths = sorted([*files, "Cargo.lock"])
    require(sorted(path.relative_to(package).as_posix() for path in package.rglob("*") if path.is_file()) == paths,
            "closed prepared input inventory")
    tomllib.loads(read(package / "Cargo.lock").decode())
    return dict(mode="cargo_library", package=PACKAGE, manifest="Cargo.toml", library="src/lib.rs",
                exports={name: dict(entry=PACKAGE.replace("-", "_") + "::" + name + "::puzzle",
                                    source="src/" + name + ".rs") for name in NAMES},
                sources=[dict(path=path, sha256=sha(package / path)) for path in paths])


def source_window():
    authoring = cargo_authoring.snapshot(ROOT)
    paths = {row["path"] for row in authoring["sources"]}
    members = tomllib.loads(read(ROOT / "Cargo.toml").decode())["workspace"]["members"]
    require(type(members) is list and len(members) <= 64, "workspace member bound")
    roots = []
    for member in [*members, "vendor/rue-lir"]:
        require(type(member) is str and not Path(member).is_absolute() and ".." not in Path(member).parts, "confined member")
        paths.add(member + "/Cargo.toml")
        if (ROOT / member / "build.rs").is_file():
            paths.add(member + "/build.rs")
        roots.extend(member + "/" + name for name in ("src", "tests", "examples", "benches"))
    paths.update(source_files(ROOT, roots, limits=WalkLimits(directories=4095, entries=12288)))
    paths.update(source_files(ROOT, "scripts", suffixes=(".py",), recursive=False,
                              limits=WalkLimits(directories=1, entries=4096)))
    paths.update(DIRECTORY + "/" + name for name in ("mutations.json", "native.rs", "naive.clsp"))
    paths.update("puzzles/precommit-" + name + "/reference.clsp" for name in ("claim", "registry"))
    paths.update(("crates/chialisp-oxide-vm/failure-protocol.json", "crates/chialisp-oxide-simulator/rejection-protocol.json"))
    require(len(paths) <= 4096, "source count bound")
    rows, total = [], 0
    for path in sorted(paths):
        payload = read(cargo_authoring.contained_file(ROOT, path), min(16 * 1024 * 1024, 64 * 1024 * 1024 - total))
        total += len(payload)
        require(total <= 64 * 1024 * 1024, "aggregate source byte bound")
        rows.append(dict(path=path, sha256=digest(payload)))
    return dict(authoring=authoring, sources=rows)


def options(package):
    return ["--manifest-path", str(package / "Cargo.toml"), "--package", PACKAGE, "--locked", "--offline"]


def native_rows(text):
    rows = [decode_json(row.encode())
            for row in re.findall(r"^PRECOMMIT_CONTROL_NATIVE (\{[^\n]+\})$", text, re.M)]
    expected = [dict(name=row["name"], **row["expected"]) for row in witnesses()]
    require(rows == expected, "four exact independent native output observations")
    results = re.findall(r"^test result: (\w+)\. (\d+) passed; (\d+) failed", text, re.M)
    require(results and all(status == "ok" and failed == "0" for status, _, failed in results)
            and sum(int(passed) for _, passed, _ in results) == 1, "one successful native test group")
    return rows


def strict_abi(envelope, name):
    closed(envelope, ("ir_version", "environment", "program"), "control envelope")
    require(type(envelope["ir_version"]) is int and envelope["ir_version"] == (6 if name.startswith("claim_") else 4)
            and envelope["environment"] == "strict", "strict versioned source control")
    body = envelope["program"]
    require(body["name"] == PACKAGE.replace("-", "_") + "::" + name + "::puzzle", "exact IR entry identity")
    byte = lambda n: dict(kind="bytes", len=n)
    tup = lambda *items: dict(kind="tuple", items=list(items))
    u64 = dict(kind="int", bits=64, signed=False)
    conditions = (tup(u64, byte(32)), tup(u64, u64), tup(u64, byte(32)), tup(u64, byte(32)),
                  tup(u64, byte(48), byte(32)), tup(u64, byte(32), u64))
    if name.startswith("claim_"):
        parameters = [tup(byte(32), byte(32), byte(32)), dict(kind="bool"), byte(32)]
        result = dict(kind="optional", item=tup(u64, byte(32)))
        for item in reversed((conditions[0], conditions[1], tup(u64, dict(kind="int", bits=32, signed=False)), conditions[5], conditions[3])):
            result = dict(kind="pair", first=item, rest=result)
    else:
        parameters = [byte(32)] * 4 + [tup(byte(48), byte(32), byte(32))]
        result = tup(*(conditions if name == "registry_original" else (*conditions[:4], conditions[5])))
    require([row["ty"] for row in body["parameters"]] == parameters and body["result"] == result,
            "exact logical control ABI")


def ledger_expected(index):
    return [] if index % 2 else [dict(stage="consensus", code="AssertHeightRelativeFailed" if index == 0 else "BadAggregateSignature")]


def reference_hashes():
    """Pin only unchanged independent Chialisp source/artifact pairs here.

    This does not admit the surrounding report as current Rust execution.
    The ledger runner separately compiles the sources and must match these
    exact retained tree identities on every actual source-control attempt.
    """
    report = load(ROOT / "corpus/validation.json")
    result = {}
    for name in ("claim", "registry"):
        rows = [row for row in report["puzzles"] if row["slug"] == "precommit-" + name]
        require(len(rows) == 1 and rows[0]["reference"] == read(ROOT / f"puzzles/precommit-{name}/reference.clsp").decode(), "unchanged reference source")
        artifacts = [row for row in rows[0]["artifacts"] if row["backend"] == "chialisp" and row["optimize_lir"] is True]
        require(len(artifacts) == 1, "unique retained reference artifact")
        artifact = artifacts[0]
        require(raw_tree.tree_hash(raw_tree.deserialize(bytes.fromhex(artifact["clvm_hex"]))).hex() == artifact["tree_hash"], "reference artifact tree identity")
        result[name] = artifact["tree_hash"]
    return result


def source_receipt_matches(files, directory):
    by_label = {row.label: row for row in files}
    for path in sorted(package_files()):
        if path.endswith(".rs") and not path.startswith("tests/"):
            label = "package/" + path
            require(label in by_label and "compiler_text" in by_label[label].kinds
                    and by_label[label].compiler_read_count > 0
                    and by_label[label].contents == read(directory / label), "compiler-read source differs from exact recipe: " + path)
    for path, kind in (("Cargo.toml", "launcher_manifest"), ("Cargo.lock", "launcher_lockfile_post_cargo")):
        label = "package/" + path
        require(label in by_label and kind in by_label[label].kinds
                and by_label[label].contents == read(directory / label), "launcher snapshot differs from retained prepared input")


def check_receipts(directory, context):
    native = load(directory / "native-receipt.json")
    closed(native, ("schema_version", "protocol", "status", "source_root", "package_root", "toolchain", "source_window", "prepared", "commands"), "native receipt")
    require(type(native["schema_version"]) is int and native["schema_version"] == 1
            and native["protocol"] == "chialisp-oxide-precommit-controls-native/v1"
            and native["status"] == "native_phase_passed_before_LAST_host_build"
            and all(native[key] == context[key] for key in ("source_root", "package_root", "toolchain", "source_window")), "native source/host linkage")
    prepared = native["prepared"]
    closed(prepared, ("mode", "package", "manifest", "library", "exports", "sources"), "prepared inputs")
    files = sorted([*package_files(context["source_root"]), "Cargo.lock"])
    require(prepared == dict(mode="cargo_library", package=PACKAGE, manifest="Cargo.toml", library="src/lib.rs",
        exports={name: dict(entry=PACKAGE.replace("-", "_") + "::" + name + "::puzzle", source="src/" + name + ".rs") for name in NAMES},
        sources=[dict(path=path, sha256=sha(directory / "package" / path)) for path in files]), "native exact prepared inputs")
    phases = (("generate-lockfile", ["--offline"]), ("check", ["--all-targets", "--locked", "--offline"]),
              ("test", ["--locked", "--offline", "--", "--nocapture"]),
              ("clippy", ["--all-targets", "--locked", "--offline", "--", "-D", "warnings"]))
    require(type(native["commands"]) is list and len(native["commands"]) == 4, "four native Cargo phases")
    for index, (row, (operation, tail)) in enumerate(zip(native["commands"], phases)):
        closed(row, ("argv", "exit_code", "log", "log_sha256"), "native command")
        require(row["argv"] == ["rustup", "run", context["toolchain"], "cargo", operation, "--manifest-path", context["package_root"] + "/Cargo.toml", *tail]
                and type(row["exit_code"]) is int and row["exit_code"] == 0
                and row["log"] == f"native-logs/{index}.log"
                and row["log_sha256"] == sha(directory / row["log"]), "native command/log identity")
    require(read(directory / "native.log") == read(directory / "native-logs/2.log"), "native observation/test log relation")
    quiet = load(directory / "last-host-window.json")
    closed(quiet, ("schema_version", "protocol", "status", "source_root", "package_root", "compiler", "toolchain", "source_window", "tools", "native_receipt_sha256", "scope", "ledger_runner"), "quiet receipt")
    require(type(quiet["schema_version"]) is int and quiet["schema_version"] == 1
            and quiet["protocol"] == "chialisp-oxide-precommit-controls-quiet-window/v1"
            and quiet["status"] == "LAST_host_build_terminal_tools_quiet"
            and all(quiet[key] == context[key] for key in context)
            and quiet["native_receipt_sha256"] == sha(directory / "native-receipt.json")
            and type(quiet["scope"]) is str and quiet["scope"], "native/LAST-host/capture linkage")


def recorded_bundle(directory, context, optimize):
    """Read every retained export/blob with the shared bounded schema-2 reader.

    Original absolute paths are checked as declarations. They are not opened
    on this host and provide no source authenticity or execution credit.
    """
    reader = cargo_bundle._Reader()
    manifest_path = cargo_bundle._contained(directory, "manifest.json")
    manifest, _ = reader.json(manifest_path)
    require(type(manifest.get("schema_version")) is int and manifest["schema_version"] == 2
            and manifest.get("status") == "wip_prototype_not_ready_for_use", "complete schema-2 bundle required")
    require(manifest.get("package") == PACKAGE and manifest.get("compiler") == context["compiler"]
            and manifest.get("driver_sha256") == context["tools"]["chialisp-oxide-driver"]["sha256"]
            and manifest.get("toolchain") == context["toolchain"]
            and manifest.get("workspace_root") == context["package_root"]
            and manifest.get("manifest_path") == context["package_root"] + "/Cargo.toml"
            and manifest.get("cargo_options") == options(Path(context["package_root"]))
            and manifest.get("rue_revision") == "245c107a8e795229ea08097142f005a4c0d91075", "retained bundle context")
    host = [line[6:] for line in context["compiler"].splitlines() if line.startswith("host: ")]
    require(len(host) == 1 and manifest.get("analysis_target") == host[0], "compiler host/analysis target")
    entries = manifest.get("entries")
    names = {PACKAGE.replace("-", "_") + "::" + name + "::puzzle": name for name in NAMES}
    require(type(entries) is list and len(entries) == 4 and all(type(entry) is dict for entry in entries)
            and {entry.get("entry") for entry in entries} == set(names),
            "complete four-entry bundle")
    declarations = cargo_bundle._preflight_ir_declarations(manifest, reader, optimize)
    paths, details, result = set(), {}, {}
    for entry in entries:
        name = names[entry["entry"]]
        program, path, pin, declaration = cargo_bundle._artifact_details(
            entry, directory, reader, paths, optimize, 2, ir_declaration=declarations[entry["entry"]])
        strict_abi(program, name)
        details[entry["entry"]] = (entry["ir"], declaration, pin)
        result[name] = entry
    source_files_read = cargo_bundle.validate_receipt(manifest, directory, details, reader, paths)
    source_receipt_matches(source_files_read, directory.parent.parent)
    return result


def validate(report, directory, *, current=True):
    closed(report, ("schema_version", "protocol", "status", "scope", "context", "native", "bundles", "runs", "ledger", "commands", "files"), "source packet")
    require(type(report["schema_version"]) is int and report["schema_version"] == 1
            and report["protocol"] == PROTOCOL and report["status"] == "actual_source_controls_captured"
            and report["scope"] == SCOPE, "source packet protocol/scope")
    context = report["context"]
    closed(context, ("source_root", "package_root", "compiler", "toolchain", "tools", "ledger_runner", "source_window"), "capture context")
    for key in ("source_root", "package_root"):
        value = context[key]
        require(type(value) is str and len(value.encode()) <= 4096 and value.startswith("/") and not value.startswith("//")
                and Path(value).as_posix() == value and ".." not in Path(value).parts, "normalized source/package declaration")
    require(context["toolchain"] == tomllib.loads(read(ROOT / "rust-toolchain.toml").decode())["toolchain"]["channel"]
            and type(context["compiler"]) is str and len(context["compiler"]) <= 65536
            and context["compiler"].startswith("rustc "), "pinned compiler declaration")
    closed(context["tools"], TOOLS, "tools")
    for name, row in context["tools"].items():
        closed(row, ("path", "sha256"), "tool")
        require(row["path"] == context["source_root"] + "/target/debug/" + name
                and type(row["sha256"]) is str and re.fullmatch("[0-9a-f]{64}", row["sha256"]), "tool path/digest declaration")
    closed(context["ledger_runner"], ("path", "sha256"), "ledger runner")
    require(context["ledger_runner"]["path"] == context["source_root"] + "/target/debug/examples/precommit_controls"
            and type(context["ledger_runner"]["sha256"]) is str
            and re.fullmatch("[0-9a-f]{64}", context["ledger_runner"]["sha256"]), "ledger runner identity")
    if current:
        require(context["source_window"] == source_window(), "stale source/compiler/reader inputs")
    files = report["files"]
    require(type(files) is list and 1 <= len(files) <= 256, "retained inventory bound")
    paths, total = [], 0
    for row in files:
        closed(row, ("path", "sha256"), "file identity")
        path = cargo_bundle._contained(directory, row["path"])
        payload = read(path, min(16 * 1024 * 1024, 64 * 1024 * 1024 - total), allow_empty=True)
        total += len(payload)
        require(digest(payload) == row["sha256"], "retained bytes changed: " + row["path"])
        paths.append(row["path"])
    require(paths == sorted(set(paths)), "sorted unique inventory")
    actual = source_files(directory.parent, directory.name, suffixes=None,
                          limits=WalkLimits(depth=5, directories=16, entries=1024, sources=512))
    actual = [Path(path).relative_to(directory.name).as_posix() for path in actual]
    require(sorted(path for path in actual if path != "report.json") == paths, "closed retained file inventory")
    for path, payload in package_files(context["source_root"]).items():
        require(read(cargo_bundle._contained(directory, "package/" + path)) == payload, "retained package recipe mismatch")
    check_receipts(directory, context)
    native = report["native"]
    closed(native, ("test_groups", "observations", "log", "log_sha256"), "native observations")
    require(type(native["test_groups"]) is int and native["test_groups"] == 1
            and native["log"] == "native.log" and sha(directory / native["log"]) == native["log_sha256"]
            and native["observations"] == native_rows(read(directory / native["log"]).decode()), "native output/log binding")
    closed(report["bundles"], ("off", "on"), "paired bundles")
    entries = {}
    for mode in ("off", "on"):
        bundle = report["bundles"][mode]
        closed(bundle, ("manifest", "manifest_sha256", "recompiled"), "bundle declaration")
        require(bundle["manifest"] == f"bundles/{mode}/manifest.json"
                and sha(directory / bundle["manifest"]) == bundle["manifest_sha256"], "manifest identity")
        entries[mode] = recorded_bundle(directory / f"bundles/{mode}", context, mode == "on")
        require(type(bundle["recompiled"]) is list and [row.get("name") for row in bundle["recompiled"]] == list(NAMES), "all entry recompilations")
        for row in bundle["recompiled"]:
            closed(row, ("name", "artifacts"), "recompilation")
            require(type(row["artifacts"]) is list and len(row["artifacts"]) == 2
                    and all(type(item.get("optimize")) is bool for item in row["artifacts"])
                    and [item.get("optimize") for item in row["artifacts"]] == [False, True], "both IR replay modes")
            selected = row["artifacts"][int(mode == "on")]
            require(all(selected[key] == entries[mode][row["name"]]["artifact"][key]
                        for key in ("clvm_hex", "tree_hash", "bytes")), "retained IR recompilation artifact mismatch")
    require(type(report["runs"]) is list and len(report["runs"]) == 8, "eight primary VM observations")
    expected_runs = [(mode, row) for mode in ("off", "on") for row in witnesses()]
    for run, (mode, spec) in zip(report["runs"], expected_runs):
        closed(run, ("mode", "name", "solution", "module_tree_hash", "execution"), "VM observation")
        require(run["mode"] == mode and run["name"] == spec["name"] and run["solution"] == spec["solution"]
                and run["module_tree_hash"] == entries[mode][run["name"]]["artifact"]["tree_hash"]
                and check_execution(run["execution"]) == spec["expected"], "independent VM source witness")
    ledger = report["ledger"]
    closed(ledger, ("protocol", "sdk", "consensus", "vm", "network", "transaction_attempts", "runs"), "SDK ledger")
    require(ledger["protocol"] == "chialisp-oxide-precommit-source-controls-ledger/v1"
            and ledger["sdk"] == "chia-sdk-test 0.36.0" and ledger["consensus"] == "chia-consensus 0.36.1"
            and ledger["vm"] == "clvmr 0.16.2" and ledger["network"] == "TESTNET11"
            and type(ledger["transaction_attempts"]) is int and ledger["transaction_attempts"] == 8
            and type(ledger["runs"]) is list and len(ledger["runs"]) == 8, "eight separate pinned SDK attempts")
    references = reference_hashes()
    for index, (row, (mode, spec)) in enumerate(zip(ledger["runs"], expected_runs)):
        closed(row, ("mode", "name", "control", "module_tree_hash", "reference_tree_hash", "scenarios"), "ledger observation")
        control = CONTROLS[index % 4]
        require(row["mode"] == mode and row["name"] == spec["name"] and row["control"] == control
                and row["module_tree_hash"] == entries[mode][row["name"]]["artifact"]["tree_hash"]
                and row["reference_tree_hash"] == references["registry" if index % 4 < 2 else "claim"], "ledger module/control identity")
        require(type(row["scenarios"]) is list and len(row["scenarios"]) == 1
                and row["scenarios"][0].get("passed") is True, "typed successful scenario")
        require(row["scenarios"] == [dict(name=f"source control {control}: one complete-spend submission",
                                         passed=True, scope="ledger", rejections=ledger_expected(index % 4))], "ledger distinguishing witness/outcome")
    require(ledger["runs"][0]["reference_tree_hash"] == ledger["runs"][1]["reference_tree_hash"]
            == ledger["runs"][4]["reference_tree_hash"] == ledger["runs"][5]["reference_tree_hash"]
            and ledger["runs"][2]["reference_tree_hash"] == ledger["runs"][3]["reference_tree_hash"]
            == ledger["runs"][6]["reference_tree_hash"] == ledger["runs"][7]["reference_tree_hash"], "independent counterpart retained across modes")
    commands = report["commands"]
    require(type(commands) is list and len(commands) == 19, "two exports + eight recompiles + eight VM commands + SDK command")
    expected_roles = [role for mode in ("off", "on") for role in
                      ["export-" + mode, *[role for name in NAMES for role in (f"recompile-{mode}-{name}", f"vm-{mode}-{name}")]]] + ["sdk-ledger"]
    require([row.get("role") for row in commands] == expected_roles, "complete ordered capture commands")
    capture_paths, export_paths = [], []
    for index, command in enumerate(commands):
        closed(command, ("role", "argv", "exit_code", "stdout", "stderr"), "command")
        require(type(command["exit_code"]) is int and command["exit_code"] == 0
                and type(command["argv"]) is list and len(command["argv"]) <= 32
                and all(type(arg) is str and len(arg.encode()) <= 4096 for arg in command["argv"])
                and command["stdout"] == f"logs/{index:02}.stdout" and command["stderr"] == f"logs/{index:02}.stderr"
                and command["stdout"] in paths and command["stderr"] in paths, "successful retained command/logs")
        role, argv = command["role"], command["argv"]
        if role.startswith("export-"):
            mode = role.removeprefix("export-")
            prefix = [context["tools"]["cargo-chialisp-oxide"]["path"], "build", *options(Path(context["package_root"])), "--output-dir"]
            require(argv[:len(prefix)] == prefix and len(argv) == len(prefix) + (2 if mode == "off" else 1)
                    and argv[len(prefix)].endswith(f"/fresh-{mode}")
                    and (mode == "on" or argv[-1] == "--no-optimize"), "exact source-aware extraction command")
            export_paths.append((mode, argv[len(prefix)]))
        elif role.startswith("recompile-"):
            _, mode, name = role.split("-", 2)
            entry = entries[mode][name]
            suffix = f"/bundles/{mode}/{entry['ir']}"
            require(len(argv) == 3 and argv[0] == context["tools"]["chialisp-oxide-verifier"]["path"]
                    and argv[1].endswith(suffix) and argv[2] == "--artifacts", "exact IR recompilation command")
            capture_paths.append(argv[1][:-len(suffix)])
            artifacts = decode_json(read(directory / command["stdout"]))
            require(artifacts == report["bundles"][mode]["recompiled"][NAMES.index(name)]["artifacts"], "recompile command output binding")
        elif role.startswith("vm-"):
            _, mode, name = role.split("-", 2)
            entry = entries[mode][name]
            suffix = f"/bundles/{mode}/{entry['clvm']}"
            require(len(argv) == 7 and argv[:2] == [context["tools"]["chialisp-oxide"]["path"], "run-batch"]
                    and argv[2].endswith(suffix) and argv[3] == "--solutions"
                    and argv[4] == argv[2][:-len(suffix)] + f"/inputs/{mode}-{name}.json"
                    and argv[5:] == ["--max-cost", "10000000"], "exact VM replay command")
            capture_paths.append(argv[2][:-len(suffix)])
            observation = next(row for row in report["runs"] if row["mode"] == mode and row["name"] == name)
            require(decode_json(read(directory / command["stdout"])) == [observation["execution"]]
                    and load(directory / f"inputs/{mode}-{name}.json") == [observation["solution"]], "VM command/output/input binding")
        else:
            require(len(argv) == 2 and argv[0] == context["ledger_runner"]["path"]
                    and argv[1].endswith("/ledger-request.json")
                    and decode_json(read(directory / command["stdout"])) == ledger
                    and load(directory / "ledger.json") == ledger, "SDK command/output binding")
            capture_paths.append(argv[1][:-len("/ledger-request.json")])
    require(len(set(capture_paths)) == 1 and capture_paths[0].startswith("/")
            and str(Path(capture_paths[0])) == capture_paths[0]
            and ".." not in Path(capture_paths[0]).parts, "one normalized capture root")
    require(all(path == capture_paths[0] + f"-work/fresh-{mode}" for mode, path in export_paths),
            "extraction work is adjacent to the retained capture")
    request = load(directory / "ledger-request.json")
    require(request == dict(protocol=ledger["protocol"], runs=[dict(mode=mode, name=row["name"],
            module_hex=entries[mode][row["name"]]["artifact"]["clvm_hex"]) for mode, row in expected_runs]), "SDK request uses every exact candidate artifact")
    return dict(status="validated_recorded_source_controls", native_test_groups=1, native_output_observations=4,
                primary_vm_observations=8, sdk_transaction_attempts=8, scope=SCOPE)
