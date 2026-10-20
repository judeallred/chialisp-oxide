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


MODULE_PROMOTIONS = {
    # Typed signed 64-bit adapters of the pinned Using Modules programs. The
    # tutorial case is the page's own example input; divergences are declared
    # per case and may only cover the independently compiled languages.
    "module-add": {"solution": "(2 3)", "expected_output": "5", "expected_output_hex": "05"},
    "module-times-ten": {"solution": "(5)", "expected_output": "50", "expected_output_hex": "32"},
    "module-factorial": {"solution": "(5)", "expected_output": "120", "expected_output_hex": "78"},
    # The Currying page's module, plus its page-argument instances: each build's
    # artifact curried in the standard form, cross-checked against the SDK and
    # executed in five builds. Only the Chialisp instance equals the page result.
    "currying-multiply": {"solution": "(2 5)", "expected_output": "10", "expected_output_hex": "0a",
                          "curried": {"argument": "2", "page_result": "(a (q 18 2 5) (c (q . 2) 1))", "solution": "(5)",
                                      "expected_output": "10", "expected_output_hex": "0a"}},
}
CURRY_SCHEME = "(a (q . M) (c (q . A) 1))"
MODULE_ABI = "strict_typed_i64_list"
MODULE_CLAIM = "executed_differential_cases_with_declared_original_divergences_only"
MODULE_DOMAIN = "signed_64_bit_checked_adapter_of_unbounded_original"
DIVERGENCE_BACKENDS = {"chialisp", "rue_source"}
ALL_MODES = {("rust_mir", False), ("rust_mir", True), ("chialisp", True), ("rue_source", False), ("rue_source", True)}


def check_curried_examples(slug, puzzle, curried):
    """Page-argument instances: standard form, SDK cross-checks, five-build agreement, no divergences."""
    examples = puzzle.get("curried_examples")
    require(isinstance(examples, list) and examples, f"{slug}: missing curried-instance evidence")
    page = [example for example in examples if example["argument"] == curried["argument"]]
    require(len(page) == 1 and page[0]["page_result"] == curried["page_result"], f"{slug}: page argument must have exactly one curried example")
    for example in examples:
        require(example["scheme"] == CURRY_SCHEME and example["cases"], f"{slug}: curried example must use the standard form with executed cases")
        instances = example["instances"]
        require({(item["backend"], item["optimize_lir"]) for item in instances} == ALL_MODES and len(instances) == len(ALL_MODES),
                f"{slug}: curried instances must cover every build once")
        require(all(item["sdk_owned_curry_bytes_equal"] is True and item["sdk_hash_equal"] is True for item in instances),
                f"{slug}: SDK curry cross-checks must agree")
        chialisp = [item for item in instances if item["backend"] == "chialisp"]
        require(len(chialisp) == 1 and (chialisp[0].get("page_result_equal") is True) == ("page_result" in example)
                and all("page_result_equal" not in item for item in instances if item["backend"] != "chialisp"),
                f"{slug}: page identity may bind only the Chialisp build")
        for case in example["cases"]:
            require("declared_divergence" not in case and {(run["backend"], run["optimize_lir"]) for run in case["runs"]} == ALL_MODES
                    and all(run["execution"]["outcome"] == case["native"] for run in case["runs"]),
                    f"{slug}: curried case {case['label']!r} must agree with native Rust in every build")
    tutorial = [case for case in page[0]["cases"] if case["solution"] == curried["solution"]]
    require(len(tutorial) == 1 and tutorial[0]["native"] == {"status": "accepted", "output_hex": curried["expected_output_hex"]},
            f"{slug}: the page's curried input must produce the independently specified result")
    return {"scheme": CURRY_SCHEME, "page_argument": curried["argument"], "page_result": curried["page_result"],
            "expected_solution": curried["solution"], "expected_output": curried["expected_output"],
            "expected_output_hex": curried["expected_output_hex"], "examples": len(examples),
            "instance_cases": sum(len(example["cases"]) for example in examples),
            "claim_scope": "executed_curried_instance_cases_in_five_builds_only",
            "instance_hash_identity": "chialisp_build_equals_page_result_only"}


def check_module_promotion(row, excerpt, report, root):
    """Promote one exact complete module span through a declared typed adapter."""
    slug = row["id"].split(":", 1)[1]
    contract = MODULE_PROMOTIONS[slug]
    require(row["kind"] == "module" and row["implementation_status"] == "implemented"
            and row["validation_status"] == "differential_only", f"{slug}: unsupported promotion status")
    require(not any(key in row for key in ("validated", "passed", "executed")), f"{slug}: unsupported execution claim")
    paths = {"rust_source": f"puzzles/{slug}/puzzle.rs", "reference": f"puzzles/{slug}/reference.clsp",
             "rue_source": f"puzzles/{slug}/puzzle.rue", "manifest": f"puzzles/{slug}/puzzle.json"}
    require(row["local_examples"] == [paths["rust_source"], paths["reference"], paths["rue_source"]]
            and row["evidence"] == ["corpus/validation.json"], f"{slug}: missing exact local source/evidence bindings")
    payloads = {key: confined_file(root, path).read_bytes() for key, path in paths.items()}
    require(payloads["reference"] == excerpt.encode(), f"{slug}: reference must be the complete fenced module span, without a wrapper")
    manifest = load(root / paths["manifest"])
    selected = [item for item in report["puzzles"] if item["slug"] == slug]
    require(len(selected) == 1, f"{slug}: no unique executed corpus row")
    puzzle = selected[0]
    require(puzzle["validated"] is True and puzzle["kind"] == "pure_function" and puzzle["simulator"] == []
            and puzzle["generated_cases"] == 0, f"{slug}: expected pure-function evidence without ledger or generated-sweep claims")
    require(manifest["slug"] == slug and manifest["kind"] == puzzle["kind"]
            and manifest["environment"] == puzzle.get("environment", "strict") == "strict"
            and manifest["reference_mode"] == puzzle.get("reference_mode", "module") == "module",
            f"{slug}: promotion requires the default strict typed ABI and original-module compilation")
    provenance = puzzle["provenance"]
    require(provenance == manifest["provenance"] and provenance["classification"] == "canonical_tutorial_port"
            and provenance["example_id"] == row["id"], f"{slug}: canonical example identity mismatch")
    require(provenance["references"] == [row["url"], row["source"]["url"]], f"{slug}: canonical page and pinned span references mismatch")
    require(provenance["upstream"] == {
        "repository": REPOSITORY, "revision": REVISION, "source_path": row["source"]["path"],
        "start_line": row["source"]["start_line"], "end_line": row["source"]["end_line"],
    }, f"{slug}: upstream source identity/span mismatch")
    require(provenance["source_adaptation"] == {
        "kind": "extract_complete_chialisp_module", "source_span_sha256": row["source"]["sha256"],
        "reference_sha256": sha256(payloads["reference"]),
        "description": "Copy the complete fenced module source lines exactly, including their comments and original LF endings.",
    }, f"{slug}: manifest does not bind the exact complete-module extraction")
    require(isinstance(provenance["limitations"], list) and len(provenance["limitations"]) >= 4
            and all(isinstance(item, str) and item for item in provenance["limitations"]), f"{slug}: missing explicit adapter limitations")
    require(puzzle["source"].encode() == payloads["rust_source"] and puzzle["reference"].encode() == payloads["reference"]
            and puzzle["rue"]["path"] == paths["rue_source"] and puzzle["rue"]["source"].encode() == payloads["rue_source"],
            f"{slug}: execution report sources are stale")
    recorded = [{key: case[key] for key in ("label", "solution", "declared_divergence") if key in case} for case in puzzle["cases"]]
    require(manifest["cases"] == recorded, f"{slug}: execution cases differ from the current manifest")
    expected = {"status": "accepted", "output_hex": contract["expected_output_hex"]}
    tutorial = divergent = rejected_everywhere = 0
    for case in puzzle["cases"]:
        require({(run["backend"], run["optimize_lir"]) for run in case["runs"]} == ALL_MODES,
                f"{slug}: missing independent reference or optimization setting")
        divergence = case.get("declared_divergence")
        if divergence is not None:
            require(set(divergence) == {"backends", "outcome", "reason"} and set(divergence["backends"]) <= DIVERGENCE_BACKENDS
                    and divergence["backends"] and divergence["outcome"] != case["native"] and divergence["reason"],
                    f"{slug}: declared divergence may only bind the independently compiled languages")
            divergent += 1
        for run in case["runs"]:
            bound = divergence["outcome"] if divergence is not None and run["backend"] in divergence["backends"] else case["native"]
            require(run["execution"]["outcome"] == bound, f"{slug}: compiled result differs from its bound expectation")
        if case["solution"] == contract["solution"]:
            require(case["label"].startswith("tutorial") and case["native"] == expected and divergence is None,
                    f"{slug}: the tutorial input must produce the independently specified page result everywhere")
            tutorial += 1
        if divergence is None and case["native"] == {"status": "rejected"}:
            rejected_everywhere += 1
    require(tutorial == 1 and divergent >= 1 and rejected_everywhere >= 1,
            f"{slug}: promotion needs the page input, at least one declared divergence and one shared rejection")
    curried = contract.get("curried")
    if curried is None:
        require("curried_examples" not in puzzle, f"{slug}: unexpected curried-instance evidence")
        curried_claim = {}
    else:
        curried_claim = {"curried_instances": check_curried_examples(slug, puzzle, curried)}
    adaptation = {"operation": "extract_complete_chialisp_module", "source_span_sha256": row["source"]["sha256"],
                  "report": "corpus/validation.json", "report_sha256": sha256((root / "corpus/validation.json").read_bytes()),
                  "slug": slug}
    for key, path in paths.items():
        adaptation[key] = path
        adaptation[key + "_sha256"] = sha256(payloads[key])
    require(row["source_adaptation"] == adaptation, f"{slug}: stale or changed complete-module extraction/source/evidence binding")
    require(row["validation_contract"] == {
        "abi": MODULE_ABI, "environment": "strict", "reference_mode": "module",
        "expected_solution": contract["solution"], "expected_output": contract["expected_output"],
        "expected_output_hex": contract["expected_output_hex"], "integer_domain": MODULE_DOMAIN,
        "declared_divergences": divergent, "claim_scope": MODULE_CLAIM,
        "ledger": "not_applicable_pure_function", "compiler_preservation": "not_claimed", **curried_claim,
    }, f"{slug}: changed ABI/result/proof-scope claim")


def check_promotion(row, excerpt, report, root):
    """Bind only explicitly reviewed examples to actual source/report bytes."""
    if row["id"] == "chialisp-example:password-coin":
        return check_password_promotion(row, excerpt, report, root)
    slug = row["id"].split(":", 1)[1]
    if slug in MODULE_PROMOTIONS:
        return check_module_promotion(row, excerpt, report, root)
    require(slug in EXPECTATIONS, "Only the three closed Intro expressions and the reviewed module ports have a promotion contract")
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
                == {("rust_mir", False), ("rust_mir", True), ("chialisp", True), ("rue_source", False), ("rue_source", True)},
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


def check_password_promotion(row, excerpt, report, root):
    """Promote the exact module span, not its page or a secure coin design."""
    from password_evidence import validate as check_password_evidence
    from password_provenance import SPAN_SHA

    require(row["id"] == "chialisp-example:password-coin" and row["kind"] == "puzzle"
            and row["implementation_status"] == "implemented"
            and row["validation_status"] == "differential_only", "Unreviewed password promotion state")
    require(not any(key in row for key in ("validated", "passed", "executed")), "Unsupported password execution claim")
    source = row["source"]
    require(source["path"] == "docs/chialisp-primer/first-smart-coin.md"
            and type(source["start_line"]) is int and source["start_line"] == 21
            and type(source["end_line"]) is int and source["end_line"] == 40
            and source["sha256"] == SPAN_SHA, "Password promotion is only the exact complete20-line module")
    paths = {"rust_source": "puzzles/password-coin/puzzle.rs", "reference": "puzzles/password-coin/reference.clsp",
             "rue_source": "puzzles/password-coin/puzzle.rue", "manifest": "puzzles/password-coin/puzzle.json"}
    require(row["local_examples"] == [paths["rust_source"], paths["reference"], paths["rue_source"]]
            and row["evidence"] == ["corpus/validation.json"], "Password promotion lacks exact three-source/report bindings")
    payloads = {key: confined_file(root, path).read_bytes() for key, path in paths.items()}
    require(excerpt.encode() == payloads["reference"] and sha256(payloads["reference"]) == SPAN_SHA,
            "Password reference must be the complete original module, without extraction wrappers")
    # Reuse the independently authored whole-tree/hash/curry/ledger validator;
    # do not copy the policy relation into an inventory promotion check.
    check_password_evidence(report, reader=lambda path: confined_file(root, path).read_bytes())
    selected = [p for p in report["puzzles"] if p["slug"] == "password-coin"]
    require(len(selected) == 1 and selected[0]["provenance"]["example_id"] == row["id"], "Password example/report identity mismatch")
    adaptation = {"operation": "extract_complete_chialisp_module", "source_span_sha256": SPAN_SHA,
                  "report": "corpus/validation.json", "report_sha256": sha256((root / "corpus/validation.json").read_bytes()),
                  "slug": "password-coin"}
    for key, path in paths.items():
        adaptation[key] = path
        adaptation[key + "_sha256"] = sha256(payloads[key])
    require(row["source_adaptation"] == adaptation, "Stale or changed complete-module extraction/source/evidence binding")
    contract = row["validation_contract"]
    require(type(contract.get("whole_page_completed")) is bool
            and type(contract.get("runtime_umbrella_semantic_rejection_credit")) is bool,
            "Password completion/semantic-credit flags must be exact booleans")
    require(contract == {"abi": "raw_one_tree", "environment": "raw", "reference_mode": "module",
        "claim_scope": "executed_raw_differential_cases_and_separate_fixed_instance_observations_only",
        "instance_protocol": "chialisp-oxide-password-instances/v1",
        "ledger": "separate_fixed_commitment_instance_observations_only", "hash_proof": "not_claimed",
        "compiler_preservation": "not_claimed", "whole_page_completed": False,
        "runtime_umbrella_semantic_rejection_credit": False}, "Unreviewed password ABI/proof/page/ledger claim")


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
        elif short_id in MODULE_PROMOTIONS:
            contract = MODULE_PROMOTIONS[short_id]
            require(len(observations) == 1 and observations[0]["solution"] == contract["solution"]
                    and observations[0]["expected_output"] == contract["expected_output"]
                    and observations[0]["validation_status"] == ("differential_only" if promoted else "not_run")
                    and ("linked executed differential cases" if promoted else "not an executed result") in observations[0]["basis"],
                    "Module expectations must distinguish page expectations and executed differential evidence")
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


class ModulePromotionControls(unittest.TestCase):
    def test_module_promotion_rejects_rust_bound_or_vacuous_divergences_and_stale_bindings(self):
        inventory = load(ROOT / "corpus/tutorial-examples.json")
        report = checked_report(ROOT)
        row = next(item for item in inventory["examples"] if item["id"] == "chialisp-example:module-add")
        lines = (ROOT / row["source"]["snapshot"]).read_text().splitlines(keepends=True)
        excerpt = "".join(lines[row["source"]["start_line"] - 1:row["source"]["end_line"]])
        check_module_promotion(row, excerpt, report, ROOT)
        for mutation in ("rust_bound", "vacuous", "undeclared_run", "tutorial_moved", "no_divergence", "no_shared_rejection",
                         "wrapped_reference", "environment", "count", "report_hash", "claim_scope"):
            changed, evidence = copy.deepcopy(row), copy.deepcopy(report)
            puzzle = next(item for item in evidence["puzzles"] if item["slug"] == "module-add")
            divergent = next(case for case in puzzle["cases"] if "declared_divergence" in case)
            if mutation == "rust_bound": divergent["declared_divergence"]["backends"] = ["rust_mir"]
            elif mutation == "vacuous": divergent["declared_divergence"]["outcome"] = copy.deepcopy(divergent["native"])
            elif mutation == "undeclared_run":
                run = next(run for run in divergent["runs"] if run["backend"] == "rust_mir")
                run["execution"]["outcome"] = copy.deepcopy(divergent["declared_divergence"]["outcome"])
            elif mutation == "tutorial_moved": next(case for case in puzzle["cases"] if case["label"].startswith("tutorial"))["solution"] = "(3 4)"
            elif mutation == "no_divergence":
                for case in puzzle["cases"]: case.pop("declared_divergence", None)
            elif mutation == "no_shared_rejection":
                for case in puzzle["cases"]:
                    if case["native"] == {"status": "rejected"} and "declared_divergence" not in case:
                        case["native"] = {"status": "accepted", "output_hex": "80"}
            elif mutation == "wrapped_reference": puzzle["reference"] = "(mod ENV " + puzzle["reference"] + ")\n"
            elif mutation == "environment": puzzle["environment"] = "raw"
            elif mutation == "count": changed["validation_contract"]["declared_divergences"] += 1
            elif mutation == "report_hash": changed["source_adaptation"]["report_sha256"] = "0" * 64
            else: changed["validation_contract"]["claim_scope"] = "compiler_preservation_proof"
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError, StopIteration)):
                check_module_promotion(changed, excerpt, evidence, ROOT)


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
        for identity in ("chialisp:chialisp-primer/intro", "chialisp:chialisp-primer/first-smart-coin"):
            for mutation in ("complete_page", "page_validation", "coverage"):
                changed = copy.deepcopy(original)
                page = next(item for item in changed["entries"] if item["id"] == identity)
                if mutation == "complete_page":
                    page["implementation_status"] = "implemented"
                elif mutation == "page_validation":
                    page["validation_status"] = "differential_only"
                    page["local_examples"], page["evidence"] = ["puzzles/intro-add/puzzle.rs"], ["corpus/validation.json"]
                else:
                    page["example_coverage"]["implemented_examples"].pop()
                with self.subTest(page=identity, mutation=mutation):
                    self.assertTrue(checks.validate(changed))

    def test_password_promotion_is_source_bound_and_keeps_instance_credit_separate(self):
        inventory = load(ROOT / "corpus/tutorial-examples.json")
        report = checked_report(ROOT)
        row = next(item for item in inventory["examples"] if item["id"] == "chialisp-example:password-coin")
        lines = (ROOT / row["source"]["snapshot"]).read_text().splitlines(keepends=True)
        excerpt = "".join(lines[20:40])
        check_promotion(row, excerpt, report, ROOT)
        for mutation in ("operation", "reference", "manifest_hash", "report_hash", "instance_protocol", "whole_page",
                         "numeric_page_flag", "numeric_credit_flag", "hash_proof", "compiler_proof", "fixed_hash",
                         "umbrella_credit", "numeric_umbrella_credit", "consensus_code", "ordinary_ledger", "unknown_id"):
            changed, evidence = copy.deepcopy(row), copy.deepcopy(report)
            puzzle = next(p for p in evidence["puzzles"] if p["slug"] == "password-coin")
            if mutation == "operation": changed["source_adaptation"]["operation"] = "extract_single_quoted_run_expression"
            elif mutation == "reference": puzzle["reference"] = "(mod ENV (x))\n"
            elif mutation == "manifest_hash": changed["source_adaptation"]["manifest_sha256"] = "0" * 64
            elif mutation == "report_hash": changed["source_adaptation"]["report_sha256"] = "0" * 64
            elif mutation == "instance_protocol": changed["validation_contract"]["instance_protocol"] = "unreviewed/v2"
            elif mutation == "whole_page": changed["validation_contract"]["whole_page_completed"] = True
            elif mutation == "numeric_page_flag": changed["validation_contract"]["whole_page_completed"] = 0
            elif mutation == "numeric_credit_flag": changed["validation_contract"]["runtime_umbrella_semantic_rejection_credit"] = 0
            elif mutation == "hash_proof": changed["validation_contract"]["hash_proof"] = "universal_sha_proof"
            elif mutation == "compiler_proof": changed["validation_contract"]["compiler_preservation"] = "proved"
            elif mutation == "fixed_hash": puzzle["curried_instances"]["fixed_argument"] = "80"
            elif mutation in ("umbrella_credit", "numeric_umbrella_credit"):
                puzzle["curried_instances"]["ledger"][0]["attempts"][2]["consensus"]["semantic_rejection_credit"] = True if mutation == "umbrella_credit" else 1
            elif mutation == "consensus_code": puzzle["curried_instances"]["ledger"][0]["attempts"][3]["consensus"]["code"] = "GeneratorRuntimeError"
            elif mutation == "ordinary_ledger": puzzle["simulator"] = [{"passed": True}]
            else: changed["id"] = "chialisp-example:unreviewed-password-port"
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError)):
                check_promotion(changed, excerpt, evidence, ROOT)
        with self.assertRaisesRegex(ValueError, "complete original module"):
            check_promotion(row, "(mod ENV (x))\n", report, ROOT)

    def test_password_host_workflow_is_not_promoted_by_project_instance_tests(self):
        original = load(ROOT / "corpus/tutorial-examples.json")
        for identity in ("chialisp-support:password-create-spend", "chialisp-support:password-insecurity"):
            changed = copy.deepcopy(original)
            record = next(row for row in changed["supporting_material"] if row["id"] == identity)
            record["validation_status"] = "differential_only"
            record["evidence"] = ["corpus/validation.json"]
            with self.subTest(record=identity), self.assertRaises(ValueError):
                validate(changed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, default=ROOT / "corpus/tutorial-examples.json")
    parser.add_argument("--upstream-tree", type=Path, help="optional pinned GitHub tree JSON; no network is performed")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.TestSuite([unittest.defaultTestLoader.loadTestsFromTestCase(InventoryControls),
                                    unittest.defaultTestLoader.loadTestsFromTestCase(ModulePromotionControls)])
        result = unittest.TextTestRunner().run(suite)
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
