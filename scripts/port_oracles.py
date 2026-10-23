"""Registry of independent expected-outcome oracles for ports checked by the generic gate.

Each entry names an oracle callable from a reviewed independent relation
(Python hashlib, raw-tree codecs), the generated inputs the corpus executes
and, where the manifest omits it, nothing else. Ports whose manifests declare
`expected` outcomes per case need no entry here: the generic gate reads them.
"""
from pathlib import Path
from functools import lru_cache
import runpy
import raw_tree

ROOT = Path(__file__).resolve().parents[1]


def _hashing():
    import hashing_vectors
    return hashing_vectors


def _precommit():
    import precommit_oracle
    return precommit_oracle


def _factories():
    return runpy.run_path(str(ROOT / "scripts/check-canonical-factories.py"))


@lru_cache(maxsize=1)
def _cat_relation():
    return runpy.run_path(str(ROOT / "experiments/cat-genesis-tail/vectors.py"))["relation"]


PORTS = {
    "cat-genesis-by-coin-id": {
        "expected": lambda solution: _cat_relation()(raw_tree.parse(solution))[0],
        "generated": lambda: _hashing().raw_generated(),
    },
    "sha256tree": {"expected": lambda solution: _hashing().expected("sha256tree", solution),
                   "generated": lambda: _hashing().generated_for("sha256tree")},
    "singleton-launcher": {"expected": lambda solution: _hashing().expected("singleton-launcher", solution),
                           "generated": lambda: _hashing().generated_for("singleton-launcher")},
    "p2-conditions": {"expected": lambda solution: _factories()["expected"](solution)[0],
                      "generated": lambda: _hashing().raw_generated()},
    "curry-factory": {"expected": lambda solution: _factories()["expected_curry"](solution), "generated": lambda: []},
    "precommit-claim": {"expected": lambda solution: _precommit().expected("precommit-claim", solution),
                        "generated": lambda: []},
    "precommit-registry": {"expected": lambda solution: _precommit().expected("precommit-registry", solution),
                           "generated": lambda: []},
}
