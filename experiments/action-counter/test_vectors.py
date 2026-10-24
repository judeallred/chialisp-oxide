"""Pure specification regressions; no Rust, compiler, VM or SDK execution."""
import sys
sys.dont_write_bytecode = True
import unittest

import vectors


class MutantSpecification(unittest.TestCase):
    def test_original_overflow_and_identity_mutant_have_distinct_max_domains(self):
        maximum = b"\x00" + b"\xff" * 8
        environment = vectors.proper([(b"", maximum)])
        self.assertEqual(
            vectors.expected("increment", environment),
            {"status": "rejected", "stage": "policy"},
        )
        # Explicit wire bytes for ((nil . MAX) . nil), independent of the
        # specification codec used to construct ordinary expected outputs.
        identity_output = b"\xff\xff\x80\x89\x00" + b"\xff" * 8 + b"\x80"
        self.assertEqual(
            vectors.expected("increment", environment, "increment_no_change"),
            {"status": "accepted", "output_hex": identity_output.hex()},
        )

    def test_identity_mutant_retains_the_typed_input_domain(self):
        for invalid in (b"\x00", b"\xff", b"\x01" + b"\x00" * 8, (b"", b"")):
            environment = vectors.proper([(b"", invalid)])
            self.assertEqual(
                vectors.expected("increment", environment, "increment_no_change"),
                {"status": "rejected", "stage": "abi"},
            )


if __name__ == "__main__":
    unittest.main()
