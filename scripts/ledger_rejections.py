"""Check recorded consensus observations, separately from the CLVM 0.21 protocol.

The closed simulator table describes the expected consensus rejection(s) for
one named scenario grouping. SDK signing/preflight errors are never ledger
observations. An old baseline may omit observations; current evidence may not.
"""

import copy
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
PROTOCOL = "chia-consensus-0.36.1/v1"
PROTOCOL_PATH = "crates/chialisp-oxide-simulator/rejection-protocol.json"
TABLE = json.loads((ROOT / PROTOCOL_PATH).read_text())


def require(condition, message):
    if not condition:
        raise ValueError(message)


require(set(TABLE) == {"schema_version", "protocol", "stage", "codes", "scenarios", "puzzles"}
        and type(TABLE["schema_version"]) is int and TABLE["schema_version"] == 1
        and TABLE["protocol"] == PROTOCOL and TABLE["stage"] == "consensus", "Unsupported ledger rejection protocol")
require(isinstance(TABLE["codes"], list) and len(TABLE["codes"]) == len(set(TABLE["codes"]))
        and all(isinstance(code, str) and re.fullmatch(r"[A-Z][A-Za-z0-9]*", code) for code in TABLE["codes"]),
        "Malformed closed consensus code set")
CODES = set(TABLE["codes"])
SCENARIOS = TABLE["scenarios"]
require(isinstance(SCENARIOS, dict) and SCENARIOS and all(
    isinstance(name, str) and name and isinstance(codes, list) and all(code in CODES for code in codes)
    for name, codes in SCENARIOS.items()), "Malformed consensus scenario map")
PUZZLES = TABLE["puzzles"]
require(isinstance(PUZZLES, dict) and PUZZLES, "Missing per-puzzle consensus contracts")
for slug, profile in PUZZLES.items():
    require(isinstance(slug, str) and re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", slug)
            and isinstance(profile, list) and profile, "Malformed per-puzzle consensus contract")
    for row in profile:
        require(isinstance(row, dict) and set(row) == {"name", "codes"}
                and row["name"] in SCENARIOS and isinstance(row["codes"], list)
                and all(code in CODES for code in row["codes"]), "Malformed per-puzzle scenario/code contract")
    names = [row["name"] for row in profile]
    require(len(names) == len(set(names)), "Duplicate per-puzzle consensus scenario")


def check_scenario(value, label="ledger scenario", *, require_typed=True, expected_codes=None):
    require(isinstance(value, dict), f"{label}: malformed scenario")
    expected_keys = {"name", "passed", "scope", "rejections"}
    if not require_typed and "rejections" not in value:
        expected_keys.remove("rejections")
    require(set(value) == expected_keys, f"{label}: missing or unexpected consensus observations")
    require(value["passed"] is True and value["scope"] == "ledger", f"{label}: unsuccessful/non-ledger scenario")
    require(isinstance(value["name"], str) and value["name"] in SCENARIOS, f"{label}: unknown scenario name")
    if "rejections" in value:
        require(isinstance(value["rejections"], list), f"{label}: malformed consensus observations")
        for row in value["rejections"]:
            require(isinstance(row, dict) and set(row) == {"stage", "code"}
                    and row["stage"] == "consensus" and isinstance(row["code"], str) and row["code"] in CODES,
                    f"{label}: unknown/non-consensus rejection")
        expected = [{"stage": "consensus", "code": code} for code in (SCENARIOS[value["name"]] if expected_codes is None else expected_codes)]
        require(value["rejections"] == expected, f"{label}: wrong or missing expected consensus rejection sequence")
    return value


def check_ledger_report(report, *, require_typed=True):
    header = report.get("simulator", {})
    if require_typed or "rejection_protocol" in header:
        require(header.get("rejection_protocol") == PROTOCOL, "Missing/unsupported ledger rejection protocol")
    for puzzle in report["puzzles"]:
        comparisons = puzzle.get("simulator_comparisons", [])
        require(isinstance(comparisons, list) and all(isinstance(row, dict)
                and isinstance(row.get("backend"), str) and type(row.get("optimize_lir")) is bool
                for row in comparisons), f"{puzzle['slug']}: malformed ledger comparison modes")
        scenarios = puzzle["simulator"]
        require(isinstance(scenarios, list), "Malformed ledger scenario list")
        names = [row.get("name") for row in scenarios]
        require(len(names) == len(set(names)), "Duplicate ledger scenario grouping")
        if require_typed:
            kind = puzzle.get("kind")
            require(kind in ("coin_puzzle", "pure_function", "program_factory"), "Unknown puzzle kind for ledger contract")
            if kind == "coin_puzzle":
                require(puzzle["slug"] in PUZZLES, f"{puzzle['slug']}: no reviewed ledger scenario contract")
                require(names == [row["name"] for row in PUZZLES[puzzle["slug"]]], f"{puzzle['slug']}: missing, extra or reordered required ledger scenarios")
                comparisons = puzzle.get("simulator_comparisons", [])
                require([(row["backend"], row["optimize_lir"]) for row in comparisons]
                        == [("chialisp", True), ("rue_source", True)],
                        f"{puzzle['slug']}: missing required language ledger scenario runs")
            else:
                require(not scenarios and not puzzle.get("simulator_comparisons", []),
                        f"{puzzle['slug']}: non-spend function claims ledger observations")
        expected_codes = {row["name"]: row["codes"] for row in PUZZLES.get(puzzle["slug"], [])}
        for scenario in scenarios:
            check_scenario(scenario, f"{puzzle['slug']}/Rust", require_typed=require_typed,
                           expected_codes=expected_codes.get(scenario["name"]))
        for comparison in puzzle.get("simulator_comparisons", []):
            for scenario in comparison["scenarios"]:
                check_scenario(scenario, f"{puzzle['slug']}/{comparison['backend']}", require_typed=require_typed,
                               expected_codes=expected_codes.get(scenario["name"]))
            require(comparison["scenarios"] == scenarios, f"{puzzle['slug']}: consensus observations differ across languages")


class LedgerProtocolTests(unittest.TestCase):
    def test_mode_and_passed_flags_reject_numeric_boolean_substitutes(self):
        scenarios = [{"name": row["name"], "passed": True, "scope": "ledger",
                      "rejections": [{"stage": "consensus", "code": code} for code in row["codes"]]}
                     for row in PUZZLES["fixed-payment"]]
        original = {"simulator": {"rejection_protocol": PROTOCOL}, "puzzles": [{
            "slug": "fixed-payment", "kind": "coin_puzzle", "simulator": scenarios,
            "simulator_comparisons": [{"backend": backend, "optimize_lir": True, "scenarios": copy.deepcopy(scenarios)}
                                      for backend in ("chialisp", "rue_source")]}]}
        check_ledger_report(original)
        for value in (0, 1, 0.0, 1.0):
            for index in range(2):
                changed = copy.deepcopy(original)
                changed["puzzles"][0]["simulator_comparisons"][index]["optimize_lir"] = value
                for typed in (False, True):
                    with self.subTest(value=value, mode=index, typed=typed), self.assertRaises(ValueError):
                        check_ledger_report(changed, require_typed=typed)
            changed = copy.deepcopy(original)
            puzzle = changed["puzzles"][0]
            rows = [puzzle["simulator"], *[row["scenarios"] for row in puzzle["simulator_comparisons"]]]
            for row in rows:
                row[0]["passed"] = value
            with self.subTest(value=value, field="passed"), self.assertRaises(ValueError):
                check_ledger_report(changed)

    def test_every_named_scenario_and_historical_exemption(self):
        for name, codes in SCENARIOS.items():
            row = {"name": name, "passed": True, "scope": "ledger",
                   "rejections": [{"stage": "consensus", "code": code} for code in codes]}
            with self.subTest(name=name):
                check_scenario(row)
                old = {key: value for key, value in row.items() if key != "rejections"}
                check_scenario(old, require_typed=False)
                with self.assertRaises(ValueError): check_scenario(old)

    def test_whole_scenario_omission_and_unknown_coin_are_rejected(self):
        for slug, profile in PUZZLES.items():
            scenarios = [{"name": row["name"], "passed": True, "scope": "ledger",
                          "rejections": [{"stage": "consensus", "code": code} for code in row["codes"]]}
                         for row in profile]
            original = {"simulator": {"rejection_protocol": PROTOCOL}, "puzzles": [{
                "slug": slug, "kind": "coin_puzzle", "simulator": scenarios,
                "simulator_comparisons": [{"backend": backend, "optimize_lir": True, "scenarios": copy.deepcopy(scenarios)}
                                          for backend in ("chialisp", "rue_source")]}]}
            check_ledger_report(original)
            for index in range(len(profile)):
                bad = copy.deepcopy(original)
                puzzle = bad["puzzles"][0]
                puzzle["simulator"].pop(index)
                for comparison in puzzle["simulator_comparisons"]: comparison["scenarios"].pop(index)
                with self.subTest(slug=slug, omitted=index), self.assertRaises(ValueError): check_ledger_report(bad)
                # Historical baselines keep the explicit coverage exemption.
                check_ledger_report(bad, require_typed=False)
            bad = copy.deepcopy(original)
            bad["puzzles"][0]["simulator"].reverse()
            for comparison in bad["puzzles"][0]["simulator_comparisons"]: comparison["scenarios"].reverse()
            with self.assertRaises(ValueError): check_ledger_report(bad)
        unknown = {"simulator": {"rejection_protocol": PROTOCOL}, "puzzles": [{"slug": "new-coin-without-contract", "kind": "coin_puzzle", "simulator": [], "simulator_comparisons": []}]}
        with self.assertRaises(ValueError): check_ledger_report(unknown)
        for kind in ("pure_function", "program_factory"):
            unknown["puzzles"][0]["kind"] = kind
            check_ledger_report(unknown)
            bad = copy.deepcopy(unknown)
            bad["puzzles"][0]["simulator"] = copy.deepcopy(scenarios)
            with self.assertRaises(ValueError): check_ledger_report(bad)

    def test_boolean_wrong_amount_requires_minting_not_amount_assertion(self):
        profile = PUZZLES["boolean-gate"]
        wrong = next(row for row in profile if row["name"] == "underfunded coin rejected with rollback")
        # The fixture creates 1,000 without ASSERT_MY_AMOUNT. A 999-coin spend
        # therefore violates conservation, not a condition it never emitted.
        self.assertEqual(wrong["codes"], ["MintingCoin"])
        scenarios = [{"name": row["name"], "passed": True, "scope": "ledger",
                      "rejections": [{"stage": "consensus", "code": code} for code in row["codes"]]}
                     for row in profile]
        report = {"simulator": {"rejection_protocol": PROTOCOL}, "puzzles": [{
            "slug": "boolean-gate", "kind": "coin_puzzle", "simulator": scenarios,
            "simulator_comparisons": [{"backend": backend, "optimize_lir": True, "scenarios": copy.deepcopy(scenarios)}
                                      for backend in ("chialisp", "rue_source")]}]}
        check_ledger_report(report)
        for collection in [report["puzzles"][0]["simulator"], *[row["scenarios"] for row in report["puzzles"][0]["simulator_comparisons"]]]:
            next(row for row in collection if row["name"] == wrong["name"])["rejections"][0]["code"] = "AssertMyAmountFailed"
        with self.assertRaises(ValueError): check_ledger_report(report)

    def test_preflight_unrelated_consensus_and_unknown_codes_fail(self):
        base = {"name": "missing signature rejected with rollback", "passed": True, "scope": "ledger",
                "rejections": [{"stage": "consensus", "code": "BadAggregateSignature"}]}
        for code in ("MissingKey", "GeneratorRuntimeError", "CostExceeded", "UnknownCode", "DoubleSpend"):
            bad = copy.deepcopy(base)
            bad["rejections"][0]["code"] = code
            with self.subTest(code=code), self.assertRaises(ValueError): check_scenario(bad)
        for mutation in ("stage", "missing", "empty", "extra", "positive", "order", "unknown_name"):
            bad = copy.deepcopy(base)
            if mutation == "stage": bad["rejections"][0]["stage"] = "sdk_preflight"
            elif mutation == "missing": del bad["rejections"]
            elif mutation == "empty": bad["rejections"] = []
            elif mutation == "extra": bad["rejections"].append(copy.deepcopy(bad["rejections"][0]))
            elif mutation == "positive": bad["name"] = "valid spend produces exact recipients and total"
            elif mutation == "unknown_name": bad["name"] = "invented scenario"
            else:
                bad["name"] = "recovery requires both delay and recovery key"
                bad["rejections"] += [{"stage": "consensus", "code": "AssertSecondsAbsoluteFailed"}]
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): check_scenario(bad)


if __name__ == "__main__":
    unittest.main()
