"""Bounded independent backreference reader for recorded generator comparisons.

The value stack is an immutable CLVM list. Backreferences traverse that stack
using least-significant path bits, excluding the leading sentinel. Shared
subtrees stay shared; comparisons have their own expanded-work bound.
This is a fixture checker, not the VM or a general consensus codec.
"""
from raw_tree import require


def decode(data):
    require(type(data) is bytes and 0 < len(data) <= 8_000_000, "generator codec byte bound")
    cursor, work = 0, 0

    def atom():
        nonlocal cursor
        require(cursor < len(data), "truncated atom")
        head = data[cursor]
        cursor += 1
        if head < 128: return bytes([head])
        if head == 128: return b""
        prefix = 0
        while head & (128 >> prefix): prefix += 1
        require(prefix <= 5 and cursor + prefix - 1 <= len(data), "invalid atom prefix")
        size = head & ((1 << (7-prefix))-1)
        for _ in range(prefix-1):
            size = (size << 8) | data[cursor]
            cursor += 1
        require(cursor + size <= len(data), "truncated atom payload")
        value = data[cursor:cursor+size]
        cursor += size
        return value

    stack, pending = b"", [False]
    while pending:
        work += 1
        require(work <= 1_000_000, "generator codec work bound")
        if pending.pop():
            require(type(stack) is tuple and type(stack[1]) is tuple, "pair stack underflow")
            right, tail = stack
            left, tail = tail
            stack = ((left, right), tail)
            continue
        require(cursor < len(data), "truncated generator")
        head = data[cursor]
        if head == 255:
            cursor += 1
            pending.extend((True, False, False))
            continue
        if head == 254:
            cursor += 1
            path_bytes = atom()
            require(len(path_bytes) <= 128, "backreference path byte bound")
            path = int.from_bytes(path_bytes, "big")
            value = stack if path else b""
            while path > 1:
                work += 1
                require(work <= 1_000_000 and type(value) is tuple, "invalid/bounded backreference path")
                value = value[path & 1]
                path >>= 1
        else:
            value = atom()
        stack = value, stack
    require(cursor == len(data) and type(stack) is tuple and stack[1] == b"", "trailing generator data")
    return stack[0]


def same_tree(left, right, max_work=100_000):
    pending, work = [(left, right)], 0
    while pending:
        work += 1
        require(work <= max_work, "expanded generator comparison work bound")
        a, b = pending.pop()
        require(type(a) is type(b), "generator tree shape differs")
        if type(a) is tuple:
            pending.extend(((a[0], b[0]), (a[1], b[1])))
        else:
            require(type(a) is bytes, "invalid generator atom")
            work += len(a)
            require(work <= max_work, "expanded generator comparison work bound")
            require(a == b, "generator atom differs")
