#!/usr/bin/env python3
"""Offline checks for the pinned tutorial program inventory and source snapshot.

This validates provenance and claim scope, not program execution. An optional
--upstream-tree file checks against a manually downloaded pinned GitHub tree;
the default command never accesses the network. See inventory.reproduction.
"""

import argparse
import copy
import hashlib
import json
from pathlib import Path
import re
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
    variants = []
    for row in examples:
        check_status(row)
        require(row["kind"] in ("closed_expression", "module", "puzzle") and row["feasibility"] and row["semantic_gaps"],
                f"Missing feasibility/semantic contract: {row['id']}")
        require(row["url"] == page_by_id[row["page_id"]]["url"], "Example page URL mismatch")
        representation = "shell_command" if row["kind"] == "closed_expression" else "chialisp_source"
        excerpt = check_span(row, row["page_id"], representation)
        if representation == "shell_command":
            require(re.fullmatch(r"run '\(.+\)'\n?", excerpt), "Closed-expression source must be the pinned single run command")
        else:
            require("(mod " in excerpt, "Module source body is missing")
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
                    and observations[0]["validation_status"] == "not_run"
                    and "not an executed result" in observations[0]["basis"], "Intro expectations must remain explicitly unexecuted")
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
    require(inventory["recommended_next"] == [f"chialisp-example:{identity}" for identity in EXPECTATIONS], "Recommended next slice must remain the three closed Intro expressions")
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
                changed["examples"][0]["validation_status"] = "differential_only"
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
    print(f"PASS: 8 pinned pages + Apache-2.0 license; 12 unimplemented programs, 1 naming variant, "
          f"{len(inventory['supporting_material'])} unexecuted supporting records; offline provenance/claim checks")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
