import unittest
import check  # Establish the repository shared-codec import path.
from codec import decode, same_tree


class BackreferenceControls(unittest.TestCase):
    def test_atom_pair_nil_value_and_virtual_stack_paths(self):
        examples = [
            ("80", b""),
            ("ff01fe02", (b"\x01", b"\x01")),
            ("ffff0102fe02", ((b"\x01", b"\x02"), (b"\x01", b"\x02"))),
            ("ff01fe01", (b"\x01", (b"\x01", b""))),
            ("ff01fe03", (b"\x01", b"")),
            ("ff01fe00", (b"\x01", b"")),
        ]
        for encoded, tree in examples:
            with self.subTest(encoded=encoded): same_tree(decode(bytes.fromhex(encoded)), tree)

    def test_truncation_trailing_and_paths_into_atoms_are_rejected(self):
        for encoded in ("", "ff", "fe", "fe02", "ff01fe04", "8080", "82ff", "fc"):
            with self.subTest(encoded=encoded):
                with self.assertRaises(ValueError): decode(bytes.fromhex(encoded))

    def test_shared_subtree_comparison_is_bounded(self):
        tree = b""
        for _ in range(20): tree = tree, tree
        with self.assertRaisesRegex(ValueError, "work bound"):
            same_tree(tree, tree, max_work=100)
        with self.assertRaises(ValueError): same_tree((b"", b""), b"")
        with self.assertRaises(ValueError): same_tree(b"a", b"b")
        with self.assertRaisesRegex(ValueError, "work bound"):
            same_tree(b"x"*101, b"x"*101, max_work=100)
        with self.assertRaisesRegex(ValueError, "path byte bound"):
            decode(bytes.fromhex("fec081") + bytes(129))
