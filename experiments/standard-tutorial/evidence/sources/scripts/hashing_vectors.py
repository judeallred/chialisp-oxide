"""Independent byte/tree expectations and fixed input coverage for the hash ports.

The oracle is iterative Python hashlib, not either source translation or CLVM.
It predicts finite test data; it is not a preservation or collision-resistance proof.
"""
import hashlib
import json
import random
import runpy
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CODEC = runpy.run_path(str(ROOT / "scripts/check-canonical-factories.py"))
parse, serialize, deserialize = (CODEC[name] for name in ("parse", "serialize", "deserialize"))
SLUGS = ("sha256tree", "singleton-launcher")
MODES = [("rust_mir", False), ("rust_mir", True), ("chialisp", True),
         ("rue_source", False), ("rue_source", True)]
GENERATED_SHA256 = "dd2246fee1017cc089e804c7a055f75106c10929a652d7d915b49ca1277f2fb8"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def proper(*items):
    result = b""
    for item in reversed(items):
        result = item, result
    return result


def display(tree):
    if isinstance(tree, bytes):
        return "0x" + tree.hex() if tree else "()"
    items = []
    while isinstance(tree, tuple):
        items.append(display(tree[0]))
        tree = tree[1]
    return "(" + " ".join(items) + ("" if tree == b"" else " . " + display(tree)) + ")"


def sha_tree(tree, *, atom_domain=b"\x01", pair_domain=b"\x02", normalize=False, swap=False):
    """Postorder stack; exact atom bytes and ordered child digests are retained."""
    todo, results, work = [(tree, False)], [], 0
    while todo:
        value, visited = todo.pop()
        work += 1
        require(work <= 100_000, "Independent hash oracle work budget exceeded")
        if isinstance(value, bytes):
            payload = value.lstrip(b"\0") if normalize else value
            results.append(hashlib.sha256(atom_domain + payload).digest())
        elif not visited:
            require(isinstance(value, tuple) and len(value) == 2, "Expected a raw atom/pair")
            todo.extend([(value, True), (value[1], False), (value[0], False)])
        else:
            right, left = results.pop(), results.pop()
            results.append(hashlib.sha256(pair_domain + (right + left if swap else left + right)).digest())
    require(len(results) == 1, "Incomplete independent hash traversal")
    return results[0]


def expected(slug, solution, *, strict_tail=False, normalize=False, drop_zero=False,
             atom_domain=b"\x01", pair_domain=b"\x02", swap=False, omit_third=False):
    tree = parse(solution)
    options = dict(normalize=normalize, atom_domain=atom_domain, pair_domain=pair_domain, swap=swap)
    if slug == "sha256tree":
        output = sha_tree(tree, **options)
        if drop_zero:
            output = output.lstrip(b"\0")
    else:
        require(slug == "singleton-launcher", "Unknown hash fixture")
        fields = []
        for _ in range(3):
            if not isinstance(tree, tuple):
                return {"status": "rejected"}
            fields.append(tree[0])
            tree = tree[1]
        if strict_tail and tree != b"":
            return {"status": "rejected"}
        first, second, third = fields
        committed = proper(first, second) if omit_third else proper(first, second, third)
        output = proper(proper(b"\x33", first, second), proper(b"\x3c", sha_tree(committed, **options)))
    return {"status": "accepted", "output_hex": serialize(output).hex()}


def spine(depth, left, leaf):
    for _ in range(depth):
        leaf = (leaf, left) if left is not None else (b"", leaf)
    return leaf


def rows(entries):
    return [{"label": label, "solution": display(tree)} for label, tree in entries]


def helper_named():
    nil = b""
    entries = [
        ("nil atom", nil), ("one zero byte is not nil", b"\0"),
        ("two zero bytes preserve padding", b"\0\0"), ("one byte one", b"\x01"),
        ("padded one remains different bytes", b"\0\x01"), ("negative-looking atom", b"\xff"),
        ("padded negative-looking atom", b"\xff\xff"), ("invalid UTF-8 remains bytes", b"\xff\x80\xfe\0"),
        ("UTF-8 composed spelling", bytes.fromhex("c3a9")), ("UTF-8 decomposed spelling", bytes.fromhex("65cc81")),
        ("atom containing domain tag two", b"\x02"), ("32-byte payload with leading zeros", b"\0" * 31 + b"\x01"),
        ("leading-zero digest keeps 32-byte output", b"\0\xd9"), ("pair of nil atoms", (nil, nil)),
        ("singleton padded zero", proper(b"\0\0")), ("proper three-element list", proper(b"\x01", b"\x02", b"\x03")),
        ("improper pair", (b"\x01", b"\x02")), ("swapped improper pair", (b"\x02", b"\x01")),
        ("improper zero tail", (b"\x01", b"\0")), ("improper padded zero tail", (b"\x01", b"\0\0")),
        ("program-looking raise tree is data", proper(b"\x08")), ("program-looking apply tree is data", proper(b"\x02", b"\x01", b"\x03")),
        ("nested improper branches", ((b"\0\x01", b"\0"), (b"\xff", nil))),
        ("repeated subtree remains repeated data", (proper(b"a", b"b"), proper(b"a", b"b"))),
    ]
    for size in [54, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129]:
        entries.append((f"atom length {size} around SHA padding boundary", bytes((17 * i + 3) & 255 for i in range(size))))
    entries += [("right spine depth eight", spine(8, None, b"\0\x01")),
                ("left spine depth eight", spine(8, nil, b"\0\x01")),
                ("unequal branch depths", (spine(8, None, b"left"), spine(2, nil, b"right")))]
    return rows(entries)


def launcher_named():
    h, amount, kv, nil = b"B" * 32, b"\x03\xe8", proper(proper(b"key", b"value")), b""
    return rows([
        ("nil environment rejects", nil), ("bare zero atom rejects", b"\0"),
        ("one field cannot supply amount", proper(h)), ("two fields cannot supply key values", proper(h, amount)),
        ("improper second tail lacks third pair", (h, (amount, b"\0"))),
        ("ordinary launcher fields", proper(h, amount, kv)),
        ("extra proper fields are ignored", proper(h, amount, kv, b"ignored")),
        ("improper zero tail after third field is ignored", (h, (amount, (kv, b"\0")))),
        ("improper pair tail after third field is ignored", (h, (amount, (kv, (b"extra", b"tail"))))),
        ("nil puzzle hash is copied without type check", proper(nil, amount, kv)),
        ("pair puzzle hash is copied without type check", proper((b"a", b"b"), amount, kv)),
        ("nil amount is copied unchanged", proper(h, nil, kv)),
        ("noncanonical zero amount preserves payload", proper(h, b"\0", kv)),
        ("negative-looking amount is not normalized", proper(h, b"\xff", kv)),
        ("pair amount is copied without type check", proper(h, (b"a", b"b"), kv)),
        ("nil key values hash as a nil field", proper(h, amount, nil)),
        ("improper key-value tree is hashed structurally", proper(h, amount, (b"k", b"v"))),
        ("program-looking key values remain data", proper(h, amount, proper(b"\x08"))),
        ("padded amount changes announcement hash", proper(h, b"\0\x03\xe8", kv)),
        ("all three fields may be arbitrary pairs", proper((nil, b"\0"), (b"\xff", nil), (b"\0\x01", b"\0\0"))),
    ])


def generated():
    rng = random.Random(0x534841323536)
    def tree(depth):
        if not depth or rng.randrange(3) == 0:
            return bytes(rng.randrange(256) for _ in range(rng.randrange(8)))
        return tree(depth - 1), tree(depth - 1)
    values = [(f"deterministic raw tree {i}", tree(5)) for i in range(256)]
    for size in [0, 1, 2, 31, 32, 255, 256, 1024, 8192]:
        values.append((f"all-byte payload width {size}", bytes((i * 131 + 29) & 255 for i in range(size))))
    for depth in [1, 8, 32, 64, 128]:
        values += [(f"right spine depth {depth} with padded terminal atom", spine(depth, None, b"\0\x01")),
                   (f"left spine depth {depth} with nonempty zero leaf", spine(depth, b"", b"\0"))]
    return rows(values)


def generated_payload():
    return (json.dumps(generated(), indent=2) + "\n").encode()


def raw_generated():
    """The corpus raw-environment suite, independently decoded for replay.

    This reproduces inputs, not a policy. The 275 hash-specific rows remain a
    separate pinned file; no generated execution is inferred from that file.
    """
    atoms = ["()", "1", "-1", "0x00", "0x0001", "0x80", "18446744073709551616", "0xffffffffffffffffffffffffffffffff"]
    def environment(index, depth=0):
        if index < len(atoms) or depth == 5:
            return atoms[index % len(atoms)]
        return f"({environment(index // len(atoms), depth + 1)} . {environment(index % len(atoms), depth + 1)})"
    values = [{"label": f"corpus raw tree {i}", "solution": environment(i)} for i in range(2048)]
    for width in [0, 1, 31, 32, 63, 64, 255, 256, 4096, 8192]:
        atom = "0x" + "00" * width if width else "()"
        values.append({"label": f"corpus raw atom width {width}", "solution": f"({atom} . 0x0001)"})
    value = "0x0001"
    for _ in range(128):
        value = f"({value} . 0x00)"
    values.append({"label": "corpus raw left spine depth 129", "solution": f"({value} . 0x0000)"})
    return values


def named(slug):
    require(slug in SLUGS, "Unknown hashing puzzle")
    return helper_named() if slug == SLUGS[0] else launcher_named()


def generated_for(slug):
    return (generated() if slug == SLUGS[0] else []) + raw_generated()


VALIDATION_BUDGETS = {"sha256tree": 10_000_000, "singleton-launcher": 1_000_000}
RESOURCE_BUDGET = 1_000_000


def validation_budget(slug):
    require(slug in VALIDATION_BUDGETS, "Unknown hashing comparison budget")
    return VALIDATION_BUDGETS[slug]


def resource_cases():
    """Closed union of all observed one-million-cost witnesses; no data removed."""
    selected = {row["label"]: row for row in generated()}
    labels = ["right spine depth 64 with padded terminal atom", "left spine depth 64 with nonempty zero leaf",
              "right spine depth 128 with padded terminal atom", "left spine depth 128 with nonempty zero leaf"]
    return [selected[label] for label in labels] + [raw_generated()[-1]]


def should_exhaust(backend, optimize, case_index):
    require((backend, optimize) in MODES and 0 <= case_index < 5, "Unknown resource control mode/input")
    return backend == "rust_mir" and (not optimize or case_index >= 2)
