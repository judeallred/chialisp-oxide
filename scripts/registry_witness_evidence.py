"""Bounded constructed readers for explicit generated-witness evidence.

No process, compiler, VM, cryptographic theorem or execution credit lives here.
The source checks constrain this recipe's syntax and provenance; actual retained
MIR, ordinary Rust, subset diagnostics and finite VM observations stay separate.
"""
import re
from pathlib import Path

from cargo_source_receipt import decode_json
from registry_codegen_evidence import MAX_JSON, digest, file_identity, identity, inspect_compiled_artifact, read, require, unhex

PACKAGE = "registry-witness-fixture"
CRATE = "registry_witness_fixture"
ALIASES = ("multiply", "quote257", "quote8192")
EXPORTS = tuple(name for alias in ALIASES for name in (alias + "_witness", "strict_unused_" + alias + "_witness"))
RECIPE = "chialisp-oxide-witness-rust/v1"
LIMITS = {"max_artifact_bytes": 2097152, "max_expanded_nodes": 262144,
          "max_data_depth_root_one": 4096, "max_source_bytes": 8192,
          "max_generation_work_units": 2097152}
RESOLUTION_SCOPE = "Local bounded exact envelope and artifact pin resolution only; no recovered bindings, source authenticity, source/IR/CLVM equivalence, compiler or VM execution, authorization or suitability for real funds."
GENERATION_SCOPE = "Bounded ordinary local Rust source generation after complete exact lock resolution; a local logical-tree SHA-256 guard precedes strict Apply of the supplied complete program to its opaque whole environment. Runtime commitment assumes SHA-256 collision/second-preimage resistance and does not commit original wire encoding, envelope, source or authorship. Host eligibility is not a runtime resource bound, compilation guarantee, inferred nominal ABI, authorization, complete spend, source authentication or compiler-preservation proof."
HELPER = b"fn sha256tree<'a>(tree: &'a Tree<'a>) -> [u8; 32] {\n    match tree {\n        Tree::Atom(payload) => sha256_parts(&ATOM_DOMAIN, payload, &[]),\n        Tree::Pair(first, rest) => {\n            let first_hash = sha256tree(first);\n            let rest_hash = sha256tree(rest);\n            sha256_parts(&PAIR_DOMAIN, &first_hash, &rest_hash)\n        }\n    }\n}\n"
HELPER_SHA256 = "30bd642cac1bcbfd3aa06c466cb75489a6b3f95b02baaf11485771de44cdddc7"
GUARD = b"    assert!(program_hash == EXPECTED_PROGRAM_TREE_HASH);\n"
RUN = b"    let program_hash = sha256tree(program);\n" + GUARD + b"    apply(program, environment)\n"
MUTANTS = ("guard_omitted", "constant_hash_helper", "altered_commitment", "altered_atom_domain", "swapped_pair_hash", "apply_before_guard")


def check_resolve(value, lock, dependencies):
    require(type(value) is dict and set(value) == {"status", "lock_sha256", "dependencies", "scope"}
            and value["status"] == "resolved" and value["lock_sha256"] == digest(lock)
            and value["scope"] == RESOLUTION_SCOPE and value["dependencies"] == dependencies,
            "actual complete lock resolution identities/protocol differ")
    return value


def check_command_logs(output, commands):
    """Retain exact existing raw logs, including partial terminal commands.

    Preflight count, declared per-file lengths and remaining aggregate bytes
    before reads/hashes. A record is not evidence that a command was authentic.
    """
    output = Path(output)
    require(type(commands) is list and len(commands) <= 512, "bounded actual command inventory required")
    manifest = output / "commands.json"
    if not commands:
        require(not manifest.exists(), "unexpected commands manifest without child records")
        return {"files": [], "scope": "No child command records; zero execution credit."}
    remaining, files, labels = 64 * 1024 * 1024, [], set()
    for row in commands:
        require(type(row) is dict and type(row.get("label")) is str
                and re.fullmatch(r"[a-zA-Z0-9_-]{1,160}", row["label"])
                and row["label"] not in labels, "bounded unique actual command labels required")
        labels.add(row["label"])
        require(row.get("status") in ("completed", "terminated", "launch_error", "transport_error"),
                "every attempted command must retain terminal status")
        require((type(row.get("exit_code")) is int if row["status"] in ("completed", "terminated")
                 else row.get("exit_code", "missing") is None), "actual exit code or explicit unavailable status required")
        for stream in ("stdout", "stderr"):
            path = output / (row["label"] + "." + stream)
            if stream not in row:
                require(not path.exists(), "unrecorded command log exists")
                continue
            retained = row[stream]
            require(type(retained) is dict and set(retained) == {"path", "bytes", "sha256"}
                    and retained["path"] == str(path) and type(retained["bytes"]) is int
                    and 0 <= retained["bytes"] <= min(MAX_JSON, remaining), "bounded exact command log identity required")
            observed = file_identity(path, min(MAX_JSON, remaining))
            require(observed == retained, "retained raw command log changed")
            remaining -= observed["bytes"]
            files.append(observed)
    observed = file_identity(manifest, min(MAX_JSON, remaining))
    require(decode_json(read(manifest, min(MAX_JSON, remaining))) == commands, "actual complete command manifest changed")
    files.append(observed)
    return {"files": files, "scope": "Exact bounded complete command JSON and raw stdout/stderr identities; no binary authentication or execution-preservation proof."}


def check_generated(value, source, alias, module, lock, object_id, status):
    require(type(source) is bytes and len(source) <= LIMITS["max_source_bytes"], "bounded generated source required")
    require(type(value) is dict and set(value) == {"status", "output", "generation"}
            and value["status"] == status and value["output"] == {
                "status": status, "source_sha256": digest(source), "source_bytes": len(source)},
            "actual witness output protocol/source identity differs")
    record = value["generation"]
    require(type(record) is dict and set(record) == {"schema_version", "recipe", "lock_sha256", "selected_alias",
            "object_id", "artifact", "source_sha256", "source_bytes", "metrics", "limits", "scope"}, "exact witness record fields required")
    require(record["schema_version"] == 1 and record["recipe"] == RECIPE
            and record["lock_sha256"] == digest(lock) and record["selected_alias"] == alias
            and record["object_id"] == object_id and record["artifact"] == identity(module)
            and record["source_sha256"] == digest(source) and record["source_bytes"] == len(source)
            and record["limits"] == LIMITS and record["scope"] == GENERATION_SCOPE,
            "actual witness generation record differs from exact admitted pin/source")
    metrics, inspected = record["metrics"], inspect_compiled_artifact(module)
    expected = {"artifact_bytes": len(module), "expanded_nodes": inspected["stats"]["nodes"],
                "data_depth_root_one": inspected["stats"]["depth_root_one"], "generated_source_bytes": len(source),
                "source_helpers": 2, "commitment_bytes": 32, "embedded_program_bytes": 0}
    require(type(metrics) is dict and set(metrics) == set(expected) | {"generation_work_units"}
            and all(type(value) is int and value >= 0 for value in metrics.values())
            and all(metrics[key] == value for key, value in expected.items())
            and len(source) <= metrics["generation_work_units"] <= LIMITS["max_generation_work_units"],
            "actual bounded witness generation metrics differ")
    require(digest(HELPER) == HELPER_SHA256 and source.count(HELPER) == 1 and source.count(RUN) == 1,
            "exact ordinary local helper and hash-guard-before-Apply recipe required")
    matches = re.findall(rb"const EXPECTED_PROGRAM_TREE_HASH: \[u8; 32\] = \[([^\n]+)\];", source)
    require(len(matches) == 1, "one fixed expected commitment required")
    payload = matches[0].split(b", ")
    require(len(payload) == 32 and all(re.fullmatch(rb"0x[0-9a-f]{2}", item) for item in payload)
            and bytes(int(item, 16) for item in payload).hex() == record["artifact"]["module_tree_hash"],
            "generated fixed hash differs from actual selected artifact")
    # Constants here have a closed grammar. Body-local general data literals
    # are excluded by exact source-shape comparison across all three modules.
    constants = re.findall(rb"const ([A-Z_]+): \[u8; ([0-9]+)\] = \[([^\n]*)\];", source)
    require([(name, int(size)) for name, size, _ in constants] == [
        (b"ATOM_DOMAIN", 1), (b"PAIR_DOMAIN", 1), (b"EXPECTED_PROGRAM_TREE_HASH", 32)],
        "only two small domains and one fixed32 commitment may be emitted")
    require(b"const ATOM_DOMAIN: [u8; 1] = [1];" in source
            and b"const PAIR_DOMAIN: [u8; 1] = [2];" in source
            and b"puzzles/sha256tree.clib lines 4-10" in source
            and b"cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8" in source,
            "actual helper domains/provenance differ")
    for header in (b"// Raw lock SHA-256: " + digest(lock).encode(),
                   b"// Envelope SHA-256: " + object_id.encode(),
                   b"// Artifact blob SHA-256: " + digest(module).encode(),
                   b"// Artifact tree hash: " + identity(module)["module_tree_hash"].encode(),
                   b"// Artifact bytes: " + str(len(module)).encode()):
        require(source.splitlines().count(header) == 1, "exact generated provenance header differs")
    syntax = b"\n".join(line for line in source.splitlines() if not line.startswith(b"//"))
    expected_syntax = (b"use chialisp_oxide::{Tree, apply, sha256_parts};\n"
        b"#[rustfmt::skip]\nconst ATOM_DOMAIN: [u8; 1] = [1];\n"
        b"#[rustfmt::skip]\nconst PAIR_DOMAIN: [u8; 1] = [2];\n"
        b"#[rustfmt::skip]\nconst EXPECTED_PROGRAM_TREE_HASH: [u8; 32] = [" + matches[0] + b"];\n"
        b"#[rustfmt::skip]\npub(super) fn run<'a>(program: &'a Tree<'a>, environment: &'a Tree<'a>) -> Tree<'static> {\n"
        + RUN + b"}\n#[rustfmt::skip]\n" + HELPER)
    require(re.sub(rb"\s+", b"", syntax) == re.sub(rb"\s+", b"", expected_syntax),
            "generated source must contain only the fixed small witness recipe, not embedded program data")
    return record


def inspect_ir(program, *, baseline=True):
    """Inventory complete actual retained JSON; not a symbolic IR interpreter."""
    pending, visited, largest, atoms, applies, hashes = [(program, 1)], 0, 0, 0, 0, 0
    while pending:
        value, depth = pending.pop()
        visited += 1
        require(visited <= 262144 and depth <= 256, "bounded actual IR inventory work/depth")
        if type(value) is dict:
            if value.get("op") in ("atom", "bytes"):
                payload = unhex(value["hex"], 32)
                atoms += 1
                largest = max(largest, len(payload))
            applies += value.get("op") == "apply"
            hashes += value.get("op") == "sha256"
            require(len(value) <= 128 and len(pending) + len(value) <= 262144, "bounded actual IR object")
            pending.extend((child, depth + 1) for child in value.values())
        elif type(value) is list:
            require(len(value) <= 262144 and len(pending) + len(value) <= 262144, "bounded actual IR array")
            pending.extend((child, depth + 1) for child in value)
    require(applies == 1 and (hashes >= 2 if baseline else True), "actual source must retain one strict Apply and baseline hash branches")
    return {"visited_json_nodes": visited, "maximum_byte_constant_bytes": largest,
            "atom_or_bytes_constants": atoms, "apply_nodes": applies, "sha256_nodes": hashes,
            "scope": "Complete bounded retained IR node/constant inventory only; no generic evaluation-order or compiler-preservation proof."}


def mutate(source, name):
    """Closed, source-only recipes. No Rust/compiler/VM result is inferred."""
    require(name in MUTANTS and type(source) is bytes and len(source) <= 8192,
            "known bounded generated-source mutation required")
    if name == "guard_omitted":
        old, new = GUARD, b""
    elif name == "constant_hash_helper":
        old, new = HELPER, b"fn sha256tree<'a>(tree: &'a Tree<'a>) -> [u8; 32] {\n    let _ = tree;\n    EXPECTED_PROGRAM_TREE_HASH\n}\n"
    elif name == "altered_commitment":
        pattern = rb"const EXPECTED_PROGRAM_TREE_HASH: \[u8; 32\] = \[0x([0-9a-f]{2})"
        matches = list(re.finditer(pattern, source))
        require(len(matches) == 1, "unique fixed commitment source mutation site")
        start, stop = matches[0].span(1)
        result = source[:start] + f"{int(matches[0][1], 16) ^ 1:02x}".encode() + source[stop:]
        require(result != source, "source mutation must change bytes")
        return result
    elif name == "altered_atom_domain":
        old, new = b"const ATOM_DOMAIN: [u8; 1] = [1];", b"const ATOM_DOMAIN: [u8; 1] = [2];"
    elif name == "swapped_pair_hash":
        old, new = b"sha256_parts(&PAIR_DOMAIN, &first_hash, &rest_hash)", b"sha256_parts(&PAIR_DOMAIN, &rest_hash, &first_hash)"
    else:
        old, new = RUN, (b"    let output = apply(program, environment);\n"
                        b"    let program_hash = sha256tree(program);\n" + GUARD + b"    output\n")
    require(source.count(old) == 1, "unique exact generated-source mutation site required")
    result = source.replace(old, new, 1)
    require(result != source and len(result) <= 8192, "bounded changed source mutation required")
    return result


def mutant_expected(name, *, discarded):
    """Finite intended controls: matching multiply, wrong nil, wrong path 6."""
    require(name in MUTANTS and type(discarded) is bool, "known mutant expectation required")
    accepted = lambda wire: {"status": "accepted", "output_hex": "80" if discarded else wire}
    raised = {"status": "rejected", "code": "raise"}
    path = {"status": "rejected", "code": "path_into_atom"}
    if name in ("guard_omitted", "constant_hash_helper"):
        return [accepted("2a"), accepted("80"), path]
    if name in ("altered_commitment", "altered_atom_domain", "swapped_pair_hash"):
        return [raised, raised, raised]
    return [accepted("2a"), raised, path]
