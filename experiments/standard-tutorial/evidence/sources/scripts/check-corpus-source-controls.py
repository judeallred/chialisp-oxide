#!/usr/bin/env python3
"""Capture, install or check the generic corpus actual-source VM controls."""
import argparse
import json
from pathlib import Path
import shutil

import corpus_source_controls as controls


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture", type=Path, help="new capture directory under target/")
    parser.add_argument("--install", action="store_true", help="install a checked capture as the retained packet")
    parser.add_argument("--tools", type=Path, default=controls.ROOT / "target/debug")
    parser.add_argument("--replay", action="store_true", help="re-execute retained programs through the VM CLI")
    args = parser.parse_args()
    packet = controls.ROOT / controls.PACKET
    if args.capture:
        controls.capture(args.capture, tools=args.tools.resolve())
        if args.install:
            if packet.exists():
                shutil.rmtree(packet)
            shutil.copytree(args.capture.resolve(), packet)
    report = json.loads(controls.read(packet / "report.json"))
    result = controls.validate(report, packet, cli=(args.tools / "chialisp-oxide") if args.replay else None)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
