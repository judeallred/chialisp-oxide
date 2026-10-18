"""Explicit actual-source bridge; typed post-ABI models and finite executions only."""
import copy
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import tomllib
import tempfile

from cargo_authoring import contained_file, require, snapshot, unique_object
from cargo_bundle import load_declared_export
from vm_failures import check_execution, check_replay

ROOT = Path(__file__).resolve().parents[1]
EXPERIMENT = "experiments/acyclic-scalar"
PACKAGE = "chialisp-oxide-acyclic-scalar-source"
CRATE = PACKAGE.replace("-", "_")
MODULES = ("base", "guard_mutant", "output_mutant", "strict_mutant")
POLICIES = ("guarded", "unused_argument", "unused_initializer", "copied_result")
PRIVATE = ("Cargo.toml", "Cargo.lock", "bridge.json", "specifications.json", "src/lib.rs", "src/main.rs",
           *(f"src/{module}.rs" for module in MODULES))
TOOLS = ("cargo-chialisp-oxide", "chialisp-oxide-driver", "chialisp-oxide-verifier", "chialisp-oxide")
OPTIONS = ["--manifest-path", f"{EXPERIMENT}/Cargo.toml", "--package", PACKAGE, "--locked", "--offline"]
RUE = "245c107a8e795229ea08097142f005a4c0d91075"
PROTOCOL = "chialisp-oxide-acyclic-source/v1"
BUILD_ENV = {"RUSTFLAGS": "-C overflow-checks=yes", "CARGO_NET_OFFLINE": "true", "CARGO_BUILD_JOBS": "2"}
MAX_FILE = 16 * 1024 * 1024
MAX_OUTPUT = 16 * 1024 * 1024
MODEL_SCOPE = "typed scalar/tuple acceptance and output after ABI; no failure-order, cost/resource, or compiler-preservation theorem"
# Reviewed hand-authored contracts, not specifications inferred from emitted IR.
PINS = {'Cargo.lock': '0c07f29dcdeeca108cc883e3a5ef11c3d107eadecdd7692424e74e1c519a8882', 'Cargo.toml': '108facfb3a0bd53b8b0c80d9f640a31321d6e26ad66b10f01fac6aa7803fa5b0', 'bridge.json': '4eaaa2e4104474e2be0987898fc1437738ead303e7b5d81f6f27a60341dcc49d', 'specifications.json': '50841a81815190a33dc07641a41917ed256a491b4695c2440073978e6afd1bc4', 'src/base.rs': '306ea1a073fcdc6b055d2c4b4cea113e73917cb71e34600323081b645babb278', 'src/guard_mutant.rs': '2491ccd56aa8f50df6440eab30e18b82e811f759f2148de9d66fd375a530be4f', 'src/lib.rs': '242ff4d7aa30c1813a813a857b0db7ad0f80c678e303b1f0826fbf145d1f6634', 'src/main.rs': '981bef1bfa6db9fd0eeb5fad3bed30fa3d5a5201a9a92917e495b3df62a34972', 'src/output_mutant.rs': '64387374a7a8f876fa503abaa23d439b06e14743bd5d84a190251233d454e937', 'src/strict_mutant.rs': '7042fc1f3dd8b7b7e64afa8b52470d488aad45cd028226f6cfb8405b185c22ee'}  # Reviewed formatted experiment bytes.
SCOPE = ("Actual declared Cargo sources and same-package native finite executions, plus an explicit bounded "
         "acyclic scalar SMT model of typed u8 inputs after ABI. No complete environment/ABI, failure-order, "
         "resource/cost, cryptographic security, rustc/IR/CLVM preservation or production-readiness theorem. "
         "Source/tool hashes check declared freshness and consistency, not authenticated execution or installed bytes.")
ASSUMPTIONS = ("Independent hand-authored specifications have no source-body success precondition. Installed pinned compiler/standard library/registry crates, model translator and installed solver are trusted; source/tool identities do not authenticate them. Typed u8 domain is not a whole-environment ABI proof. Actual malformed ABI checks and policy mutation/native/VM checks are finite.")


def digest(payload):
    return hashlib.sha256(payload).hexdigest()


def file_sha(path):
    # Tool/native binaries may exceed the source-file limit; hash incrementally.
    total, value = 0, hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(65536):
            total += len(chunk)
            require(total <= 1024 * 1024 * 1024, "Binary identity read budget exceeded")
            value.update(chunk)
    require(total > 0, "Empty binary identity")
    return value.hexdigest()


def read(root, relative):
    path = contained_file(root, relative)
    with path.open("rb") as stream:
        payload = stream.read(MAX_FILE + 1)
    require(len(payload) <= MAX_FILE, "Bridge file read budget exceeded: " + relative)
    return payload


def load(root, relative):
    return json.loads(read(root, relative), object_pairs_hook=unique_object)


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def fingerprints(root, paths):
    paths = sorted(set(paths))
    require(0 < len(paths) <= 4096, "Bridge source count budget exceeded")
    result, size = [], 0
    for path in paths:
        payload = read(root, path)
        size += len(payload)
        require(size <= 64 * 1024 * 1024, "Bridge aggregate source byte budget exceeded")
        result.append({"path": path, "sha256": digest(payload)})
    return result


def private_snapshot(root):
    base = root / EXPERIMENT
    return {"mode": "cargo_library", "package": PACKAGE, "manifest": "Cargo.toml", "library": "src/lib.rs",
            "exports": {f"{module}-{name}": {"entry": f"{CRATE}::{module}::{name}", "source": f"src/{module}.rs"}
                        for module in MODULES for name in POLICIES}, "sources": fingerprints(base, PRIVATE)}


def source_snapshot(root):
    # Main production/SDK/compiler/native/VM/model and local Rue source closure.
    # Registry/std/installed tool bytes and mutable environment remain assumptions.
    paths = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml",
             "scripts/check-acyclic-source.py", "scripts/acyclic_source_bridge.py", "scripts/test_acyclic_source.py",
             "scripts/cargo_authoring.py", "scripts/cargo_bundle.py", "scripts/vm_failures.py",
             "crates/chialisp-oxide-vm/failure-protocol.json"]
    for directory in ("crates", "experiments/backend", "vendor"):
        for path in (root / directory).rglob("*"):
            if path.is_file() and (path.suffix == ".rs" or path.name in ("Cargo.toml", "Cargo.lock")):
                paths.append(path.relative_to(root).as_posix())
    return {"production": fingerprints(root, paths), "main_authoring": snapshot(root), "experiment": private_snapshot(root)}


def reviewed_contract(root):
    base = root / EXPERIMENT
    for path, pin in PINS.items():
        require(digest(read(base, path)) == pin, "Reviewed bridge contract changed: " + path)
    value, specs = load(base, "bridge.json"), load(base, "specifications.json")
    require(value["protocol"] == PROTOCOL and value["schema_version"] == 1
            and value["package"] == PACKAGE and value["modules"] == list(MODULES)
            and value["policies"] == list(POLICIES), "Changed bridge identities")
    baseline = read(base, "src/base.rs").decode()
    for recipe in value["mutations"]:
        source = baseline
        for change in recipe["substitutions"]:
            require(set(change) == {"old", "new"} and change["old"] != change["new"]
                    and source.count(change["old"]) == 1, "Mutation must be an exact unique source substitution")
            source = source.replace(change["old"], change["new"], 1)
        require(source == read(base, f"src/{recipe['module']}.rs").decode(), "Actual Rust mutation differs from recipe")
    manifest = tomllib.loads(read(base, "Cargo.toml").decode())
    require(manifest["workspace"] == {} and manifest["package"]["publish"] is False
            and manifest["dependencies"] == {"chialisp-oxide": {"path": "../../crates/chialisp-oxide"}}
            and manifest["profile"]["dev"]["overflow-checks"] is True
            and manifest["profile"]["test"]["overflow-checks"] is True, "Changed private Cargo/overflow contract")
    return value, specs


def expected(module, policy, value):
    """Independently specified finite integer policy; not translated Rust/MIR."""
    require(module in MODULES and policy in POLICIES and type(value) is int and 0 <= value <= 255,
            "Unknown finite policy/input")
    if module == "strict_mutant" and policy in ("unused_argument", "unused_initializer"):
        return {"input": value, "status": "accepted", "value": 7}
    limit = 120 if module == "guard_mutant" else 100
    if value >= limit:
        return {"input": value, "status": "rejected"}
    answer = value + (2 if module == "output_mutant" else 1)
    if policy == "copied_result":
        answer *= 2
    elif policy in ("unused_argument", "unused_initializer"):
        answer = 7
    require(0 <= answer <= 255, "Independent finite oracle exceeded u8")
    return {"input": value, "status": "accepted", "value": answer}


def checked_native(rows, module, policy):
    require(type(rows) is list and len(rows) == 256, "Native batch must contain exactly256 typed inputs")
    for value, row in enumerate(rows):
        require(type(row) is dict and type(row.get("input")) is int and row["input"] == value,
                "Native batch missing/reordered/Boolean input")
        require(row == expected(module, policy, value) and ("value" not in row or type(row["value"]) is int),
                "Native outcome differs from independent finite policy")
    return rows


def encoded(value):
    require(type(value) is int and 0 <= value <= 255, "Only finite u8 scalar outputs are encoded")
    return "80" if value == 0 else f"{value:02x}" if value < 128 else f"8200{value:02x}"


def checked_vm(rows, values, module, policy, *, malformed=False, optimize=None):
    require(type(rows) is list and len(rows) == len(values), "VM batch must have exactly one object per input")
    for value, observed in zip(values, rows, strict=True):
        outcome = check_execution(observed)
        if malformed:
            require(outcome == {"status": "rejected"}, "Malformed ABI input unexpectedly accepted")
            # Existing OFF extra-arity failures can be InvalidOpArg; ON is Raise.
            wanted = "invalid_op_arg" if value == "(1 2)" and optimize is False else "raise"
            require(observed["failure"] == {"kind": "program", "code": wanted}, "Wrong exact ABI failure")
        else:
            wanted = expected(module, policy, value)
            if wanted["status"] == "accepted":
                require(outcome == {"status": "accepted", "output_hex": encoded(wanted["value"])}, "Wrong retained VM output")
            else:
                require(outcome == {"status": "rejected"} and observed["failure"] == {"kind": "program", "code": "raise"},
                        "Expected exact policy Raise, never resource/infrastructure credit")
    return rows


def checked_verdict(row, wanted):
    require(type(row) is dict and set(row) == {"model", "scope", "verification"}
            and row["model"] == "acyclic_scalar_calls/v1" and row["scope"] == MODEL_SCOPE, "Wrong explicit model result envelope/scope")
    value = row["verification"]
    require(type(value) is dict and value.get("status") == wanted and value.get("nonvacuity") is not None,
            "Wrong model verdict or vacuous model result")
    require(set(value) == ({"status", "nonvacuity"} if wanted == "proven" else {"status", "nonvacuity", "counterexample"}),
            "Unknown/missing verification fields")
    witness_value(value["nonvacuity"])
    if wanted == "refuted":
        witness_value(value["counterexample"])
    return value


def witness_value(witness):
    require(type(witness) is dict and set(witness) == {"arguments"} and type(witness["arguments"]) is list
            and len(witness["arguments"]) == 1, "Witness must contain exactly one typed u8 argument")
    row = witness["arguments"][0]
    require(type(row) is dict and set(row) == {"kind", "value"} and row["kind"] == "int"
            and isinstance(row["value"], str) and row["value"].isascii() and row["value"].isdecimal(), "Malformed u8 witness")
    value = int(row["value"])
    require(0 <= value <= 255 and str(value) == row["value"], "Witness outside canonical typed u8 domain")
    return value


def ground_nonvacuity(verification, exported):
    """Bind the model's success witness to finite actual source executions."""
    n = witness_value(verification["nonvacuity"])
    native = exported["native"][n]
    require(type(native.get("input")) is int and native["input"] == n
            and native["status"] == "accepted" and type(native.get("value")) is int,
            "Nonvacuity witness must be accepted in actual native source")
    builds = exported["builds"]
    require(len(builds) == 2 and [row["optimize"] for row in builds] == [False, True]
            and all(type(row["optimize"]) is bool for row in builds), "Nonvacuity needs retained OFF/ON source runs")
    for build in builds:
        require(check_execution(build["canonical"][n]) == {"status": "accepted", "output_hex": encoded(native["value"])},
                "Nonvacuity witness must be accepted with the same output in retained OFF/ON CLVM")
    return n


def run(command, *, root, env, expected_code=0, timeout=120):
    # File-backed capture bounds allocations before reading tool output. This is
    # not a sandbox for the trusted installed tools or a disk-work theorem.
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        process = subprocess.run([str(arg) for arg in command], cwd=root, env=env,
                                 stdout=stdout, stderr=stderr, timeout=timeout)
        stdout.seek(0)
        stderr.seek(0)
        out, err = stdout.read(MAX_OUTPUT + 1), stderr.read(MAX_OUTPUT + 1)
    require(len(out) <= MAX_OUTPUT and len(err) <= MAX_OUTPUT, "Subprocess output exceeds retained bound")
    result = subprocess.CompletedProcess(command, process.returncode, out.decode(), err.decode())
    require(result.returncode == expected_code, f"Command failed {command}: {result.returncode}\n{result.stderr[-4000:]}")
    return result


def compiler_host(compiler):
    require(isinstance(compiler, str) and compiler.startswith("rustc "), "Expected full independently queried rustc identity")
    hosts = [line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: ")]
    require(len(hosts) == 1 and re.fullmatch(r"[A-Za-z0-9_.-]+", hosts[0]), "Compiler identity requires one canonical host")
    return hosts[0]


def stored_absolute(raw):
    # Historical path coherence, not existence/current checkout equality.
    require(isinstance(raw, str) and Path(raw).is_absolute() and str(Path(raw)) == raw
            and ".." not in Path(raw).parts, "Stored source path must be normalized and absolute")
    return Path(raw)


def checked_manifest(manifest, identity):
    require(type(manifest) is dict and set(manifest) == {"analysis_target", "cargo_options", "compiler", "driver_sha256", "entries",
            "manifest_path", "package", "package_id", "rue_revision", "schema_version", "status", "toolchain", "workspace_root"},
            "Unknown/missing retained Cargo manifest fields")
    source_root = stored_absolute(identity["source_root"])
    workspace = source_root / EXPERIMENT
    driver = next(row["sha256"] for row in identity["tools"] if row["name"] == "chialisp-oxide-driver")
    require(type(manifest["schema_version"]) is int and manifest["schema_version"] == 1
            and manifest["status"] == "wip_prototype_not_ready_for_use", "Wrong retained Cargo bundle protocol")
    require(manifest["analysis_target"] == compiler_host(identity["compiler"])
            and stored_absolute(manifest["workspace_root"]) == workspace
            and stored_absolute(manifest["manifest_path"]) == workspace / "Cargo.toml",
            "Retained Cargo target/workspace/manifest differs from independently captured host/source root")
    require(manifest["compiler"] == identity["compiler"] and manifest["driver_sha256"] == driver
            and manifest["toolchain"] == identity["toolchain"] and manifest["cargo_options"] == OPTIONS
            and manifest["rue_revision"] == RUE and manifest["package"] == PACKAGE
            and isinstance(manifest["package_id"], str) and bool(manifest["package_id"]),
            "Retained manifest differs from independently captured extraction identities")
    return manifest


def identities(root, tools, z3, env):
    toolchain = tomllib.loads(read(root, "rust-toolchain.toml").decode())["toolchain"]["channel"]
    compiler = run(["rustup", "run", toolchain, "rustc", "-vV"], root=root, env=env).stdout
    require(run([tools / "chialisp-oxide-driver", "--compiler-version"], root=root, env=env).stdout.strip() == compiler.splitlines()[0],
            "Driver compiler version line differs from independently queried installed compiler")
    compiler_host(compiler)
    return {"source_root": str(root.resolve()), "toolchain": toolchain, "compiler": compiler,
            "tools": [{"name": name, "path": str((tools / name).resolve()), "sha256": file_sha(tools / name)} for name in TOOLS],
            "solver": {"path": str(z3), "sha256": file_sha(z3),
                       "version": run([z3, "--version"], root=root, env=env).stdout.strip()}}


def check_export(output, *, root, authoring, identity, optimize, entry, before):
    def current():
        require(source_snapshot(root) == before, "Declared production/SDK/private source inputs changed during extraction")
        return private_snapshot(root)
    return load_declared_export(output, root=root / EXPERIMENT, authoring=authoring, selected_entry=entry,
        compiler=identity["compiler"], driver_sha256=next(row["sha256"] for row in identity["tools"] if row["name"] == "chialisp-oxide-driver"),
        toolchain=identity["toolchain"], current_authoring=current, optimize=optimize, rue_revision=RUE, cargo_options=OPTIONS)


def produce(root, output, tools, target, z3):
    contract, specifications = reviewed_contract(root)
    output.mkdir(parents=False, exist_ok=False)
    env = dict(os.environ)
    for key in ("RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CHIALISP_OXIDE_CARGO_MODE", "CARGO_ENCODED_RUSTFLAGS"):
        env.pop(key, None)
    env.update(RUSTFLAGS="-C overflow-checks=yes", CARGO_TARGET_DIR=str(target / "extract"), CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="2",
               CHIALISP_OXIDE_DRIVER=str(tools / "chialisp-oxide-driver"), PATH=str(z3.parent) + os.pathsep + env.get("PATH", ""))
    require(Path(shutil.which("z3", path=env["PATH"])).resolve() == z3, "Explicit solver must be the z3 resolved by verifier")
    before = source_snapshot(root)
    identity = identities(root, tools, z3, env)
    # Build the same ordinary Cargo package natively before recording its identity.
    native_target = target / "native"
    native_build = run(["rustup", "run", identity["toolchain"], "cargo", "build", "--manifest-path", f"{EXPERIMENT}/Cargo.toml",
                        "--package", PACKAGE, "--locked", "--offline", "--target-dir", native_target], root=root, env=env, timeout=180)
    (output / "native-build.log").write_text(native_build.stdout + native_build.stderr)
    native = native_target / "debug" / PACKAGE
    native_sha = file_sha(native)
    require(source_snapshot(root) == before and identities(root, tools, z3, env) == identity, "Source/tools changed during native build")
    loaded, bundles = {}, []
    for optimize in (False, True):
        fresh = output / ("fresh-on" if optimize else "fresh-off")
        args = [tools / "cargo-chialisp-oxide", "build", *OPTIONS, "--output-dir", fresh]
        if not optimize:
            args.append("--no-optimize")
        extracted = run(args, root=root, env=env, timeout=240)
        (output / f"extract-{optimize}.log").write_text(extracted.stdout + extracted.stderr)
        loaded[optimize] = {}
        for module in MODULES:
            for policy in POLICIES:
                key = f"{module}::{policy}"
                loaded[optimize][key] = check_export(fresh, root=root, authoring=before["experiment"], identity=identity,
                    optimize=optimize, entry=f"{CRATE}::{key}", before=before)
        first = next(iter(loaded[optimize].values()))
        bundles.append({"optimize": optimize, "manifest": first.manifest_path.relative_to(output).as_posix(),
                        "sha256": file_sha(first.manifest_path), "IR_sha256": {key: value.ir_sha256 for key, value in loaded[optimize].items()}})
    exports = []
    native_rows = {}
    vm_count = malformed_count = 0
    for module in MODULES:
        for policy in POLICIES:
            key = f"{module}::{policy}"
            off, on = (loaded[mode][key] for mode in (False, True))
            require(off.ir_sha256 == on.ir_sha256, "Exported source IR changed with optimization mode")
            program = on.program
            table = "ir_version" in program and program["ir_version"] == 4 and program["environment"] == "strict"
            if module == "base":
                require(table and 9 <= len(program["program"]["functions"]) <= 10, "Baseline must be an actual frontend table export")
            ir_path = output / "ir" / f"{module}-{policy}.json"
            ir_path.parent.mkdir(exist_ok=True)
            ir_path.write_bytes(on.path.read_bytes())
            native_result = json.loads(run([native, key], root=root, env=env).stdout)
            native_rows[key] = checked_native(native_result, module, policy)
            compiled = json.loads(run([tools / "chialisp-oxide-verifier", ir_path, "--artifacts"], root=root, env=env).stdout)
            require(type(compiled) is list and len(compiled) == 2
                    and [row.get("optimize") for row in compiled] == [False, True]
                    and all(type(row["optimize"]) is bool for row in compiled), "Expected exact OFF/ON model artifacts")
            builds = []
            for optimize, model_artifact in zip((False, True), compiled, strict=True):
                cargo = loaded[optimize][key]
                artifact = next(row["artifact"] for row in cargo.manifest["entries"] if row["entry"] == f"{CRATE}::{key}")
                require(all(model_artifact[name] == artifact[name] for name in ("clvm_hex", "tree_hash", "bytes")),
                        "Recompiled serialized model IR differs from retained Cargo CLVM")
                code_path = output / "artifacts" / f"{module}-{policy}-{optimize}.clvm.hex"
                code_path.parent.mkdir(exist_ok=True)
                code_path.write_text(artifact["clvm_hex"] + "\n")
                solutions = output / "solutions.json"
                values = list(range(256))
                write(solutions, [f"({n})" for n in values])
                observations = json.loads(run([tools / "chialisp-oxide", "run-batch", code_path, "--solutions", solutions,
                                              "--max-cost", "10000000"], root=root, env=env).stdout)
                checked_vm(observations, values, module, policy)
                vm_count += len(observations)
                malformed = ["()", "(1 2)", "(1 . 2)", "((1 . 2))", "(0x0001)", "(0x0000)", "(256)", "(-1)"]
                write(solutions, malformed)
                bad = json.loads(run([tools / "chialisp-oxide", "run-batch", code_path, "--solutions", solutions,
                                     "--max-cost", "10000000"], root=root, env=env).stdout)
                checked_vm(bad, malformed, module, policy, malformed=True, optimize=optimize)
                malformed_count += len(bad)
                builds.append({"optimize": optimize, "cargo_artifact": artifact, "modeled_artifact": model_artifact,
                               "retained_CLVM": {"path": code_path.relative_to(output).as_posix(), "sha256": digest(code_path.read_bytes())},
                               "canonical": observations, "malformed": bad})
            exports.append({"key": key, "kind": "actual_frontend_table" if table else "actual_legacy_source",
                            "IR": {"path": ir_path.relative_to(output).as_posix(), "sha256": digest(ir_path.read_bytes())},
                            "native": native_result, "builds": builds})
    obligations, defaults = [], []
    for row in contract["obligations"]:
        key = f"{row['module']}::{row['policy']}"
        exported = next(item for item in exports if item["key"] == key)
        ir = output / exported["IR"]["path"]
        stem = key.replace("::", "-") + "-" + row["kind"]
        spec_path, query_path = output / "models" / (stem + ".spec.json"), output / "models" / (stem + ".smt2")
        write(spec_path, specifications[row["policy"]][row["kind"]])
        if row["module"] == "base" and row["kind"] == "acceptance":
            failed = run([tools / "chialisp-oxide-verifier", ir, spec_path, "--emit"],
                         root=root, env=env, expected_code=1, timeout=30)
            require(failed.returncode != 0 and "does not model runtime" in failed.stderr, "Default scalar API admitted runtime IR")
            defaults.append({"key": key, "exit_code": failed.returncode, "stderr": failed.stderr})
        query_path.write_text(run([tools / "chialisp-oxide-verifier", ir, "--acyclic-scalar", spec_path, "--emit"], root=root, env=env).stdout)
        query_sha = digest(query_path.read_bytes())
        verdict = json.loads(run([tools / "chialisp-oxide-verifier", ir, "--acyclic-scalar", spec_path], root=root, env=env,
                                 expected_code=0 if row["expected"] == "proven" else 1).stdout)
        verification = checked_verdict(verdict, row["expected"])
        ground_nonvacuity(verification, exported)
        solved = run([z3, "-T:30", "-smt2", query_path], root=root, env=env, timeout=40)
        statuses = ["sat", "unsat" if row["expected"] == "proven" else "sat"]
        require(not solved.stderr and solved.stdout.split() == statuses and digest(query_path.read_bytes()) == query_sha,
                "Exact retained SMT query did not reproduce expected statuses")
        result = {**row, "IR_sha256": exported["IR"]["sha256"], "specification": {"path": spec_path.relative_to(output).as_posix(),
                  "sha256": digest(spec_path.read_bytes())}, "query": {"path": query_path.relative_to(output).as_posix(), "sha256": query_sha},
                  "verdict": verdict, "retained_query_statuses": statuses}
        if row["expected"] == "refuted":
            n = witness_value(verification["counterexample"])
            original_key = f"base::{row['policy']}"
            original = expected("base", row["policy"], n)
            changed = expected(row["module"], row["policy"], n)
            require(original != changed, "SMT witness does not distinguish actual source mutation")
            witness_path = output / "models" / (stem + ".witness.json")
            write(witness_path, verification["counterexample"])
            replays = []
            for replay_key in (original_key, key):
                source = next(item for item in exports if item["key"] == replay_key)
                runs = json.loads(run([tools / "chialisp-oxide-verifier", output / source["IR"]["path"], "--replay", witness_path],
                                      root=root, env=env).stdout)
                require(type(runs) is list and len(runs) == 2 and all(type(v.get("optimize")) is bool for v in runs)
                        and {v["optimize"] for v in runs} == {False, True}, "Witness replay must contain exact OFF/ON")
                for replay in runs:
                    wanted = source["native"][n]
                    outcome = check_replay(replay["outcome"], expected_code="raise")
                    require(outcome == ({"status": "accepted", "output_hex": encoded(wanted["value"])}
                                        if wanted["status"] == "accepted" else {"status": "rejected"}), "Recompiled witness mismatch")
                replays.append({"key": replay_key, "native": source["native"][n], "runs": runs,
                                "retained_domain_VM_indices": [n, n]})
            result["witness"] = {"input": n, "path": witness_path.relative_to(output).as_posix(), "replays": replays}
        obligations.append(result)
    require((len(exports), len(obligations), len(defaults), vm_count, malformed_count) == (16, 12, 4, 8192, 256), "Incomplete bridge counts")
    require(source_snapshot(root) == before and identities(root, tools, z3, env) == identity
            and file_sha(native) == native_sha, "Source or tool/native binary changed during bridge")
    result = {"schema_version": 1, "protocol": PROTOCOL, "status": "validated", "scope": SCOPE,
              "sources": before, "identities": identity, "native_binary": {"path": str(native), "sha256": native_sha},
              "requested_cargo_options": OPTIONS, "requested_build_environment": BUILD_ENV, "bundles": bundles, "exports": exports, "obligations": obligations,
              "default_rejections": defaults, "counts": {"actual_exports": 16, "baseline_actual_tables": 4,
              "nonvacuous_proven_obligations": 8, "actual_source_refutations": 4, "retained_query_scripts": 12,
              "retained_query_status_checks": 24, "native_finite_cases": 4096, "retained_canonical_VM_cases": vm_count,
              "retained_malformed_VM_cases": malformed_count, "recompiled_witness_VM_cases": 16},
              "assumptions": ASSUMPTIONS}
    write(output / "result.json", result)
    return result

COUNTS = {"actual_exports": 16, "baseline_actual_tables": 4, "nonvacuous_proven_obligations": 8,
          "actual_source_refutations": 4, "retained_query_scripts": 12, "retained_query_status_checks": 24,
          "native_finite_cases": 4096, "retained_canonical_VM_cases": 8192,
          "retained_malformed_VM_cases": 256, "recompiled_witness_VM_cases": 16}


def validate_recorded(report, *, root, output):
    """Offline consistency/freshness only; never adds solver or execution credit."""
    contract, specifications = reviewed_contract(root)
    require(set(report) == {"schema_version", "protocol", "status", "scope", "sources", "identities", "native_binary",
                           "requested_cargo_options", "requested_build_environment", "bundles", "exports", "obligations", "default_rejections", "counts", "assumptions"},
            "Unknown/missing bridge report fields")
    require(type(report["schema_version"]) is int and report["schema_version"] == 1
            and report["protocol"] == PROTOCOL and report["status"] == "validated" and report["scope"] == SCOPE
            and report["assumptions"] == ASSUMPTIONS,
            "Wrong bridge schema/protocol/status/scope")
    require(report["sources"] == source_snapshot(root), "Stale recorded production/SDK/private source inputs")
    require(report["requested_cargo_options"] == OPTIONS and report["requested_build_environment"] == BUILD_ENV and report["counts"] == COUNTS
            and all(type(n) is int for n in report["counts"].values()), "Incomplete or mistyped bridge counts/options")
    keys = [f"{module}::{policy}" for module in MODULES for policy in POLICIES]
    identity = report["identities"]
    require(type(identity) is dict and set(identity) == {"source_root", "toolchain", "compiler", "tools", "solver"}
            and identity["toolchain"] == tomllib.loads(read(root, "rust-toolchain.toml").decode())["toolchain"]["channel"]
            and isinstance(identity["compiler"], str) and identity["compiler"].startswith("rustc ")
            and "\nhost: " in identity["compiler"], "Incomplete independently captured compiler identity")
    stored_absolute(identity["source_root"])
    compiler_host(identity["compiler"])
    require([tool["name"] for tool in identity["tools"]] == list(TOOLS)
            and all(set(tool) == {"name", "path", "sha256"} and Path(tool["path"]).is_absolute()
                    and isinstance(tool["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", tool["sha256"])
                    for tool in identity["tools"]), "Missing/unknown tool identities")
    require(set(identity["solver"]) == {"path", "sha256", "version"}
            and Path(identity["solver"]["path"]).is_absolute()
            and re.fullmatch(r"[0-9a-f]{64}", identity["solver"]["sha256"])
            and identity["solver"]["version"].startswith("Z3 version "), "Incomplete solver identity")
    require(set(report["native_binary"]) == {"path", "sha256"} and Path(report["native_binary"]["path"]).is_absolute()
            and re.fullmatch(r"[0-9a-f]{64}", report["native_binary"]["sha256"]), "Incomplete same-package native identity")
    require([row["optimize"] for row in report["bundles"]] == [False, True]
            and all(type(row["optimize"]) is bool for row in report["bundles"]), "Missing fresh Cargo modes")
    for bundle in report["bundles"]:
        payload = read(output, bundle["manifest"])
        require(digest(payload) == bundle["sha256"], "Changed retained extraction manifest")
        manifest = json.loads(payload, object_pairs_hook=unique_object)
        checked_manifest(manifest, identity)
        require(set(bundle["IR_sha256"]) == set(keys), "Incomplete retained Cargo IR identities")
    exports = report["exports"]
    require(type(exports) is list and [row.get("key") for row in exports] == keys, "Missing/duplicate/reordered actual exports")
    for row in exports:
        module, policy = row["key"].split("::")
        checked_native(row["native"], module, policy)
        payload = read(output, row["IR"]["path"])
        require(digest(payload) == row["IR"]["sha256"]
                and all(bundle["IR_sha256"][row["key"]] == row["IR"]["sha256"] for bundle in report["bundles"]),
                "Changed retained actual source IR or OFF/ON identity")
        ir = json.loads(payload, object_pairs_hook=unique_object)
        table = type(ir.get("ir_version")) is int and ir.get("ir_version") == 4 and ir.get("environment") == "strict"
        inner = ir["program"] if table else ir
        require(inner.get("name") == f"{CRATE}::{row['key']}", "Retained IR entry does not match actual export")
        require(row["kind"] == ("actual_frontend_table" if table else "actual_legacy_source"), "False source IR classification")
        if module == "base":
            require(table and 9 <= len(ir["program"]["functions"]) <= 10, "Baseline lost actual frontend table")
        require([build.get("optimize") for build in row["builds"]] == [False, True]
                and all(type(build["optimize"]) is bool for build in row["builds"]), "Missing/duplicate/mistyped VM modes")
        for build in row["builds"]:
            artifact, modeled = build["cargo_artifact"], build["modeled_artifact"]
            require(artifact["backend"] == "rust_mir" and type(artifact["optimize_lir"]) is bool
                    and artifact["optimize_lir"] == build["optimize"] and modeled["optimize"] == build["optimize"]
                    and type(modeled["optimize"]) is bool, "Wrong retained artifact mode/backend")
            require(all(artifact[field] == modeled[field] for field in ("bytes", "clvm_hex", "tree_hash")),
                    "Modeled IR recompilation differs from retained Cargo artifact")
            code = read(output, build["retained_CLVM"]["path"])
            require(digest(code) == build["retained_CLVM"]["sha256"] and code == (artifact["clvm_hex"] + "\n").encode()
                    and type(artifact["bytes"]) is int and artifact["bytes"] > 0
                    and len(artifact["clvm_hex"]) == 2 * artifact["bytes"], "Changed retained serialized program")
            checked_vm(build["canonical"], list(range(256)), module, policy)
            checked_vm(build["malformed"], ["()", "(1 2)", "(1 . 2)", "((1 . 2))", "(0x0001)", "(0x0000)", "(256)", "(-1)"],
                       module, policy, malformed=True, optimize=build["optimize"])
    require(len(report["obligations"]) == 12, "Wrong model obligation count")
    for row, declared in zip(report["obligations"], contract["obligations"], strict=True):
        require(all(row[key] == value for key, value in declared.items()), "Changed independent obligation identity/order")
        key = f"{row['module']}::{row['policy']}"
        source = next(item for item in exports if item["key"] == key)
        require(row["IR_sha256"] == source["IR"]["sha256"], "Model input differs from retained actual IR")
        spec = read(output, row["specification"]["path"])
        require(digest(spec) == row["specification"]["sha256"]
                and json.loads(spec, object_pairs_hook=unique_object) == specifications[row["policy"]][row["kind"]],
                "Changed independently authored specification")
        query = read(output, row["query"]["path"])
        require(digest(query) == row["query"]["sha256"]
                and query.startswith(b"; Nonvacuity\n; Explicit acyclic scalar call-frame model;")
                and query.count(b"(check-sat)") == 2, "Changed/incomplete retained query protocol")
        verification = checked_verdict(row["verdict"], row["expected"])
        ground_nonvacuity(verification, source)
        require(row["retained_query_statuses"] == ["sat", "unsat" if row["expected"] == "proven" else "sat"],
                "Wrong retained solver statuses")
        if row["expected"] == "refuted":
            witness = row["witness"]
            n = witness_value(verification["counterexample"])
            require(type(witness["input"]) is int and witness["input"] == n
                    and load(output, witness["path"]) == verification["counterexample"], "Changed retained source witness")
            require(expected("base", row["policy"], n) != expected(row["module"], row["policy"], n),
                    "Witness does not distinguish source mutation")
            require([item["key"] for item in witness["replays"]] == [f"base::{row['policy']}", key], "Incomplete witness source pair")
            for replay in witness["replays"]:
                original = next(item for item in exports if item["key"] == replay["key"])
                require(replay["native"] == original["native"][n] and replay["retained_domain_VM_indices"] == [n, n],
                        "Witness does not bind actual finite native/retained observations")
                require(len(replay["runs"]) == 2 and {v["optimize"] for v in replay["runs"]} == {False, True}
                        and all(type(v["optimize"]) is bool for v in replay["runs"]), "Incomplete witness VM modes")
                for run_ in replay["runs"]:
                    wanted = replay["native"]
                    require(check_replay(run_["outcome"], expected_code="raise") ==
                            ({"status": "accepted", "output_hex": encoded(wanted["value"])} if wanted["status"] == "accepted" else {"status": "rejected"}),
                            "Wrong source witness replay outcome")
    require([row["key"] for row in report["default_rejections"]] == [f"base::{name}" for name in POLICIES]
            and all(type(row["exit_code"]) is int and row["exit_code"] != 0
                    and "does not model runtime" in row["stderr"] for row in report["default_rejections"]),
            "Default model runtime exclusion missing")
    # Check the complete retained bundle tables, not a self-selected IR digest.
    for bundle in report["bundles"]:
        manifest = load(output, bundle["manifest"])
        checked_manifest(manifest, identity)
        entries = manifest["entries"]
        require(len(entries) == 16 and {entry["entry"] for entry in entries} == {f"{CRATE}::{key}" for key in keys},
                "Incomplete retained Cargo export table")
        base = Path(bundle["manifest"]).parent
        for entry in entries:
            require(type(entry) is dict and set(entry) == {"entry", "ir", "clvm", "artifact"}, "Malformed retained export table entry")
            key = entry["entry"].removeprefix(CRATE + "::")
            source = next(item for item in exports if item["key"] == key)
            build = next(item for item in source["builds"] if item["optimize"] == bundle["optimize"])
            require(entry["artifact"] == build["cargo_artifact"]
                    and digest(read(output, (base / entry["ir"]).as_posix())) == source["IR"]["sha256"]
                    and read(output, (base / entry["clvm"]).as_posix()) == (build["cargo_artifact"]["clvm_hex"] + "\n").encode(),
                    "Retained bundle IR/artifact table mismatch")
    return {"status": "checked_recorded_consistency_only", "scope": "No new source extraction, solver/native/VM execution or authentication credit.",
            "counts": COUNTS}


def distinct_records(left, right):
    # Python equality conflates bool with int; JSON wire representations do not.
    return json.dumps(left, sort_keys=True) != json.dumps(right, sort_keys=True)


def recorded_controls(report, *, root, output):
    """Corrupt actual retained records; never source/solver/VM execution credit."""
    mutations = [
        ("captured source root", lambda v: v["identities"].update(source_root="/constructed/other")),
        ("duplicate compiler host", lambda v: v["identities"].update(compiler=v["identities"]["compiler"] + "host: contradictory-host\n")),
        ("missing tool identity", lambda v: v["identities"]["tools"].pop()),
        ("changed extraction identity", lambda v: v["bundles"][0].update(sha256="0" * 64)),
        ("source freshness", lambda v: v["sources"]["production"][0].update(sha256="0" * 64)),
        ("numeric count Boolean", lambda v: v["counts"].update(native_finite_cases=True)),
        ("missing export", lambda v: v["exports"].pop()),
        ("wrong source class", lambda v: v["exports"][0].update(kind="constructed_adapter")),
        ("native Boolean input", lambda v: v["exports"][0]["native"][0].update(input=False)),
        ("native missing guard", lambda v: v["exports"][0]["native"][100].update(status="accepted", value=101)),
        ("resource is inconclusive", lambda v: v["exports"][0]["builds"][0]["canonical"][100].update(
            outcome={"status": "inconclusive"}, failure={"kind": "resource", "code": "cost_exceeded"})),
        ("numeric VM mode", lambda v: v["exports"][0]["builds"][0].update(optimize=0)),
        ("changed Cargo artifact", lambda v: v["exports"][0]["builds"][0]["cargo_artifact"].update(clvm_hex="80")),
        ("different modeled artifact", lambda v: v["exports"][0]["builds"][0]["modeled_artifact"].update(tree_hash="0" * 64)),
        ("nonvacuity rejected by actual source", lambda v: v["obligations"][0]["verdict"]["verification"].update(nonvacuity={"arguments": [{"kind": "int", "value": "100"}]})),
        ("model vacuity", lambda v: v["obligations"][0]["verdict"]["verification"].update(nonvacuity=None)),
        ("wrong model", lambda v: v["obligations"][0]["verdict"].update(model="scalar")),
        ("model scope widening", lambda v: v["obligations"][0]["verdict"].update(scope="compiler preservation")),
        ("different model IR", lambda v: v["obligations"][0].update(IR_sha256="0" * 64)),
        ("query binding", lambda v: v["obligations"][0]["query"].update(sha256="0" * 64)),
        ("independent spec binding", lambda v: v["obligations"][0]["specification"].update(sha256="0" * 64)),
        ("wrong solver statuses", lambda v: v["obligations"][0].update(retained_query_statuses=["sat", "sat"])),
        ("missing default exclusion", lambda v: v["default_rejections"].pop()),
        ("witness out of domain", lambda v: v["obligations"][8]["verdict"]["verification"].update(counterexample={"arguments": [{"kind": "int", "value": "256"}]})),
        ("missing witness VM mode", lambda v: v["obligations"][8]["witness"]["replays"][0]["runs"].pop()),
    ]
    rejected = []
    for label, mutate in mutations:
        validate_recorded(report, root=root, output=output)
        changed = copy.deepcopy(report)
        mutate(changed)
        require(distinct_records(changed, report), "Vacuous recorded corruption: " + label)
        try:
            validate_recorded(changed, root=root, output=output)
        except (ValueError, KeyError, TypeError):
            rejected.append(label)
        else:
            raise ValueError("Recorded corruption accepted: " + label)
    # Rebind the digest after changing actual retained manifest bytes. These
    # controls exercise identity relationships rather than digest mismatch.
    header_mutations = [
        ("analysis target", lambda v: v.update(analysis_target="constructed-incompatible-target")),
        ("manifest path", lambda v: v.update(manifest_path="/constructed/other/Cargo.toml")),
        ("workspace root", lambda v: v.update(workspace_root="/constructed/other")),
        ("toolchain", lambda v: v.update(toolchain="different-toolchain")),
        ("driver", lambda v: v.update(driver_sha256="0" * 64)),
        ("Rue", lambda v: v.update(rue_revision="0" * 40)),
        ("Cargo options", lambda v: v.update(cargo_options=OPTIONS[:-1])),
        ("package", lambda v: v.update(package="different-package")),
        ("numeric schema Boolean", lambda v: v.update(schema_version=True)),
        ("status", lambda v: v.update(status="validated")),
        ("unknown header field", lambda v: v.update(extra="unsupported")),
    ]
    header_rejected = []
    for label, mutate in header_mutations:
        validate_recorded(report, root=root, output=output)
        with tempfile.TemporaryDirectory() as directory:
            copied = Path(directory) / "evidence"
            shutil.copytree(output, copied)
            changed = copy.deepcopy(report)
            bundle = changed["bundles"][0]
            original = load(copied, bundle["manifest"])
            manifest = copy.deepcopy(original)
            mutate(manifest)
            require(distinct_records(manifest, original), "Vacuous manifest control: " + label)
            write(copied / bundle["manifest"], manifest)
            bundle["sha256"] = digest(read(copied, bundle["manifest"]))
            try:
                validate_recorded(changed, root=root, output=copied)
            except (ValueError, KeyError, TypeError):
                header_rejected.append(label)
            else:
                raise ValueError("Rebound retained manifest mutation accepted: " + label)
    return {"status": "passed_recorded_corruption_controls", "scope": "Actual retained record/file mutations only; no new execution or proof credit.",
            "rejected": rejected, "manifest_rejected_with_rebound_digest": header_rejected,
            "positive_before_each": len(rejected) + len(header_rejected)}
