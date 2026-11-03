#!/usr/bin/env python3
"""Independent integer arithmetic control; not an executed CLVM observation."""
P = int("1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab", 16)
R = int("73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001", 16)


def add(left, right):
    if left is None:
        return right
    if right is None:
        return left
    x, y = left
    u, v = right
    if x == u and (y + v) % P == 0:
        return None
    slope = ((v - y) * pow(u - x, -1, P) if x != u
             else 3 * x * x * pow(2 * y, -1, P)) % P
    out_x = (slope * slope - x - u) % P
    return out_x, (slope * (x - out_x) - y) % P


def multiply(point, scalar):
    result = None
    while scalar:
        if scalar & 1:
            result = add(result, point)
        point = add(point, point)
        scalar >>= 1
    return result


if __name__ == "__main__":
    x = 4
    y = pow(x**3 + 4, (P + 1) // 4, P)
    y = min(y, P - y)
    assert y * y % P == (x**3 + 4) % P
    assert multiply((x, y), R) is not None
    compressed = bytearray(x.to_bytes(48, "big"))
    compressed[0] |= 0x80
    assert compressed.hex() == "80" + "00" * 46 + "04"
    print("PASS independent on-curve/non-subgroup arithmetic:", compressed.hex())
