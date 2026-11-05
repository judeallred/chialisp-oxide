"""Bounded data/receipt readers for the executed registry authoring gate.

These readers do not compile or execute programs and supply no execution credit.
The small symbolic reader accepts only closed constructor/call/Apply IR. It
checks the actual fixed applied bytes and opaque environment relation; it is
neither a general IR interpreter nor a compiler-preservation proof.
"""
import hashlib
import json
from pathlib import Path
import re
import stat

from cargo_source_receipt import decode_json
from vm_failures import check_execution

MAX_JSON = 16 * 1024 * 1024
HEX = re.compile(r"(?:[0-9a-f]{2})*")
CRATE = "registry_generated_fixture"
EXPORTS = ("multiply", "standard", "atom255", "atom256", "strict_unused_apply")
TREE = {"kind": "tree"}


class Inconclusive(RuntimeError):
    """A transport/resource/infrastructure failure is not policy rejection."""


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def read(path, maximum=MAX_JSON):
    path = Path(path)
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and not path.is_symlink(), f"not a regular input: {path}")
    require(info.st_size <= maximum, f"input exceeds bounded read: {path}")
    with path.open("rb") as stream:
        raw = stream.read(maximum + 1)
    require(len(raw) <= maximum, f"input grew beyond bounded read: {path}")
    return raw


def file_identity(path, maximum=MAX_JSON):
    path = Path(path)
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and not path.is_symlink() and info.st_size <= maximum,
            f"not a bounded regular identity input: {path}")
    hasher, size = hashlib.sha256(), 0
    with path.open("rb") as stream:
        while chunk := stream.read(min(1024 * 1024, info.st_size - size + 1)):
            size += len(chunk)
            require(size <= info.st_size, f"identity input grew beyond declared length: {path}")
            hasher.update(chunk)
    require(size == info.st_size, f"identity input changed declared length: {path}")
    return {"path": str(path), "bytes": size, "sha256": hasher.hexdigest()}


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


class FrozenFiles:
    """Bounded path-deduplicated identities; declarations are not authentication.

    Each new observation must still match the first observed bytes. Final
    snapshots retain individual read errors and changed bytes so partial failed
    workflows remain inspectable without acquiring successful execution credit.
    """
    def __init__(self, *, max_files=128, max_bytes=64 * 1024 * 1024):
        require(type(max_files) is int and 0 <= max_files <= 128
                and type(max_bytes) is int and 0 <= max_bytes <= 64 * 1024 * 1024,
                "closed frozen input inventory limits")
        self.maximum_files, self.maximum_bytes, self._files = max_files, max_bytes, {}

    def files(self):
        return [dict(self._files[path]) for path in sorted(self._files)]

    def observe(self, paths):
        names, scanned = set(), 0
        for path in paths:
            scanned += 1
            require(scanned <= 128, "frozen input path scan bound")
            require(isinstance(path, (str, Path)), "explicit retained input path required")
            raw_name = str(path)
            require(len(raw_name.encode("utf-8")) <= 4096 and Path(raw_name).is_absolute(),
                    "bounded absolute retained input path required")
            name = str(Path(raw_name))
            if name not in names:
                require(name in self._files or len(self._files) + sum(item not in self._files for item in names) < self.maximum_files,
                        "frozen input file count bound")
                names.add(name)
        names = sorted(names)
        # Existing unobserved entries retain their reserved bytes. Preflight
        # each candidate's length against the remaining aggregate BEFORE its
        # hash reader opens it; a rejected addition never hashes beyond the
        # declared aggregate ceiling or mutates the prior inventory.
        remaining = self.maximum_bytes - sum(row["bytes"] for path, row in self._files.items() if path not in names)
        rows = []
        for path in names:
            row = file_identity(path, min(MAX_JSON, remaining))
            remaining -= row["bytes"]
            rows.append(row)
        for row in rows:
            require(row["path"] not in self._files or self._files[row["path"]] == row,
                    f"previously observed input changed: {row['path']}")
        candidate = dict(self._files)
        candidate.update((row["path"], row) for row in rows)
        require(sum(row["bytes"] for row in candidate.values()) <= self.maximum_bytes,
                "frozen input aggregate byte bound")
        self._files = candidate
        return rows

    def snapshot(self):
        rows, errors, remaining = [], [], self.maximum_bytes
        for path in sorted(self._files):
            try:
                row = file_identity(path, min(MAX_JSON, remaining))
                remaining -= row["bytes"]
                rows.append(row)
            except Exception as error:
                errors.append({"path": path, "error": f"{type(error).__name__}: {error}"})
        return {"status": "unreadable" if errors else "matched" if rows == self.files() else "changed",
                "files": rows, "errors": errors}


def facade_feature_record(path):
    """Check and retain the exact Cargo metadata bytes used for feature scope."""
    raw = read(path, 1024 * 1024)
    value = decode_json(raw)
    require(type(value) is dict and type(value.get("features")) is str
            and json.loads(value["features"]) == ["default"],
            "actual extraction facade unexpectedly enabled vm/alloc")
    return {"file": {"path": str(path), "bytes": len(raw), "sha256": digest(raw)},
            "features": ["default"],
            "scope": "Retained local Cargo feature metadata, not authentication; actual reviewed Apply admission and IR inspection remain separate checks."}


def unhex(value, maximum=2 * 1024 * 1024):
    require(type(value) is str and len(value) <= maximum * 2 and HEX.fullmatch(value),
            "bounded lowercase byte hex required")
    return bytes.fromhex(value)


def proper(items):
    result = b""
    for value in reversed(items):
        result = (value, result)
    return result


def encode(tree):
    pending, output, nodes, size = [(tree, 1)], [], 0, 0
    while pending:
        value, depth = pending.pop()
        nodes += 1
        require(nodes <= 8192 and depth <= 128, "data serialization shape bound")
        if type(value) is tuple:
            require(len(value) == 2, "binary tree pair required")
            raw = b"\xff"
            pending.extend(((value[1], depth + 1), (value[0], depth + 1)))
        else:
            require(type(value) is bytes and len(value) <= 8192, "bounded atom payload required")
            if len(value) == 1 and value[0] < 128:
                raw = value
            elif len(value) < 64:
                raw = bytes([128 + len(value)]) + value
            elif len(value) < 8192:
                raw = bytes([192 + (len(value) >> 8), len(value) & 255]) + value
            else:
                raw = b"\xe0" + len(value).to_bytes(2, "big") + value
        size += len(raw)
        require(size <= 65536, "data serialization byte bound")
        output.append(raw)
    return b"".join(output)


def decode(raw):
    """Bounded canonical plain tree parser, independent of the Rust generator."""
    require(0 < len(raw) <= 65536, "plain data byte bound")
    cursor, nodes, stack = 0, 0, []
    while True:
        require(cursor < len(raw), "truncated plain tree")
        nodes += 1
        require(nodes <= 8192 and len(stack) < 128, "plain data shape bound")
        head = raw[cursor]
        cursor += 1
        if head == 255:
            stack.append(None)
            continue
        if head < 128:
            value = bytes([head])
        elif head == 128:
            value = b""
        else:
            width = 0
            while head & (128 >> width):
                width += 1
            require(width <= 3 and cursor + width - 1 <= len(raw), "unsupported/truncated atom header")
            length = head & ((1 << (7 - width)) - 1)
            for byte in raw[cursor:cursor + width - 1]:
                length = length * 256 + byte
            cursor += width - 1
            require(0 < length <= 8192 and cursor + length <= len(raw), "truncated/oversized atom")
            if width > 1:
                require(length >= 1 << (7 - (width - 1) + 8 * (width - 2)), "nonminimal atom length")
            value = raw[cursor:cursor + length]
            require(not (length == 1 and value[0] < 128), "nonminimal single byte atom")
            cursor += length
        while stack and stack[-1] is not None:
            value = (stack.pop(), value)
        if stack:
            stack[-1] = value
        else:
            require(cursor == len(raw) and encode(value) == raw, "complete canonical data required")
            return value


def identity(raw):
    tree = decode(raw)
    pending, hashes = [(tree, False)], []
    while pending:
        value, visited = pending.pop()
        if type(value) is bytes:
            hashes.append(hashlib.sha256(b"\x01" + value).digest())
        elif visited:
            rest, first = hashes.pop(), hashes.pop()
            hashes.append(hashlib.sha256(b"\x02" + first + rest).digest())
        else:
            pending.extend(((value, True), (value[1], False), (value[0], False)))
    require(len(hashes) == 1, "plain hash traversal imbalance")
    return {"encoding": "clvm-legacy-plain/v1", "bytes": len(raw),
            "blob_sha256": digest(raw), "module_tree_hash": hashes[0].hex()}


def inspect_compiled_artifact(raw, *, max_bytes=2 * 1024 * 1024, max_nodes=262144, max_depth=4096):
    """Independent canonical-plain output inspection without a decoded tree.

    Compiled wrappers are not source fixture data: the fixed source generator
    can produce wrappers with more nodes/depth than their embedded module.
    These output transport caps match the existing package admission defaults
    in package/src/limits.rs. They change no compiler, generator, fixture codec,
    VM or policy limit. Actual complete package admission remains a later gate.
    Runtime is bounded by bytes/nodes; retained hash frames by maximum depth.
    """
    for value, cap in ((max_bytes, 2 * 1024 * 1024), (max_nodes, 262144), (max_depth, 4096)):
        require(type(value) is int and 0 <= value <= cap, "compiled transport limit cannot exceed package defaults")
    require(type(raw) is bytes and 0 < len(raw) <= max_bytes, "compiled transport byte bound")
    view = memoryview(raw)
    cursor, nodes, pairs, atoms, atom_bytes, depth_peak = 0, 0, 0, 0, 0, 0
    frames = []
    while True:
        depth = len(frames) + 1
        require(nodes < max_nodes and depth <= max_depth and cursor < len(raw),
                "compiled transport shape/truncation bound")
        nodes += 1
        depth_peak = max(depth_peak, depth)
        start, head = cursor, raw[cursor]
        cursor += 1
        if head == 255:
            # Both children must exist within the declared depth/node budgets.
            # Check before growing even this bounded hash-only frame stack.
            require(depth < max_depth and nodes + 2 <= max_nodes, "compiled transport pair shape bound")
            pairs += 1
            frames.append(None)
            continue
        atoms += 1
        if head < 128:
            payload = view[start:cursor]
        elif head == 128:
            payload = view[cursor:cursor]
        else:
            require(head != 254 and not raw.startswith(b"\xfd\xff2026", start),
                    "compiled transport requires canonical legacy plain encoding")
            width = 0
            while head & (128 >> width):
                width += 1
            require(1 <= width <= 6 and cursor + width - 1 <= len(raw),
                    "compiled transport invalid/truncated atom header")
            length = head & ((1 << (7 - width)) - 1)
            for byte in view[cursor:cursor + width - 1]:
                length = length * 256 + byte
            cursor += width - 1
            require(length < 0x400000000 and (width == 1 or length >= 1 << (7 * width - 8)),
                    "compiled transport noncanonical atom length")
            require(length > 0 and length <= max_bytes and cursor + length <= len(raw),
                    "compiled transport truncated/oversized atom payload")
            require(not (length == 1 and raw[cursor] < 128), "compiled transport noncanonical small atom")
            payload = view[cursor:cursor + length]
            cursor += length
        atom_bytes += len(payload)
        hasher = hashlib.sha256(b"\x01")
        hasher.update(payload)
        value = hasher.digest()
        while frames and frames[-1] is not None:
            value = hashlib.sha256(b"\x02" + frames.pop() + value).digest()
        if frames:
            frames[-1] = value
        else:
            require(cursor == len(raw), "compiled transport trailing serialization")
            return {
                "identity": {"encoding": "clvm-legacy-plain/v1", "bytes": len(raw),
                             "blob_sha256": digest(raw), "module_tree_hash": value.hex()},
                "stats": {"nodes": nodes, "pairs": pairs, "atoms": atoms,
                          "atom_payload_bytes": atom_bytes, "depth_root_one": depth_peak},
                "limits": {"max_blob_bytes": max_bytes, "max_nodes": max_nodes, "max_depth_root_one": max_depth},
                "scope": "Independent bounded streaming canonical-plain compiled-output identity/shape only; no decoded fixture tree, VM execution, package/source authenticity or compiler preservation. Package admission remains separate.",
            }


def text(tree):
    """Render bounded fixture trees as exact dotted CLVM assembly."""
    encode(tree)
    def item(value):
        return ("(" + item(value[0]) + " . " + item(value[1]) + ")"
                if type(value) is tuple else "0x" + value.hex() if value else "()")
    return item(tree)


def normalized(execution, label):
    outcome = check_execution(execution, label, require_conclusive=False, require_typed=True)
    if outcome["status"] == "inconclusive":
        raise Inconclusive(f"{label}: {execution['failure']['kind']}/{execution['failure']['code']}")
    return outcome if outcome["status"] == "accepted" else {
        "status": "rejected", "code": execution["failure"]["code"]}


class Construction:
    """Closed actual IR construction, preserving opaque caller identity."""
    def __init__(self, envelope, export, module, prefix):
        require(type(envelope) is dict and set(envelope) == {"ir_version", "environment", "program"}
                and envelope["ir_version"] == 4 and envelope["environment"] == "raw", "v4 raw construction required")
        program = envelope["program"]
        require(set(program) == {"name", "parameters", "result", "body", "functions"}
                and program["name"] == CRATE + "::" + export
                and program["parameters"] == [{"name": "solution" if export in ("multiply", "standard", "strict_unused_apply") else "environment", "ty": TREE}]
                and program["result"] == TREE, "actual generated export identity/ABI mismatch")
        require(program["body"] == {"op": "call", "function_id": 0, "arguments": [{"op": "arg", "index": 0}]}, "actual entry dispatch mismatch")
        self.functions, self.module, self.prefix = program["functions"], module, prefix
        self.argument, self.result = object(), object()
        self.active, self.seen, self.applies, self.work = set(), set(), [], 0
        require(type(self.functions) is list and 1 <= len(self.functions) <= 64, "bounded complete function table required")
        names = set()
        for index, function in enumerate(self.functions):
            require(set(function) == {"id", "name", "parameters", "result", "body"}
                    and function["id"] == index and type(function["id"]) is int
                    and type(function["name"]) is str and function["name"].startswith(CRATE + "::")
                    and function["name"] not in names, "closed unique local functions required")
            names.add(function["name"])
            require(1 <= len(function["parameters"]) <= 64 and function["result"] == TREE
                    and function["parameters"] == [{"name": f"arg{i}", "ty": TREE} for i in range(len(function["parameters"]))], "only Tree helper signatures admitted")

    def value(self, expression, arguments, locals_, depth=0):
        self.work += 1
        require(self.work <= 500000 and depth < 64 and type(expression) is dict, "construction work/depth bound")
        op = expression.get("op")
        if op == "arg":
            require(set(expression) == {"op", "index"} and type(expression["index"]) is int
                    and 0 <= expression["index"] < len(arguments), "actual argument identity")
            return arguments[expression["index"]]
        if op == "local":
            require(set(expression) == {"op", "local_id"} and type(expression["local_id"]) is int
                    and expression["local_id"] in locals_, "actual lexical identity")
            return locals_[expression["local_id"]]
        if op == "tree_atom":
            require(set(expression) == {"op", "value"} and set(expression["value"]) == {"op", "hex"}
                    and expression["value"]["op"] == "atom", "fixed atom constructor required")
            return unhex(expression["value"]["hex"], 256)
        if op == "tree_pair":
            require(set(expression) == {"op", "first", "rest"}, "pair constructor fields")
            return (self.value(expression["first"], arguments, locals_, depth + 1),
                    self.value(expression["rest"], arguments, locals_, depth + 1))
        if op == "call":
            require(set(expression) == {"op", "function_id", "arguments"}
                    and type(expression["arguments"]) is list and 1 <= len(expression["arguments"]) <= 64,
                    "bounded call fields")
            return self.function(expression["function_id"], [self.value(value, arguments, locals_, depth + 1)
                                                           for value in expression["arguments"]])
        if op == "apply":
            require(set(expression) == {"op", "program", "environment"}, "Apply fields")
            module = self.value(expression["program"], arguments, locals_, depth + 1)
            environment = self.value(expression["environment"], arguments, locals_, depth + 1)
            require(encode(module) == self.module, "actual applied bytes differ from generated locked module")
            require(environment is self.argument if self.prefix is None else
                    type(environment) is tuple and environment[0] == self.prefix and environment[1] is self.argument,
                    "Apply must preserve the complete opaque raw environment")
            self.applies.append(identity(self.module))
            require(len(self.applies) == 1, "one strict actual Apply required")
            return self.result
        raise ValueError(f"operation outside closed construction: {op!r}")

    def function(self, index, arguments):
        require(type(index) is int and 0 <= index < len(self.functions) and index not in self.active
                and index not in self.seen and len(self.active) < 64, "once-only acyclic local construction required")
        function = self.functions[index]
        require(len(arguments) == len(function["parameters"]), "actual call arity")
        self.active.add(index)
        self.seen.add(index)
        expression, locals_ = function["body"], {}
        while type(expression) is dict and expression.get("op") == "let":
            self.work += 1
            require(self.work <= 500000 and set(expression) == {"op", "local_id", "ty", "value", "body"}
                    and type(expression["local_id"]) is int and expression["local_id"] not in locals_
                    and expression["ty"] == TREE and len(locals_) < 2048, "bounded strict Tree bindings required")
            locals_[expression["local_id"]] = self.value(expression["value"], arguments, locals_)
            expression = expression["body"]
        result = self.value(expression, arguments, locals_)
        self.active.remove(index)
        return result

    def inspect(self, discarded=False):
        result = self.function(0, [self.argument])
        require(self.seen == set(range(len(self.functions))) and len(self.applies) == 1,
                "complete actual helper closure and strict Apply required")
        require(result == b"" if discarded else result is self.result,
                "actual strict application result relation differs")
        return {"functions": len(self.functions), "work": self.work, "apply_count": 1,
                "applied_artifact": self.applies[0], "discarded_result": discarded,
                "environment": "complete opaque caller" if self.prefix is None else "named fixed atom paired with complete opaque caller",
                "scope": "Closed actual IR/data inspection only; no compiler preservation claim."}
