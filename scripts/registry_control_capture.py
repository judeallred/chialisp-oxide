"""Coordinated registry control capture; importing executes no commands.

Native calls, Program validation, retained template execution and local SDK
observations are separate lanes. Captures establish finite consistency, not
execution authentication, security or compiler preservation.
"""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time
import tomllib

import cargo_authoring
import cargo_bundle
from cargo_source_receipt import decode_json  # load_declared_export invokes validate_receipt itself
from source_walk import WalkLimits, source_files
from vm_failures import check_execution

PACKAGE = "chialisp-oxide-registry-controls"
DIRECTORY = "experiments/action-registry-source-controls"
NAMES = ("alpha-drop-send", "alpha-drop-create", "beta-drop-send", "beta-drop-create")
RUE = "245c107a8e795229ea08097142f005a4c0d91075"
require = cargo_authoring.require


def read(path, maximum=16 * 1024 * 1024, *, empty=False):
    path = Path(path)
    require(not path.is_symlink(), "regular input cannot be symlink")
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    with os.fdopen(descriptor, "rb") as stream:
        metadata = os.fstat(stream.fileno())
        import stat
        require(stat.S_ISREG(metadata.st_mode) and metadata.st_size <= maximum, "bounded regular input")
        raw = stream.read(maximum + 1)
    require(len(raw) <= maximum and (empty or raw), "bounded input grew or empty")
    return raw


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def save(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    raw = (json.dumps(value, indent=2) + "\n").encode()
    require(len(raw) <= 16 * 1024 * 1024, "draft command/report output limit")
    with path.open("xb") as stream:
        stream.write(raw)


def private_snapshot(package):
    """Every regular package member except ignored build/VCS/cache directories.

    source_walk provides bounded traversal. No cached return of source hashes.
    """
    paths = source_files(package.parent, package.name, suffixes=None,
        limits=WalkLimits(depth=8, sources=512, directories=64, entries=1024))
    rows, total = [], 0
    for path in paths:
        path = Path(path).relative_to(package.name).as_posix()
        require(not any(p in ("target", "node_modules", ".git", "__pycache__") for p in Path(path).parts),
                "unexpected generated files in package declaration")
        raw = read(package / path, min(2 * 1024 * 1024, 8 * 1024 * 1024 - total))
        total += len(raw)
        rows.append(dict(path=path, sha256=digest(raw)))
    require(total <= 8 * 1024 * 1024, "complete package source bound")
    return dict(mode="cargo_library", package=PACKAGE, manifest="Cargo.toml", library="src/lib.rs",
        exports={name: dict(entry=PACKAGE.replace("-", "_") + "::" + name.replace("-", "_") + "::puzzle",
                            source="src/" + name.replace("-", "_") + ".rs") for name in NAMES},
        sources=sorted(rows, key=lambda row: row["path"]))


def strict_control_abi(envelope, name):
    require(set(envelope) == {"ir_version", "environment", "program"}
            and type(envelope["ir_version"]) is int and envelope["ir_version"] == 5
            and envelope["environment"] == "strict", "strict pair-capable control envelope")
    body = envelope["program"]
    require(body["name"] == PACKAGE.replace("-", "_") + "::" + name.replace("-", "_") + "::puzzle",
            "exact entry identity")
    byte = lambda n: dict(kind="bytes", len=n)
    pair = lambda first, rest: dict(kind="pair", first=first, rest=rest)
    tup = lambda *items: dict(kind="tuple", items=list(items))
    uint = dict(kind="int", bits=64, signed=False)
    truth = pair(dict(kind="tree"), pair(dict(kind="bool"), pair(byte(32), byte(32))))
    send = tup(uint, uint, dict(kind="unit"), byte(32))
    create = tup(uint, byte(32), uint, tup(byte(32)))
    signature = tup(uint, byte(48), byte(32))
    output = pair(truth, tup(create if name.endswith("send") else send, signature))
    require([row["ty"] for row in body["parameters"]] == [byte(48), byte(32), truth, byte(32), byte(32)]
            and body["result"] == output, "exact logical input and altered output ABI")


def capture(root, target, output, *, source_snapshot, verify_native, validate_sdk, validate_baseline):
    """Execute only after the caller supplies all three reviewed fail-closed hooks.

    source_snapshot(): bounded complete implementation/standard-library window.
    verify_native(): actual current native gates/logs and eight output observations.
    validate_sdk(record, modules): independent complete SDK/wire reader.
    This core does not implement those hooks or any false fallback for them.
    """
    root, target, output = map(lambda p: Path(p).resolve(), (root, target, output))
    require(all(callable(f) for f in (source_snapshot, verify_native, validate_sdk, validate_baseline)), "reviewed hooks required")
    require(not output.exists() and output.is_relative_to(root / "target"), "new isolated target output")
    package = root / DIRECTORY
    toolchain = tomllib.loads(read(root / "rust-toolchain.toml").decode())["toolchain"]["channel"]
    env = dict(os.environ)
    for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
                "CARGO_BUILD_TARGET", "RUST_MIN_STACK", "RUSTC_BOOTSTRAP", "CARGO_MAKEFLAGS", "MAKEFLAGS", "MFLAGS"):
        env.pop(key, None)
    env.update(RUSTUP_TOOLCHAIN=toolchain, RUSTUP_AUTO_INSTALL="0", CARGO_NET_OFFLINE="true",
        CARGO_TARGET_DIR=str(target), CHIALISP_OXIDE_DRIVER=str(target / "debug/chialisp-oxide-driver"),
        CARGO_BUILD_JOBS="2", PYTHONDONTWRITEBYTECODE="1")
    tool_paths = {name: target / "debug" / name for name in (
        "cargo-chialisp-oxide", "chialisp-oxide-driver", "chialisp-oxide", "chialisp-oxide-verifier")}
    tool_paths["registry_controls"] = target / "debug/examples/registry_controls"
    def tool_snapshot():
        return {name: dict(path=str(path), bytes=path.stat().st_size,
                    sha256=digest(read(path, 512 * 1024 * 1024))) for name, path in tool_paths.items()}
    window, tools, native = source_snapshot(), tool_snapshot(), verify_native()
    output.mkdir(parents=True)
    commands = []
    def unchanged():
        require(source_snapshot() == window and tool_snapshot() == tools, "source/tool capture window changed")
    def execute(role, argv, *, stdout_bound=16 * 1024 * 1024):
        unchanged()
        stdout_path = output / "logs" / f"{len(commands):02}-{role}.stdout"
        stderr_path = stdout_path.with_suffix(".stderr")
        stdout_path.parent.mkdir(exist_ok=True)
        start = time.monotonic()
        with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
            proc = subprocess.Popen(list(map(str, argv)), cwd=root, env=env, stdout=stdout, stderr=stderr,
                                    start_new_session=True)
            try:
                while proc.poll() is None:
                    require(time.monotonic() - start <= 1800 and stdout_path.stat().st_size <= stdout_bound
                            and stderr_path.stat().st_size <= 2 * 1024 * 1024,
                            "capture resource limit; inconclusive")
                    time.sleep(0.25)
            except BaseException:
                os.killpg(proc.pid, signal.SIGTERM)
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.wait()
                raise
        row = dict(role=role, argv=list(map(str, argv)), exit_code=proc.returncode,
            stdout=stdout_path.relative_to(output).as_posix(), stderr=stderr_path.relative_to(output).as_posix())
        commands.append(row)
        save(output / "command-records" / f"{len(commands):02}.json", row)
        require(proc.returncode == 0, "command failure is not rejection evidence")
        raw = read(stdout_path, stdout_bound, empty=True)
        read(stderr_path, 2 * 1024 * 1024, empty=True)
        unchanged()
        return raw
    # Independent full compiler query and driver query precede extraction.
    compiler = execute("compiler", ["rustup", "run", toolchain, "rustc", "-vV"]).decode()
    driver = execute("driver", [tool_paths["chialisp-oxide-driver"], "--compiler-version"]).decode().strip()
    require(compiler.splitlines()[0] == driver, "compiler and driver identity differ")
    execute("source-recipes", ["python3", "-B", package / "verify_sources.py"])
    fixtures = decode_json(read(package / "fixtures/native-vectors.json"))
    require([row["id"] for row in fixtures["cases"]] == list(NAMES), "complete ordered native fixtures")
    bundles, control_entries, vm, pending_templates = {}, {}, [], []
    for label, source_root, snapshot, count, options in (
        ("baseline", root, lambda: cargo_authoring.snapshot(root), 40,
            ["--locked", "--manifest-path", str(root / "puzzles/Cargo.toml")]),
        ("controls", package, lambda: private_snapshot(package), 4,
            ["--manifest-path", str(package / "Cargo.toml"), "--package", PACKAGE, "--locked", "--offline"]),
    ):
        for mode in ("off", "on"):
            destination = output / "bundles" / label / mode
            execute(f"extract-{label}-{mode}", [tool_paths["cargo-chialisp-oxide"], "build", *options,
                "--output-dir", destination, *(["--no-optimize"] if mode == "off" else [])])
            authoring = snapshot()
            require(len(authoring["exports"]) == count, "complete expected authoring count")
            first = next(iter(authoring["exports"].values()))["entry"]
            # This loads the complete immutable bundle and its schema-2 receipt
            # before any selected entry is used.
            admitted = cargo_bundle.load_declared_export(destination, root=source_root, authoring=authoring,
                selected_entry=first, compiler=compiler, driver_sha256=tools["chialisp-oxide-driver"]["sha256"],
                toolchain=toolchain, current_authoring=snapshot, optimize=mode == "on",
                rue_revision=RUE, cargo_options=options)
            require(type(admitted.manifest["schema_version"]) is int and admitted.manifest["schema_version"] == 2,
                    "schema2 required; never artifact-only fallback")
            bundles[f"{label}-{mode}"] = dict(manifest=str(admitted.manifest_path.relative_to(output)),
                manifest_sha256=digest(read(admitted.manifest_path)))
            if label == "baseline":
                recompiled = []
                # Python transport/source admission is not Program validation.
                # The actual Rust verifier validates/recompiles every baseline
                # body before any baseline artifact is selected for comparison.
                for entry_row in admitted.manifest["entries"]:
                    slug = next(slug for slug, export in authoring["exports"].items()
                                if export["entry"] == entry_row["entry"])
                    role = f"recompile-baseline-{slug}-{mode}"
                    artifacts = decode_json(execute(role, [tool_paths["chialisp-oxide-verifier"],
                        admitted.manifest_path.parent / entry_row["ir"], "--artifacts"]))
                    require(type(artifacts) is list and len(artifacts) == 2
                        and [a["optimize"] for a in artifacts] == [False, True], "actual two-mode verifier output")
                    selected = artifacts[int(mode == "on")]
                    require(type(selected["optimize"]) is bool and all(selected[k] == entry_row["artifact"][k]
                            for k in ("bytes", "clvm_hex", "tree_hash")), "all40 actual selected baseline recompilations")
                    recompiled.append(dict(entry=entry_row["entry"], role=role,
                        optimize=mode == "on", bytes=selected["bytes"], tree_hash=selected["tree_hash"]))
                require(len(recompiled) == 40, "all40 actual Programs validated before selection")
                bundles[f"{label}-{mode}"]["recompiled"] = recompiled
                validate_baseline(admitted, mode)
                continue
            for name in NAMES:
                entry = authoring["exports"][name]["entry"]
                loaded = cargo_bundle.load_declared_export(destination, root=source_root, authoring=authoring,
                    selected_entry=entry, compiler=compiler, driver_sha256=tools["chialisp-oxide-driver"]["sha256"],
                    toolchain=toolchain, current_authoring=snapshot, optimize=mode == "on",
                    rue_revision=RUE, cargo_options=options)
                strict_control_abi(loaded.program, name)
                row = next(row for row in loaded.manifest["entries"] if row["entry"] == entry)
                artifacts = decode_json(execute(f"recompile-{name}-{mode}",
                    [tool_paths["chialisp-oxide-verifier"], loaded.path, "--artifacts"]))
                selected = [row for row in artifacts if row["optimize"] is (mode == "on")]
                require(len(selected) == 1 and all(selected[0][k] == row["artifact"][k]
                    for k in ("bytes", "clvm_hex", "tree_hash")), "selected retained artifact recompilation")
                spec = next(spec for spec in fixtures["cases"] if spec["id"] == name)
                pending_templates.append(dict(name=name, mode=mode, spec=spec,
                    clvm=loaded.manifest_path.parent / row["clvm"], artifact=row["artifact"]))
                control_entries[(name, mode)] = row
    # All four complete schema2 bundles and all eighty-eight actual Programs
    # have now been admitted/validated/recompiled. Only now execute retained VM
    # bytes, followed by the separate local SDK lane.
    require(len(pending_templates) == 8 and len(control_entries) == 8, "complete paired controls before execution")
    for pending in pending_templates:
        name, mode, spec = pending["name"], pending["mode"], pending["spec"]
        solution = output / "inputs" / f"{name}-{mode}.json"
        save(solution, [spec["environment_assembly"]])
        execution = decode_json(execute(f"template-{name}-{mode}", [tool_paths["chialisp-oxide"],
            "run-batch", pending["clvm"], "--solutions", solution, "--max-cost", "10000000"]))
        require(type(execution) is list and len(execution) == 1
            and check_execution(execution[0]) == spec["variant_expected"], "exact native/template witness")
        vm.append(dict(name=name, optimize_lir=mode == "on", execution=execution[0],
            input=spec["environment_assembly"], artifact=pending["artifact"]))
    runs = [dict(name=name, optimize_lir=mode == "on", entry=control_entries[(name, mode)]["entry"],
                 module_hex=control_entries[(name, mode)]["artifact"]["clvm_hex"])
            for name in NAMES for mode in ("off", "on")]
    request = output / "sdk-request.json"
    save(request, dict(protocol="chialisp-oxide-registry-source-controls-request/v1", runs=runs))
    records = []
    for attempt in (1, 2):
        raw = execute(f"sdk-{attempt}", [tool_paths["registry_controls"], request])
        with (output / f"sdk-{attempt}.json").open("xb") as stream:
            stream.write(raw)
        record = decode_json(raw)
        validate_sdk(record, runs)
        records.append(raw)
    require(records[0] == records[1], "independent SDK repeats must match byte-for-byte")
    unchanged()
    # This is an un-packaged capture. Root's independent reader must validate
    # native logs, SDK semantics, closure and bounded inventory before promotion.
    save(output / "capture.json", dict(status="captured_pending_portable_reader", source_window=window,
        tools=tools, native=native, bundles=bundles, vm=vm, commands=commands,
        sdk="sdk-1.json", repeat="sdk-2.json", scope="Finite local observations only; no authentication or preservation proof"))


# Native/build phases share the same bounded whole-source and dated-tool window.
def environment(root, target):
    toolchain = tomllib.loads(read(root / "rust-toolchain.toml").decode())["toolchain"]["channel"]
    env = dict(os.environ)
    for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
                "CARGO_BUILD_TARGET", "RUST_MIN_STACK", "RUSTC_BOOTSTRAP", "CARGO_MAKEFLAGS", "MAKEFLAGS", "MFLAGS"):
        env.pop(key, None)
    env.update(RUSTUP_TOOLCHAIN=toolchain, RUSTUP_AUTO_INSTALL="0", CARGO_NET_OFFLINE="true",
        CARGO_TARGET_DIR=str(target), CHIALISP_OXIDE_DRIVER=str(target / "debug/chialisp-oxide-driver"),
        CARGO_BUILD_JOBS="2", PYTHONDONTWRITEBYTECODE="1")
    return env


def compiler_tools(root, env):
    rows = []
    for name in ("cargo", "rustc", "rustfmt", "clippy-driver"):
        result = subprocess.run(["rustup", "which", "--toolchain", env["RUSTUP_TOOLCHAIN"], name],
            cwd=root, env=env, capture_output=True, timeout=60)
        require(result.returncode == 0 and not result.stderr and len(result.stdout) <= 4096,
                "dated tool path query failed/inconclusive")
        path = Path(result.stdout.decode().strip()).resolve(strict=True)
        require(path.name == name, "actual dated tool basename")
        rows.append(dict(tool=name, path=str(path), sha256=digest(read(path, 512 * 1024 * 1024))))
    return rows


def full_sources(root):
    import precommit_source_controls as p
    require(p.ROOT.resolve() == root, "actual current source root")
    paths = {row["path"] for row in p.source_window()["sources"]}
    paths.update(source_files(root, ["crates", "puzzles", "experiments/backend", "experiments/byte-proofs",
        "vendor", DIRECTORY], suffixes=None, limits=WalkLimits(sources=4096, directories=4096, entries=16384)))
    paths.update(("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml"))
    require(len(paths) <= 4096, "complete native source count")
    rows, remaining = [], 64 * 1024 * 1024
    for name in sorted(paths):
        payload = read(root / name, min(16 * 1024 * 1024, remaining))
        remaining -= len(payload)
        rows.append(dict(path=name, bytes=len(payload), sha256=digest(payload)))
    return rows


def check_retained_sources(root, rows):
    require(type(rows) is list and rows and len(rows) <= 4096, "bounded complete retained source rows")
    require(len({row["path"] for row in rows}) == len(rows), "unique retained source paths")
    remaining = 64 * 1024 * 1024
    for row in rows:
        require(set(row) == {"path", "bytes", "sha256"} and type(row["bytes"]) is int
                and 0 < row["bytes"] <= 16 * 1024 * 1024, "exact source role")
        name = row["path"]
        require(type(name) is str and Path(name).as_posix() == name and not Path(name).is_absolute()
                and ".." not in Path(name).parts, "confined source role")
        require(row["bytes"] <= remaining, "complete retained source aggregate precharge")
        remaining -= row["bytes"]
    for row in rows:
        data = read(root / row["path"])
        require(len(data) == row["bytes"] and digest(data) == row["sha256"], "actual retained source changed")


def lock_admission(root):
    package = root / DIRECTORY
    def external(path):
        return sorted((row["name"], row["version"], row.get("source"), row.get("checksum"))
            for row in tomllib.loads(read(path).decode())["package"] if row.get("source"))
    root_rows, controls = external(root / "Cargo.lock"), external(package / "Cargo.lock")
    require(len(controls) == 127 and set(controls) <= set(root_rows), "127 reviewed external pins, without upgrades")
    require(read(package / "root-lock.seed") == read(root / "Cargo.lock"), "retained reviewed root seed")
    return dict(root_external_count=len(root_rows), resolved_external_count=len(controls), unpinned_tuples=[],
        root_seed_exact=True, resolved_lock_sha256=digest(read(package / "Cargo.lock")),
        seed_sha256=digest(read(package / "root-lock.seed")), external_tuples=controls)


def native_observations(root, text):
    import re
    fixtures = decode_json(read(root / DIRECTORY / "fixtures/native-vectors.json"))["cases"]
    prefix, summary_prefix = "REGISTRY_RUST_CONTROL_NATIVE ", "REGISTRY_RUST_CONTROL_NATIVE_SUMMARY "
    rows = [decode_json(line[line.index(prefix) + len(prefix):].encode()) for line in text.splitlines() if prefix in line]
    summaries = [decode_json(line[line.index(summary_prefix) + len(summary_prefix):].encode())
                 for line in text.splitlines() if summary_prefix in line]
    require([row["id"] for row in rows] == list(NAMES) and len(fixtures) == 4, "four ordered native observations")
    for row, case in zip(rows, fixtures, strict=True):
        require(row["baseline"] == case["baseline_expected"] and row["variant"] == case["variant_expected"],
                "exact independent original/variant native outputs")
    counts = dict(baseline_policy_calls=4, variant_policy_calls=4, abi_observations=0, VM_executions=0, SDK_submissions=0)
    require(json.dumps(summaries, sort_keys=True) == json.dumps([counts], sort_keys=True), "exact finite native counts")
    tests = re.findall(r"^test result: (\w+)\. (\d+) passed; (\d+) failed", text, re.M)
    require(tests and all(status == "ok" and failed == "0" for status, _, failed in tests)
            and sum(int(passed) for _, passed, _ in tests) == 2, "two actual successful native tests")
    return dict(observations=rows, summary=counts)


def native_commands(root):
    package = root / DIRECTORY
    common = ["--manifest-path", str(package / "Cargo.toml"), "--all-targets", "--locked", "--offline",
              "--features", "native-vm", "-j", "2"]
    return [("recipe-before", ["python3", str(package / "verify_sources.py")]),
        ("fmt", ["cargo", "fmt", "--manifest-path", str(package / "Cargo.toml"), "-p", PACKAGE, "--", "--check"]),
        ("check", ["cargo", "check", *common]), ("test", ["cargo", "test", *common, "--", "--nocapture", "--test-threads=1"]),
        ("clippy", ["cargo", "clippy", *common, "--", "-D", "warnings"]),
        ("recipe-after", ["python3", str(package / "verify_sources.py")])]


def verify_native_gate(root, directory, env):
    native = decode_json(read(directory / "receipt.json"))
    require(native.get("status") == "passed" and native.get("source_and_tool_window_quiet") is True,
            "successful quiet native receipt required")
    gates = native["gates"]
    commands = native_commands(root)
    require(len(gates) == len(commands) and all(g["name"] == name and g["command"] == command
        and type(g["exit_code"]) is int and g["exit_code"] == 0
        for g, (name, command) in zip(gates, commands, strict=True)), "exact actual native gates")
    before, after = [decode_json(read(directory / name)) for name in ("frozen-source-before.json", "frozen-source-after.json")]
    require(before == after == full_sources(root), "complete current native source window")
    check_retained_sources(root, before)
    tool_before, tool_after = [decode_json(read(directory / name)) for name in ("tools-before.json", "tools-after.json")]
    require(tool_before == tool_after == compiler_tools(root, env), "actual native compiler-tool window")
    actual = native_observations(root, read(directory / "test.stdout", 2 * 1024 * 1024).decode())
    require(json.dumps(actual["summary"], sort_keys=True) == json.dumps(native["native_counts"], sort_keys=True), "actual native count sidecar")
    lock_admission(root)
    return native


def run_phase(root, target, output, phase, native_dir=None):
    """Run native or LAST build with bounded logs and preserved failed attempts."""
    import re
    root, target, output = map(lambda p: Path(p).resolve(), (root, target, output))
    require(phase in ("native", "last") and not output.exists() and output.is_relative_to(root / "target")
            and target.is_relative_to(root / "target"), "fresh isolated phase paths")
    output.mkdir(parents=True)
    env = environment(root, target)
    receipt = dict(status="in_progress", target=str(target), toolchain=env["RUSTUP_TOOLCHAIN"], jobs=2,
        scope="Native/build phase only; no frontend, ABI, VM, SDK, live or proof credit.", gates=[])
    try:
        before, tools_before = full_sources(root), compiler_tools(root, env)
        save(output / "frozen-source-before.json", before)
        save(output / "tools-before.json", tools_before)
        if phase == "native":
            # Locked Cargo alone does not establish that dependency pins are
            # the reviewed root pins. Admit them before executing any gate.
            admitted_lock = lock_admission(root)
            save(output / "lock-admission-before.json", admitted_lock)
        if phase == "last":
            require(native_dir is not None and Path(native_dir).resolve().is_relative_to(root / "target"),
                    "LAST phase requires isolated actual preceding native gate")
            verify_native_gate(root, Path(native_dir), env)
            require((Path(native_dir) / "receipt.json").stat().st_mtime_ns <= (output / "frozen-source-before.json").stat().st_mtime_ns,
                    "actual native receipt before LAST window")
        commands = native_commands(root) if phase == "native" else [
            ("build", ["cargo", "build", "--workspace", "--bins", "--examples", "--locked", "--offline", "-j", "2"])]
        for name, command in commands:
            require(before == full_sources(root) and tools_before == compiler_tools(root, env), "phase inputs changed before command")
            start = time.monotonic()
            stdout_path, stderr_path = output / (name + ".stdout"), output / (name + ".stderr")
            with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
                proc = subprocess.Popen(command, cwd=root, env=env, stdout=stdout, stderr=stderr, start_new_session=True)
                try:
                    while proc.poll() is None:
                        require(time.monotonic() - start <= 900 and stdout_path.stat().st_size <= 2 * 1024 * 1024
                                and stderr_path.stat().st_size <= 2 * 1024 * 1024, "phase resource limit; inconclusive")
                        time.sleep(0.25)
                except BaseException:
                    os.killpg(proc.pid, signal.SIGTERM)
                    try:
                        proc.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(proc.pid, signal.SIGKILL)
                        proc.wait()
                    raise
            read(stdout_path, 2 * 1024 * 1024, empty=True)
            read(stderr_path, 2 * 1024 * 1024, empty=True)
            receipt["gates"].append(dict(name=name, command=command, exit_code=proc.returncode,
                elapsed_seconds=time.monotonic() - start))
            save(output / (name + "-gate.json"), receipt["gates"][-1])
            require(proc.returncode == 0, "phase gate failed; no rejection credit")
        after, tools_after = full_sources(root), compiler_tools(root, env)
        save(output / "frozen-source-after.json", after)
        save(output / "tools-after.json", tools_after)
        require(before == after and tools_before == tools_after, "whole native/build window must stay quiet")
        receipt.update(source_and_tool_window_quiet=True, source_members=len(before))
        if phase == "native":
            observations = native_observations(root, read(output / "test.stdout", 2 * 1024 * 1024).decode())
            save(output / "actual-native-observations.json", observations)
            require(lock_admission(root) == admitted_lock, "reviewed lock pins changed during native phase")
            save(output / "lock-admission.json", admitted_lock)
            receipt["native_counts"] = observations["summary"]
            paths = re.findall(r"Running [^\n]*\(([^()\n]+)\)", read(output / "test.stderr", 2 * 1024 * 1024).decode())
            require(len(paths) == 2 and len(set(paths)) == 2, "two actual native test executables")
            archive_rows, remaining = [], 512 * 1024 * 1024
            for name in paths:
                executable = Path(name).resolve(strict=True)
                require(executable.is_relative_to(target / "debug/deps"), "actual test executable belongs to isolated target")
                payload = read(executable, remaining)
                remaining -= len(payload)
                archive = output / "frozen-test-executables" / executable.name
                archive.parent.mkdir(exist_ok=True)
                archive.write_bytes(payload)
                archive_rows.append(dict(path=str(executable), archive=archive.relative_to(output).as_posix(),
                    bytes=len(payload), sha256=digest(payload)))
            save(output / "native-executables.json", archive_rows)
        else:
            built = []
            for name in ("chialisp-oxide-driver", "cargo-chialisp-oxide", "chialisp-oxide", "chialisp-oxide-verifier", "examples/registry_controls"):
                path = target / "debug" / name
                payload = read(path, 512 * 1024 * 1024)
                built.append(dict(name=name, path=str(path), bytes=len(payload), sha256=digest(payload)))
            save(output / "built-tool-identities.json", built)
        require(before == full_sources(root) and tools_before == compiler_tools(root, env), "phase changed during evidence serialization")
        receipt["status"] = "passed"
    except BaseException as error:
        receipt.update(status="failed_or_inconclusive", error=repr(error))
        raise
    finally:
        # Preserve end state even when a command fails; never replace old files.
        for name, snapshot in (("frozen-source-after.json", lambda: full_sources(root)),
                               ("tools-after.json", lambda: compiler_tools(root, env))):
            if not (output / name).exists():
                try:
                    save(output / name, snapshot())
                except BaseException as error:
                    receipt.setdefault("end_snapshot_errors", []).append(dict(path=name, error=repr(error)))
                    receipt["status"] = "failed_or_inconclusive"
        save(output / "receipt.json", receipt)
    return receipt
