"""Independent raw CLVM tree codec shared by evidence gates.

Trees are Python tuples (first, rest) and byte atoms; nil is b"". This is a
small reference implementation for checking recorded evidence, not a VM.
"""
import hashlib
import re


def require(condition, message):
    if not condition:
        raise ValueError(message)


def serialize(tree):
    if isinstance(tree, tuple):
        return b"\xff" + serialize(tree[0]) + serialize(tree[1])
    require(isinstance(tree, bytes), "Expected raw atom or pair")
    size = len(tree)
    if not size:
        return b"\x80"
    if size == 1 and tree[0] < 128:
        return tree
    for prefix_size in range(1, 6):
        if size < 1 << (7 - prefix_size + 8 * (prefix_size - 1)):
            prefix = ((255 << (8 - prefix_size)) & 255) | (size >> (8 * (prefix_size - 1)))
            return bytes([prefix]) + size.to_bytes(prefix_size, "big")[1:] + tree
    raise ValueError("Atom exceeds the bounded checker codec")


def deserialize(data):
    require(len(data) <= 8_000_000, "Artifact exceeds checker bound")
    cursor = 0

    def read():
        nonlocal cursor
        require(cursor < len(data), "Truncated tree")
        head = data[cursor]
        cursor += 1
        if head == 255:
            return read(), read()
        if head < 128:
            return bytes([head])
        if head == 128:
            return b""
        prefix_size = 0
        while head & (128 >> prefix_size):
            prefix_size += 1
        require(prefix_size <= 5, "Unsupported oversized/backreference atom encoding")
        end = cursor + prefix_size - 1
        require(end <= len(data), "Truncated atom length")
        size = head & ((1 << (7 - prefix_size)) - 1)
        for byte in data[cursor:end]:
            size = (size << 8) | byte
        cursor = end
        end = cursor + size
        require(end <= len(data), "Truncated atom")
        result = data[cursor:end]
        cursor = end
        return result

    tree = read()
    require(cursor == len(data), "Trailing serialized bytes")
    return tree


def tree_hash(tree):
    if isinstance(tree, bytes):
        return hashlib.sha256(b"\x01" + tree).digest()
    return hashlib.sha256(b"\x02" + tree_hash(tree[0]) + tree_hash(tree[1])).digest()


def parse(text):
    """Read this fixture's explicit integer/hex/proper/improper-tree grammar."""
    tokens = re.findall(r"0x[0-9a-fA-F]*|-?[0-9]+|[().]", text)
    require("".join(tokens) == re.sub(r"\s+", "", text), "Unsupported fixture input syntax")
    cursor = 0

    def read():
        nonlocal cursor
        require(cursor < len(tokens), "Missing input tree")
        token = tokens[cursor]
        cursor += 1
        if token == "(":
            items, tail = [], b""
            while cursor < len(tokens) and tokens[cursor] != ")":
                if tokens[cursor] == ".":
                    require(items, "A dotted pair requires a first element")
                    cursor += 1
                    tail = read()
                    break
                items.append(read())
            require(cursor < len(tokens) and tokens[cursor] == ")", "Missing closing parenthesis")
            cursor += 1
            for item in reversed(items):
                tail = item, tail
            return tail
        require(token not in (".", ")"), "Unexpected tree punctuation")
        if token.startswith("0x"):
            return bytes.fromhex(token[2:])
        value = int(token)
        if value == 0:
            return b""
        size = 1
        while not -(1 << (size * 8 - 1)) <= value < (1 << (size * 8 - 1)):
            size += 1
        return value.to_bytes(size, "big", signed=True)

    result = read()
    require(cursor == len(tokens), "Trailing fixture input")
    return result
