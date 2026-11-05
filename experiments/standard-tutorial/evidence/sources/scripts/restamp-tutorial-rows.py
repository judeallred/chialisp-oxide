#!/usr/bin/env python3
"""Refresh existing tutorial report pins after capture; preserve every claim.

The tutorial validator must run afterward to check source identities, contracts
and executed observations. Updating these pins adds no execution evidence.
"""
import hashlib
import json
from pathlib import Path
import sys

root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd()
path = root / "corpus/tutorial-examples.json"
payload = path.read_text()
inventory = json.loads(payload)
report_sha = hashlib.sha256((root / "corpus/validation.json").read_bytes()).hexdigest()
updated = 0
for row in inventory["examples"]:
    adaptation = row.get("source_adaptation")
    if not isinstance(adaptation, dict) or "report_sha256" not in adaptation:
        continue
    if adaptation.get("report") != "corpus/validation.json":
        raise ValueError("Only current-corpus report references may be restamped")
    adaptation["report_sha256"] = report_sha
    updated += 1
if not updated:
    raise ValueError("No promoted current-corpus report references found")
indent = len(payload.splitlines()[1]) - len(payload.splitlines()[1].lstrip())
path.write_text(json.dumps(inventory, indent=indent) + "\n")
print(f"Restamped {updated} existing report references; tutorial claims unchanged")
