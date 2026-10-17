#!/usr/bin/env python3
"""Finite native tests of the actual SDK module; no Cargo or CLVM admission.

The compiler, native object generation, OS/runtime and oracle implementations
remain trusted. File hashes check freshness, not cryptographic proof of a run.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import re
import shutil
import ssl
import stat
import subprocess
import sys
import tomllib

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SDK = ROOT / "crates/chialisp-oxide/src/hashing.rs"
COMMIT = "daf2e5e18b3abc4311d9da3d96fd64138f43d508"
CHANNEL = "nightly-2026-07-14"
SCOPE = "Finite execution of the actual SDK SHA-256 module and compiled mutations; not universal SHA correctness, collision resistance, compiler preservation, primitive admission or production readiness."
SOURCES = [HERE / "check-native.py", HERE / "bridge.rs", HERE / "vectors.json",
           ROOT / "rust-toolchain.toml", SDK]
MUTATIONS = {
    "round_constant": ("0x428a2f98", "0x428a2f99"),
    "padding_marker": ("0x80\n", "0x81\n"),
    "bit_length": ("message_len << 3", "message_len << 2"),
    "choose_operands": ("(ee & ff) ^ (!ee & gg)", "(ee & gg) ^ (!ee & ff)"),
    "digest_endian": ("24 - (i % 4) * 8", "(i % 4) * 8"),
    "ignore_second_part": ("a_len + b.len() as u128", "a_len"),
}
HEX = re.compile(r"[0-9a-f]{64}\Z")


def require(test, message):
    if not test:
        raise ValueError(message)


def load(path):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, f"Duplicate JSON key: {key}")
            result[key] = value
        return result
    return json.loads(path.read_text(), object_pairs_hook=pairs)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def fingerprint(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for part in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(part)
    details = path.stat()
    return {"path": str(path), "sha256": digest.hexdigest(), "bytes": details.st_size,
            "mode": stat.S_IMODE(details.st_mode)}


def run(command, **kwargs):
    return subprocess.run([str(x) for x in command], check=True, capture_output=True,
                          timeout=180, cwd=ROOT, **kwargs)


def context(rustc):
    rustc = rustc.resolve(strict=True)
    require(tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
            == CHANNEL, "Repository toolchain changed")
    version = run([rustc, "-vV"], text=True).stdout
    fields = dict(line.split(": ", 1) for line in version.splitlines() if ": " in line)
    require(fields.get("commit-hash") == COMMIT, "Wrong live rustc commit")
    host = fields["host"]
    sysroot = Path(run([rustc, "--print", "sysroot"], text=True).stdout.strip()).resolve(strict=True)
    cc = Path(shutil.which("cc") or "").resolve(strict=True)
    openssl = Path("/usr/bin/openssl")
    require(openssl.is_file(), "Required /usr/bin/openssl is missing")
    target_lib = sysroot / "lib/rustlib" / host / "lib"
    libraries = sorted({p for p in target_lib.rglob("*") if p.is_file()}
                       | {p for p in (target_lib.parent / "codegen-backends").rglob("*") if p.is_file()}
                       | {p for glob in ("libLLVM*", "librustc_driver*")
                          for p in (sysroot / "lib").glob(glob) if p.is_file()})
    require(any(p.name.startswith("libcore-") for p in libraries)
            and any(p.name.startswith("libstd-") for p in libraries), "Missing installed core/std")
    source_inputs = [{**fingerprint(p), "path": p.relative_to(ROOT).as_posix()} for p in SOURCES]
    inputs = [fingerprint(p) for p in [rustc, cc, openssl, Path(sys.executable).resolve(), *libraries]]
    require(len({row["path"] for row in inputs}) == len(inputs), "Duplicate toolchain input")
    return {"compiler": {"path": str(rustc), "version": version, "sysroot": str(sysroot), "host": host},
            "tools": {"cc": str(cc), "cc_version": run([cc, "--version"], text=True).stdout,
                      "openssl": str(openssl), "openssl_version": run([openssl, "version"], text=True).stdout.strip(),
                      "python": sys.version, "python_ssl": ssl.OPENSSL_VERSION},
            "sources": source_inputs, "installed_inputs": inputs}


def cases():
    config = load(HERE / "vectors.json")
    require(config["schema_version"] == 1 and len(config["known_answers"]) == 3, "Invalid KAT schema")
    answers = config["known_answers"]
    for row in answers:
        require(HEX.fullmatch(row["sha256"]) and sha(row["message_ascii"].encode("ascii")) == row["sha256"],
                f"Fixed known answer disagrees: {row['id']}")
    rng = random.Random(config["seed"])
    result = [(b"", b"", b""), (b"abc", b"", b""), (b"a", b"b", b"c")]
    lengths = config["lengths"]
    require(len(lengths) == 26 and all(type(n) is int and 0 <= n <= 1_000_000 for n in lengths), "Length budget")
    require(config["random_cases"] == 512 and config["random_max_bytes"] == 8192, "Generator budget changed")
    for n in lengths:
        message = rng.randbytes(n)
        for i, j in [(0, 0), (0, n), (n, n), (n // 3, 2 * n // 3), (min(n, 55), min(n, 64))]:
            result.append((message[:i], message[i:j], message[j:]))
    for _ in range(config["random_cases"]):
        message = rng.randbytes(rng.randrange(config["random_max_bytes"] + 1))
        i = rng.randrange(len(message) + 1)
        j = rng.randrange(i, len(message) + 1)
        result.append((message[:i], message[i:j], message[j:]))
    for first in [b"", b"\x00", b"\x00\x00", b"\x01", b"\x00\x01", b"\x80", b"\xff", b"\x80\x00"]:
        for second in [b"", b"\x00", b"\xff"]:
            result.append((first, second, b"\xff\x00"))
    long = answers[2]["message_ascii"].encode("ascii")
    result.extend([(long, b"", b""), (long[:1], long[1:55], long[55:])])
    expected = [sha(b"".join(parts)) for parts in result]
    require(len(result) == 671 and expected[0] == answers[0]["sha256"]
            and expected[1] == answers[1]["sha256"] and expected[-1] == answers[2]["sha256"], "KAT/case coverage mismatch")
    openssl_indices = [0, 1, 2] + [3 + i * 5 + 3 for i in range(len(lengths))] + list(range(len(result) - 26, len(result)))
    return result, expected, openssl_indices


def within(work, relative):
    require(isinstance(relative, str) and relative and Path(relative).as_posix() == relative
            and not Path(relative).is_absolute() and not any(p in ("", ".", "..") for p in relative.split("/")), "Invalid retained path")
    path = work / relative
    require(path.resolve().is_relative_to(work), "Retained path escapes work directory")
    return path


def commands(current, work, label):
    folder = work / label
    level = 0 if label == "checked-debug" else 2
    flags = ["--edition=2024", "--sysroot", current["compiler"]["sysroot"], "--target", current["compiler"]["host"],
             "-C", f"opt-level={level}", "-C", "overflow-checks=on", "-C", "debug-assertions=on", "-C", "panic=unwind",
             "-C", f"linker={current['tools']['cc']}"]
    compiler = current["compiler"]["path"]
    bridge = [compiler, *flags, "--crate-name", "native_sha_bridge", str(HERE / "bridge.rs")]
    result = []
    if label in MUTATIONS:
        library = folder / "libmutated_hashing.rlib"
        result.append([compiler, *flags, "--crate-name", "mutated_hashing", "--crate-type=rlib", str(folder / "hashing.rs"), "-o", str(library)])
        bridge += ["--cfg", "native_sha_mutant", "--extern", f"mutated_hashing={library}"]
    else:
        require(label in ("checked-debug", "checked-optimized"), "Unknown build label")
    result.append([*bridge, "-o", str(folder / "hash-parts")])
    return result


def outputs(binary, inputs):
    transcript = "".join(":".join(part.hex() for part in parts) + "\n" for parts in inputs)
    lines = run([binary], input=transcript, text=True).stdout.splitlines()
    require(len(lines) == len(inputs) and all(HEX.fullmatch(value) for value in lines), "Invalid native output protocol")
    return lines


def witness(index, inputs, expected, observed):
    return {"case": index, "parts_hex": [p.hex() for p in inputs[index]],
            "expected_sha256": expected[index], "observed_sha256": observed[index]}


def generate(current, work, execution_note):
    require(not work.exists(), "Work directory must be fresh; existing evidence is never overwritten")
    require(not work.is_relative_to(ROOT), "Use an external work directory; do not put build outputs in the repo")
    work.mkdir(parents=True)
    inputs, expected, indices = cases()
    source = SDK.read_text()
    rows = []
    for label in ["checked-debug", "checked-optimized", *MUTATIONS]:
        folder = work / label
        folder.mkdir()
        artifacts = []
        mutation = None
        if label in MUTATIONS:
            before, after = MUTATIONS[label]
            require(source.count(before) == 1, f"Mutation must replace exactly one occurrence: {label}")
            path = folder / "hashing.rs"
            path.write_text(source.replace(before, after, 1))
            artifacts.append(path)
            mutation = {"before": before, "after": after}
        build = commands(current, work, label)
        diagnostics = []
        for command in build:
            diagnostics.append(run(command, text=True).stderr)
        binary = folder / "hash-parts"
        observed = outputs(binary, inputs)
        differences = [i for i, (left, right) in enumerate(zip(expected, observed)) if left != right]
        if mutation:
            require(differences, f"Compiled mutant escaped finite tests: {label}")
            artifacts.append(folder / "libmutated_hashing.rlib")
        else:
            require(not differences, f"Actual SDK disagrees with SHA expected outputs: {label}")
        artifacts.append(binary)
        row = {"id": label, "native_typecheck": "passed", "commands": build, "compiler_stderr": diagnostics,
               "observed": observed, "mismatching_cases": len(differences), "mutation": mutation,
               "first_witness": witness(differences[0], inputs, expected, observed) if differences else None,
               "artifacts": [{**fingerprint(p), "path": p.relative_to(work).as_posix()} for p in artifacts]}
        rows.append(row)
        print(f"PASS {label}: {len(inputs)} requests, {len(differences)} mismatches", flush=True)
    openssl = []
    for index in indices:
        command = [current["tools"]["openssl"], "dgst", "-sha256", "-binary"]
        actual = run(command, input=b"".join(inputs[index])).stdout.hex()
        require(actual == expected[index], "OpenSSL differential mismatch")
        openssl.append({"case": index, "sha256": actual})
    require(context(Path(current["compiler"]["path"])) == current, "Inputs changed during execution")
    return {"schema_version": 1, "status": "passed", "scope": SCOPE, "context": current,
            "platform": {"system": platform.system(), "machine": platform.machine(), "execution_note": execution_note},
            "work_directory": str(work), "case_count": len(inputs), "expected_outputs_sha256": sha(("\n".join(expected) + "\n").encode()),
            "original_modes": 2, "compiled_mutants": 6, "native_process_batches": 8, "native_protocol_requests": 8 * len(inputs),
            "known_answers": load(HERE / "vectors.json")["known_answers"], "openssl_observations": openssl,
            "builds": rows,
            "limitations": ["The installed pinned compiler/std, linker, OS/runtime and mutable files are trusted; file hashes do not authenticate execution.",
                            "Python hashlib may share OpenSSL. The separate /usr/bin/openssl command and fixed known answers are additional checks, not independent cryptographic proofs.",
                            "The original isolated RustCrypto dev oracle uses the same family as clvmr; this harness has no RustCrypto/Cargo dependency.",
                            "No CLVM execution or source/MIR bridge admission is performed by this native-only harness."]}


def check(report, current):
    require(report["schema_version"] == 1 and report["status"] == "passed" and report["scope"] == SCOPE, "Invalid evidence claim")
    require(report["context"] == current, "Stale source/toolchain/tool inputs")
    work = Path(report["work_directory"])
    require(work.is_absolute() and work.resolve() == work and not work.is_relative_to(ROOT), "Invalid work directory")
    inputs, expected, indices = cases()
    require(report["case_count"] == 671 and report["original_modes"] == 2 and report["compiled_mutants"] == 6
            and report["native_process_batches"] == 8 and report["native_protocol_requests"] == 5368, "Incorrect coverage counts")
    require(report["known_answers"] == load(HERE / "vectors.json")["known_answers"], "KAT source mismatch")
    require(report["expected_outputs_sha256"] == sha(("\n".join(expected) + "\n").encode()), "Case/oracle transcript changed")
    require(report["openssl_observations"] == [{"case": i, "sha256": expected[i]} for i in indices], "Missing/changed OpenSSL observations")
    require([row["id"] for row in report["builds"]] == ["checked-debug", "checked-optimized", *MUTATIONS], "Missing/duplicate/unexpected builds")
    source = SDK.read_text()
    for row in report["builds"]:
        label = row["id"]
        require(row["native_typecheck"] == "passed" and row["commands"] == commands(current, work, label), "Build/native prerequisite changed")
        require(len(row["compiler_stderr"]) == len(row["commands"]) and all(isinstance(v, str) for v in row["compiler_stderr"]), "Missing compiler diagnostics")
        observed = row["observed"]
        require(len(observed) == len(inputs) and all(HEX.fullmatch(v) for v in observed), "Incomplete/invalid observed outputs")
        differences = [i for i, (left, right) in enumerate(zip(expected, observed)) if left != right]
        require(row["mismatching_cases"] == len(differences), "Incorrect mutation mismatch count")
        names = [f"{label}/hash-parts"]
        if label in MUTATIONS:
            before, after = MUTATIONS[label]
            require(source.count(before) == 1 and row["mutation"] == {"before": before, "after": after}, "Mutation patch changed")
            mutant = within(work, f"{label}/hashing.rs")
            require(mutant.read_text() == source.replace(before, after, 1), "Mutant is not the exact once-patched SDK")
            require(differences and row["first_witness"] == witness(differences[0], inputs, expected, observed), "Missing/incorrect concrete mutation witness")
            names = [f"{label}/hashing.rs", f"{label}/libmutated_hashing.rlib", *names]
        else:
            require(not differences and row["mutation"] is None and row["first_witness"] is None, "Actual SDK must pass all native cases")
        require([item["path"] for item in row["artifacts"]] == names, "Missing/unexpected retained artifacts")
        for item in row["artifacts"]:
            path = within(work, item["path"])
            require(item == {**fingerprint(path), "path": item["path"]}, "Stale/missing retained artifact")
    return {"cases_per_build": 671, "native_builds": 8, "compiled_mutants": 6, "openssl_cases": len(indices)}


def self_test(report, current):
    controls = []
    def corrupt(name, edit):
        bad = copy.deepcopy(report)
        edit(bad)
        require(bad != report, f"No-op control: {name}")
        try:
            check(bad, current)
        except (KeyError, ValueError, OSError, TypeError):
            controls.append(name)
            return
        raise ValueError(f"Corrupt native evidence passed: {name}")
    corrupt("source_binding", lambda r: r["context"]["sources"].pop())
    corrupt("source_hash", lambda r: r["context"]["sources"][0].update(sha256="0" * 64))
    corrupt("compiler_version", lambda r: r["context"]["compiler"].update(version="unreviewed"))
    corrupt("missing_toolchain_input", lambda r: r["context"]["installed_inputs"].pop())
    corrupt("toolchain_hash", lambda r: r["context"]["installed_inputs"][0].update(sha256="0" * 64))
    corrupt("removed_build", lambda r: r["builds"].pop())
    corrupt("duplicated_build", lambda r: r["builds"].append(r["builds"][0]))
    corrupt("unknown_mutant", lambda r: r["builds"][-1].update(id="unknown"))
    corrupt("native_prerequisite", lambda r: r["builds"][2].update(native_typecheck="failed"))
    corrupt("original_output", lambda r: r["builds"][0]["observed"].__setitem__(0, "0" * 64))
    corrupt("mutant_witness", lambda r: r["builds"][2]["first_witness"].update(expected_sha256="0" * 64))
    corrupt("mutation_patch", lambda r: r["builds"][2]["mutation"].update(after="unchanged"))
    corrupt("mismatch_count", lambda r: r["builds"][2].update(mismatching_cases=0))
    corrupt("incomplete_observations", lambda r: r["builds"][1]["observed"].pop())
    corrupt("uncompiled_command", lambda r: r["builds"][0]["commands"][0].__setitem__(0, "/unreviewed/rustc"))
    corrupt("copied_positive_source", lambda r: r["builds"][0]["commands"][0].__setitem__(-3, "/copied/bridge.rs"))
    corrupt("unchecked_overflow", lambda r: r["builds"][0]["commands"][0].__setitem__(r["builds"][0]["commands"][0].index("overflow-checks=on"), "overflow-checks=off"))
    corrupt("binary_hash", lambda r: r["builds"][0]["artifacts"][0].update(sha256="0" * 64))
    corrupt("binary_path_escape", lambda r: r["builds"][0]["artifacts"][0].update(path="../hash-parts"))
    corrupt("openssl_observation", lambda r: r["openssl_observations"][0].update(sha256="0" * 64))
    corrupt("case_transcript", lambda r: r.update(expected_outputs_sha256="0" * 64))
    corrupt("coverage_count", lambda r: r.update(native_protocol_requests=1))
    corrupt("proof_overclaim", lambda r: r.update(scope="universal verified SHA"))
    binary = within(Path(report["work_directory"]), "checked-debug/hash-parts")
    saved = binary.with_name("hash-parts.saved-for-control")
    require(not saved.exists(), "Stale missing-binary control")
    binary.rename(saved)
    try:
        try:
            check(report, current)
        except OSError:
            controls.append("actual_missing_binary")
        else:
            raise ValueError("Actual missing native binary passed freshness")
    finally:
        saved.rename(binary)
    return controls


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rustc", type=Path, help="Exact pinned rustc binary; default resolves the dated repository toolchain")
    parser.add_argument("--work-dir", type=Path, help="Fresh external directory for retained binaries, required when executing")
    parser.add_argument("--output", type=Path, default=HERE / "results.json")
    parser.add_argument("--execution-note", default="Native host process; physical hardware/emulation is not attested by this runner.")
    parser.add_argument("--check", action="store_true", help="Check current files and recorded results without compiling/executing the kernel")
    parser.add_argument("--self-test", action="store_true", help="Reject deliberately corrupted recorded evidence")
    args = parser.parse_args()
    rustc = args.rustc
    if rustc is None:
        rustc = Path(run(["rustup", "which", "--toolchain", CHANNEL, "rustc"], text=True).stdout.strip())
    current = context(rustc)
    if args.check:
        report = load(args.output)
    else:
        require(args.work_dir is not None, "--work-dir is required for native execution")
        report = generate(current, args.work_dir.resolve(), args.execution_note)
    counts = check(report, current)
    if args.self_test:
        names = self_test(report, current)
        print(f"PASS {len(names)} freshness corruption controls: {', '.join(names)}", flush=True)
    if not args.check:
        args.output.write_text(json.dumps(report, indent=2) + "\n")
    print("PASS", json.dumps(counts), flush=True)


if __name__ == "__main__":
    main()
