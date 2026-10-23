#!/usr/bin/env python3
"""Independent finite raw-tree specification; no compiler, VM or SDK execution."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1] / "scripts"))
import raw_tree


class ProgramFailure(Exception):
    pass


def path(environment, number):
    value = environment
    while number > 1:
        if type(value) is not tuple:
            raise ProgramFailure("path_into_atom")
        value = value[number & 1]
        number >>= 1
    return value


def relation(environment):
    """Specify acceptance independently over the whole environment.

    Paths 47/2/45 are visible in the retained canonical artifact. Failure codes
    here are specifications awaiting actual VM capture, never recorded runs.
    """
    try:
        delta = path(environment, 47)
        if delta != b"":
            raise ProgramFailure("raise")
        genesis = path(environment, 2)
        parent = path(environment, 45)
        if type(parent) is not bytes or type(genesis) is not bytes:
            raise ProgramFailure("invalid_op_arg")
        if parent != genesis:
            raise ProgramFailure("raise")
    except ProgramFailure as error:
        return {"status": "rejected"}, str(error)
    return {"status": "accepted", "output_hex": "80"}, None


def proper(items, tail=b""):
    for item in reversed(items):
        tail = item, tail
    return tail


def environment(genesis=b"\x11" * 32, parent=b"\x11" * 32, delta=b"", *,
                truths=None, tail=None, ignored=None):
    if truths is None:
        truths = proper([b"unused-cat-struct", b"unused-my-id", parent], b"unused-coin-tail")
    if tail is None:
        tail = proper([b"unused-inner-conditions", b"unused-final-arg"])
    if ignored is None:
        ignored = [b"unused-parent-is-cat", b"unused-lineage"]
    return proper([genesis, truths, *ignored, delta], tail)


def cases():
    rows = []

    def add(name, value, why):
        expected, code = relation(value)
        row = {"name": name, "environment_hex": raw_tree.serialize(value).hex(),
               "expected": expected, "why": why}
        if code:
            row["reference_failure_specification"] = {"kind": "program", "code": code,
                "scope": "Independent specification, not an executed VM observation; Rust panic codes may differ."}
        rows.append(row)

    add("equal-32-byte-parent", environment(), "Normal nil-delta genesis check.")
    for width in (0, 1, 31, 33, 256, 257, 1024):
        value = b"\xa5" * width
        add(f"equal-arbitrary-width-{width}", environment(value, value), "Canonical equality is byte equality, not a Bytes32 ABI.")
    for label, value in (("single-zero", b"\0"), ("padded-zero", b"\0\0"), ("signed-looking", b"\xff\x80")):
        add("equal-parent-" + label, environment(value, value), "Parent/genesis atom bytes retain all padding and signs.")
    add("mismatched-parent", environment(parent=b"\x22" * 32), "Unequal atoms fail the genesis check.")
    add("nil-parent-is-not-zero-byte", environment(b"", b"\0"), "Byte equality does not normalize integers.")
    add("zero-byte-parent-is-not-padded-zero", environment(b"\0", b"\0\0"), "Padded atom payloads remain distinct.")
    for label, delta in (("one", b"\1"), ("zero-byte", b"\0"), ("padded-zero", b"\0\0"),
                         ("signed-looking", b"\xff"), ("pair", (b"", b""))):
        add("non-nil-delta-" + label, environment(delta=delta), "Only the nil atom takes the equality branch.")
    add("five-fields-no-final-arguments", environment(tail=b""), "Unused final arguments are not required by compiled paths.")
    add("improper-unused-environment-tail", environment(tail=b"\xff\0"), "No exact tuple or proper-tail decoder may be introduced.")
    add("arbitrary-unused-values", environment(truths=proper([(b"", b"\xff"), (b"\0", b"\1"), b"\x11" * 32], (b"\0", b"\xff")),
        ignored=[(b"", b"\xff"), b"\xff\0"], tail=(b"\xff", b"\0")), "Unused Truths fields, arguments and tails remain opaque trees.")
    for label, truths in (("nil", b""), ("atom", b"\xff"), ("one-cell", (b"", b"")),
                          ("two-cells", proper([b"", b""])), ("improper-second-tail", (b"", (b"", b"\xff")))):
        add("malformed-truths-" + label, environment(truths=truths), "The needed parent projection must exist, irrespective of unused fields.")
    add("parent-is-pair", environment(parent=(b"", b"")), "CLVM equality requires scalar operands after projection.")
    add("genesis-is-pair", environment(genesis=(b"", b"")), "The genesis operand must be an atom, with no width restriction.")
    add("both-equality-operands-pairs", environment(genesis=(b"", b""), parent=(b"", b"")), "Pair structure is not structural equality here.")
    add("delta-guard-precedes-malformed-truths", environment(genesis=(b"", b""), truths=b"", delta=b"\0"), "Non-nil delta raises before either equality operand or malformed Truths is inspected.")
    add("parent-projection-precedes-genesis-atom-check", environment(genesis=(b"", b""), truths=b""), "Both equality arguments are evaluated before scalar validation: malformed parent path fails first.")
    for label, value in (("nil", b""), ("atom", b"\xff"), ("one-cell", (b"", b"")),
                         ("missing-delta", proper([b"", b"", b"", b""])),
                         ("improper-before-delta", proper([b"", b"", b"", b""], b"\xff"))):
        add("environment-" + label, value, "The delta projection fails before the policy's equality branch.")
    return rows


def document():
    return {"protocol": "chialisp-oxide-cat-genesis-tail-vectors/v1", "status": "specified_not_executed",
            "source_role": "raw whole-environment translation of unchanged canonical genesis_by_coin_id",
            "scope": "Finite independent native/reference specifications only; no compiler, VM, CAT issuance, complete spend, source authenticity or compiler-preservation claim.",
            "cases": cases()}


def check_provenance():
    record = json.loads((HERE / "provenance.json").read_text())
    assert record["revision"] == "d122456feeef55708d354a065623037bfa010714"
    assert record["root_tree_sha1"] == "89e5f55c5afdde2c436b9a2c08b8e30ecf11eb62"
    assert record["license"] == "Apache-2.0"
    for row in record["files"]:
        payload = (HERE / row["retained_path"]).read_bytes()
        assert len(payload) == row["bytes"]
        assert hashlib.sha256(payload).hexdigest() == row["sha256"]
        assert hashlib.sha1(f"blob {len(payload)}\0".encode() + payload).hexdigest() == row["git_blob_sha1"]
    raw = bytes.fromhex((HERE / "upstream/genesis_by_coin_id.clsp.hex").read_text().strip())
    assert len(raw) == 45
    assert raw_tree.tree_hash(raw_tree.deserialize(raw)).hex() == record["canonical_artifact"]["module_tree_hash"] == "493afb89eed93ab86741b2aa61b8f5de495d33ff9b781dfc8919e602b2afa150"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="write intentional deterministic specifications, without execution credit")
    options = parser.parse_args()
    check_provenance()
    encoded = (json.dumps(document(), indent=2) + "\n").encode()
    vector_path = HERE / "vectors.json"
    if options.write:
        vector_path.write_bytes(encoded)
    assert vector_path.read_bytes() == encoded, "stale independent vector specifications"
    rows = document()["cases"]
    assert len({row["name"] for row in rows}) == len(rows)
    for row in rows:
        value = raw_tree.deserialize(bytes.fromhex(row["environment_hex"]))
        assert relation(value)[0] == row["expected"]
    print(f"PASS exact retained provenance and {len(rows)} independent vector specifications; no compiler/VM/SDK execution")
