"""Bounded portable CAT source-control packet; recorded consistency, not execution.

Installation first requires the original host-current capture validator. Raw
schema-2 declarations and logs are copied unchanged. Their absolute origin paths
are metadata only; this reader never follows them or executes a subprocess.
"""
import hashlib
import json
import os
import re
from pathlib import Path, PurePosixPath
import shutil
import stat
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
HERE = ROOT / "experiments/cat-genesis-tail"
sys.path.insert(0, str(HERE))
import rust_capture as capture
import rust_lifecycle as evidence
import cargo_bundle
from cargo_source_receipt import decode_json, validate_receipt
from registry_codegen_evidence import require

PROTOCOL = "chialisp-oxide-cat-genesis-source-controls-packet/v1"
MAX_FILES = 128
MAX_FILE = 2 * 1024 * 1024
MAX_TOTAL = 8 * 1024 * 1024
CORE_COMMANDS = {"prepare-lock", "native-fmt", "native-tests", "native-clippy", "host-fmt", "host-build", "host-clippy", "compiler-query", "driver-query", "subset-check", "extract-off", "extract-on", "rust-lifecycle-attempt1", "rust-lifecycle-attempt2", "recorded-reader"}
OPTIONAL_COMMANDS = {"last-workspace-build", "canonical-host-build"}


def relative(value):
    require(type(value) is str and 0 < len(value) <= 512, "bounded packet path")
    require(re.fullmatch(r"[A-Za-z0-9_./-]+", value) is not None, "ASCII unambiguous packet path")
    path = PurePosixPath(value)
    require(not path.is_absolute() and path.as_posix() == value
            and all(part not in ("", ".", "..") for part in path.parts), "closed relative packet path")
    return value


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def read(path, maximum=MAX_FILE):
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size <= maximum,
            "bounded unaliased regular packet file")
    with path.open("rb") as source:
        raw = source.read(maximum + 1)
    require(len(raw) == info.st_size and len(raw) <= maximum, "packet file changed or grew")
    return raw


def files(root):
    require(root.is_dir() and not root.is_symlink(), "regular packet directory")
    result = []
    scanned = 0
    directories = 0
    directory_paths = []
    regulars = 0
    def visit(directory, depth):
        nonlocal scanned, directories, regulars
        require(depth <= 6, "packet directory depth")
        # Charge entries before retaining or sorting. Empty directories count.
        children = []
        with os.scandir(directory) as entries:
            for entry in entries:
                scanned += 1
                require(scanned <= 512, "packet scanned entry bound")
                relative(Path(entry.path).relative_to(root).as_posix())
                info = entry.stat(follow_symlinks=False)
                require(not stat.S_ISLNK(info.st_mode), "packet symlink")
                if stat.S_ISDIR(info.st_mode):
                    directories += 1
                    directory_paths.append(Path(entry.path))
                    require(directories <= 128, "packet directory count")
                else:
                    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1, "packet file type/alias")
                    regulars += 1
                    require(regulars <= MAX_FILES, "packet file count")
                children.append((Path(entry.path), stat.S_ISDIR(info.st_mode)))
        for child, is_directory in sorted(children):
            if is_directory:
                visit(child, depth + 1)
            else:
                result.append(child)
                require(len(result) <= MAX_FILES, "packet file count")
    visit(root, 0)
    needed = set()
    for path in result:
        parent = path.parent
        while parent != root:
            needed.add(parent)
            parent = parent.parent
    require(set(directory_paths) == needed, "unlisted empty packet directory")
    return result


def inventory(paths, root):
    """Reserve every declared byte before the first read/hash."""
    total = 0
    rows = []
    require(len(paths) <= MAX_FILES, "packet file count")
    for path in paths:
        name = relative(path.relative_to(root).as_posix())
        info = path.lstat()
        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size <= MAX_FILE,
                "packet preflight regular per-file bound")
        require(info.st_size <= MAX_TOTAL - total, "packet aggregate bound")
        total += info.st_size
        rows.append((name, path, info.st_size))
    result = []
    for name, path, size in rows:
        raw = read(path, size)
        result.append({"path": name, "bytes": size, "sha256": digest(raw)})
    return result


def load(root, name):
    return decode_json(read(root / relative(name)))


def origin_name(path, origin):
    require(type(path) is str and path.startswith(origin + "/"), "origin capture path relation")
    return relative(path[len(origin) + 1:])


def original_sources():
    original = capture.read(capture.POLICY_SOURCE).decode()
    recipe = decode_json(capture.read(HERE / "mutations.json"))
    require(recipe["source"] == "../../puzzles/cat-genesis-by-coin-id/puzzle.rs"
            and [row["name"] for row in recipe["controls"]] == list(capture.CONTROLS), "fixed shared source recipes")
    result = {}
    for row in recipe["controls"]:
        source = original
        for edit in row["edits"]:
            require(source.count(edit["before"]) == 1, "unique actual source mutation")
            source = source.replace(edit["before"], edit["after"])
        result[row["name"]] = source
    return result


def check_identity(root, row, origin, aliases):
    require(set(row) == {"path", "bytes", "sha256"}, "exact retained file identity")
    name = origin_name(row["path"], origin)
    name = aliases.get(name, name)
    raw = read(root / name)
    require(type(row["bytes"]) is int and row["bytes"] == len(raw)
            and row["sha256"] == digest(raw), "retained file byte binding")
    return raw


def compiler_host(compiler):
    require(type(compiler) is str and len(compiler) <= 16_384, "bounded compiler declaration")
    hosts = [line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: ")]
    require(len(hosts) == 1 and 0 < len(hosts[0]) <= 128 and hosts[0].isascii(), "exactly one compiler host")
    return hosts[0]


def check_prepared(root, prepared, origin, extraction):
    require(set(prepared) == {"protocol", "root_lock", "templates", "materialized", "scope"}
            and prepared["protocol"] == "chialisp-oxide-cat-genesis-rust-prepared/v1", "fixed prepared protocol/schema")
    capture.check_project_inputs(root / "project", extraction["source_root"])
    names = ("Cargo.toml", "Cargo.lock", "src/lib.rs", *("src/" + c + ".rs" for c in capture.CONTROLS), "tests/native.rs", "vectors.json")
    for rows in (prepared["materialized"], extraction["materialized"],
                 load(root, "native-window-before.json")["materialized"]):
        require(type(rows) is list and len(rows) == len(names), "complete materialized identity inventory")
        require([row["path"] for row in rows] == [origin + "/project/" + name for name in names], "ordered exact materialized paths")
        for row in rows:
            check_identity(root, row, origin, {})
    templates = [capture.POLICY_SOURCE, *(HERE / n for n in ("src/lib.rs", "mutations.json", "rust-native.rs", "vectors.json"))]
    rows = prepared["templates"]
    require(type(rows) is list and len(rows) == len(templates), "fixed template identity inventory")
    for row, template in zip(rows, templates):
        require(set(row) == {"path", "bytes", "sha256"}
                and row["path"] == extraction["source_root"] + "/" + template.relative_to(ROOT).as_posix(), "template origin/source relation")
        raw = read(template)
        require(type(row["bytes"]) is int and row["bytes"] == len(raw)
                and row["sha256"] == digest(raw), "current exact prepared template bytes")


def admit_bundle(root, mode, artifacts, sources, compiler, driver):
    directory = root / ("bundle-" + mode)
    manifest = load(root, "bundle-" + mode + "/manifest.json")
    require(manifest["status"] == "wip_prototype_not_ready_for_use"
            and manifest["analysis_target"] == compiler_host(compiler), "fixed bundle status/independent compiler host")
    require(manifest["schema_version"] == 2 and manifest["compiler"] == compiler
            and manifest["driver_sha256"] == driver and manifest["toolchain"] == capture.TOOLCHAIN
            and manifest["package"] == capture.PACKAGE, "complete schema2/compiler/package relation")
    require(manifest["cargo_options"] == ["--manifest-path", manifest["manifest_path"], "--locked"]
            and manifest["manifest_path"] == manifest["workspace_root"] + "/Cargo.toml", "origin Cargo argv relation")
    reader = cargo_bundle._Reader()
    declarations = cargo_bundle._preflight_ir_declarations(manifest, reader, mode == "on")
    require(set(declarations) == {capture.CRATE + "::" + c + "::puzzle" for c in capture.CONTROLS},
            "complete exact three-export inventory")
    paths = set()
    admitted = {}
    details = {}
    for entry in manifest["entries"]:
        program, path, sha, size = cargo_bundle._artifact_details(
            entry, directory, reader, paths, mode == "on", 2,
            ir_declaration=declarations[entry["entry"]])
        require(program.get("environment") == "raw", "whole environment artifact ABI")
        admitted[entry["entry"]] = (entry, path, sha, size)
        details[entry["entry"]] = (entry["ir"], size, sha)
    attachments = validate_receipt(manifest, directory, details, reader, paths)
    expected_sources = {"package/Cargo.toml", "package/Cargo.lock", "package/src/lib.rs",
                        *("package/src/" + c + ".rs" for c in capture.CONTROLS)}
    require({s.label for s in attachments} == expected_sources, "exact complete source attachment inventory")
    for source in attachments:
        require(source.contents == read(root / "project" / source.label.removeprefix("package/")),
                "source attachment/materialized bytes")
        if source.label.endswith(".rs"):
            require(source.kinds == ("compiler_text",) and source.compiler_read_count >= 1,
                    "actual compiler source-read declaration")
    for control in capture.CONTROLS:
        entry_name = capture.CRATE + "::" + control + "::puzzle"
        entry, path, sha, size = admitted[entry_name]
        selected = [a for a in artifacts["artifacts"] if a["mode"] == mode and a["control"] == control]
        require(len(selected) == 1, "unique actual mode/control")
        selected = selected[0]
        require(selected["entry"] == entry_name and selected["clvm_hex"] == entry["artifact"]["clvm_hex"]
                and selected["source_receipt"] == manifest["source_receipt"], "actual artifact/bundle binding")
        require(selected["ir"]["sha256"] == sha and selected["ir"]["bytes"] == size,
                "actual artifact/IR binding")
        require(read(root / "project/src" / (control + ".rs")) == sources[control].encode(),
                "actual independently reconstructed policy source")
    required = {"manifest.json", *(e["ir"] for e in manifest["entries"]),
                *(e["clvm"] for e in manifest["entries"]),
                *(s["blob"] for s in manifest["source_receipt"]["files"])}
    require({p.relative_to(directory).as_posix() for p in files(directory)} == required, "closed complete bundle file inventory")
    return manifest


def validate_native(root, native, origin, aliases):
    specification = evidence.vector_specification()
    require(read(HERE / "vectors.json") == (json.dumps(specification, indent=2) + "\n").encode(),
            "independent vector specification")
    expected = [{"control": "original", "name": row["name"], "actual": row["expected"]}
                for row in specification["cases"]]
    expected += [{"control": c, "name": n, "actual": {"status": "accepted", "output_hex": "80"}}
                 for c, n in (("drop_delta_guard", "non-nil-delta-one"),
                              ("drop_genesis_equality", "mismatched-parent"))]
    require(len(expected) == 39 and native["status"] == "passed"
            and type(native["native_test_groups"]) is int and native["native_test_groups"] == 1
            and native["observations"] == expected, "independent exact39 native observations")
    stdout = check_identity(root, native["native_stdout"], origin, aliases).decode()
    command = decode_json(check_identity(root, native["native_command"], origin, aliases))
    import re
    require(re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", stdout)
            == [("1", "0", "0")], "one actual retained native group")
    rows = [decode_json(line.split("CAT_GENESIS_RUST_NATIVE ", 1)[1].encode())
            for line in stdout.splitlines() if "CAT_GENESIS_RUST_NATIVE " in line]
    require(rows == expected, "native stdout exact witness binding")
    project = origin + "/project"
    require(command["argv"] == ["rustup", "run", capture.TOOLCHAIN, "cargo", "test", "--manifest-path",
            project + "/Cargo.toml", "--locked", "--test", "native", "--", "--nocapture"], "native command exact argv")
    require(native["source_window"] == load(root, "native-window-before.json"), "native window binding")
    require(command["target"] == str(Path(native["runner"]["path"]).parents[2]), "native target/runner origin relation")


def check_source_window(window):
    origin = window["source_root"]
    require(type(origin) is str and origin.startswith("/"), "source origin metadata")
    for key, anchor, expected_paths in (("sources", origin, capture.source_input_paths),
                                        ("compiler_sources", window["compiler_root"], capture.compiler_input_paths)):
        rows = window[key]
        require(type(rows) is list and len(rows) <= 4096, "source window count")
        expected = expected_paths(ROOT)  # Discovery itself has source_walk budgets.
        require(type(expected) is list and len(expected) <= 4096, "expected source path count")
        require([origin_name(row["path"], anchor) for row in rows] == expected, "complete reviewed current source closure")
        declared = 0
        for name in expected:
            size = (ROOT / name).stat().st_size
            require(size <= MAX_FILE and size <= capture.MAX_RETAINED_BYTES - declared, "source closure declared byte bound")
            declared += size
        seen = set()
        for row in rows:
            name = origin_name(row["path"], anchor)
            require(name not in seen, "duplicate source window path")
            seen.add(name)
            raw = read(ROOT / name)
            require(type(row["bytes"]) is int and len(raw) == row["bytes"]
                    and digest(raw) == row["sha256"], "current reviewed source byte identity")


def check_command_role(label, command, origin, extraction, native, compiler_target):
    project = origin + "/project/Cargo.toml"
    source = extraction["source_root"]
    manifest = source + "/experiments/cat-genesis-tail/Cargo.toml"
    cargo = ["rustup", "run", capture.TOOLCHAIN, "cargo"]
    tools = extraction["tools"]
    require(command["cwd"] == source, "command working directory origin")
    if label.startswith("rust-lifecycle-attempt"):
        expected = [native["runner"]["path"], origin + "/artifacts.json"]
    elif label.startswith("extract-"):
        mode = label.removeprefix("extract-")
        expected = [tools[1]["path"], "build", "--manifest-path", project, "--locked", "--output-dir", origin + "/bundle-" + mode]
        if mode == "off": expected.append("--no-optimize")
    elif label == "driver-query": expected = [tools[0]["path"], "--compiler-version"]
    elif label == "compiler-query": expected = ["rustup", "run", capture.TOOLCHAIN, "rustc", "-vV"]
    elif label == "subset-check": expected = [tools[1]["path"], "check", "--manifest-path", project, "--locked"]
    elif label == "prepare-lock": expected = cargo + ["metadata", "--format-version", "1", "--manifest-path", project, "--offline"]
    elif label == "recorded-reader":
        require(type(command["argv"][0]) is str and command["argv"][0].startswith("/"), "opaque Python interpreter origin")
        expected = [command["argv"][0], source + "/experiments/cat-genesis-tail/rust_lifecycle.py", "--report", origin + "/rust-lifecycle-attempt1.stdout", "--capture", origin, "--self-test"]
    elif label == "last-workspace-build": expected = cargo + ["build", "--workspace", "--bins", "--examples", "--locked", "--offline", "--jobs", "2"]
    elif label == "canonical-host-build": expected = cargo + ["build", "--manifest-path", manifest, "--example", "canonical_lifecycle", "--locked"]
    else:
        group, action = label.split("-", 1)
        target = project if group == "native" else manifest
        if action == "fmt": expected = cargo + ["fmt", "--manifest-path", target, "--check"]
        elif action == "clippy": expected = cargo + ["clippy", "--manifest-path", target, "--all-targets", "--locked", "--", "-D", "warnings"]
        elif action == "tests": expected = cargo + ["test", "--manifest-path", target, "--locked", "--test", "native", "--", "--nocapture"]
        else: expected = cargo + ["build", "--manifest-path", target, "--example", "rust_lifecycle", "--locked"]
    require(command["argv"] == expected, "closed command role/argv")
    if label in {"compiler-query", "driver-query", "subset-check", "extract-off", "extract-on"}:
        require(command["target"] == compiler_target, "shared compiler target relation")
    elif label == "last-workspace-build":
        require(command["target"] == source + "/target", "last workspace target relation")
    else:
        require(command["target"] == str(Path(native["runner"]["path"]).parents[2]), "command native target relation")


def structural_inventory(root):
    root = Path(root).absolute()
    require(not root.is_symlink(), "packet root symlink")
    root = root.resolve()
    all_rows = inventory(files(root), root)
    packet = load(root, "report.json")
    require(set(packet) == {"protocol", "scope", "origin_capture", "files", "aliases"}
            and packet["protocol"] == PROTOCOL and packet["scope"] == "recorded_consistency_only",
            "closed packet protocol/schema")
    require(type(packet["origin_capture"]) is str and packet["origin_capture"].startswith("/")
            and len(packet["origin_capture"]) <= 512, "bounded opaque origin")
    origin = packet["origin_capture"]
    aliases = {"rust-lifecycle-attempt1.stdout": "capture.json", "rust-lifecycle-attempt2.stdout": "capture.json",
               "rust-lifecycle-attempt2.stderr": "rust-lifecycle-attempt1.stderr"}
    require(packet["aliases"] == aliases, "only byte-identical repeat aliases")
    actual = [row for row in all_rows if row["path"] != "report.json"]
    require(packet["files"] == actual, "exact closed packet inventory/bytes")
    return root, packet, actual


def recorded_packet_paths(base):
    """Relative inventory including index; hashes/bounds only, no current-source checks.

    No process or producer calls. The content declarations are unauthenticated.
    """
    _, _, rows = structural_inventory(base)
    return tuple(sorted(["report.json", *(row["path"] for row in rows)]))


def validate(root):
    root, packet, actual = structural_inventory(root)
    origin = packet["origin_capture"]
    aliases = packet["aliases"]
    require({path.name for path in root.iterdir() if path.is_dir()} == {"bundle-off", "bundle-on", "project"}, "closed packet root directories")
    artifacts = load(root, "artifacts.json")
    report = load(root, "capture.json")
    require(report["protocol"] == "chialisp-oxide-rust-cat-genesis-lifecycle/v1" and report["status"] == "executed"
            and report["artifacts"] == artifacts and artifacts["status"] == "admitted_fresh_bundles",
            "actual report wire/artifact identity")
    require(report["raw_max_cost"] == 1_000_000 and report["component_max_cost"] == 250_000_000
            and report["sdk_transaction_max_cost"] == 5_500_000_000
            and report["sdk_max_block_cost_clvm"] == 11_000_000_000, "closed VM/SDK limits")
    require(report["canonical_tail_hex"] == read(HERE / "upstream/genesis_by_coin_id.clsp.hex").decode().strip(),
            "canonical module bytes")
    require([lane["mode"] for lane in report["lanes"]] == ["off", "on"], "exact two-mode report inventory")
    sources = original_sources()
    for prefix in ("native", "extract", "sdk"):
        require(read(root / (prefix + "-window-before.json")) == read(root / (prefix + "-window-after.json")),
                "retained source/tool windows identical")
    extraction = load(root, "extract-window-before.json")
    native = load(root, "native.json")
    check_source_window(extraction)
    require(load(root, "sdk-window-before.json")["source_tools"] == extraction, "SDK/extraction window relation")
    native_window = load(root, "native-window-before.json")
    require(native_window["sources"] == extraction["sources"] and native_window["materialized"] == extraction["materialized"], "native/extraction source relation")
    require(extraction["tools"][3] == native["runner"], "actual runner code identity relation")
    require(artifacts["compiler"] == read(root / "compiler-query.stdout").decode()
            and artifacts["driver_declaration"] == read(root / "driver-query.stdout").decode()
            and artifacts["driver_declaration"] == artifacts["compiler"].splitlines()[0] + "\n",
            "independent retained compiler declarations")
    manifests = [admit_bundle(root, mode, artifacts, sources, artifacts["compiler"],
                             extraction["tools"][0]["sha256"]) for mode in ("off", "on")]
    require(all(m["workspace_root"] == origin + "/project" for m in manifests), "bundle origin/project relation")
    for artifact in artifacts["artifacts"]:
        mode = artifact["mode"]
        old_manifest = artifact["manifest"]["path"]
        rel = origin_name(old_manifest, origin)
        require(rel.startswith("bundle-" + mode + "/") and len(PurePosixPath(rel).parts) == 3,
                "original artifact manifest location")
        raw = read(root / ("bundle-" + mode) / "manifest.json")
        require(artifact["manifest"]["bytes"] == len(raw) and artifact["manifest"]["sha256"] == digest(raw),
                "exact raw manifest identity")
        require(artifact["ir"]["path"] == old_manifest.rsplit("/", 1)[0] + "/" + next(
            e["ir"] for e in manifests[mode == "on"]["entries"] if e["entry"] == artifact["entry"]),
                "exact origin IR path relation")
    capture.validate_lock(read(ROOT / "Cargo.lock"), read(root / "project/Cargo.lock"))
    prepared = load(root, "prepared.json")
    check_prepared(root, prepared, origin, extraction)
    root_lock = prepared["root_lock"]
    require(root_lock["path"] == extraction["source_root"] + "/Cargo.lock"
            and root_lock["bytes"] == (ROOT / "Cargo.lock").stat().st_size
            and root_lock["sha256"] == digest(read(ROOT / "Cargo.lock")), "reviewed root lock receipt")
    validate_native(root, native, origin, aliases)
    labels = {path.name.removesuffix(".command.json") for path in root.glob("*.command.json")}
    require(CORE_COMMANDS <= labels <= CORE_COMMANDS | OPTIONAL_COMMANDS, "closed complete command inventory")
    base = {"artifacts.json", "native.json", "prepared.json", "execution.json", "report.json",
            *(prefix + "-window-" + end + ".json" for prefix in ("native", "extract", "sdk") for end in ("before", "after"))}
    base |= {label + "." + ext for label in labels for ext in ("command.json", "stdout", "stderr")}
    base -= set(aliases)
    base.add("capture.json")
    require({path.name for path in root.iterdir() if path.is_file()} == base, "closed root packet file inventory")
    project_files = {"Cargo.toml", "Cargo.lock", "src/lib.rs", *("src/" + c + ".rs" for c in capture.CONTROLS), "tests/native.rs", "vectors.json"}
    require({p.relative_to(root / "project").as_posix() for p in files(root / "project")} == project_files, "closed materialized project inventory")
    compiler_target = load(root, "compiler-query.command.json")["target"]
    require(type(compiler_target) is str and compiler_target.startswith("/") and len(compiler_target) <= 512, "opaque bounded compiler target")
    for path in sorted(root.glob("*.command.json")):
        command = decode_json(read(path))
        require(set(command) == {"argv", "cwd", "exit_code", "target", "jobs", "offline", "status", "failure_kind", "failure_detail", "timeout_seconds", "cleanup_timeout_seconds", "max_stream_bytes", "stdout", "stderr"}, "closed command receipt schema")
        check_command_role(path.name.removesuffix(".command.json"), command, origin, extraction, native, compiler_target)
        require(command["status"] == "passed" and type(command["exit_code"]) is int and command["exit_code"] == 0
                and command["failure_kind"] is None and command["failure_detail"] is None
                and type(command["jobs"]) is int and command["jobs"] == 2 and command["offline"] is True
                and command["max_stream_bytes"] == capture.MAX_FILE
                and command["timeout_seconds"] == capture.COMMAND_TIMEOUT
                and command["cleanup_timeout_seconds"] == capture.COMMAND_CLEANUP_TIMEOUT,
                "successful bounded retained command")
        for stream in ("stdout", "stderr"):
            check_identity(root, command[stream], origin, aliases)
    execution = load(root, "execution.json")
    require(execution["status"] == "executed_and_repeat_byte_identical", "repeat completed")
    for key in ("first_report", "repeat_report"):
        require(check_identity(root, execution[key], origin, aliases) == read(root / "capture.json"),
                "actual repeat report byte binding")
    result = evidence.validate_observations(report, artifacts, sources)
    return {"protocol": PROTOCOL, "status": "recorded_consistency_checked", "files": len(actual),
            "bytes": sum(row["bytes"] for row in actual), **result}


def retained_paths(source):
    paths = files(source)  # Streaming entry/count/type preflight, no unbounded rglob.
    declared = 0
    for path in paths:
        size = path.lstat().st_size
        require(size <= MAX_FILE and size <= capture.MAX_RETAINED_BYTES - declared,
                "raw host capture declared transport allowance")
        declared += size
    selected = [p for p in paths if p.parent == source
                and p.name not in ("rust-lifecycle-attempt2.stdout", "rust-lifecycle-attempt2.stderr")]
    selected += [p for p in paths if p.is_relative_to(source / "project")]
    bundles = []
    for mode in ("off", "on"):
        pointer = decode_json(read(source / ("bundle-" + mode) / "latest.json"))
        require(set(pointer) == {"bundle", "manifest"} and len(PurePosixPath(relative(pointer["bundle"])).parts) == 1
                and pointer["manifest"] == pointer["bundle"] + "/manifest.json", "closed raw bundle pointer")
        directory = source / ("bundle-" + mode) / relative(pointer["bundle"])
        manifest = decode_json(read(directory / "manifest.json"))
        required = {"manifest.json", *(e["ir"] for e in manifest["entries"]),
                    *(e["clvm"] for e in manifest["entries"]),
                    *(s["blob"] for s in manifest["source_receipt"]["files"])}
        for name in sorted(required):
            bundles.append((directory / relative(name), "bundle-" + mode + "/" + name))
    return [(p, p.relative_to(source).as_posix()) for p in selected] + bundles


def install(source, destination):
    source, destination = Path(source).resolve(), Path(destination).absolute()
    # No historical/portable bypass: this runs the unchanged host-current checks.
    report = load(source, "rust-lifecycle-attempt1.stdout")
    evidence.validate(report, source)
    require(read(source / "rust-lifecycle-attempt1.stdout") == read(source / "rust-lifecycle-attempt2.stdout")
            and read(source / "rust-lifecycle-attempt1.stderr") == read(source / "rust-lifecycle-attempt2.stderr"),
            "exact repeated reports and trace before deduplication")
    chosen = retained_paths(source)
    require(len(chosen) < MAX_FILES and len({n for _, n in chosen}) == len(chosen), "closed installer inventory")
    # Original capture retains nlink1 files too. Reserve all before copying.
    inventory([p for p, _ in chosen], source)
    require(not destination.exists() and not destination.is_symlink(), "refuse existing packet destination")
    destination.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix=".cat-packet-", dir=destination.parent))
    try:
        for original, name in chosen:
            if name == "rust-lifecycle-attempt1.stdout": name = "capture.json"
            target = stage / relative(name)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(read(original))
        capture.write(stage / "report.json", {"protocol": PROTOCOL, "scope": "recorded_consistency_only",
            "origin_capture": str(source), "files": inventory(files(stage), stage),
            "aliases": {"rust-lifecycle-attempt1.stdout": "capture.json", "rust-lifecycle-attempt2.stdout": "capture.json",
                        "rust-lifecycle-attempt2.stderr": "rust-lifecycle-attempt1.stderr"}})
        result = validate(stage)
        require(not destination.exists(), "packet destination appeared")
        os.rename(stage, destination)
        return result
    finally:
        if stage.exists():
            shutil.rmtree(stage)


def self_test(root):
    """Coherent inventory refresh ensures controls reach content checks too."""
    root = Path(root).resolve()
    validate(root)
    def change(name, function):
        def mutate(base):
            value = load(base, name)
            function(value)
            (base / name).write_text(json.dumps(value, indent=2) + "\n")
        return mutate
    def write_manifest(base, mode, manifest):
        raw = (json.dumps(manifest, indent=2) + "\n").encode()
        (base / ("bundle-" + mode) / "manifest.json").write_bytes(raw)
        def update(value):
            if isinstance(value, dict):
                if value.get("mode") == mode and "source_receipt" in value:
                    value["source_receipt"] = manifest["source_receipt"]
                    value["manifest"]["bytes"] = len(raw)
                    value["manifest"]["sha256"] = digest(raw)
                for child in value.values(): update(child)
            elif isinstance(value, list):
                for child in value: update(child)
        for name in ("artifacts.json", "capture.json"):
            document = load(base, name)
            update(document)
            (base / name).write_text(json.dumps(document, indent=2) + "\n")
    def coherent_target(base):
        manifest = load(base, "bundle-off/manifest.json")
        target = "unreviewed-host-target"
        manifest["analysis_target"] = target
        manifest["source_receipt"]["build"]["analysis_target"] = target
        manifest["source_receipt"]["unit"]["analysis_target"] = target
        write_manifest(base, "off", manifest)
    def coherent_project_change(name):
        def mutate(base):
            replacements = {"tests/native.rs": b"#[test] fn replaced_native_test() {}\n",
                            "vectors.json": b'{"cases": []}\n',
                            "src/lib.rs": b"// replaced generated export library\n"}
            raw = replacements.get(name)
            if raw is None:
                old = read(base / "project" / name)
                source = load(base, "extract-window-before.json")["source_root"]
                raw = old.replace((source + "/crates/chialisp-oxide").encode(), b"/unreviewed/facade")
                require(raw != old, "manifest mutation witness")
            (base / "project" / name).write_bytes(raw)
            origin = load(base, "report.json")["origin_capture"]
            def rebind(value):
                if isinstance(value, dict):
                    if value.get("path") == origin + "/project/" + name:
                        value["bytes"] = len(raw)
                        value["sha256"] = digest(raw)
                    for child in value.values(): rebind(child)
                elif isinstance(value, list):
                    for child in value: rebind(child)
            records = ["prepared.json", "native.json", *(prefix + "-window-" + end + ".json"
                       for prefix in ("native", "extract", "sdk") for end in ("before", "after"))]
            for record in records:
                value = load(base, record)
                rebind(value)
                (base / record).write_text(json.dumps(value, indent=2) + "\n")
            if name in ("Cargo.toml", "src/lib.rs"):
                for mode in ("off", "on"):
                    directory = base / ("bundle-" + mode)
                    manifest = load(base, "bundle-" + mode + "/manifest.json")
                    row = next(r for r in manifest["source_receipt"]["files"] if r["path"] == "package/" + name)
                    (directory / row["blob"]).unlink()
                    row.update({"bytes": len(raw), "sha256": digest(raw), "blob": "source-" + digest(raw) + ".bin"})
                    (directory / row["blob"]).write_bytes(raw)
                    write_manifest(base, mode, manifest)
        return mutate
    controls = [
        *(("coherent materialized " + name, coherent_project_change(name))
          for name in ("tests/native.rs", "vectors.json", "src/lib.rs", "Cargo.toml")),
        ("coherent target vs compiler host", coherent_target),
        ("bundle WIP status", change("bundle-off/manifest.json", lambda m: m.__setitem__("status", "production"))),
        ("prepared materialized identity", change("prepared.json", lambda p: p["materialized"][0].__setitem__("sha256", "00" * 32))),
        ("prepared template identity", change("prepared.json", lambda p: p["templates"][0].__setitem__("sha256", "00" * 32))),
        ("prepared protocol", change("prepared.json", lambda p: p.__setitem__("protocol", "wrong"))),
        ("protocol", change("report.json", lambda p: p.__setitem__("protocol", "wrong"))),
        ("unknown index field", change("report.json", lambda p: p.__setitem__("extra", 0))),
        ("unknown command field", change("rust-lifecycle-attempt1.command.json", lambda c: c.__setitem__("extra", 0))),
        ("command argv", change("rust-lifecycle-attempt1.command.json", lambda c: c.__setitem__("argv", ["/bin/true"]))),
        ("failed command", change("rust-lifecycle-attempt1.command.json", lambda c: c.__setitem__("exit_code", 1))),
        ("repeat output binding", change("rust-lifecycle-attempt2.command.json", lambda c: c["stdout"].__setitem__("sha256", "00" * 32))),
        ("native boolean group", change("native.json", lambda n: n.__setitem__("native_test_groups", True))),
        ("native witness", change("native.json", lambda n: n["observations"][-1].__setitem__("actual", {"status": "rejected"}))),
        ("missing export", change("bundle-off/manifest.json", lambda m: m["entries"].pop())),
        ("mode alias", change("capture.json", lambda r: r["lanes"][1].__setitem__("mode", "off"))),
        ("missing source control", change("capture.json", lambda r: r["lanes"][0]["source_controls"].pop())),
        ("wrong asset", change("capture.json", lambda r: r["lanes"][0]["source_controls"][1].__setitem__("asset_id", "00" * 32))),
        ("wrong consensus code", change("capture.json", lambda r: r["lanes"][0]["lifecycle"]["attempts"][1]["outcome"].__setitem__("code", "DoubleSpend"))),
        ("state marker", change("capture.json", lambda r: r["lanes"][0]["lifecycle"]["attempts"][1]["outcome"].__setitem__("full_simulator_equality", False))),
        ("resource rejection", change("capture.json", lambda r: next(x for x in r["raw_tail_runs"] if x["execution"]["failure"])["execution"]["failure"].__setitem__("kind", "resource"))),
        ("raw output", change("capture.json", lambda r: r["raw_tail_runs"][0]["execution"]["outcome"].__setitem__("output_hex", "01"))),
        ("source closure omission", change("extract-window-before.json", lambda w: w["sources"].pop())),
        ("source blob bytes", lambda b: next((b / "bundle-off").glob("source-*.bin")).write_bytes(b"wrong")),
        ("IR bytes", lambda b: next((b / "bundle-on").glob("*.ir.json")).write_bytes(b"{}")),
        ("extra file", lambda b: (b / "extra.json").write_text("{}")),
    ]
    for name, mutate in controls:
        with tempfile.TemporaryDirectory(prefix="cat-packet-control-") as temporary:
            candidate = Path(temporary) / "packet"
            shutil.copytree(root, candidate)
            mutate(candidate)
            packet = load(candidate, "report.json")
            packet["files"] = inventory([p for p in files(candidate) if p != candidate / "report.json"], candidate)
            (candidate / "report.json").write_text(json.dumps(packet, indent=2) + "\n")
            try:
                validate(candidate)
            except (ValueError, KeyError, IndexError, StopIteration):
                continue
            raise ValueError("portable corruption accepted: " + name)
    return len(controls)
