"""Independent bounded transport checks for Cargo source declarations.

No compiler, VM, subprocess or original-host source lookup occurs here.
Consistent declarations and attachments do not authenticate compilation, a
complete build graph, or equivalence of source, IR and CLVM.
"""
from dataclasses import dataclass
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import stat

from cargo_authoring import require, unique_object

MAX_JSON_BYTES = 16 * 1024 * 1024
MAX_JSON_DEPTH = 96
MAX_JSON_NODES = 262144
MAX_FILES = 4096
MAX_FILE_BYTES = 2 * 1024 * 1024
MAX_SOURCE_BYTES = 8 * 1024 * 1024
MAX_EXPORTS = 1024
MAX_IR_BYTES = 8 * 1024 * 1024
MAX_READS = 16384
MAX_READ_BYTES = 64 * 1024 * 1024
MAX_DIRECTORY_ENTRIES = 16384
MAX_FEATURES = 128
MAX_FEATURE_BYTES = 64 * 1024
SHA = re.compile(r"[0-9a-f]{64}\Z")
GIT = re.compile(r"[0-9a-f]{40}\Z")
PROTOCOL = "chialisp-oxide-declared-source/v1"
SCOPE = "selected_unit_reads_and_launcher_snapshots"
KINDS = ("compiler_binary", "compiler_text", "launcher_build_script_source",
         "launcher_lockfile_post_cargo", "launcher_manifest")


@dataclass(frozen=True)
class CargoSourceFile:
    label: str
    path: Path
    contents: bytes
    sha256: str
    kinds: tuple
    compiler_read_count: int


def decode_json(payload, *, max_depth=MAX_JSON_DEPTH, max_nodes=MAX_JSON_NODES):
    """Charge lexical depth/nodes before JSON allocates its recursive values.

    This small preflight does not decide JSON grammar. The standard decoder
    still rejects invalid syntax/UTF-8/numbers and the hook rejects duplicate
    decoded keys, including escaped aliases. Whitespace/string scanning keeps
    constant auxiliary space; a key is charged before scanning its contents.
    """
    require(len(payload) <= MAX_JSON_BYTES, "Cargo JSON byte limit exceeded")
    depth, nodes, index = 0, 0, 0
    delimiters = b" \t\r\n,:{}[]"
    while index < len(payload):
        byte = payload[index]
        if byte in b" \t\r\n,:":
            index += 1
            continue
        if byte in b"}]":
            depth -= 1
            require(depth >= 0, "Invalid Cargo JSON delimiters")
            index += 1
            continue
        nodes += 1
        require(nodes <= max_nodes, "Cargo JSON node limit exceeded")
        require(depth + 1 <= max_depth, "Cargo JSON depth limit exceeded")
        if byte in b"{[":
            depth += 1
            index += 1
        elif byte == ord('"'):
            index += 1
            while index < len(payload):
                byte = payload[index]
                index += 1
                if byte == ord('"'):
                    break
                if byte == ord("\\"):
                    if index < len(payload) and payload[index] == ord("u"):
                        code, index = _unicode_escape(payload, index)
                        if 0xD800 <= code <= 0xDBFF:
                            require(payload[index:index + 2] == b"\\u",
                                    "Unpaired Cargo JSON Unicode surrogate")
                            code, index = _unicode_escape(payload, index + 1)
                            require(0xDC00 <= code <= 0xDFFF,
                                    "Invalid Cargo JSON Unicode surrogate pair")
                        else:
                            require(not 0xDC00 <= code <= 0xDFFF,
                                    "Unpaired Cargo JSON Unicode surrogate")
                    else:
                        index += 1
            else:
                raise ValueError("Unterminated Cargo JSON string")
        else:
            start = index
            while index < len(payload) and payload[index] not in delimiters:
                index += 1
            require(index > start, "Invalid Cargo JSON token")
    require(depth == 0, "Incomplete Cargo JSON containers")
    try:
        # json.loads(bytes) also accepts UTF-16/32 and strips a UTF-8 BOM.
        # The wire protocol is strict UTF-8, matching serde_json admission.
        return json.loads(payload.decode("utf-8"), object_pairs_hook=unique_object,
                          parse_float=_finite_float,
                          parse_constant=lambda _: _invalid_number())
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise ValueError("Invalid bounded Cargo JSON") from error


def _invalid_number():
    raise ValueError("Non-JSON numeric constant")


def _finite_float(token):
    value = float(token)
    require(math.isfinite(value), "Nonfinite Cargo JSON number")
    return value


def _unicode_escape(payload, index):
    digits = payload[index + 1:index + 5]
    require(len(digits) == 4 and all(c in b"0123456789abcdefABCDEF" for c in digits),
            "Invalid Cargo JSON Unicode escape")
    return int(digits, 16), index + 5


def _object(value, fields, label):
    require(type(value) is dict and set(value) == set(fields),
            f"Malformed or unknown {label} fields")
    return value


def _text(value, label, limit=65536):
    require(type(value) is str and value and len(value) <= limit
            and len(value.encode("utf-8")) <= limit and "\0" not in value,
            f"Invalid or oversized {label}")
    return value


def _integer(value, maximum, label, minimum=0):
    require(type(value) is int and minimum <= value <= maximum,
            f"Invalid or excessive {label}")
    return value


def _label(value):
    _text(value, "source label", 4096)
    require("\\" not in value and not any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in value),
            "Source label contains control characters")
    path = PurePosixPath(value)
    require(not path.is_absolute() and path.as_posix() == value
            and all(p not in ("", ".", "..") for p in value.split("/")),
            "Source label must be a normalized relative POSIX path")
    return value


def _scoped(value):
    _label(value)
    require(value.startswith(("package/", "generated/"))
            or value in ("workspace/Cargo.toml", "workspace/Cargo.lock",
                         "entry/library.rs", "entry/build-script.rs"),
            "Source label is outside the declared scope")
    return value


def _hash(value, label):
    require(type(value) is str and SHA.fullmatch(value), f"Invalid {label}")
    return value


def _sorted_unique(values, label):
    require(values == sorted(set(values)), f"{label} must be sorted and unique")


def regular_path(root, relative):
    """Check lexical components before opening; trusted local-directory scope."""
    _label(relative)
    path = Path(root)
    try:
        for part in PurePosixPath(relative).parts:
            path /= part
            metadata = path.lstat()
            require(not stat.S_ISLNK(metadata.st_mode), "Cargo symlink alias is unsupported in the named bundle")
    except OSError as error:
        raise ValueError("Cargo input is missing or cannot be inspected") from error
    require(stat.S_ISREG(metadata.st_mode), "Cargo input must be a regular file")
    return path


def _read_source(path, expected_bytes, reader):
    metadata = path.stat()
    require(metadata.st_size == expected_bytes, "Source blob declared length mismatch")
    require(expected_bytes <= min(MAX_FILE_BYTES, reader.remaining),
            "Source blob read budget exceeded")
    with path.open("rb") as contents:
        payload = contents.read(expected_bytes + 1)
    require(len(payload) == expected_bytes, "Source blob changed or has trailing bytes")
    reader.remaining -= len(payload)
    return payload


def validate_receipt(manifest, bundle_root, ir_details, reader, artifact_paths):
    """Admit all source rows/blobs before returning checked attached bytes.

    ir_details maps every export to (relative IR filename, exact length, SHA).
    It comes from actual bounded reads, never a reserialization of Program.
    """
    fields = ("schema_version", "status", "package", "package_id", "manifest_path",
              "workspace_root", "compiler", "toolchain", "analysis_target",
              "driver_sha256", "rue_revision", "cargo_options", "entries",
              "source_receipt")
    _object(manifest, fields, "source-aware Cargo manifest")
    require(type(manifest["schema_version"]) is int and manifest["schema_version"] == 2,
            "Unsupported source-aware bundle version")
    receipt = _object(manifest["source_receipt"],
                      ("protocol", "scope", "build", "unit", "exports", "files",
                       "snapshots", "omitted"), "source receipt")
    require(receipt["protocol"] == PROTOCOL and receipt["scope"] == SCOPE,
            "Unsupported source receipt protocol/scope")
    build = _object(receipt["build"],
                    ("package", "package_id", "compiler_sha256", "toolchain",
                     "analysis_target", "driver_sha256", "rue_revision",
                     "cargo_options", "optimize_lir"), "receipt build")
    for key in ("package", "package_id", "toolchain", "analysis_target",
                "driver_sha256", "rue_revision"):
        _text(manifest[key], f"Cargo {key}")
        require(build[key] == manifest[key], f"Receipt {key} declaration mismatch")
    _hash(build["driver_sha256"], "driver digest")
    require(GIT.fullmatch(build["rue_revision"]), "Invalid Rue revision")
    _text(manifest["compiler"], "compiler declaration")
    require(_hash(build["compiler_sha256"], "compiler text digest")
            == hashlib.sha256(manifest["compiler"].encode("utf-8")).hexdigest(),
            "Receipt compiler text digest mismatch")
    require(type(build["optimize_lir"]) is bool, "Receipt mode must be Boolean")
    options = build["cargo_options"]
    require(type(options) is list and len(options) <= MAX_EXPORTS
            and options == manifest["cargo_options"], "Receipt Cargo options mismatch")
    for flag in options:
        _text(flag, "Cargo option")
    require(all(entry["artifact"]["optimize_lir"] is build["optimize_lir"]
                for entry in manifest["entries"]), "Receipt optimization mode mismatch")
    unit = _object(receipt["unit"],
                   ("crate_name", "analysis_target", "features", "library_source"),
                   "receipt unit")
    crate = _text(unit["crate_name"], "selected crate name", 256)
    require(all(c == "_" or c.isalnum() for c in crate)
            and unit["analysis_target"] == manifest["analysis_target"],
            "Invalid selected source unit")
    features = unit["features"]
    require(type(features) is list and len(features) <= MAX_FEATURES,
            "Source feature limit exceeded")
    feature_bytes = 0
    for feature in features:
        _text(feature, "feature declaration", 4096)
        feature_bytes += len(feature.encode("utf-8"))
        require(feature_bytes <= MAX_FEATURE_BYTES, "Source feature byte limit exceeded")
        require(not any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in feature),
                "Feature declaration contains control characters")
    _sorted_unique(features, "Source features")
    exports = receipt["exports"]
    require(type(exports) is list and 0 < len(exports) <= MAX_EXPORTS,
            "Source export limit exceeded")
    named, ir_paths = {}, set()
    for row in exports:
        _object(row, ("entry", "ir", "bytes", "sha256"), "source export")
        name = _text(row["entry"], "source export name", 256)
        ir = _label(row["ir"])
        size = _integer(row["bytes"], MAX_IR_BYTES, "source IR length", 1)
        digest = _hash(row["sha256"], "source IR digest")
        require(name not in named and ir not in ir_paths, "Duplicate source export/path")
        named[name] = (ir, size, digest)
        ir_paths.add(ir)
    _sorted_unique(list(named), "Source exports")
    require(named == ir_details, "Source receipt differs from complete exact IR inventory")
    omitted = _object(receipt["omitted"],
                      ("outside_roots_reads", "outside_roots_bytes"), "omitted reads")
    reads = _integer(omitted["outside_roots_reads"], MAX_READS, "omitted read count")
    omitted_bytes = _integer(omitted["outside_roots_bytes"], (1 << 64) - 1, "omitted read bytes")
    require(reads or not omitted_bytes, "Omitted bytes without an omitted read")
    # Outside bytes are a declaration of compiler-owned reads. They are not
    # retained, copied or hashed by capture and do not consume its copy budget.
    read_bytes = 0
    files = receipt["files"]
    require(type(files) is list and 0 < len(files) <= MAX_FILES, "Source file limit exceeded")
    by_label, blobs, expanded = {}, {}, 0
    for row in files:
        _object(row, ("path", "kinds", "compiler_read_count", "bytes", "sha256", "blob"),
                "source file")
        label = _scoped(row["path"])
        require(label not in by_label, "Duplicate source label")
        kinds = row["kinds"]
        require(type(kinds) is list and kinds and all(type(k) is str and k in KINDS for k in kinds),
                "Invalid source observation kinds")
        _sorted_unique(kinds, "Source observation kinds")
        compiler_kinds = sum(k in ("compiler_text", "compiler_binary") for k in kinds)
        count = _integer(row["compiler_read_count"], MAX_READS, "source read count")
        require((compiler_kinds == 0 and count == 0) or (compiler_kinds > 0 and count >= compiler_kinds),
                "Compiler read count disagrees with observation kinds")
        require(not compiler_kinds or label.startswith(("package/", "generated/"))
                or label == "entry/library.rs", "Compiler observation outside capture roots")
        size = _integer(row["bytes"], MAX_FILE_BYTES, "source file length")
        expanded += size
        reads += count
        read_bytes += size * count
        require(expanded <= MAX_SOURCE_BYTES and reads <= MAX_READS
                and read_bytes <= MAX_READ_BYTES, "Source aggregate/read budget exceeded")
        digest = _hash(row["sha256"], "source content digest")
        require(row["blob"] == f"source-{digest}.bin", "Source blob filename/digest mismatch")
        require(row["blob"] not in blobs or blobs[row["blob"]] == (size, digest),
                "Conflicting deduplicated source blob")
        blobs[row["blob"]] = (size, digest)
        by_label[label] = row
    _sorted_unique(list(by_label), "Source files")
    snapshots = _object(receipt["snapshots"],
                        ("package_manifest", "workspace_manifest", "lockfile", "build_script"),
                        "source snapshots")
    references = {"launcher_manifest": set(), "launcher_lockfile_post_cargo": set(),
                  "launcher_build_script_source": set()}
    for key, kind in (("package_manifest", "launcher_manifest"),
                      ("workspace_manifest", "launcher_manifest"),
                      ("lockfile", "launcher_lockfile_post_cargo"),
                      ("build_script", "launcher_build_script_source")):
        label = snapshots[key]
        if label is None and key in ("lockfile", "build_script"):
            continue
        _scoped(label)
        require(label in by_label and kind in by_label[label]["kinds"],
                "Snapshot missing its declared source observation")
        references[kind].add(label)
    library = _scoped(unit["library_source"])
    require(library in by_label and "compiler_text" in by_label[library]["kinds"],
            "Selected library missing actual compiler text observation")
    for label, row in by_label.items():
        for kind in row["kinds"]:
            require(kind not in references or label in references[kind],
                    "Unreferenced launcher source declaration")
    present, count = set(), 0
    with os.scandir(bundle_root) as entries:
        for entry in entries:
            count += 1
            require(count <= MAX_DIRECTORY_ENTRIES, "Cargo directory entry limit exceeded")
            require(entry.name != "source-unit.json", "Private source unit cannot be published")
            if entry.name.startswith("source-") and entry.name.endswith(".bin"):
                require(entry.name in blobs and entry.is_file(follow_symlinks=False),
                        "Unexpected reserved source blob or nonregular source")
                present.add(entry.name)
    require(present == set(blobs), "Missing source blob inventory")
    contents = {}
    for blob, (size, digest) in blobs.items():
        path = regular_path(bundle_root, blob)
        require(path.resolve() not in artifact_paths, "Source blob aliases an artifact")
        raw = _read_source(path, size, reader)
        require(hashlib.sha256(raw).hexdigest() == digest, "Source blob content digest mismatch")
        contents[blob] = (path, raw)
    return tuple(CargoSourceFile(label, *contents[row["blob"]], row["sha256"],
                                 tuple(row["kinds"]), row["compiler_read_count"])
                 for label, row in by_label.items())
