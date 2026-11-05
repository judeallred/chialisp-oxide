"""Independent finite expected recipes and mathematical expected relation.

No Rust/Rue compiler or CLVM VM is called. Opaque prehash leaves deliberately
retain arbitrary byte widths because the original legacy helper validates none.
This model is not a hash security or compiler-preservation proof.
"""
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path


@dataclass(frozen=True)
class Prehashed:
    payload: bytes


def proper(*items):
    result = b""
    for item in reversed(items):
        result = item, result
    return result


def tree_hash(tree):
    if isinstance(tree, Prehashed):
        return tree.payload
    if isinstance(tree, bytes):
        return hashlib.sha256(b"\x01" + tree).digest()
    first, rest = tree
    return hashlib.sha256(b"\x02" + tree_hash(first) + tree_hash(rest)).digest()


def curry(module, arguments):
    environment = b"\x01"
    for argument in reversed(arguments):
        environment = proper(b"\x04", (b"\x01", argument), environment)
    return proper(b"\x02", (b"\x01", module), environment)


def expected(tree):
    fields = []
    for _ in range(5):
        if not isinstance(tree, tuple):
            return {"status": "rejected"}
        fields.append(tree[0])
        tree = tree[1]
    module_hash, launcher_id, launcher_puzzle_hash, inner_hash, my_id = fields
    if not all(isinstance(value, bytes) for value in [module_hash, inner_hash, my_id]):
        return {"status": "rejected"}
    singleton_struct = module_hash, (launcher_id, launcher_puzzle_hash)
    # Construct the standard program SHAPE instead of copying the source's
    # expanded hash-update formulas. Supplied digests are opaque hash leaves.
    full_hash = tree_hash(curry(Prehashed(module_hash), [singleton_struct, Prehashed(inner_hash)]))
    announcement_name = hashlib.sha256(full_hash + my_id).digest()
    output = proper(proper(b"\x3f", announcement_name), proper(b"\x3c", b"$"), proper(b"\x46", my_id))
    return {"status": "accepted", "output_hex": serialize(output).hex()}


def serialize(tree):
    if isinstance(tree, tuple):
        return b"\xff" + serialize(tree[0]) + serialize(tree[1])
    size = len(tree)
    if size == 0:
        return b"\x80"
    if size == 1 and tree[0] < 0x80:
        return tree
    for limit, prefix, width in [(0x40, 0x80, 1), (0x2000, 0xc0, 2), (0x100000, 0xe0, 3), (0x8000000, 0xf0, 4)]:
        if size < limit:
            return bytes([prefix | (size >> (8 * (width - 1)))]) + size.to_bytes(width, "big")[1:] + tree
    raise ValueError("research atom exceeds codec bound")


def deserialize(data):
    def read(offset):
        first = data[offset]
        offset += 1
        if first == 0xff:
            left, offset = read(offset)
            right, offset = read(offset)
            return (left, right), offset
        if first < 0x80:
            return bytes([first]), offset
        if first < 0xc0:
            size, width = first & 0x3f, 1
        elif first < 0xe0:
            size, width = first & 0x1f, 2
        elif first < 0xf0:
            size, width = first & 0x0f, 3
        elif first < 0xf8:
            size, width = first & 7, 4
        else:
            raise ValueError("unsupported research serialization prefix")
        for _ in range(width - 1):
            size = (size << 8) | data[offset]
            offset += 1
        end = offset + size
        if end > len(data):
            raise ValueError("truncated atom")
        return data[offset:end], end
    tree, end = read(0)
    if end != len(data):
        raise ValueError("trailing bytes")
    return tree


def display(tree):
    if isinstance(tree, bytes):
        return "()" if tree == b"" else "0x" + tree.hex()
    return "(" + display(tree[0]) + " . " + display(tree[1]) + ")"


def environment(fields, tail=b""):
    result = tail
    for value in reversed(fields):
        result = value, result
    return result


def named():
    base = [bytes([tag]) * 32 for tag in [0x11, 0x22, 0x33, 0x44, 0x55]]
    rows = [("ordinary five32-byte fields", proper(*base)), ("nil hash fields remain valid raw atoms", proper(*([b""] * 5)))]
    for index, name in [(0, "module prehash"), (3, "inner prehash"), (4, "coin id")]:
        for width in [0, 31, 33, 8192]:
            fields = base.copy()
            fields[index] = bytes((17 * j + index) & 255 for j in range(width))
            rows.append((f"{name} width{width} is not narrowed", proper(*fields)))
        for payload in [b"\0", b"\0\0", b"\0\x01"]:
            fields = base.copy()
            fields[index] = payload
            rows.append((f"{name} preserves atom{payload.hex()}", proper(*fields)))
    trees = [b"", b"\0", (b"\x01", b"\x02"), proper(b"\x08"), (proper(b"\0\x01"), b"\0\0")]
    for index, name in [(1, "launcher id"), (2, "launcher puzzle hash")]:
        for ordinal, value in enumerate(trees):
            fields = base.copy()
            fields[index] = value
            rows.append((f"{name} arbitrary tree{ordinal}", proper(*fields)))
    for label, tail in [("nonemptyzero", b"\0"), ("padding", b"\0\0"), ("extra field", proper(b"ignored")), ("program data", (b"\x08", b"\0"))]:
        rows.append(("ignored tail " + label, environment(base, tail)))
    for count in range(5):
        rows.append((f"missing field{count + 1}", proper(*base[:count])))
    for index, name in [(0, "module prehash"), (3, "inner prehash"), (4, "coin id")]:
        fields = base.copy()
        fields[index] = (b"\x01", b"\x02")
        rows.append((name + " pair rejects", proper(*fields)))
    for index, name in [(1, "launcher id"), (2, "launcher puzzle hash")]:
        nested = b"\0\x01"
        for _ in range(128):
            nested = b"\0", nested
        fields = base.copy()
        fields[index] = nested
        rows.append((name + " deep arbitrary tree", proper(*fields)))
    rows.extend([("bare nonemptyzero", b"\0"), ("truncated improper environment", environment(base[:3], b"\0\0"))])
    fields = base.copy()
    fields[4] = b"\xff"
    rows.append(("negative-looking my id stays raw bytes", proper(*fields)))
    return rows


def generated():
    widths = [0, 1, 2, 31, 32, 33, 55, 56, 63, 64, 65, 255]
    rows = []
    for index in range(128):
        fields = [bytes((17 * index + 31 * field + offset) & 255 for offset in range(widths[(index + field) % len(widths)])) for field in range(5)]
        fields[1] = fields[1], (b"\0", fields[2])
        if index % 4 == 3:
            fields[[0, 3, 4][(index // 4) % 3]] = proper(b"\x08")
        tail = [b"", b"\0", b"\0\0", proper(b"extra")][index % 4]
        rows.append((f"deterministic raw fields{index}", environment(fields, tail)))
    return rows


def records(rows):
    return [{"label": label, "solution": display(tree), "input_hex": serialize(tree).hex(),
             "expected": expected(tree)} for label, tree in rows]


def check_expected_relations():
    here = Path(__file__).resolve().parent
    binary = bytes.fromhex((here.parents[1] / "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles/p2_singleton.clsp.hex").read_text())
    tree = deserialize(binary)
    assert serialize(tree) == binary and len(binary) == 403
    assert tree_hash(tree).hex() == "40f828d8dd55603f4ff9fbf6b73271e904e69406982f4fbefae2c8dcceaf9834"
    # Independent shape model agrees with materialized trees when prehashes
    # actually come from known programs. No VM or source compiler is involved.
    module, inner = proper(b"\x01", b"module data"), proper(b"\x01", b"inner data")
    fields = [tree_hash(module), (b"\0\x01", b"\0"), proper(b"\x08"), tree_hash(inner), b"\xff"]
    singleton_struct = fields[0], (fields[1], fields[2])
    assert tree_hash(curry(Prehashed(fields[0]), [singleton_struct, Prehashed(fields[3])])) == tree_hash(curry(module, [singleton_struct, inner]))
    for _, tree in named() + generated():
        assert deserialize(serialize(tree)) == tree
    return {"named": len(named()), "generated": len(generated()), "status": "expected_relation_codec_and_hash_arithmetic_no_VM_invocation"}


if __name__ == "__main__":
    here = Path(__file__).resolve().parent
    checks = check_expected_relations()
    result = {"scope": "Independent finite expected relations; executed native/VM/ledger observations are recorded separately", "checks": checks,
              "named": records(named()), "generated": records(generated())}
    (here / "vectors.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(checks))
