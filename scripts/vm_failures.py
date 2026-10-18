"""Validate typed CLVM failures without interpreting human-readable diagnostics.

Native policy results have a separate wire shape. Historical reports may omit VM
failure metadata only when explicitly requested by a baseline comparator; current
proof and validation evidence must always use the shared, exhaustive protocol.
"""

import hashlib
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
PROTOCOL = "clvmr-0.21.0/v1"
PROTOCOL_PATH = "crates/chialisp-oxide-vm/failure-protocol.json"
HEX = re.compile(r"(?:[0-9a-f]{2})+")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load_protocol():
    table = json.loads((ROOT / PROTOCOL_PATH).read_text())
    require(set(table) == {"schema_version", "protocol", "failures"}
            and type(table["schema_version"]) is int and table["schema_version"] == 1
            and table["protocol"] == PROTOCOL, "Unsupported VM failure protocol")
    mapping = {}
    for row in table["failures"]:
        require(set(row) == {"kind", "code"} and row["kind"] in ("program", "resource", "infrastructure")
                and isinstance(row["code"], str) and re.fullmatch(r"[a-z][a-z0-9]*(?:_[a-z0-9]+)*", row["code"])
                and row["code"] not in mapping, "Malformed VM failure table")
        mapping[row["code"]] = row["kind"]
    require(bool(mapping), "Empty VM failure table")
    return mapping


FAILURES = load_protocol()


def check_failure(value, label="VM failure"):
    require(isinstance(value, dict) and set(value) == {"kind", "code"}, f"{label}: missing/malformed typed failure metadata")
    require(isinstance(value["code"], str) and value["code"] in FAILURES,
            f"{label}: unknown VM failure code")
    require(value["kind"] == FAILURES[value["code"]], f"{label}: inconsistent VM failure kind/code")
    return value["kind"]


def check_native_outcome(value, label="native policy"):
    require(isinstance(value, dict), f"{label}: malformed native outcome")
    status = value.get("status")
    if status == "accepted":
        require(set(value) == {"status", "output_hex"} and isinstance(value["output_hex"], str)
                and HEX.fullmatch(value["output_hex"]), f"{label}: malformed accepted native output")
    else:
        require(status == "rejected" and set(value) == {"status"}, f"{label}: malformed native rejection")
    return value


def check_execution(value, label="VM execution", *, require_conclusive=True, require_typed=True):
    require(isinstance(value, dict) and {"outcome", "cost", "error"} <= value.keys()
            and value.keys() <= {"outcome", "cost", "error", "failure"}, f"{label}: malformed execution envelope")
    result = value["outcome"]
    require(isinstance(result, dict), f"{label}: malformed VM outcome")
    status = result.get("status")
    if status == "accepted":
        check_native_outcome(result, label)
        require(type(value["cost"]) is int and value["cost"] > 0 and value["error"] is None
                and "failure" not in value, f"{label}: inconsistent successful execution metadata")
    else:
        require(status in ("rejected", "inconclusive") and set(result) == {"status"}, f"{label}: unknown VM outcome")
        require(value["cost"] is None and isinstance(value["error"], str) and value["error"],
                f"{label}: malformed VM failure diagnostics/cost")
        if "failure" not in value and not require_typed:
            require(status == "rejected", f"{label}: historical inconclusive outcome lacks classification")
        else:
            kind = check_failure(value.get("failure"), label)
            require((status == "rejected") == (kind == "program"),
                    f"{label}: resource/infrastructure failure cannot be a program rejection")
        require(not require_conclusive or status != "inconclusive", f"{label}: inconclusive VM execution is not validation evidence")
    return result


def check_replay(value, label="VM replay", *, expected_code=None):
    require(isinstance(value, dict), f"{label}: malformed replay")
    status = value.get("status")
    if status == "accepted":
        require(set(value) == {"status", "output_hex", "cost"}, f"{label}: inconsistent accepted replay fields")
        check_native_outcome({key: value[key] for key in ("status", "output_hex")}, label)
        require(type(value["cost"]) is int and value["cost"] > 0, f"{label}: invalid replay cost")
        result = {"status": "accepted", "output_hex": value["output_hex"]}
    else:
        require(status == "rejected" and set(value) == {"status", "error", "failure"}
                and isinstance(value["error"], str) and value["error"], f"{label}: missing/malformed rejected replay")
        require(check_failure(value["failure"], label) == "program", f"{label}: non-program failure is not a replay rejection")
        if expected_code is not None:
            require(value["failure"]["code"] == expected_code, f"{label}: expected program failure code {expected_code}")
        result = {"status": "rejected"}
    return result


def execution_paths():
    """Closed wire/ledger and current aggregate-reader dependency paths."""
    paths = [PROTOCOL_PATH, "scripts/vm_failures.py", "www/scripts/vm-failures.mjs",
             "crates/chialisp-oxide-simulator/rejection-protocol.json",
             "crates/chialisp-oxide-simulator/Cargo.toml", "crates/chialisp-oxide-simulator/src/lib.rs",
             "scripts/ledger_rejections.py", "www/scripts/ledger-rejections.mjs",
             "crates/chialisp-oxide-vm/Cargo.toml",
             "crates/chialisp-oxide-simulator/p2-singleton-rejection-protocol.json", "www/scripts/p2-protocol.mjs",
             "scripts/p2_singleton_evidence.py", "scripts/p2_append_migration.py", "scripts/check-p2-singleton.py",
             "scripts/password_evidence.py", "scripts/password_ledger.py", "scripts/password_provenance.py",
             "scripts/password_vectors.py", "scripts/hashing_vectors.py", "scripts/check-canonical-factories.py",
             "scripts/cargo_authoring.py", "scripts/cargo_bundle.py", "scripts/check-readable-puzzles.py",
             "scripts/check-hashing-puzzles.py", "scripts/hashing_provenance.py",
             "corpus/validate_published_puzzles.py",
             "scripts/runtime_cost_migration.py", "scripts/runtime_inline_migration.py",
             "scripts/runtime_refinement_migration.py", "scripts/runtime_password_migration.py",
             "corpus/cost-optimization-baseline.json", "corpus/helper-inlining-baseline.json",
             "corpus/runtime-refinement-baseline.json", "corpus/p2-singleton-append-baseline.json",
             "puzzles/p2-singleton/vectors.py", "puzzles/p2-singleton/vectors.json",
             "puzzles/p2-singleton/generated-cases.json"]
    from p2_singleton_evidence import RUNTIME_INPUTS
    paths += list(RUNTIME_INPUTS)
    paths += [str(path.relative_to(ROOT)) for path in (ROOT / "crates/chialisp-oxide-vm/src").rglob("*.rs")]
    return sorted(set(paths))


def execution_sources():
    return [{"path": path, "sha256": hashlib.sha256((ROOT / path).read_bytes()).hexdigest()}
            for path in execution_paths()]


class FailureProtocolTests(unittest.TestCase):
    def test_every_code_kind_and_envelope(self):
        for code, kind in FAILURES.items():
            value = {"outcome": {"status": "rejected" if kind == "program" else "inconclusive"},
                     "cost": None, "error": "arbitrary human diagnostic", "failure": {"kind": kind, "code": code}}
            with self.subTest(code=code):
                check_execution(value, require_conclusive=False)
                if kind != "program":
                    with self.assertRaises(ValueError): check_execution(value)

    def test_forged_missing_unknown_and_success_metadata(self):
        rejected = {"outcome": {"status": "rejected"}, "cost": None, "error": "Raise(spoofed)",
                    "failure": {"kind": "program", "code": "raise"}}
        import copy
        for mutation in ("missing", "unknown", "resource", "infrastructure", "forged", "null", "cost"):
            bad = copy.deepcopy(rejected)
            if mutation == "missing": del bad["failure"]
            elif mutation == "unknown": bad["failure"]["code"] = "future_code"
            elif mutation == "resource": bad["failure"] = {"kind": "resource", "code": "cost_exceeded"}
            elif mutation == "infrastructure": bad["failure"] = {"kind": "infrastructure", "code": "internal_error"}
            elif mutation == "forged": bad["failure"]["code"] = "cost_exceeded"
            elif mutation == "null": bad["failure"] = None
            else: bad["cost"] = 1
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): check_execution(bad)
        success = {"outcome": {"status": "accepted", "output_hex": "80"}, "cost": 1, "error": None}
        check_execution(success)
        for failure in (None, {"kind": "program", "code": "raise"}):
            with self.assertRaises(ValueError): check_execution({**success, "failure": failure})
        historical = {key: value for key, value in rejected.items() if key != "failure"}
        check_execution(historical, require_typed=False)
        with self.assertRaises(ValueError): check_execution(historical)

    def test_replay_and_native_are_distinct(self):
        check_native_outcome({"status": "rejected"})
        with self.assertRaises(ValueError): check_replay({"status": "rejected"})
        for code, kind in FAILURES.items():
            replay = {"status": "rejected", "error": "Raise(spoofed)", "failure": {"kind": kind, "code": code}}
            if kind == "program": check_replay(replay)
            else:
                with self.assertRaises(ValueError): check_replay(replay)
        with self.assertRaises(ValueError):
            check_replay({"status": "rejected", "error": "Raise(spoofed)", "failure": {"kind": "program", "code": "path_into_atom"}}, expected_code="raise")


if __name__ == "__main__":
    unittest.main()
