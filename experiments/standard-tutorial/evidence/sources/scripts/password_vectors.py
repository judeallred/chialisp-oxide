"""Independent exact-byte password-module relation and bounded input recipes.

Python hashlib supplies SHA-256. This is a finite test oracle, not a hash
security theorem or a Rust/MIR/CLVM preservation proof. Trees are bytes or
ordered pairs; nil is empty bytes, never numeric zero normalization.
"""
import hashlib
import json

from hashing_vectors import CODEC, MODES, display, proper, raw_generated, require

serialize, deserialize = (CODEC[name] for name in ("serialize", "deserialize"))
SLUG = "password-coin"
MAX_COST = 1_000_000
PUBLIC_PASSWORD = b"abc"  # Deliberately public fixture data, not a secret.
PUBLIC_HASH = hashlib.sha256(PUBLIC_PASSWORD).digest()


def parse(text):
    """Read explicit trees plus the pinned disassembler's ASCII string form.

    Rue prints only graphic ASCII/spaces in quotes and escapes only quote and
    backslash. Convert those bytes to hex before the independent tree parser;
    do not run Rue's assembler to predict the expected input.
    """
    require(isinstance(text, str) and len(text) <= 2_097_152, "Password fixture text exceeds grammar bound")
    result, cursor = [], 0
    while cursor < len(text):
        if text[cursor] != '"':
            result.append(text[cursor])
            cursor += 1
            continue
        cursor += 1
        atom = bytearray()
        while cursor < len(text) and text[cursor] != '"':
            char = text[cursor]
            cursor += 1
            if char == "\\":
                require(cursor < len(text) and text[cursor] in ('"', "\\"), "Unsupported ASCII fixture string escape")
                char = text[cursor]
                cursor += 1
            require(32 <= ord(char) <= 126, "Fixture quoted atoms require graphic ASCII/spaces")
            atom.append(ord(char))
        require(cursor < len(text), "Unterminated quoted fixture atom")
        cursor += 1
        result.append(" 0x" + atom.hex() + " ")
    return CODEC["parse"]("".join(result))


def relation(tree, *, normalize=False, strict_tail=False, require_conditions_list=False):
    """Return the third field iff exact atom SHA comparison succeeds.

    This predicts conclusive behavior with enough resources. It does not model
    VM cost or convert resource failure to a policy rejection.
    """
    fields = []
    for _ in range(3):
        if not isinstance(tree, tuple) or len(tree) != 2:
            return {"status": "rejected"}
        fields.append(tree[0])
        tree = tree[1]
    expected_hash, password, conditions = fields
    if not isinstance(expected_hash, bytes) or not isinstance(password, bytes):
        return {"status": "rejected"}
    if normalize:
        password = password.lstrip(b"\0")
        expected_hash = expected_hash.lstrip(b"\0")
    if hashlib.sha256(password).digest() != expected_hash:
        return {"status": "rejected"}
    if strict_tail and tree != b"":
        return {"status": "rejected"}
    if require_conditions_list:
        remainder = conditions
        while isinstance(remainder, tuple):
            remainder = remainder[1]
        if remainder != b"":
            return {"status": "rejected"}
    return {"status": "accepted", "output_hex": serialize(conditions).hex()}


def expected(solution, **mutations):
    return relation(parse(solution), **mutations)


def curry_tree(module, argument):
    """Independent materialized C(M,H) = (a (q . M) (c (q . H) 1))."""
    return proper(b"\x02", (b"\x01", module), proper(b"\x04", (b"\x01", argument), b"\x01"))


def native_environment(argument, solution):
    return argument, solution


def conditions(destination=b"B" * 32, amount=b"\x03\xe8"):
    return proper(proper(b"\x33", destination, amount))


def named_trees():
    """Reconstruct the reviewed 48 raw cases, without stored expected answers."""
    abc, xyz = PUBLIC_PASSWORD, b"xyz"
    committed, other_hash = PUBLIC_HASH, hashlib.sha256(xyz).digest()
    output = conditions()
    rows = []

    def add(label, hash_value, password, selected=output, tail=b""):
        rows.append((label, (hash_value, (password, (selected, tail)))))

    add("valid_spend", committed, abc)
    add("unsigned_retarget", committed, abc, conditions(b"C" * 32))
    add("wrong_password", committed, xyz)
    add("different_committed_hash", other_hash, xyz)
    add("invalid_pair_destination", committed, abc, conditions((b"\x01", b"\x02")))
    add("invalid_pair_amount", committed, abc, conditions(amount=(b"\x01", b"\x02")))
    add("arbitrary_condition_atom", committed, abc, b"\0")
    for index, password in enumerate([b"", b"\0", b"\0\0", b"\0\x01", b"\x80", b"\xff", bytes.fromhex("c3a9"), bytes.fromhex("65cc81")]):
        add(f"password_bytes_{index}", hashlib.sha256(password).digest(), password, b"\0\x01")
    for width in [55, 56, 63, 64, 65, 119, 120, 127, 128, 8192]:
        password = bytes((17 * index + 3) & 255 for index in range(width))
        add(f"password_width_{width}", hashlib.sha256(password).digest(), password, proper(b"\x08"))
    for label, selected in [
        ("conditions_nil", b""),
        ("conditions_program_data", proper(b"\x08")),
        ("conditions_improper", (b"\x01", b"\x02")),
        ("conditions_nested", proper(proper(b"\x01"), (b"\0\x01", b"\0"))),
    ]:
        add(label, committed, abc, selected)
    for label, tail in [("extra", proper(b"\x09")), ("zero", b"\0"), ("padded", b"\0\0"), ("pair", (b"\x01", b"\x02"))]:
        add("ignored_tail_" + label, committed, abc, b"\x01", tail)
    add("pair_password", committed, (b"\x01", b"\x02"), b"")
    add("pair_hash", (b"\x01", b"\x02"), abc, b"")
    for label, hash_value in [("short", committed[:-1]), ("long", b"\0" + committed), ("empty", b""), ("wrong", other_hash)]:
        add("hash_" + label, hash_value, abc, b"")
    rows.extend([
        ("missing_conditions_match", (committed, proper(abc))),
        ("missing_conditions_mismatch", (other_hash, proper(abc))),
    ])
    large = b"x" * 65_536
    large_hash = hashlib.sha256(large).digest()
    rows.append(("large_missing_conditions_match", (large_hash, proper(large))))
    add("large_pair_hash", (b"\x01", b"\x02"), large, b"")
    add("large_password_success", large_hash, large, b"\0\x01")
    rows.extend([
        ("bare_nil", b""), ("bare_atom", b"\0\x01"),
        ("missing_password", (committed, b"")),
        ("missing_password_improper", (committed, b"\0")),
    ])
    require(len(rows) == 48 and len({label for label, _ in rows}) == 48, "Password case recipe changed")
    return rows


def named():
    return [{"label": label, "solution": display(tree)} for label, tree in named_trees()]


def generated_for():
    return generated() + raw_generated()


def generated():
    """128 matched correct/wrong-hash pairs with byte-rich payloads and trees."""
    widths = [0, 1, 2, 3, 31, 32, 55, 56, 63, 64, 65, 119, 120, 127, 128, 129, 255, 256, 1024, 8192]
    selections = [b"", b"\0", b"\0\0", proper(b"\x08"), (b"\x01", b"\x02"),
                  proper(proper(b"data"), (b"\0\x01", b"\0"))]
    tails = [b"", b"\0", b"\0\0", proper(b"ignored"), (b"extra", b"tail")]
    result = []
    for index in range(256):
        base = index // 2
        password = bytes((131 * offset + 17 * base + 29) & 255 for offset in range(widths[base % len(widths)]))
        digest = hashlib.sha256(password).digest()
        if index % 2:
            digest = bytes([digest[0] ^ 1]) + digest[1:]
        tree = digest, (password, (selections[base % len(selections)], tails[base % len(tails)]))
        result.append({"label": f"deterministic password bytes {index}", "solution": display(tree)})
    return result


def generated_payload():
    return (json.dumps(generated(), indent=2) + "\n").encode()


def instance_cases():
    """Keep S independently from H: all cases use the single public fixed H."""
    return [{"label": "derived tail: " + label, "solution": display(tree[1] if isinstance(tree, tuple) else tree)}
            for label, tree in named_trees()]
