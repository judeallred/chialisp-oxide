#!/usr/bin/env python3
"""Check retained source/file consistency; this is not execution authentication.

Actual replay is a separate opt-in command through an explicitly selected CLI.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

base = Path(__file__).resolve().parent
packet = base / "evidence"
receipt = json.loads((packet / "receipt.json").read_text())
if receipt["schema"] != 1:
    raise SystemExit("unsupported receipt")
for root, records in ((base,receipt["source_files"]),(packet,receipt["files"])):
    for name, digest in records.items():
        path = root/name
        if Path(name).is_absolute() or ".." in Path(name).parts or path.is_symlink() or not path.is_file():
            raise SystemExit(f"nonregular or escaped retained file {name}")
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise SystemExit(f"stale retained file {name}")
actual={str(p.relative_to(packet)) for p in packet.rglob("*") if p.is_file()}
if actual != set(receipt["files"]) | {"receipt.json"}:
    raise SystemExit("retained file inventory mismatch")
for mode in ("on","off"):
    manifest=json.loads((packet/f"rust-{mode}"/"manifest.json").read_text())
    if manifest["schema_version"] != 2 or len(manifest["entries"]) != 1:
        raise SystemExit("one complete source-aware export required")
    if manifest["entries"][0]["entry"] != "chialisp_oxide_authenticated_continuations::step":
        raise SystemExit("wrong retained export")
    if manifest["driver_sha256"] != receipt["tool_files"]["chialisp-oxide-driver"]:
        raise SystemExit("retained driver relation mismatch")
parser=argparse.ArgumentParser()
parser.add_argument("--replay-cli",type=Path)
parser.add_argument("--output",type=Path)
args=parser.parse_args()
if bool(args.replay_cli) != bool(args.output):
    raise SystemExit("replay requires both --replay-cli and new --output")
if args.replay_cli:
    paths={mode:next((packet/f"rust-{mode}").glob("*.clvm.hex")) for mode in ("on","off")}
    subprocess.run([sys.executable,"-B",str(base/"replay.py"),"--cli",str(args.replay_cli),
                    "--rust-on",str(paths["on"]),"--rust-off",str(paths["off"]),
                    "--reference",str(packet/"reference.clvm.hex"),"--output",str(args.output)],check=True)
    fresh=json.loads((args.output/"report.json").read_text())
    if fresh != json.loads((packet/"report.json").read_text()):
        raise SystemExit("fresh retained-program replay differs")
print("retained source/file consistency passed; reader alone adds no execution credit")
