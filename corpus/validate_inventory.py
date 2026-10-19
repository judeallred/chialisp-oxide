#!/usr/bin/env python3
"""Check provenance and prevent unsupported coverage claims in the inventory."""

from collections import Counter
import json
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
IMPLEMENTATION_STATES = {"planned", "in_progress", "implemented", "excluded"}
VALIDATION_STATES = {"not_run", "differential_only", "simulator_checked"}


def validate(inventory: dict, root: Path = ROOT) -> list[str]:
    errors = []
    if type(inventory.get("schema_version")) is not int or inventory["schema_version"] != 1:
        errors.append("unsupported schema_version")
    entries = inventory.get("entries", [])
    if not entries:
        errors.append("inventory has no entries")
    ids = Counter(entry.get("id") for entry in entries)
    errors.extend(f"duplicate id: {key}" for key, count in ids.items() if count > 1)
    example_inventory_path = root / "corpus/tutorial-examples.json"
    examples = json.loads(example_inventory_path.read_text())["examples"] if example_inventory_path.is_file() else []
    for entry in entries:
        identity = entry.get("id", "<missing id>")
        source = entry.get("source", {})
        if not re.fullmatch(r"[a-f0-9]{40}", source.get("revision", "")):
            errors.append(f"{identity}: source revision must be an immutable Git commit")
        if not source.get("repository", "").startswith("https://github.com/"):
            errors.append(f"{identity}: missing upstream repository")
        if not source.get("path") or not source.get("license"):
            errors.append(f"{identity}: missing upstream path or source license")
        implementation = entry.get("implementation_status")
        validation = entry.get("validation_status")
        if implementation not in IMPLEMENTATION_STATES:
            errors.append(f"{identity}: unknown implementation_status {implementation}")
        if validation not in VALIDATION_STATES:
            errors.append(f"{identity}: unknown validation_status {validation}")
        if validation != "not_run":
            if implementation != "implemented":
                errors.append(f"{identity}: validation requires an implemented example")
            if not entry.get("local_examples") or not entry.get("evidence"):
                errors.append(f"{identity}: validation requires source and evidence paths")
        for filename in entry.get("local_examples", []) + entry.get("evidence", []):
            path = (root / filename).resolve()
            if not path.is_relative_to(root) or not path.is_file():
                errors.append(f"{identity}: invalid local source/evidence path: {filename}")
        for reference in entry.get("puzzle_references", []):
            url = reference.get("url", "")
            pinned = bool(re.search(r"/blob/[a-f0-9]{40}/", url))
            if reference.get("revision_pinned") != pinned:
                errors.append(f"{identity}: incorrect pin claim for {url}")
        promoted = [row["id"] for row in examples if row["page_id"] == identity
                    and row["implementation_status"] == "implemented"]
        if promoted or "example_coverage" in entry:
            # A source span and project-specific host tests do not complete
            # surrounding installation, wallet, funding or RPC workflows.
            reviewed = {
                "chialisp:chialisp-primer/intro": {
                    "examples": ["chialisp-example:intro-list", "chialisp-example:intro-add", "chialisp-example:intro-conditional"],
                    "scope": "individual_closed_expressions_only",
                },
                "chialisp:chialisp-primer/first-smart-coin": {
                    "examples": ["chialisp-example:password-coin"],
                    "scope": "password_module_and_separate_instances_only",
                },
                "chialisp:chialisp-primer/using-modules": {
                    "examples": ["chialisp-example:module-add", "chialisp-example:module-times-ten", "chialisp-example:module-factorial"],
                    "scope": "individual_typed_module_adapters_only",
                },
            }
            contract = reviewed.get(identity)
            if contract is None or promoted != contract["examples"]:
                errors.append(f"{identity}: individual-example coverage changed without a reviewed page contract")
            if implementation != "in_progress" or validation != "not_run" or entry.get("local_examples") or entry.get("evidence"):
                errors.append(f"{identity}: partial example coverage must not promote the complete page")
            if entry.get("example_coverage") != {
                "inventory": "corpus/tutorial-examples.json", "implemented_examples": promoted,
                "scope": contract["scope"] if contract is not None else None,
            }:
                errors.append(f"{identity}: missing or stale individual-example coverage links")
    return errors


def main() -> int:
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "corpus/inventory.json"
    inventory = json.loads(path.read_text())
    errors = validate(inventory)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    entries = inventory["entries"]
    categories = Counter(entry["kind"] for entry in entries)
    checked = sum(entry["validation_status"] != "not_run" for entry in entries)
    print(f"{len(entries)} source entries; {checked} with migration evidence")
    print(", ".join(f"{count} {kind}" for kind, count in sorted(categories.items())))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
