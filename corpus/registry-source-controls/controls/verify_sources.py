#!/usr/bin/env python3
"""Pure textual source derivation checks; never invokes compilers or policies."""
import hashlib
import json
from pathlib import Path


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    package = Path(__file__).resolve().parent
    root = package.parents[1]
    recipes = json.loads((package / "fixtures/source-recipes.json").read_text())

    def admitted(identity, expected_path):
        require(identity["path"] == expected_path, "exact source path")
        raw = (root / expected_path).read_bytes()
        require(type(identity["bytes"]) is int, "integer byte length")
        require(len(raw) == identity["bytes"], "source byte length")
        require(hashlib.sha256(raw).hexdigest() == identity["sha256"], "source digest")
        return raw

    original_recipe = admitted(recipes["recipe_original"], "puzzles/registry_recipe.rs")
    recipe_copy = admitted(
        recipes["recipe_copy"],
        "experiments/action-registry-source-controls/src/registry_recipe.rs",
    )
    require(original_recipe == recipe_copy, "unmodified shared recipe copy")
    order = ["alpha-drop-send", "alpha-drop-create", "beta-drop-send", "beta-drop-create"]
    require([row["id"] for row in recipes["variants"]] == order, "complete ordered variants")
    for row in recipes["variants"]:
        side, _, kind = row["id"].split("-")
        original = admitted(row["original"], f"puzzles/registry-set-{side}/puzzle.rs")
        target = admitted(
            row["variant"],
            f"experiments/action-registry-source-controls/src/{side}_drop_{kind}.rs",
        )
        require(len(row["source_edits"]) == 4, "four explicit source transformations")
        derived = original
        for edit in row["source_edits"]:
            require(type(edit["matches"]) is int and edit["matches"] == 1, "one-match edit")
            before, after = edit["before"].encode(), edit["after"].encode()
            require(before and derived.count(before) == 1, "unique exact source edit")
            derived = derived.replace(before, after, 1)
        prefix = f"//! Testing variant: omit exactly the returned {kind.upper()} condition.\n"
        require(row["additional_doc_prefix"] == prefix, "exact testing notice")
        require(prefix.encode() + derived == target, "direct recipe reproduces final source")
        require(b"pub type SetOutput" not in target, "no shared output alias replacement")
    print("PASS: four exact formatted-source recipes and unchanged recipe copy; no policy execution")


if __name__ == "__main__":
    main()
