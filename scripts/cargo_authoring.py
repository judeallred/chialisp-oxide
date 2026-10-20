"""Hash declared Cargo teaching-library inputs; not full build provenance."""

import hashlib
import json
from pathlib import Path
import re


AUTHORING = "puzzles/authoring.json"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def load_json(path):
    return json.loads(path.read_text(), object_pairs_hook=unique_object)


def contained_file(root, relative):
    require(isinstance(relative, str), "Expected a relative file path")
    path = Path(relative)
    require(not path.is_absolute() and path.parts and ".." not in path.parts
            and path.as_posix() == relative,
            f"Expected a normalized confined relative path: {relative}")
    root = root.resolve()
    resolved = (root / path).resolve()
    require(resolved.is_relative_to(root) and resolved.is_file(),
            f"Missing file or path escapes its declared root: {relative}")
    return resolved


def snapshot(root):
    metadata = load_json(contained_file(root, AUTHORING))
    require(type(metadata.get("schema_version")) is int and metadata["schema_version"] == 1
            and metadata["mode"] == "cargo_library", "Unsupported Cargo authoring configuration")
    require(metadata["package"] == "chialisp-oxide-puzzles" and metadata["manifest"] == "puzzles/Cargo.toml"
            and metadata["library"] == "puzzles/lib.rs", "Unexpected Cargo teaching library")
    exports, inputs = metadata["exports"], metadata["inputs"]
    require(isinstance(exports, dict) and exports, "Missing Cargo export declarations")
    require(isinstance(inputs, list) and all(isinstance(path, str) for path in inputs)
            and len(inputs) == len(set(inputs)), "Invalid or duplicate Cargo input paths")
    required = {AUTHORING, metadata["manifest"], metadata["library"], "puzzles/fixture_keys.rs", "Cargo.toml", "Cargo.lock",
                "rust-toolchain.toml", ".cargo/config.toml", "crates/chialisp-oxide/Cargo.toml",
                "crates/chialisp-oxide/src/lib.rs", "crates/chialisp-oxide/src/cons.rs",
                "crates/chialisp-oxide/src/domain.rs", "crates/chialisp-oxide/src/tree.rs",
                "crates/chialisp-oxide/src/owned_tree.rs", "crates/chialisp-oxide/src/hashing.rs",
                "crates/chialisp-oxide/src/apply.rs",
                "crates/chialisp-oxide-macros/Cargo.toml", "crates/chialisp-oxide-macros/src/lib.rs"}
    entries = set()
    for slug, export in exports.items():
        require(isinstance(slug, str) and re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", slug),
                "Malformed Cargo export slug")
        require(set(export) == {"entry", "source"}
                and export["entry"] == f"chialisp_oxide_puzzles::{slug.replace('-', '_')}::puzzle"
                and export["source"] == f"puzzles/{slug}/puzzle.rs",
                f"Cargo export identity/source mismatch: {slug}")
        require(export["entry"] not in entries, "Duplicate Cargo entry name")
        entries.add(export["entry"])
        required.add(export["source"])
    require(required <= set(inputs), "Declared Cargo inputs omit required authoring files")
    sources = []
    for path in sorted(inputs):
        payload = contained_file(root, path).read_bytes()
        sources.append({"path": path, "sha256": hashlib.sha256(payload).hexdigest()})
    return {"mode": metadata["mode"], "package": metadata["package"],
            "manifest": metadata["manifest"], "library": metadata["library"],
            "exports": exports, "sources": sources}


def check_recorded(recorded, root, slugs):
    """Validate current source identities and hashes before ignoring metadata in a diff."""
    expected = snapshot(root)
    require(set(expected["exports"]) == set(slugs), "Cargo exports differ from reported puzzle set")
    require(isinstance(recorded, dict) and set(recorded) == set(expected),
            "Missing or unexpected recorded Cargo authoring fields")
    paths = []
    for source in recorded["sources"]:
        require(set(source) == {"path", "sha256"}, "Malformed recorded Cargo input")
        contained_file(root, source["path"])
        require(isinstance(source["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", source["sha256"]),
                "Malformed recorded Cargo input hash")
        paths.append(source["path"])
    require(len(paths) == len(set(paths)), "Duplicate recorded Cargo input paths")
    require(recorded == expected, "Recorded Cargo authoring metadata or input hashes are stale")
