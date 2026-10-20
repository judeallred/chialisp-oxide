#!/usr/bin/env python3
"""Check pinned port evidence from each port's manifest and an independent expectation.

Default validates every registered port (scripts/port_oracles.py) and every
manifest that declares `expected` outcomes per case. --self-test runs the
generic report/manifest/source corruption controls; --replay re-executes the
retained artifacts through the pinned VM CLI. Port-specific extras (resource
controls, factory relations, ledger composition) keep their dedicated gates.
No compiler is executed and no compiler-preservation claim is made.
"""
import argparse
import json
from pathlib import Path

from cargo_authoring import unique_object
import port_evidence as evidence
from port_oracles import PORTS

ROOT = Path(__file__).resolve().parents[1]


def declared_ports(report):
    slugs = []
    for puzzle in report["puzzles"]:
        slug = puzzle["slug"]
        if slug in PORTS:
            continue
        manifest = evidence.load_manifest(slug)
        if evidence.declared_expectations(manifest) is not None:
            slugs.append(slug)
    return slugs


def validate(report, slug, reader=evidence.read_source):
    entry = PORTS.get(slug, {})
    expected = entry["expected"] if entry else None
    generated = len(entry["generated"]()) if entry else 0
    return evidence.validate_port(report, slug, expected=expected, reader=reader, generated=generated)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--slug", action="append", help="limit to these ports (default: every registered/declared port)")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/chialisp-oxide")
    args = parser.parse_args()
    report = json.loads(args.report.read_text(), object_pairs_hook=unique_object)
    evidence.check_header(report)
    slugs = args.slug or [*PORTS, *declared_ports(report)]
    for slug in slugs:
        summary = validate(report, slug)
        print(f"PASS {slug}: {json.dumps(summary, sort_keys=True)}")
        if args.self_test:
            count = evidence.self_test_port(report, slug, lambda changed, reader, slug=slug: validate(changed, slug, reader))
            print(f"PASS {slug}: {count} generic report/manifest/source corruption controls")
        if args.replay:
            entry = PORTS.get(slug, {})
            extra = entry["generated"]() if entry else []
            total = evidence.replay_port(report, slug, args.cli.resolve(), expected=entry["expected"] if entry else None, extra_cases=extra)
            print(f"PASS {slug}: {total} retained-artifact VM replays")
    print(f"PASS: {len(slugs)} ports checked from manifest data and independent expectations; finite recorded evidence, not a preservation proof")


if __name__ == "__main__":
    main()
