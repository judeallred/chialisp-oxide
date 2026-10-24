"""Local bounded process/log helpers, adapted from the reviewed CAT capture.
Source-only preparation; subprocess execution occurs only through explicit phases.
"""
import sys
sys.dont_write_bytecode = True
import hashlib
import json
import math
import os
from pathlib import Path
import re
import selectors
import signal
import stat
import subprocess
import time
ROOT = Path(__file__).resolve().parents[2]
MAX_FILE = 16 * 1024 * 1024
MAX_RETAINED_BYTES = 128 * 1024 * 1024
MAX_RETAINED_FILES = 4096
COMMAND_TIMEOUT = 600
COMMAND_CLEANUP_TIMEOUT = 5

def same_json(actual, expected):
    """JSON equality that preserves integer/float/boolean distinctions."""
    return json.dumps(actual, sort_keys=True, separators=(",", ":"), allow_nan=False) == json.dumps(expected, sort_keys=True, separators=(",", ":"), allow_nan=False)

def require(condition, message):
    if not condition:
        raise ValueError(message)

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
    remaining = MAX_FILE - 1
    with path.open("xb") as target:
        for chunk in json.JSONEncoder(indent=2, ensure_ascii=True).iterencode(value):
            require(len(chunk) <= remaining, "serialized record exceeds bounded output")
            remaining -= len(chunk)
            target.write(chunk.encode("ascii"))
        target.write(b"\n")


def command(output, label, argv, *, target, env_extra=None, timeout=COMMAND_TIMEOUT, maximum=MAX_FILE):
    require(os.name == "posix", "reviewed command supervision requires POSIX process groups")
    require(type(label) is str and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,80}", label), "closed command log label")
    require(type(maximum) is int and 0 < maximum <= MAX_FILE, "command stream bound")
    require(type(timeout) in (int, float) and math.isfinite(timeout) and 0 < timeout <= COMMAND_TIMEOUT, "command lifetime bound")
    environment = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="2", CARGO_NET_OFFLINE="true", PYTHONDONTWRITEBYTECODE="1")
    for name in ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTC_BOOTSTRAP", "CHIALISP_OXIDE_DRIVER"):
        environment.pop(name, None)
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
        "rustc": environment.get("RUSTC"), "driver": environment.get("CHIALISP_OXIDE_DRIVER"),
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



def check_receipt(value, argv, *, target, rustc, driver=None):
    """Exact role/profile checks, separately from stream identity admission."""
    expected={'argv':[str(v) for v in argv],'cwd':str(ROOT),'target':str(target),
              'jobs':2,'offline':True,'rustc':str(rustc),'driver':str(driver) if driver else None,
              'exit_code':0,'status':'passed','failure_kind':None,'failure_detail':None,
              'timeout_seconds':COMMAND_TIMEOUT,'cleanup_timeout_seconds':COMMAND_CLEANUP_TIMEOUT,
              'max_stream_bytes':MAX_FILE}
    require(set(value)==set(expected)|{'stdout','stderr'},'exact command receipt fields')
    for key,want in expected.items():
        require(type(value[key]) is type(want) and value[key]==want,'exact command role/profile: '+key)


def recorded_command(output, label, argv, *, target, rustc, driver=None):
    value=json.loads(read(output/(label+'.command.json')))
    check_receipt(value,argv,target=target,rustc=rustc,driver=driver)
    for stream in ('stdout','stderr'):
        require(same_json(value[stream],identity(output/(label+'.'+stream))),'exact command stream identity')
    return value
