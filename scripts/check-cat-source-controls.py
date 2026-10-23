#!/usr/bin/env python3
"""Install a fresh host capture or check a portable recorded CAT packet."""
import argparse
import json
from pathlib import Path
from cat_source_controls import install, validate, self_test, recorded_packet_paths

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--packet", type=Path, default=Path(__file__).resolve().parents[1] / "corpus/cat-genesis-source-controls")
parser.add_argument("--install-capture", type=Path)
parser.add_argument("--list-json", action="store_true")
parser.add_argument("--self-test", action="store_true")
args = parser.parse_args()
result = install(args.install_capture, args.packet) if args.install_capture else validate(args.packet)
if args.self_test: result["constructed_corruption_controls"] = self_test(args.packet)
if args.list_json: result = {"files": list(recorded_packet_paths(args.packet))}
print(json.dumps(result, indent=2))
