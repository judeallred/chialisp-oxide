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


def _bandit():
    import bandit_oracle
    return bandit_oracle


def _raffle():
    import raffle_oracle
    return raffle_oracle


def _ordered_payout():
    import ordered_payout_oracle
    return ordered_payout_oracle


@lru_cache(maxsize=1)
def _spuds_recipe():
    return runpy.run_path(str(ROOT / "experiments/spuds/oracle.py"))


def _spuds(solution):
    """Strict typed decoding around the independent experiments/spuds host recipe."""
    p = _precommit()
    arguments = p.fields(raw_tree.parse(solution), 4)
    if arguments is None:
        return {"status": "rejected"}
    terms, take, next_key, next_payout = arguments
    terms = p.fields(terms, 4)
    if terms is None or take not in (b"", b"\1"):
        return {"status": "rejected"}
    module, key, payout, round_atom = terms
    if type(round_atom) is not bytes or len(round_atom) > 2:
        return {"status": "rejected"}
    round_value = int.from_bytes(round_atom, "big", signed=True) if round_atom else 0
    if not 0 <= round_value <= 255 or p.integer(round_value) != round_atom:
        return {"status": "rejected"}
    if not all(type(v) is bytes for v in (module, key, payout, next_key, next_payout)):
        return {"status": "rejected"}
    try:
        conditions = _spuds_recipe()["step"](module, key, payout, round_value, take == b"\1", next_key, next_payout)
    except ValueError:
        return {"status": "rejected"}
    encoded = [p.proper(*(bytes.fromhex(x) if type(x) is str else p.integer(x) for x in row)) for row in conditions]
    return {"status": "accepted", "output_hex": raw_tree.serialize(p.proper(*encoded)).hex()}


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
    "ordered-payout": {"expected": lambda solution: _ordered_payout().expected(solution), "generated": lambda: []},
    "spuds": {"expected": _spuds, "generated": lambda: []},
    "raffle": {"expected": lambda solution: _raffle().expected(solution), "generated": lambda: []},
    "one-armed-bandit": {"expected": lambda solution: _bandit().expected(solution), "generated": lambda: []},
}
