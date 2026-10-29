#!/usr/bin/env python3
"""Execute the fixed public host recipe; no wallet, network or compiler invocation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import subprocess
import time


def run(binary, output, name, args=(), refusal=False):
    command = [str(binary), *map(str, args)]
    started = time.monotonic()
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    streams = {"stdout": bytearray(), "stderr": bytearray()}
    reader = selectors.DefaultSelector()
    for label, stream in (("stdout", process.stdout), ("stderr", process.stderr)):
        os.set_blocking(stream.fileno(), False)
        reader.register(stream, selectors.EVENT_READ, label)
    failure = None
    try:
        while reader.get_map():
            if time.monotonic() - started > 30:
                raise RuntimeError(f"{name}: process deadline (inconclusive)")
            for key, _ in reader.select(0.1):
                chunk = os.read(key.fileobj.fileno(), 65536)
                if not chunk:
                    reader.unregister(key.fileobj)
                    continue
                target = streams[key.data]
                if len(target) + len(chunk) > 1024 * 1024:
                    raise RuntimeError(f"{name}: output limit (inconclusive)")
                target.extend(chunk)
        code = process.wait(timeout=max(0.1, 30 - (time.monotonic() - started)))
    except BaseException as error:
        failure = repr(error)
        process.kill()
        process.wait(timeout=5)
        raise
    finally:
        reader.close()
        process.stdout.close()
        process.stderr.close()
        for label, content in streams.items():
            (output / f"{name}.{label}").write_bytes(content)
        receipt = {"argv": command, "exit_code": process.returncode,
                   "failure": failure, "timeout_seconds": 30,
                   "max_stream_bytes": 1024 * 1024,
                   "streams": {label: {"bytes": len(value), "sha256": hashlib.sha256(value).hexdigest()}
                               for label, value in streams.items()}}
        (output / f"{name}.command.json").write_text(json.dumps(receipt, indent=2) + "\n")
    if refusal:
        assert code == 1 and not streams["stdout"] and refusal in streams["stderr"], f"{name}: expected exact checkpoint diagnostic"
    else:
        assert code == 0, f"{name}: unexpected failure"
    return bytes(streams["stdout"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--execute", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    binary = args.execute.resolve(strict=True)
    output = args.output.absolute()
    output.mkdir(parents=True, exist_ok=False)
    assert not any(key.startswith("RUST_BIGDECIMAL_") for key in os.environ), "Clear decimal build settings"
    full = run(binary, output, "full-first")
    assert run(binary, output, "full-repeat") == full, "Independent full reports differ"
    pause = run(binary, output, "pause-first", ["--stop-after", "1"])
    checkpoint = output / "pause-first.stdout"
    assert run(binary, output, "resume-first", ["--resume", checkpoint]) == full, "Resumed report differs"
    report = json.loads(full)
    assert report["status"] == "executed_local_simulator"
    assert report["sdk_transaction_attempts"] == 20 and report["component_vm_runs"] == 48
    assert [row["batch_size"] for row in report["registration_steps"]] == [1, 2, 3]
    assert len([row for row in report["transactions"] if row["status"] == "accepted"]) == 11
    prefix = json.loads(pause)
    assert prefix["status"] == "paused_after_validated_step" and prefix["sdk_transaction_attempts"] == 7
    assert len(prefix["registration_steps"]) == 1
    cases = {}
    changed = json.loads(pause)
    changed["registration_steps"][0]["current_registry"]["coin_id"] = "00" * 32
    cases["changed-coin"] = json.dumps(changed).encode()
    changed = json.loads(pause)
    changed["untrusted_assertion"] = "accept"
    cases["unknown-field"] = json.dumps(changed).encode()
    changed = json.loads(pause)
    changed["recipe_fingerprint"] = "00" * 32
    cases["wrong-recipe"] = json.dumps(changed).encode()
    cases["oversized"] = b" " * (1024 * 1024 + 1)
    cases["malformed"] = b"{"
    cases["duplicate-root"] = b'{"x":1,"x":2}'
    cases["duplicate-nested"] = b'{"outer":{"x":1,"x":2}}'
    cases["duplicate-escaped-nested"] = b'{"outer":{"x":1,"\\u0078":2}}'
    old_path = Path(__file__).resolve().parents[2] / "corpus/xchandles-simulator/reference/old-checkpoint.json"
    assert old_path.stat().st_size <= 1024 * 1024
    cases["old-source-checkpoint"] = old_path.read_bytes()
    for name, content in cases.items():
        filename = output / f"checkpoint-{name}.json"
        filename.write_bytes(content)
        if name in ("changed-coin", "unknown-field"):
            expected = b"Error: resumed full checkpoint prefix differs from actual fresh SDK recipe"
        elif name in ("wrong-recipe", "old-source-checkpoint"):
            expected = b"Error: checkpoint protocol/status/current recipe mismatch"
        elif name == "oversized":
            expected = b"Error: bounded regular public checkpoint/source descriptor"
        elif name.startswith("duplicate-"):
            expected = b"duplicate checkpoint object key"
        else:
            expected = b"Error: invalid bounded unique-key checkpoint JSON"
        run(binary, output, f"refuse-{name}", ["--resume", filename], refusal=expected)
    fifo, alias = output / "checkpoint-fifo", output / "checkpoint-symlink"
    os.mkfifo(fifo)
    alias.symlink_to(checkpoint)
    try:
        run(binary, output, "refuse-fifo", ["--resume", fifo], refusal=b"Error: bounded regular public checkpoint/source descriptor")
        run(binary, output, "refuse-symlink", ["--resume", alias], refusal=b"Too many levels of symbolic links")
    finally:
        fifo.unlink()
        alias.unlink()
    receipt = {"protocol": "chialisp-oxide-xchandles-fresh-process-gate/v1", "status": "passed",
               "full_report_sha256": hashlib.sha256(full).hexdigest(),
               "recipe_fingerprint": report["recipe_fingerprint"],
               "full_reports": 3, "checkpoint_refusals": 11,
               "sdk_attempts_per_full_report": 20,
               "scope": "Fresh finite public host execution. Repeats and prefix replays add no distinct coverage; checkpoint refusals are infrastructure checks. Historical packets remain unchanged. No live network, execution authentication or compiler-preservation credit."}
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, sort_keys=True))


if __name__ == "__main__":
    main()
