#!/usr/bin/env python3
"""Offline checks for the pinned tutorial program inventory and source snapshot.

This validates provenance and recorded execution claim scope; it does not run
the compiler or VM. An optional
--upstream-tree file checks against a manually downloaded pinned GitHub tree;
the default command never accesses the network. See inventory.reproduction.
"""

import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
REVISION = "dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8"
TREE = "438fe8ece6587e9d96b40e2a1c03f953d89cf150"
REPOSITORY = "https://github.com/Chia-Network/chialisp-web"
SNAPSHOT = f"corpus/upstream/chialisp-web/{REVISION}"
PROGRAM_IDS = {
    "intro-list", "intro-add", "intro-conditional", "module-add", "module-times-ten",
    "module-factorial", "password-coin", "currying-multiply", "inner-require-signature",
    "inner-any-conditions", "morph-coin-doubler", "morph-any-with-signature",
}
EXPECTATIONS = {"intro-list": "(1 2 3)", "intro-add": "5", "intro-conditional": "100"}
EXPECTED_HEX = {"intro-list": "ff01ff02ff0380", "intro-add": "05", "intro-conditional": "64"}
# These are malformed only for the default strict ABI. The three closed
# expressions explicitly ignore their environment, including these shapes.
IGNORED_ENVIRONMENTS = {"()", "1", "(1 2 3)", "(1 . 2)", "((1) . 2)", "0x0001", "-1", "18446744073709551616"}

sys.path.insert(0, str(ROOT / "scripts"))
from cargo_authoring import check_recorded  # noqa: E402


def require(condition, message):
    if not condition:
        raise ValueError(message)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def load(path):
    return json.loads(path.read_text(), object_pairs_hook=unique_object)


def confined_file(root, relative):
    require(isinstance(relative, str), "Expected a relative snapshot path")
    path = Path(relative)
    require(not path.is_absolute() and path.parts and ".." not in path.parts
            and path.as_posix() == relative, f"Invalid snapshot path: {relative}")
    resolved = (root / path).resolve()
    require(resolved.is_relative_to(root.resolve()) and resolved.is_file(),
            f"Missing snapshot or path escapes repository: {relative}")
    return resolved


def sha256(payload):
    return hashlib.sha256(payload).hexdigest()


def git_blob(payload):
    return hashlib.sha1(b"blob " + str(len(payload)).encode() + b"\0" + payload).hexdigest()


def check_status(record, implementation=True):
    label = record["id"]
    if implementation:
        require(record["implementation_status"] == "not_started", f"{label}: implementation claim requires a reviewed inventory schema update")
        require(record["local_examples"] == [], f"{label}: no local port is registered in this snapshot")
    require(record["validation_status"] == "not_run" and record["evidence"] == [],
            f"{label}: no executed migration evidence is registered")
    require(not any(key in record for key in ("validated", "passed", "executed")),
            f"{label}: unsupported execution claim")


def checked_report(root):
    """Reuse the corpus freshness gate without copying its source/ABI checks."""
    path = root / "corpus/validation.json"
    report = load(path)
    spec = importlib.util.spec_from_file_location("readable_corpus_checks", ROOT / "scripts/check-readable-puzzles.py")
    checks = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checks)
    checks.check_report(report)
    checks.check_sources(report, root)
    check_recorded(report.get("authoring"), root, [row["slug"] for row in report["puzzles"]])
    return report


def check_promotion(row, excerpt, report, root):
    """Bind only the reviewed closed expressions to actual source/report bytes."""
    slug = row["id"].split(":", 1)[1]
    require(slug in EXPECTATIONS, "Only the three closed Intro expressions have a reviewed promotion contract")
    require(row["kind"] == "closed_expression" and row["implementation_status"] == "implemented"
            and row["validation_status"] == "differential_only", f"{slug}: unsupported promotion status")
    require(not any(key in row for key in ("validated", "passed", "executed")), f"{slug}: unsupported execution claim")
    reference_path = f"puzzles/{slug}/reference.clsp"
    rust_path, manifest_path = f"puzzles/{slug}/puzzle.rs", f"puzzles/{slug}/puzzle.json"
    require(row["local_examples"] == [rust_path, reference_path]
            and row["evidence"] == ["corpus/validation.json"], f"{slug}: missing exact local source/evidence bindings")
    match = re.fullmatch(r"run '(\(.+\))'\n?", excerpt)
    require(match is not None, f"{slug}: expected one exact pinned run command")
    extracted = (match[1] + "\n").encode()
    reference = confined_file(root, reference_path).read_bytes()
    rust = confined_file(root, rust_path).read_bytes()
    manifest_bytes = confined_file(root, manifest_path).read_bytes()
    require(reference == extracted, f"{slug}: reference must be the exact extracted expression plus LF, without a source wrapper")
    manifest = load(root / manifest_path)
    selected = [item for item in report["puzzles"] if item["slug"] == slug]
    require(len(selected) == 1, f"{slug}: no unique executed corpus row")
    puzzle = selected[0]
    require(puzzle["validated"] is True and puzzle["kind"] == "pure_function"
            and puzzle["simulator"] == [] and type(puzzle["generated_cases"]) is int
            and puzzle["generated_cases"] == 256,
            f"{slug}: expected pure-function evidence with 256 generated environments and no ledger claims")
    require(manifest["slug"] == slug and manifest["kind"] == puzzle["kind"]
            and manifest["environment"] == puzzle["environment"] == "ignored"
            and manifest["reference_mode"] == puzzle["reference_mode"] == "expression",
            f"{slug}: promotion requires explicit ignored nullary ABI and original-expression compilation")
    provenance = puzzle["provenance"]
    require(provenance == manifest["provenance"]
            and provenance["classification"] == "canonical_tutorial_port"
            and provenance["example_id"] == row["id"], f"{slug}: canonical example identity mismatch")
    require(provenance["references"] == [row["url"], row["source"]["url"]],
            f"{slug}: canonical page and pinned snippet references mismatch")
    require(provenance["upstream"] == {
        "repository": REPOSITORY, "revision": REVISION, "source_path": row["source"]["path"],
        "start_line": row["source"]["start_line"], "end_line": row["source"]["end_line"],
    }, f"{slug}: upstream source identity/span mismatch")
    require(provenance["source_adaptation"] == {
        "kind": "extract_run_expression", "source_span_sha256": row["source"]["sha256"],
        "reference_sha256": sha256(reference),
        "description": "Remove only the shell run command and quotes; preserve expression text and append LF.",
    }, f"{slug}: manifest does not bind the exact source extraction")
    require(puzzle["source"].encode() == rust and puzzle["reference"].encode() == reference,
            f"{slug}: execution report source/reference is stale")
    require(manifest["cases"] == [{"label": case["label"], "solution": case["solution"]} for case in puzzle["cases"]],
            f"{slug}: execution cases differ from the current manifest")
    require(IGNORED_ENVIRONMENTS <= {case["solution"] for case in puzzle["cases"]},
            f"{slug}: missing ignored-environment adversarial observations")
    expected = {"status": "accepted", "output_hex": EXPECTED_HEX[slug]}
    for case in puzzle["cases"]:
        require(case["native"] == expected, f"{slug}: native result differs from the independent source expectation")
        require({(run["backend"], run["optimize_lir"]) for run in case["runs"]}
                == {("rust_mir", False), ("rust_mir", True), ("chialisp", True)},
                f"{slug}: missing independent reference or optimization setting")
        require(all(run["execution"]["outcome"] == expected for run in case["runs"]),
                f"{slug}: compiled result differs from the independent source expectation")
    require(row["source_adaptation"] == {
        "operation": "extract_single_quoted_run_expression", "source_span_sha256": row["source"]["sha256"],
        "reference": reference_path, "reference_sha256": sha256(reference),
        "rust_source": rust_path, "rust_source_sha256": sha256(rust),
        "manifest": manifest_path, "manifest_sha256": sha256(manifest_bytes),
        "report": "corpus/validation.json", "report_sha256": sha256((root / "corpus/validation.json").read_bytes()),
        "slug": slug,
    }, f"{slug}: stale or changed source-adaptation evidence")
    require(row["validation_contract"] == {
        "abi": "ignored_environment_nullary", "environment": "ignored", "reference_mode": "expression",
        "expected_output": EXPECTATIONS[slug], "expected_output_hex": EXPECTED_HEX[slug],
        "claim_scope": "executed_differential_cases_only", "ledger": "not_applicable_pure_function",
    }, f"{slug}: changed ABI/result/proof-scope claim")


def validate(inventory, root=ROOT, upstream_tree=None):
    require(type(inventory["schema_version"]) is int and inventory["schema_version"] == 1,
            "Unsupported tutorial inventory schema")
    require(inventory["upstream"] == {"repository": REPOSITORY, "revision": REVISION, "git_tree_sha1": TREE},
            "Tutorial revision/repository/tree pin changed without review")
    require(inventory["snapshot_root"] == SNAPSHOT, "Unexpected pinned snapshot root")
    index = load(root / "corpus/inventory.json")
    parents = {entry["id"]: entry for entry in index["entries"] if entry["kind"] == "tutorial"}
    pages = inventory["pages"]
    require(len(pages) == 8 and len({page["id"] for page in pages}) == 8, "Expected eight unique tutorial pages")
    page_by_id, page_by_path, blobs, expected_files = {}, {}, {}, set()
    for page in pages:
        identity, source = page["id"], page["source"]
        require(identity in parents, f"Unknown parent-page reference: {identity}")
        parent = parents[identity]
        require(page["title"] == parent["title"] and page["url"] == parent["url"], f"Parent-page identity mismatch: {identity}")
        require(parent["source"]["repository"] == REPOSITORY and parent["source"]["revision"] == REVISION
                and source["path"] == parent["source"]["path"], f"Parent-page source mismatch: {identity}")
        require(source["snapshot"] == f"{SNAPSHOT}/{source['path']}", "Snapshot path does not match upstream path")
        payload = confined_file(root, source["snapshot"]).read_bytes()
        require(sha256(payload) == source["sha256"] == parent["source"]["sha256"], f"Page SHA256 mismatch: {identity}")
        require(git_blob(payload) == source["git_blob_sha1"], f"Page Git blob mismatch: {identity}")
        require(source["path"] not in page_by_path, "Duplicate tutorial source path")
        page_by_id[identity], page_by_path[source["path"]] = page, (page, payload)
        blobs[source["path"]] = source["git_blob_sha1"]
        expected_files.add(source["snapshot"])
    license = inventory["license"]
    require(license["spdx"] == "Apache-2.0" and license["path"] == "LICENSE"
            and license["snapshot"] == f"{SNAPSHOT}/LICENSE", "Upstream license identity/path mismatch")
    payload = confined_file(root, license["snapshot"]).read_bytes()
    require(sha256(payload) == license["sha256"] == "7db0cf1060026bf2a544ea29cf9a96fba7d3781f93a1d06d6fee18251f1c207a"
            and git_blob(payload) == license["git_blob_sha1"] == "37b84d60dd8a34d854a41e94c34eaf587648ac6c",
            "Upstream Apache-2.0 license content mismatch")
    blobs["LICENSE"] = license["git_blob_sha1"]
    expected_files.add(license["snapshot"])
    actual_files = {str(path.relative_to(root)) for path in (root / SNAPSHOT).rglob("*") if path.is_file()}
    require(actual_files == expected_files, "Snapshot must contain exactly eight pages and the upstream license")

    identifiers, covered = set(page_by_id), {identity: [] for identity in page_by_id}

    def check_span(record, page_id, representation):
        require(record["id"] not in identifiers, f"Duplicate inventory identity: {record['id']}")
        identifiers.add(record["id"])
        require(page_id in page_by_id, f"Unknown source parent: {page_id}")
        source = record["source"]
        page, data = page_by_path[source["path"]]
        require(page["id"] == page_id, f"Snippet points at another parent page: {record['id']}")
        require(source["snapshot"] == page["source"]["snapshot"]
                and source["file_sha256"] == page["source"]["sha256"]
                and source["git_blob_sha1"] == page["source"]["git_blob_sha1"], "Snippet file provenance mismatch")
        start, end = source["start_line"], source["end_line"]
        lines = data.splitlines(keepends=True)
        require(type(start) is int and type(end) is int and 1 <= start <= end <= len(lines), "Invalid inclusive snippet line span")
        excerpt = b"".join(lines[start - 1:end])
        require(sha256(excerpt) == source["sha256"], f"Snippet hash/span mismatch: {record['id']}")
        require(source["representation"] == representation, f"Snippet representation mismatch: {record['id']}")
        require(source["url"] == f"{REPOSITORY}/blob/{REVISION}/{source['path']}#L{start}-L{end}", "Snippet URL pin/span mismatch")
        if representation != "documentation":
            require(start > 1 and end < len(lines) and lines[start - 2].startswith(b"```")
                    and lines[end].startswith(b"```"), "Program span must contain one complete fenced source body")
        covered[page_id].append((start, end))
        return excerpt.decode()

    examples = inventory["examples"]
    require({row["id"] for row in examples} == {f"chialisp-example:{identity}" for identity in PROGRAM_IDS}
            and len(examples) == 12, "Distinct program inventory changed without review")
    variants, report = [], None
    for row in examples:
        require(row["kind"] in ("closed_expression", "module", "puzzle") and row["feasibility"] and row["semantic_gaps"],
                f"Missing feasibility/semantic contract: {row['id']}")
        require(row["url"] == page_by_id[row["page_id"]]["url"], "Example page URL mismatch")
        representation = "shell_command" if row["kind"] == "closed_expression" else "chialisp_source"
        excerpt = check_span(row, row["page_id"], representation)
        if representation == "shell_command":
            require(re.fullmatch(r"run '\(.+\)'\n?", excerpt), "Closed-expression source must be the pinned single run command")
        else:
            require("(mod " in excerpt, "Module source body is missing")
        promoted = row["implementation_status"] != "not_started" or row["validation_status"] != "not_run"
        if promoted:
            if report is None:
                report = checked_report(root)
            check_promotion(row, excerpt, report, root)
        else:
            check_status(row)
            require("source_adaptation" not in row and "validation_contract" not in row,
                    f"{row['id']}: unexecuted row must not register promotion evidence")
        require(row["external_includes"] == re.findall(r"\(include\s+([^\s()]+)\)", excerpt), "Include declarations do not match pinned code")
        for include in row["external_includes"]:
            dependency = inventory["external_dependencies"][include]
            require(dependency["status"] == "unresolved" and dependency["revision"] is None and dependency["sha256"] is None,
                    "Unversioned tutorial includes must not be claimed pinned")
        short_id = row["id"].split(":", 1)[1]
        observations = row["proposed_observations"]
        if short_id in EXPECTATIONS:
            require(len(observations) == 1 and observations[0]["solution"] == "()"
                    and observations[0]["expected_output"] == EXPECTATIONS[short_id]
                    and observations[0]["validation_status"] == ("differential_only" if promoted else "not_run")
                    and ("linked executed differential cases" if promoted else "not an executed result") in observations[0]["basis"],
                    "Intro expectations must distinguish source expectations and executed differential evidence")
        else:
            require(observations == [], "Additional proposed observations need review")
        for variant in row["variants"]:
            variants.append(variant)
            check_status(variant)
            require(variant["variant_of"] == row["id"] and variant["relation"] == "parameter_naming_only", "Variant parent/relation mismatch")
            check_span(variant, row["page_id"], "chialisp_source")
    require(len(variants) == 1 and variants[0]["id"] == "chialisp-example-variant:currying-multiply-uppercase"
            and variants[0]["variant_of"] == "chialisp-example:currying-multiply", "Expected one naming variant, not an additional program")
    for page in pages:
        require(page["program_ids"] == [row["id"] for row in examples if row["page_id"] == page["id"]], "Page/program membership mismatch")
    for row in inventory["supporting_material"]:
        require(row["id"].startswith("chialisp-support:") and row["kind"] and row["notes"], "Malformed supporting workflow record")
        check_status(row, implementation=False)
        check_span(row, row["page_id"], "documentation")
    for page_id, page in page_by_id.items():
        lines = page_by_path[page["source"]["path"]][1].decode().splitlines()
        opening = None
        for number, line in enumerate(lines, 1):
            if line.startswith("```"):
                if opening is None:
                    opening = number + 1
                else:
                    require(any(start <= opening and number - 1 <= end for start, end in covered[page_id]),
                            f"Unclassified fenced tutorial material: {page_id}:{opening}")
                    opening = None
        require(opening is None, f"Unclosed source code fence: {page_id}")
    require(inventory["scope"]["program_count"] == 12 and inventory["scope"]["variant_count"] == 1,
            "Program/variant totals must not count workflows or compiled outputs")
    initial = [f"chialisp-example:{identity}" for identity in EXPECTATIONS]
    require(inventory.get("initial_port_slice", inventory["recommended_next"]) == initial,
            "Initial port slice must remain the three closed Intro expressions")
    require(inventory["recommended_next"] == ([] if report is not None else initial),
            "A subsequent recommended port slice requires an explicit reviewed semantic contract")
    if upstream_tree is not None:
        require(upstream_tree["sha"] == TREE and not upstream_tree.get("truncated", False), "Wrong or truncated upstream Git tree")
        upstream = {entry["path"]: entry for entry in upstream_tree["tree"]}
        for path, blob in blobs.items():
            require(upstream[path]["type"] == "blob" and upstream[path]["sha"] == blob,
                    f"Upstream tree/blob identity mismatch: {path}")


class InventoryControls(unittest.TestCase):
    def test_provenance_scope_and_claim_mutations_are_rejected(self):
        original = load(ROOT / "corpus/tutorial-examples.json")
        validate(original)
        for mutation in ("span", "snippet_hash", "revision", "status", "license", "path", "parent", "duplicate"):
            changed = copy.deepcopy(original)
            if mutation == "span":
                changed["examples"][0]["source"]["start_line"] += 1
            elif mutation == "snippet_hash":
                changed["examples"][0]["source"]["sha256"] = "0" * 64
            elif mutation == "revision":
                changed["upstream"]["revision"] = "0" * 40
            elif mutation == "status":
                changed["examples"][0]["validation_status"] = "simulator_checked"
            elif mutation == "license":
                changed["license"]["sha256"] = "0" * 64
            elif mutation == "path":
                changed["pages"][0]["source"]["snapshot"] = "../outside.md"
            elif mutation == "parent":
                changed["examples"][0]["page_id"] = "chialisp:missing"
            else:
                changed["examples"].append(copy.deepcopy(changed["examples"][0]))
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError)):
                validate(changed)

    def test_promoted_source_policy_and_execution_bindings_reject_mutations(self):
        inventory = load(ROOT / "corpus/tutorial-examples.json")
        report = checked_report(ROOT)
        row = next(item for item in inventory["examples"] if item["id"] == "chialisp-example:intro-add")
        lines = (ROOT / row["source"]["snapshot"]).read_text().splitlines(keepends=True)
        excerpt = "".join(lines[row["source"]["start_line"] - 1:row["source"]["end_line"]])
        check_promotion(row, excerpt, report, ROOT)
        for mutation in ("source", "reference", "reference_mode", "environment", "upstream", "references",
                         "native", "vm", "missing_mode", "missing_environment", "generated", "ledger",
                         "snippet", "manifest_hash", "report_hash", "result_contract", "claim_scope"):
            changed, evidence = copy.deepcopy(row), copy.deepcopy(report)
            puzzle = next(item for item in evidence["puzzles"] if item["slug"] == "intro-add")
            if mutation == "source":
                puzzle["source"] += "// stale source\n"
            elif mutation == "reference":
                puzzle["reference"] = "(mod () (+ 2 3))\n"
            elif mutation == "reference_mode":
                puzzle["reference_mode"] = "module"
            elif mutation == "environment":
                puzzle["environment"] = "strict"
            elif mutation == "upstream":
                puzzle["provenance"]["upstream"]["revision"] = "0" * 40
            elif mutation == "references":
                puzzle["provenance"]["references"] = [row["url"]]
            elif mutation == "native":
                puzzle["cases"][0]["native"]["output_hex"] = "06"
            elif mutation == "vm":
                puzzle["cases"][0]["runs"][0]["execution"]["outcome"]["output_hex"] = "06"
            elif mutation == "missing_mode":
                puzzle["cases"][0]["runs"].pop()
            elif mutation == "missing_environment":
                puzzle["cases"].pop()
            elif mutation == "generated":
                puzzle["generated_cases"] = 0
            elif mutation == "ledger":
                puzzle["simulator"] = [{"passed": True}]
            elif mutation == "snippet":
                changed["source"]["sha256"] = "0" * 64
            elif mutation == "manifest_hash":
                changed["source_adaptation"]["manifest_sha256"] = "0" * 64
            elif mutation == "report_hash":
                changed["source_adaptation"]["report_sha256"] = "0" * 64
            elif mutation == "result_contract":
                changed["validation_contract"]["expected_output_hex"] = "06"
            else:
                changed["validation_contract"]["claim_scope"] = "compiler_preservation_proof"
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError)):
                check_promotion(changed, excerpt, evidence, ROOT)
        with self.assertRaisesRegex(ValueError, "exact extracted expression"):
            check_promotion(row, "run '(mod () (+ 2 3))'\n", report, ROOT)

    def test_individual_results_do_not_promote_the_parent_tutorial(self):
        spec = importlib.util.spec_from_file_location("page_inventory_checks", ROOT / "corpus/validate_inventory.py")
        checks = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(checks)
        original = load(ROOT / "corpus/inventory.json")
        self.assertEqual(checks.validate(original), [])
        for mutation in ("complete_page", "page_validation", "coverage"):
            changed = copy.deepcopy(original)
            page = next(item for item in changed["entries"] if item["id"] == "chialisp:chialisp-primer/intro")
            if mutation == "complete_page":
                page["implementation_status"] = "implemented"
            elif mutation == "page_validation":
                page["validation_status"] = "differential_only"
                page["local_examples"], page["evidence"] = ["puzzles/intro-add/puzzle.rs"], ["corpus/validation.json"]
            else:
                page["example_coverage"]["implemented_examples"].pop()
            with self.subTest(mutation=mutation):
                self.assertTrue(checks.validate(changed))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, default=ROOT / "corpus/tutorial-examples.json")
    parser.add_argument("--upstream-tree", type=Path, help="optional pinned GitHub tree JSON; no network is performed")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(InventoryControls))
        return 0 if result.wasSuccessful() else 1
    try:
        inventory = load(args.inventory)
        validate(inventory, upstream_tree=load(args.upstream_tree) if args.upstream_tree else None)
    except (ValueError, KeyError, TypeError, OSError) as error:
        parser.exit(1, f"Tutorial inventory invalid: {error}\n")
    implemented = sum(row["implementation_status"] == "implemented" for row in inventory["examples"])
    print(f"PASS: 8 pinned pages + Apache-2.0 license; {implemented}/12 implemented programs, 1 naming variant, "
          f"{len(inventory['supporting_material'])} unexecuted supporting records; offline provenance/claim checks")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
