#!/usr/bin/env python3
"""Staged isolated actual-Cargo Rust TAIL capture; no implicit driver rebuild.

prepare/native do not execute the Rust driver. extract requires its owner's
exclusive lane. Reports bind finite local consistency, not execution authenticity.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import math
import selectors
import signal
import stat
import time
import tomllib
import subprocess
import sys
sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
POLICY_SOURCE = ROOT / "puzzles/cat-genesis-by-coin-id/puzzle.rs"
sys.path.insert(0, str(ROOT / "scripts"))
import cargo_bundle
from registry_codegen_evidence import require
from compiled_codec import inspect as inspect_compiled_artifact
from source_walk import source_files

TOOLCHAIN = "nightly-2026-07-14"
PACKAGE = "chialisp-oxide-cat-genesis-controls"
CRATE = PACKAGE.replace("-", "_")
CONTROLS = ("original", "drop_delta_guard", "drop_genesis_equality")
MAX_FILE = 16 * 1024 * 1024
MAX_RETAINED_BYTES = 64 * 1024 * 1024
MAX_RETAINED_FILES = 4096
COMMAND_TIMEOUT = 600
COMMAND_CLEANUP_TIMEOUT = 5


class CommandFailure(ValueError):
    """Host/process failure, never a puzzle or consensus rejection."""
    def __init__(self, kind, label):
        self.kind = kind
        super().__init__(label + " failed (" + kind + "); retained capped logs and command receipt")



def read(path, maximum=MAX_FILE):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= maximum, "bounded regular file: " + str(path))
    with path.open("rb") as source:
        value = source.read(maximum + 1)
    require(len(value) <= maximum, "file grew beyond bound")
    return value


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def identity(path, maximum=MAX_FILE):
    raw = read(path, maximum)
    return {"path": str(path), "bytes": len(raw), "sha256": digest(raw)}


def write(path, value):
    with path.open("x") as target:
        json.dump(value, target, indent=2)
        target.write("\n")


def command(output, label, argv, *, target, env_extra=None, timeout=COMMAND_TIMEOUT, maximum=MAX_FILE):
    require(os.name == "posix", "reviewed command supervision requires POSIX process groups")
    require(type(label) is str and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,80}", label), "closed command log label")
    require(type(maximum) is int and 0 < maximum <= MAX_FILE, "command stream bound")
    require(type(timeout) in (int, float) and math.isfinite(timeout) and 0 < timeout <= COMMAND_TIMEOUT, "command lifetime bound")
    environment = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="2", CARGO_NET_OFFLINE="true", PYTHONDONTWRITEBYTECODE="1")
    environment.update(env_extra or {})
    argv = [str(value) for value in argv]
    paths = {name: output / (label + "." + name) for name in ("stdout", "stderr")}
    process = None
    kind = None
    detail = None
    group_killed = False
    counts = {name: 0 for name in paths}
    deadline = time.monotonic() + timeout

    def kill_group():
        nonlocal group_killed
        if process is not None and not group_killed:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            group_killed = True

    # Pipe chunks are bounded, and each log's remaining allowance is checked
    # before writing. Even the overflowing chunk can retain only its prefix.
    with paths["stdout"].open("xb") as stdout, paths["stderr"].open("xb") as stderr:
        with selectors.DefaultSelector() as selected:
            try:
                process = subprocess.Popen(argv, cwd=ROOT, env=environment, stdin=subprocess.DEVNULL,
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
                for name, log in (("stdout", stdout), ("stderr", stderr)):
                    pipe = getattr(process, name)
                    os.set_blocking(pipe.fileno(), False)
                    selected.register(pipe, selectors.EVENT_READ, (name, log))
                while True:
                    exited = process.poll() is not None
                    if exited:
                        # A successful parent must not leave grandchildren
                        # running or retaining these pipe descriptors.
                        kill_group()
                    if exited and not selected.get_map():
                        break
                    remaining_time = deadline - time.monotonic()
                    if remaining_time <= 0:
                        kind = "timeout"
                        break
                    for key, _ in selected.select(min(0.05, remaining_time)):
                        raw = os.read(key.fileobj.fileno(), 65536)
                        if not raw:
                            selected.unregister(key.fileobj)
                            key.fileobj.close()
                            continue
                        name, log = key.data
                        remaining = maximum - counts[name]
                        kept = raw[:remaining]
                        log.write(kept)
                        counts[name] += len(kept)
                        if len(raw) > remaining:
                            kind = "output_limit"
                            detail = name
                            break
                    if kind:
                        break
            except BaseException as error:
                kind = "interrupted" if isinstance(error, KeyboardInterrupt) else "infrastructure"
                detail = type(error).__name__
            finally:
                # Kill the whole inherited process group, then reap the direct
                # child. Descendants are killed even after the parent exits;
                # non-child reaping belongs to the OS. This is no sandbox for
                # children deliberately escaping their process group.
                try:
                    kill_group()
                    if process is not None:
                        process.wait(timeout=COMMAND_CLEANUP_TIMEOUT)
                except (OSError, subprocess.TimeoutExpired) as error:
                    kind = "infrastructure"
                    detail = "cleanup_" + type(error).__name__
                for key in list(selected.get_map().values()):
                    key.fileobj.close()
                if process is not None:
                    for pipe in (process.stdout, process.stderr):
                        if pipe is not None:
                            pipe.close()
    exit_code = process.returncode if process is not None else None
    if kind is None and exit_code != 0:
        kind = "nonzero_exit"
    write(output / (label + ".command.json"), {
        "argv": argv, "cwd": str(ROOT), "exit_code": exit_code,
        "target": str(target), "jobs": 2, "offline": True,
        "status": "failed" if kind else "passed", "failure_kind": kind, "failure_detail": detail,
        "timeout_seconds": timeout, "cleanup_timeout_seconds": COMMAND_CLEANUP_TIMEOUT,
        "max_stream_bytes": maximum,
        **{name: identity(path, maximum) for name, path in paths.items()},
    })
    if kind:
        raise CommandFailure(kind, label)
    return read(paths["stdout"], maximum)


def retained_identities(output, paths, *, maximum=MAX_RETAINED_BYTES, max_files=MAX_RETAINED_FILES):
    """Reserve the complete declared inventory before any file read or hash."""
    require(type(maximum) is int and 0 <= maximum <= MAX_RETAINED_BYTES, "retained aggregate bound")
    require(type(max_files) is int and 0 <= max_files <= MAX_RETAINED_FILES, "retained count bound")
    require(len(paths) <= max_files and len(paths) == len(set(paths)), "retained file count/duplicates")
    planned = []
    remaining = maximum
    for relative in sorted(paths):
        require(type(relative) is str, "retained path string")
        part = Path(relative)
        require(part.as_posix() == relative and not part.is_absolute() and part.parts
                and all(x not in ("..", ".") for x in part.parts), "retained normalized relative path")
        path = output / part
        metadata = path.lstat()
        require(stat.S_ISREG(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode), "retained regular file")
        require(metadata.st_size <= MAX_FILE and metadata.st_size <= remaining, "complete retained bundle aggregate bound")
        remaining -= metadata.st_size  # Charge before any read/hash.
        planned.append((path, metadata.st_size))
    result = []
    for path, declared in planned:
        raw = read(path, declared)  # A grown file cannot use another file's reservation.
        require(len(raw) == declared, "retained file changed after preflight")
        result.append({"path": str(path), "bytes": declared, "sha256": digest(raw)})
    return result


def materialized(project):
    return [identity(project / name) for name in ("Cargo.toml", "Cargo.lock", "src/lib.rs", *("src/" + c + ".rs" for c in CONTROLS), "tests/native.rs", "vectors.json")]


def snapshot(project):
    paths = [row["path"] for row in materialized(project)]
    return {"mode":"cargo_library", "package":PACKAGE,"manifest":"Cargo.toml","library":"src/lib.rs",
            "exports":{c:{"entry":CRATE+"::"+c+"::puzzle","source":"src/"+c+".rs"} for c in CONTROLS},
            "sources":[{"path":str(Path(p).relative_to(project)),"sha256":digest(read(Path(p)))} for p in paths]}


def source_input_paths(root):
    paths = source_files(root, ["crates", "vendor/rue-lir"], suffixes=(".rs", ".toml", ".json", ".md"))
    paths += source_files(root, ["puzzles"], suffixes=(".rs", ".toml", ".json", ".md", ".rue", ".clsp", ".clib", ".hex"))
    paths += source_files(root, ["experiments/cat-genesis-tail"], suffixes=(".rs", ".toml", ".lock", ".json", ".md", ".py", ".clsp", ".clib", ".hex"), names=("LICENSE",))
    paths += ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml", "scripts/cargo_bundle.py", "scripts/cargo_authoring.py", "scripts/cargo_source_receipt.py", "scripts/source_walk.py", "scripts/registry_codegen_evidence.py", "scripts/check-registry-codegen.py", "scripts/vm_failures.py", "scripts/raw_tree.py", "scripts/cat_source_controls.py", "scripts/check-cat-source-controls.py", "scripts/test_cat_source_controls.py"]
    return sorted(set(paths))


def compiler_input_paths(root):
    paths=source_files(root,["crates","vendor/rue-lir"],suffixes=(".rs",".toml",".json",".md"))
    paths += source_files(root,["puzzles"],suffixes=(".rs",".toml",".json",".md",".rue",".clsp",".clib",".hex"))
    paths += ["Cargo.toml","Cargo.lock","rust-toolchain.toml",".cargo/config.toml"]
    return sorted(set(paths))


def quiet(project, tools=(), compiler_root=None):
    compiler_sources=[]
    if compiler_root is not None:
        compiler_root=Path(compiler_root).resolve()
        compiler_sources=[identity(compiler_root/p) for p in compiler_input_paths(compiler_root)]
    return {"source_root":str(ROOT),"sources":[identity(ROOT/p) for p in source_input_paths(ROOT)],"compiler_root":str(compiler_root) if compiler_root else None,"compiler_sources":compiler_sources,"materialized":materialized(project),
            "tools":[identity(p, 512 * 1024 * 1024) for p in tools],"scope":"Reviewed local source, materialized files and actual tool identities; no installed-byte authentication or complete build-environment claim."}


def validate_lock(root_bytes, project_bytes):
    """Cargo may prune the root lock or add local controls, never select new pins."""
    root = tomllib.loads(root_bytes.decode())["package"]
    project = tomllib.loads(project_bytes.decode())["package"]
    known = {(p["name"], p["version"], p["source"]): p.get("checksum") for p in root if "source" in p}
    require(len(known) == sum("source" in p for p in root), "duplicate root external lock identity")
    require(any(p["name"] == PACKAGE and "source" not in p for p in project), "resolved local control package")
    seen = set()
    for package in project:
        if "source" not in package:
            continue
        key = (package["name"], package["version"], package["source"])
        require(key not in seen and key in known and package.get("checksum") == known[key],
                "control lock selected an unreviewed external dependency identity")
        seen.add(key)


def expected_project_inputs(source_root):
    """Exact fixed construction; origin is text only and never dereferenced."""
    require(type(source_root) is str and source_root.startswith("/") and len(source_root) <= 512,
            "bounded source-root declaration")
    return {
        "Cargo.toml": ('[package]\nname = "'+PACKAGE+'"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[workspace]\n\n[dependencies]\nchialisp-oxide = { path = '+json.dumps(source_root + "/crates/chialisp-oxide")+' }\n\n[dev-dependencies]\nbumpalo = "3"\nclvmr = "=0.21.0"\nhex = "0.4"\nserde_json = "1"\n').encode(),
        "src/lib.rs": ("//! Generated exact-source control exports; WIP, not ready for funds.\n"
                       + "".join("pub mod " + c + ";\n" for c in sorted(CONTROLS))).encode(),
        "tests/native.rs": read(HERE/"rust-native.rs"),
        "vectors.json": read(HERE/"vectors.json"),
    }


def check_project_inputs(project, source_root):
    for name, expected in expected_project_inputs(source_root).items():
        require(read(project/name) == expected, "exact generated manifest/library/native/vector input: " + name)


def verify_prepared(output):
    record=json.loads(read(output/"prepared.json")); project=output/"project"
    check_project_inputs(project, str(ROOT))
    require(record["materialized"] == materialized(project), "exact prepared sources/manifest/lock/native tests changed")
    require(record["root_lock"] == identity(ROOT/"Cargo.lock"), "reviewed root lock changed")
    validate_lock(read(ROOT/"Cargo.lock"), read(project/"Cargo.lock"))
    recipe=json.loads(read(HERE/"mutations.json")); original=read(POLICY_SOURCE).decode()
    require(record["templates"]==[identity(p) for p in (POLICY_SOURCE, *(HERE/n for n in ("src/lib.rs","mutations.json","rust-native.rs","vectors.json")))], "actual source recipes/native templates changed")
    for control in recipe["controls"]:
        source=original
        for edit in control["edits"]:
            require(source.count(edit["before"])==1,"unique source edit")
            source=source.replace(edit["before"],edit["after"])
        require(read(project/"src"/(control["name"]+".rs"))==source.encode(),"materialized exact source recipe")
    return project


def prepare(args):
    output=args.output; output.mkdir(parents=True,exist_ok=False)
    project=output/"project"; (project/"src").mkdir(parents=True); (project/"tests").mkdir()
    original=read(POLICY_SOURCE).decode(); recipe=json.loads(read(HERE/"mutations.json"))
    require([row["name"] for row in recipe["controls"]]==list(CONTROLS),"exact three source controls")
    for control in recipe["controls"]:
        source=original
        for edit in control["edits"]:
            require(source.count(edit["before"])==1,"unique exact source mutation")
            source=source.replace(edit["before"],edit["after"])
        (project/"src"/(control["name"]+".rs")).write_text(source)
    for name, expected in expected_project_inputs(str(ROOT)).items():
        (project/name).write_bytes(expected)
    (project/"Cargo.lock").write_bytes(read(ROOT/"Cargo.lock"))
    # Resolve the generated workspace while preserving existing external pins.
    # Full metadata repeats an unused dependency graph in the bounded packet.
    command(output,"prepare-lock",["rustup","run",TOOLCHAIN,"cargo","update","--workspace","--manifest-path",project/"Cargo.toml","--offline"],target=args.native_target)
    validate_lock(read(ROOT/"Cargo.lock"), read(project/"Cargo.lock"))
    write(output/"prepared.json",{"protocol":"chialisp-oxide-cat-genesis-rust-prepared/v1","root_lock":identity(ROOT/"Cargo.lock"),"templates":[identity(p) for p in (POLICY_SOURCE, *(HERE/n for n in ("src/lib.rs","mutations.json","rust-native.rs","vectors.json")))],"materialized":materialized(project),"scope":"Exact generated ordinary Cargo source recipes, no compilation/execution credit."})
    print("prepared",project)


def native(args):
    output=args.output; project=verify_prepared(output); before=quiet(project)
    write(output/"native-window-before.json",before)
    command(output,"native-fmt",["rustup","run",TOOLCHAIN,"cargo","fmt","--manifest-path",project/"Cargo.toml","--check"],target=args.native_target)
    stdout=command(output,"native-tests",["rustup","run",TOOLCHAIN,"cargo","test","--manifest-path",project/"Cargo.toml","--locked","--test","native","--","--nocapture"],target=args.native_target).decode()
    tests=re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;",stdout)
    require(tests==[("1","0","0")],"one actual executed native integration test")
    observations=[json.loads(line.split("CAT_GENESIS_RUST_NATIVE ",1)[1]) for line in stdout.splitlines() if "CAT_GENESIS_RUST_NATIVE " in line]
    vectors=json.loads(read(HERE/"vectors.json"))["cases"]
    expected=[{"control":"original","name":row["name"],"actual":row["expected"]} for row in vectors]
    expected += [{"control":c,"name":name,"actual":{"status":"accepted","output_hex":"80"}} for c,name in [("drop_delta_guard","non-nil-delta-one"),("drop_genesis_equality","mismatched-parent")]]
    require(observations==expected,"exact 39 actual native output witnesses")
    command(output,"native-clippy",["rustup","run",TOOLCHAIN,"cargo","clippy","--manifest-path",project/"Cargo.toml","--all-targets","--locked","--","-D","warnings"],target=args.native_target)
    command(output,"host-fmt",["rustup","run",TOOLCHAIN,"cargo","fmt","--manifest-path",HERE/"Cargo.toml","--check"],target=args.native_target)
    command(output,"host-build",["rustup","run",TOOLCHAIN,"cargo","build","--manifest-path",HERE/"Cargo.toml","--example","rust_lifecycle","--locked"],target=args.native_target)
    command(output,"host-clippy",["rustup","run",TOOLCHAIN,"cargo","clippy","--manifest-path",HERE/"Cargo.toml","--all-targets","--locked","--","-D","warnings"],target=args.native_target)
    after=quiet(project);write(output/"native-window-after.json",after); require(after==before,"source changed through native and last host build")
    runner=args.native_target/"debug/examples/rust_lifecycle"
    write(output/"native.json",{"status":"passed","native_test_groups":1,"observations":observations,"native_stdout":identity(output/"native-tests.stdout"),"native_command":identity(output/"native-tests.command.json"),"runner":identity(runner,512*1024*1024),"source_window":before})
    print("PASS native39, ordinary fmt/Clippy and last host build")


def extract(args):
    output=args.output; project=verify_prepared(output); native_record=json.loads(read(output/"native.json"))
    require(native_record["source_window"]==quiet(project),"native/LAST-host source window changed before extraction")
    runner=Path(native_record["runner"]["path"])
    require(identity(runner,512*1024*1024)==native_record["runner"],"last host runner changed")
    tools=(args.driver,args.registry_cli,args.cli,runner)
    before=quiet(project,tools,args.compiler_root);write(output/"extract-window-before.json",before)
    extra={"CHIALISP_OXIDE_DRIVER":str(args.driver)}
    compiler=command(output,"compiler-query",["rustup","run",TOOLCHAIN,"rustc","-vV"],target=args.compiler_target).decode()
    declaration=command(output,"driver-query",[args.driver,"--compiler-version"],target=args.compiler_target).decode()
    require(declaration==compiler.splitlines()[0]+"\n","actual independent compiler and driver declaration")
    command(output,"subset-check",[args.registry_cli,"check","--manifest-path",project/"Cargo.toml","--locked"],target=args.compiler_target,env_extra=extra)
    authoring=snapshot(project); artifacts=[]
    for mode in ("off","on"):
        argv=[args.registry_cli,"build","--manifest-path",project/"Cargo.toml","--locked","--output-dir",output/("bundle-"+mode)]
        if mode=="off":argv.append("--no-optimize")
        command(output,"extract-"+mode,argv,target=args.compiler_target,env_extra=extra)
        for c in CONTROLS:
            entry=CRATE+"::"+c+"::puzzle"
            export=cargo_bundle.load_declared_export(output/("bundle-"+mode),root=project,authoring=authoring,selected_entry=entry,compiler=compiler,driver_sha256=before["tools"][0]["sha256"],toolchain=TOOLCHAIN,current_authoring=lambda:snapshot(project),optimize=mode=="on",cargo_options=["--manifest-path",str(project/"Cargo.toml"),"--locked"])
            require(export.manifest["schema_version"]==2,"complete fresh schema2 bundle")
            expected={"package/Cargo.toml","package/Cargo.lock","package/src/lib.rs",*("package/src/"+name+".rs" for name in CONTROLS)}
            require({s.label for s in export.source_files}==expected,"exact complete library source inventory")
            for source in export.source_files:
                require(source.contents==read(project/source.label.removeprefix("package/")),"actual source receipt matches materialized source")
                if source.label.endswith(".rs"):require(source.kinds==("compiler_text",) and source.compiler_read_count>=1,"actual rustc source-read receipt")
            require(export.program.get("environment")=="raw","actual whole-environment ABI")
            selected=next(e for e in export.manifest["entries"] if e["entry"]==entry)
            raw=bytes.fromhex(selected["artifact"]["clvm_hex"]); observed=inspect_compiled_artifact(raw)
            require(observed["identity"]["module_tree_hash"]==selected["artifact"]["tree_hash"],"independent bounded compiled artifact identity")
            artifacts.append({"mode":mode,"control":c,"entry":entry,"clvm_hex":raw.hex(),**observed["identity"],"compiled_inspection":observed,"manifest":identity(export.manifest_path),"ir":identity(export.path),"source_sha256":digest(read(project/"src"/(c+".rs"))),"source_receipt":export.source_receipt})
    for c in CONTROLS:
        require(artifacts[CONTROLS.index(c)]["ir"]["sha256"]==artifacts[3+CONTROLS.index(c)]["ir"]["sha256"],"same source IR across modes")
    after=quiet(project,tools,args.compiler_root);write(output/"extract-window-after.json",after);require(after==before,"source/tool/runner changed during extraction")
    write(output/"artifacts.json",{"protocol":"chialisp-oxide-cat-genesis-rust-artifacts/v1","status":"admitted_fresh_bundles","compiler":compiler,"driver_declaration":declaration,"artifacts":artifacts,"scope":"Actual complete source-receipted Cargo bundles, independently bounded compiled identities, not source authenticity or IR/CLVM equivalence."})
    print("PASS six extracted and independently admitted actual Rust artifacts")


def execute(args):
    output=args.output;project=verify_prepared(output)
    native_record=json.loads(read(output/"native.json"));runner=Path(native_record["runner"]["path"])
    extraction=json.loads(read(output/"extract-window-before.json"))
    tools=tuple(Path(row["path"]) for row in extraction["tools"])
    require(quiet(project,tools,extraction["compiler_root"])==extraction,"native/LAST-host/extraction inputs changed before SDK phase")
    retained_paths=source_files(output,["bundle-off","bundle-on"],suffixes=None)
    retained_paths += ["artifacts.json","prepared.json","native.json","extract-window-before.json","extract-window-after.json"]
    def retained():
        return retained_identities(output, retained_paths)
    before={"source_tools":quiet(project,tools,extraction["compiler_root"]),"retained":retained()};write(output/"sdk-window-before.json",before)
    first=command(output,"rust-lifecycle-attempt1",[runner,output/"artifacts.json"],target=args.native_target)
    second=command(output,"rust-lifecycle-attempt2",[runner,output/"artifacts.json"],target=args.native_target)
    require(first==second,"independent SDK reports differ; preserved both attempts")
    after={"source_tools":quiet(project,tools,extraction["compiler_root"]),"retained":retained()};write(output/"sdk-window-after.json",after)
    require(before==after,"source/tool/runner/artifacts changed through SDK repeat")
    command(output,"recorded-reader",[sys.executable,HERE/"rust_lifecycle.py","--report",output/"rust-lifecycle-attempt1.stdout","--capture",output,"--self-test"],target=args.native_target)
    write(output/"execution.json",{"status":"executed_and_repeat_byte_identical","first_report":identity(output/"rust-lifecycle-attempt1.stdout"),"repeat_report":identity(output/"rust-lifecycle-attempt2.stdout"),"source_window":"matched native/LAST-host/extraction/SDK and complete retained bundles","scope":"Finite two-mode raw Rust TAIL and canonical CAT2 SDK lifecycle observations. Repeat adds no scenario coverage. No live/Sage/corpus or authenticated execution credit."})
    print("PASS independent SDK repeat byte-identical and retained reader controls")


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("phase",choices=("prepare","native","extract","execute"));parser.add_argument("--output",type=Path,required=True);parser.add_argument("--native-target",type=Path,required=True);parser.add_argument("--compiler-target",type=Path);parser.add_argument("--compiler-root",type=Path)
    for name in ("driver","registry-cli","cli"):parser.add_argument("--"+name,type=Path)
    args=parser.parse_args();args.output=args.output.resolve();args.native_target=args.native_target.resolve()
    require(not args.output.is_relative_to(ROOT/"experiments"),"generated outputs stay outside source")
    if args.phase=="extract":
        require(all((args.compiler_target,args.compiler_root,args.driver,args.registry_cli,args.cli)),"explicit coordinated extraction tools/target required")
        args.compiler_target=args.compiler_target.resolve();args.compiler_root=args.compiler_root.resolve()
        for name in ("driver","registry_cli","cli"):setattr(args,name,getattr(args,name).resolve())
    globals()[args.phase](args)

if __name__=="__main__":main()
