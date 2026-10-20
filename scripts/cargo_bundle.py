"""Load a fresh declared Cargo export without assuming its policy body.

Call only after a successful build into a fresh output directory. The caller
supplies independently captured compiler identity and a source-snapshot callback
which is checked before and after reading. This binds declared local inputs and
the requested build metadata; it is not authenticated build provenance or an IR,
compiler-preservation, CLVM-content, or complete-build-environment proof.

The proof/checker must still deserialize and validate the selected Program,
check its intended ABI, and independently compile/replay its semantics. Listed
CLVM artifacts are checked for publication completeness and consistency with
their manifest metadata, not equivalence to the exported IR.
"""

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path, PurePosixPath
import re

from cargo_authoring import require, unique_object


PROTOCOL = "cargo-chialisp-oxide-artifact/v1"
MAX_JSON_BYTES = 16 * 1024 * 1024
MAX_TOTAL_BYTES = 64 * 1024 * 1024
MAX_EXPORTS = 1024
MAX_SOURCE_FILES = 4096
MAX_SOURCE_FILE_BYTES = 16 * 1024 * 1024
MAX_TOTAL_SOURCE_BYTES = 64 * 1024 * 1024
HEX = re.compile(r"(?:[0-9a-f]{2})+")
SHA256 = re.compile(r"[0-9a-f]{64}")


@dataclass(frozen=True)
class CargoExport:
    program: dict
    path: Path
    manifest_path: Path
    manifest: dict
    ir_sha256: str


def _relative(raw):
    require(isinstance(raw, str) and raw and "\\" not in raw,
            "Cargo artifact path must be a normalized relative POSIX path")
    path = PurePosixPath(raw)
    require(not path.is_absolute() and path.parts and path.as_posix() == raw
            and all(part not in (".", "..") for part in path.parts),
            f"Cargo artifact path is not normalized/confined: {raw}")
    return path


def _contained(root, relative):
    path = root.joinpath(*_relative(relative).parts).resolve()
    require(path.is_relative_to(root) and path.is_file(),
            f"Cargo artifact is missing or escapes its bundle: {relative}")
    return path


class _Reader:
    def __init__(self):
        self.remaining = MAX_TOTAL_BYTES

    def bytes(self, path):
        limit = min(MAX_JSON_BYTES, self.remaining)
        with path.open("rb") as source:
            payload = source.read(limit + 1)
        require(len(payload) <= limit, f"Cargo bundle read budget exceeded: {path}")
        self.remaining -= len(payload)
        return payload

    def json(self, path):
        payload = self.bytes(path)
        try:
            value = json.loads(payload, object_pairs_hook=unique_object)
        except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
            raise ValueError(f"Invalid Cargo artifact JSON: {path}") from error
        require(isinstance(value, dict), f"Cargo artifact must be an object: {path}")
        return value, payload


def _identity(program):
    """Read the identity of supported wire versions; not Program validation."""
    legacy = {"name", "parameters", "result", "body"}
    if "ir_version" in program:
        require(set(program) == {"ir_version", "environment", "program"},
                "Malformed versioned Cargo IR envelope")
        version, environment = program["ir_version"], program["environment"]
        require(type(version) is int and version in (2, 3, 4, 5, 6),
                "Unsupported Cargo IR version")
        require((version == 2 and environment == "ignored")
                or (version == 3 and environment == "raw")
                or (version in (4, 5, 6) and environment in ("strict", "ignored", "raw")),
                "Unsupported Cargo IR environment/version combination")
        program = program["program"]
        fields = legacy | ({"functions"} if version in (4, 5, 6) else set())
        require(isinstance(program, dict) and set(program) == fields,
                "Malformed versioned Cargo IR program")
    else:
        require(set(program) == legacy, "Malformed legacy strict Cargo IR program")
    require(isinstance(program["name"], str) and program["name"],
            "Cargo IR requires an export name")
    return program["name"]


def _absolute(raw, label):
    require(isinstance(raw, str) and Path(raw).is_absolute(),
            f"Cargo {label} must be absolute")
    return Path(raw).resolve()


def _expected(root, authoring, selected_entry):
    require(isinstance(authoring, dict) and authoring.get("mode") == "cargo_library",
            "Expected a declared Cargo-library snapshot")
    require(isinstance(authoring.get("package"), str) and authoring["package"],
            "Expected a declared Cargo package")
    _relative(authoring["manifest"])
    _relative(authoring["library"])
    exports = authoring.get("exports")
    require(isinstance(exports, dict) and 0 < len(exports) <= MAX_EXPORTS,
            "Expected a bounded nonempty declared Cargo export set")
    names = []
    for export in exports.values():
        require(isinstance(export, dict) and set(export) == {"entry", "source"}
                and isinstance(export["entry"], str) and export["entry"],
                "Malformed declared Cargo export")
        _relative(export["source"])
        names.append(export["entry"])
    require(len(names) == len(set(names)) and selected_entry in names,
            "Duplicate declared exports or selected entry is undeclared")
    sources = authoring.get("sources")
    require(isinstance(sources, list) and 0 < len(sources) <= MAX_SOURCE_FILES,
            "Missing or excessive declared Cargo input fingerprints")
    paths = []
    for source in sources:
        require(isinstance(source, dict) and set(source) == {"path", "sha256"}
                and isinstance(source["sha256"], str) and SHA256.fullmatch(source["sha256"]),
                "Malformed declared Cargo input fingerprint")
        _contained(root, source["path"])
        paths.append(source["path"])
    require(len(paths) == len(set(paths)) and {authoring["manifest"], authoring["library"]}
            | {export["source"] for export in exports.values()} <= set(paths),
            "Duplicate Cargo fingerprints or missing declared source/manifest/library")
    return set(names)


def _check_fingerprints(root, authoring):
    """Rehash bytes independently even if the caller accidentally caches metadata."""
    remaining = MAX_TOTAL_SOURCE_BYTES
    for source in authoring["sources"]:
        path = _contained(root, source["path"])
        limit = min(MAX_SOURCE_FILE_BYTES, remaining)
        digest, consumed = hashlib.sha256(), 0
        with path.open("rb") as contents:
            while True:
                chunk = contents.read(min(65536, limit - consumed + 1))
                if not chunk:
                    break
                consumed += len(chunk)
                require(consumed <= limit, f"Declared Cargo source read budget exceeded: {source['path']}")
                digest.update(chunk)
        remaining -= consumed
        require(digest.hexdigest() == source["sha256"],
                f"Declared Cargo source fingerprint changed: {source['path']}")


def _artifact(entry, root, reader, paths, optimize):
    require(set(entry) == {"entry", "ir", "clvm", "artifact"},
            "Malformed Cargo artifact entry")
    ir_path = _contained(root, entry["ir"])
    clvm_path = _contained(root, entry["clvm"])
    for path in (ir_path, clvm_path):
        require(path not in paths, "Cargo entries alias an artifact file")
        paths.add(path)
    program, payload = reader.json(ir_path)
    require(_identity(program) == entry["entry"], "Cargo IR export identity mismatch")
    artifact = entry["artifact"]
    require(isinstance(artifact, dict) and artifact.get("backend") == "rust_mir"
            and type(artifact.get("optimize_lir")) is bool
            and artifact["optimize_lir"] == optimize
            and type(artifact.get("bytes")) is int and artifact["bytes"] > 0
            and isinstance(artifact.get("clvm_hex"), str) and HEX.fullmatch(artifact["clvm_hex"])
            and len(artifact["clvm_hex"]) == 2 * artifact["bytes"]
            and isinstance(artifact.get("tree_hash"), str) and SHA256.fullmatch(artifact["tree_hash"])
            and isinstance(artifact.get("disassembly"), str) and artifact["disassembly"],
            "Malformed Cargo CLVM artifact metadata")
    require(reader.bytes(clvm_path) == (artifact["clvm_hex"] + "\n").encode("ascii"),
            "Published Cargo CLVM file differs from its manifest metadata")
    return program, ir_path, hashlib.sha256(payload).hexdigest()


def load_declared_export(output, *, root, authoring, selected_entry, compiler,
                         driver_sha256, toolchain, current_authoring, optimize,
                         rue_revision=None, cargo_options=None):
    """Load one export while checking the complete declared bundle's identity.

    ``compiler`` is the full independently queried pinned ``rustc -vV`` output;
    ``driver_sha256`` is captured from the executable before extraction. The
    required callback must recompute the declared metadata AND source hashes,
    rather than returning a cached snapshot. No processes are spawned here.
    ``optimize`` is separate from Cargo-only options: the launcher consumes its
    outer ``--no-optimize`` flag before recording those Cargo options.
    """
    root, output = Path(root).resolve(), Path(output).resolve()
    require(callable(current_authoring), "A fresh Cargo authoring snapshot callback is required")
    require(current_authoring() == authoring, "Declared Cargo inputs changed before bundle loading")
    expected = _expected(root, authoring, selected_entry)
    _check_fingerprints(root, authoring)
    require(type(optimize) is bool, "Expected an explicit Boolean optimization mode")
    require(cargo_options is None or (isinstance(cargo_options, list)
            and all(isinstance(flag, str) for flag in cargo_options)),
            "Requested Cargo options must be a list of strings")
    require(isinstance(compiler, str) and compiler.startswith("rustc ")
            and isinstance(driver_sha256, str) and SHA256.fullmatch(driver_sha256)
            and isinstance(toolchain, str) and toolchain,
            "Missing independently captured compiler identity")
    hosts = [line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: ")]
    require(len(hosts) == 1 and hosts[0], "Expected a full rustc -vV compiler identity")
    reader = _Reader()
    pointer, _ = reader.json(_contained(output, "latest.json"))
    require(set(pointer) == {"bundle", "manifest"}, "Malformed atomic Cargo bundle pointer")
    bundle = _relative(pointer["bundle"])
    require(len(bundle.parts) == 1 and pointer["manifest"] == f"{bundle}/manifest.json",
            "Malformed atomic Cargo bundle publication path")
    bundle_root = output.joinpath(*bundle.parts).resolve()
    require(bundle_root.is_relative_to(output) and bundle_root.is_dir(),
            "Cargo bundle directory escapes its publication root")
    manifest_path = _contained(output, pointer["manifest"])
    require(manifest_path.parent == bundle_root, "Cargo manifest escapes its named bundle")
    manifest, _ = reader.json(manifest_path)
    require(type(manifest.get("schema_version")) is int and manifest["schema_version"] == 1
            and manifest.get("status") == "wip_prototype_not_ready_for_use",
            "Unsupported Cargo bundle schema/status")
    require(manifest.get("package") == authoring["package"]
            and _absolute(manifest.get("manifest_path"), "manifest path")
            == _contained(root, authoring["manifest"])
            and _absolute(manifest.get("workspace_root"), "workspace root") == root,
            "Cargo bundle does not describe the declared package/manifest/workspace")
    require(manifest.get("compiler") == compiler and manifest.get("driver_sha256") == driver_sha256
            and manifest.get("toolchain") == toolchain and manifest.get("analysis_target") == hosts[0],
            "Cargo bundle compiler/driver/toolchain/analysis target identity mismatch")
    require(rue_revision is None or manifest.get("rue_revision") == rue_revision,
            "Cargo bundle Rue revision mismatch")
    require(isinstance(manifest.get("cargo_options"), list)
            and all(isinstance(flag, str) for flag in manifest["cargo_options"]),
            "Recorded Cargo options must be a list of strings")
    require(cargo_options is None or manifest.get("cargo_options") == cargo_options,
            "Cargo bundle arguments differ from the requested build")
    entries = manifest.get("entries")
    require(isinstance(entries, list) and len(entries) == len(expected),
            "Cargo export count differs from its declaration")
    names = [entry.get("entry") if isinstance(entry, dict) else None for entry in entries]
    require(all(isinstance(name, str) for name in names) and len(names) == len(set(names))
            and set(names) == expected, "Cargo export identities differ from their declarations")
    loaded, paths = {}, set()
    for entry in entries:
        loaded[entry["entry"]] = _artifact(entry, manifest_path.parent, reader, paths, optimize)
    require(current_authoring() == authoring, "Declared Cargo inputs changed during bundle loading")
    _check_fingerprints(root, authoring)
    program, path, digest = loaded[selected_entry]
    return CargoExport(program, path, manifest_path, manifest, digest)
