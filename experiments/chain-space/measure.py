#!/usr/bin/env python3
"""Measure how much of a pinned mainnet block range is repeated puzzle reveals (#22).

`fetch` reads a pinned range of mainnet transaction blocks from the public Coinset
full-node API into an ignored capture directory and records a SHA-256 digest of
every response. `analyze` uncurries each puzzle reveal, names modules from the
pinned chia-puzzles 0.20.3 constants, and aggregates reveal, module, curried
argument and solution bytes, repetition within a block (what in-block
backreferences can already remove) and repetition across blocks (what a
hypothetical "module by hash" reference would additionally remove), beside each
block's actual generator size.

Measured numbers are observations of this range only. The cross-block figure is a
projection for a protocol change that does not exist; it is not achievable today.
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import sys
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import raw_tree  # noqa: E402

sys.setrecursionlimit(200_000)

ORIGIN = "https://api.coinset.org"
COST_PER_BYTE = 12_000
MAX_RESPONSE = 64 * 1024 * 1024
CHIA_PUZZLES = "chia-puzzles-0.20.3/src/programs.rs"


def post(endpoint, body):
    request = urllib.request.Request(f"{ORIGIN}/{endpoint}", data=json.dumps(body).encode(),
                                     headers={"Content-Type": "application/json", "User-Agent": "chain-space-measure"})
    with urllib.request.urlopen(request, timeout=60) as response:
        raw = response.read(MAX_RESPONSE + 1)
    if len(raw) > MAX_RESPONSE:
        raise ValueError(f"{endpoint}: response exceeds bound")
    document = json.loads(raw)
    if not document.get("success"):
        raise ValueError(f"{endpoint}: unsuccessful response")
    return raw, document


def fetch(start, count, output):
    output.mkdir(parents=True, exist_ok=False)
    height, rows = start, []
    while len(rows) < count:
        _, record = post("get_block_record_by_height", {"height": height})
        record = record["block_record"]
        if record.get("timestamp") is not None:  # transaction blocks only
            header = record["header_hash"]
            spends_raw, _ = post("get_block_spends", {"header_hash": header})
            block_raw, _ = post("get_block", {"header_hash": header})
            name = f"{height:09d}"
            (output / f"{name}.spends.json.gz").write_bytes(gzip.compress(spends_raw, mtime=0))
            (output / f"{name}.block.json.gz").write_bytes(gzip.compress(block_raw, mtime=0))
            rows.append(dict(height=height, header_hash=header,
                             spends_sha256=hashlib.sha256(spends_raw).hexdigest(),
                             block_sha256=hashlib.sha256(block_raw).hexdigest()))
            time.sleep(0.2)
        height += 1
    (output / "index.json").write_text(json.dumps(dict(origin=ORIGIN, start=start, blocks=rows), indent=2) + "\n")


def known_modules():
    """Module names from the workspace's pinned chia-puzzles constants."""
    path = next(Path.home().glob(f".cargo/registry/src/*/{CHIA_PUZZLES}"))
    text = path.read_text()
    found = re.findall(r"pub const ([A-Z0-9_]+)_HASH: \[u8; 32\] =\s*hex!\(\"([0-9a-f]{64})\"\)", text)
    return {digest: name.lower() for name, digest in found}


def serialized_length(tree):
    return len(raw_tree.serialize(tree))


def uncurry(tree):
    """Return (module, [arguments]) for the standard curried form, else None."""
    if type(tree) is not tuple or tree[0] != b"\x02":
        return None
    rest = tree[1]
    if type(rest) is not tuple or type(rest[0]) is not tuple or rest[0][0] != b"\x01":
        return None
    module, environment = rest[0][1], rest[1]
    if type(environment) is not tuple:
        return None
    environment = environment[0]
    arguments = []
    while environment != b"\x01":
        if (type(environment) is not tuple or environment[0] != b"\x04" or type(environment[1]) is not tuple
                or type(environment[1][0]) is not tuple or environment[1][0][0] != b"\x01"):
            return None
        arguments.append(environment[1][0][1])
        environment = environment[1][1][0]
    return module, arguments


def analyze(capture):
    index = json.loads((capture / "index.json").read_text())
    names = known_modules()
    modules, seen_before, blocks, largest = {}, set(), [], []
    totals = dict(spends=0, reveal_bytes=0, solution_bytes=0, module_bytes=0, argument_bytes=0,
                  uncurried_spends=0, in_block_repeat_module_bytes=0, cross_block_repeat_module_bytes=0,
                  generator_bytes=0)
    for row in index["blocks"]:
        name = f"{row['height']:09d}"
        spends_raw = gzip.decompress((capture / f"{name}.spends.json.gz").read_bytes())
        block_raw = gzip.decompress((capture / f"{name}.block.json.gz").read_bytes())
        if hashlib.sha256(spends_raw).hexdigest() != row["spends_sha256"] or hashlib.sha256(block_raw).hexdigest() != row["block_sha256"]:
            raise ValueError(f"{name}: capture digest mismatch")
        generator = json.loads(block_raw)["block"].get("transactions_generator") or "0x"
        generator_bytes = len(bytes.fromhex(generator.removeprefix("0x")))
        in_block = set()
        block = dict(height=row["height"], spends=0, reveal_bytes=0, generator_bytes=generator_bytes)
        for spend in json.loads(spends_raw)["block_spends"]:
            reveal = bytes.fromhex(spend["puzzle_reveal"].removeprefix("0x"))
            solution = bytes.fromhex(spend["solution"].removeprefix("0x"))
            totals["spends"] += 1; block["spends"] += 1
            totals["reveal_bytes"] += len(reveal); block["reveal_bytes"] += len(reveal)
            totals["solution_bytes"] += len(solution)
            parsed = uncurry(raw_tree.deserialize(reveal))
            if parsed is None:
                key, module_size = "not-curried:" + raw_tree.tree_hash(raw_tree.deserialize(reveal)).hex(), len(reveal)
            else:
                module, arguments = parsed
                digest = raw_tree.tree_hash(module).hex()
                key, module_size = digest, serialized_length(module)
                totals["uncurried_spends"] += 1
                totals["argument_bytes"] += sum(serialized_length(a) for a in arguments)
            totals["module_bytes"] += module_size
            stats = modules.setdefault(key, dict(name=names.get(key.split(":")[-1], "unknown"), spends=0, reveal_bytes=0,
                                                 solution_bytes=0, module_bytes=module_size))
            stats["spends"] += 1; stats["reveal_bytes"] += len(reveal); stats["solution_bytes"] += len(solution)
            largest.append((len(solution), row["height"], stats["name"]))
            if key in in_block:
                totals["in_block_repeat_module_bytes"] += module_size
            else:
                in_block.add(key)
                if key in seen_before:
                    totals["cross_block_repeat_module_bytes"] += module_size
        seen_before |= in_block
        totals["generator_bytes"] += generator_bytes
        blocks.append(block)
    top = sorted(modules.items(), key=lambda item: -item[1]["reveal_bytes"])[:12]
    return dict(
        scope=__doc__.strip().split("\n\n")[1],
        origin=index["origin"], first_height=index["blocks"][0]["height"], last_height=index["blocks"][-1]["height"],
        transaction_blocks=len(index["blocks"]), capture_digests=index["blocks"],
        cost_per_generator_byte=COST_PER_BYTE, totals=totals,
        shares=dict(
            module_bytes_of_reveal=round(totals["module_bytes"] / max(totals["reveal_bytes"], 1), 4),
            in_block_repeat_of_reveal=round(totals["in_block_repeat_module_bytes"] / max(totals["reveal_bytes"], 1), 4),
            cross_block_repeat_of_reveal=round(totals["cross_block_repeat_module_bytes"] / max(totals["reveal_bytes"], 1), 4),
            generator_of_raw_spend_bytes=round(totals["generator_bytes"] / max(totals["reveal_bytes"] + totals["solution_bytes"], 1), 4),
            solution_of_raw_spend_bytes=round(totals["solution_bytes"] / max(totals["reveal_bytes"] + totals["solution_bytes"], 1), 4)),
        largest_solutions=[dict(solution_bytes=n, height=h, module=m) for n, h, m in sorted(largest, reverse=True)[:5]],
        top_modules=[dict(module_hash=k, **v) for k, v in top])


def curry(module, arguments):
    """Rebuild the standard curried form (a (q . MODULE) (c (q . A1) ... 1))."""
    environment = b"\x01"
    for argument in reversed(arguments):
        environment = (b"\x04", ((b"\x01", argument), (environment, b"")))
    return (b"\x02", ((b"\x01", module), (environment, b"")))


def storage(capture, level=3):
    """Local spend-table storage: raw, zstd, and a module-deduplicated store (#23)."""
    from compression import zstd
    index = json.loads((capture / "index.json").read_text())
    raw_rows, dedup_rows, modules, round_trips = [], [], {}, 0
    for row in index["blocks"]:
        name = f"{row['height']:09d}"
        spends_raw = gzip.decompress((capture / f"{name}.spends.json.gz").read_bytes())
        if hashlib.sha256(spends_raw).hexdigest() != row["spends_sha256"]:
            raise ValueError(f"{name}: capture digest mismatch")
        for spend in json.loads(spends_raw)["block_spends"]:
            reveal = bytes.fromhex(spend["puzzle_reveal"].removeprefix("0x"))
            solution = bytes.fromhex(spend["solution"].removeprefix("0x"))
            raw_rows.append(reveal + solution)
            parsed = uncurry(raw_tree.deserialize(reveal))
            if parsed is None:
                dedup_rows.append(reveal + solution)
                continue
            module, arguments = parsed
            module_bytes = raw_tree.serialize(module)
            digest = raw_tree.tree_hash(module)
            modules.setdefault(digest, module_bytes)
            argument_bytes = raw_tree.serialize(tuple_list(arguments))
            # Byte-exact round trip of the stored form back to the original reveal.
            rebuilt = raw_tree.serialize(curry(module, arguments))
            if rebuilt != reveal:
                raise ValueError(f"{name}: reveal did not round-trip byte for byte")
            round_trips += 1
            dedup_rows.append(digest + argument_bytes + solution)
    raw = b"".join(raw_rows)
    dedup = b"".join(modules.values()) + b"".join(dedup_rows)
    sizes = dict(raw_spend_table=len(raw), raw_spend_table_zstd=len(zstd.compress(raw, level=level)),
                 dedup_store=len(dedup), dedup_store_zstd=len(zstd.compress(dedup, level=level)),
                 distinct_modules=len(modules), distinct_module_bytes=sum(len(v) for v in modules.values()))
    return dict(scope="Local spend-table storage only: reveal and solution bytes per spend, not a node database, "
                      "block bodies or generator backref encoding. Measured over the same pinned range as report.json.",
                zstd_level=level, spends=len(raw_rows), byte_exact_reveal_round_trips=round_trips, sizes=sizes,
                relative=dict(zstd_vs_raw=round(sizes["raw_spend_table_zstd"] / sizes["raw_spend_table"], 4),
                              dedup_vs_raw=round(sizes["dedup_store"] / sizes["raw_spend_table"], 4),
                              dedup_zstd_vs_raw_zstd=round(sizes["dedup_store_zstd"] / sizes["raw_spend_table_zstd"], 4)))


def tuple_list(items):
    result = b""
    for item in reversed(items):
        result = (item, result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fetch"); f.add_argument("--start", type=int, required=True); f.add_argument("--count", type=int, required=True); f.add_argument("--output", type=Path, required=True)
    a = sub.add_parser("analyze"); a.add_argument("--capture", type=Path, required=True); a.add_argument("--report", type=Path)
    t = sub.add_parser("storage"); t.add_argument("--capture", type=Path, required=True); t.add_argument("--report", type=Path)
    args = parser.parse_args()
    if args.command == "fetch":
        fetch(args.start, args.count, args.output)
    elif args.command == "storage":
        text = json.dumps(storage(args.capture), indent=2) + "\n"
        (args.report.write_text(text) if args.report else sys.stdout.write(text))
    else:
        report = analyze(args.capture)
        text = json.dumps(report, indent=2) + "\n"
        (args.report.write_text(text) if args.report else sys.stdout.write(text))


if __name__ == "__main__":
    main()
