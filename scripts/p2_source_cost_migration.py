"""Closed source-only P2 cost transition after actual current validation.

Recorded consistency is separate from execution, authentication, compiler
preservation and universal cost dominance. The new profile stays pending until
an independent final capture is reviewed. Restored history is never current
source input. Existing four cost/append profiles remain immutable.
"""
import copy
import importlib.util
import json
from pathlib import Path
import re
import tomllib

from cargo_authoring import contained_file, unique_object
import p2_append_migration as append
import runtime_cost_migration as cost
from password_vectors import proper, serialize, parse
import p2_singleton_evidence as ledger

ROOT = Path(__file__).resolve().parents[1]
PATH = ROOT / "corpus/p2-source-cost-baseline.json"
PIN = "ee2689f1bdf3a1fd275f3d2374068b38b12d0f52c3135fe8ee5de455a5c8bb5a"
SOURCE = "puzzles/p2-singleton/puzzle.rs"
BEFORE_SOURCE = "1ef0b592106b0d497d21b2f28c8c675e5a714a647142a4bc9c377be5f586d7f9"
AFTER_SOURCE = "72140f55999d8bdfe431372b4f380973e91be19ef6364a546255ddcba9b6f281"
BEFORE_CANONICAL = "b95dfeb63fb1df2cc980135644b35c41cd50f04bad508dd71af5519c5b82c89b"
BASELINE = {"commit":"e20f6d126aedbf76a8c179701af654ca96550074",
    "tree_sha1":"6f795121c9998515bb0e38c16cf6d00924254492",
    "report_path":"corpus/validation.json", "report_git_blob_sha1":"4e739b7cd0f55130742ae44e8d0a91aac2cecf11",
    "report_sha256":"104e4e706e1183cff556ee0a559b8410ff6a5cb6b76bc8a512c04eb3a00d31da",
    "canonical_report_sha256":BEFORE_CANONICAL}
IMMUTABLE = {"corpus/cost-optimization-baseline.json":cost.PIN,
    "corpus/helper-inlining-baseline.json":"0b7326f83065ab1bc186a1e9b0d3b5c581fa7cc3110845cc4b6db70a39b5da52",
    "corpus/runtime-refinement-baseline.json":"11f14c3af4b400bf1569a8a7e3991c381e584a1415057610aedb0d0a26a88d41",
    "corpus/p2-singleton-append-baseline.json":append.PIN}
RETAINED = {"p2":"corpus/p2-source-cost-before-p2.json",
    "metrics":"corpus/p2-source-cost-before-metrics.json",
    "controls":"corpus/p2-source-cost-before-controls.json"}
METRICS_PATH = "corpus/p2-singleton-costs.json"
CONTROLS_PATH = "corpus/p2-source-cost-controls.json"
CONTROLS_SCOPE = "Three actual native Rust policy-mutation tests, a fresh three-export Cargo identity check and six typed VM counterexamples captured separately. Recorded readers establish source/metadata consistency only; no authenticated execution, consensus admission, compiler preservation or generic security claim."
IDENTITY_SCOPE = "Fresh caller-supplied three-export Cargo bundle identity and current source consistency only. Exact reviewed Rust mutations are checked separately from IR identity. The main declared Cargo/SDK and seven private inputs are checked before/after loading. This gate does not extract, validate Program semantics, compile or replay native/CLVM programs, authenticate execution, capture the complete build environment, or prove compiler/IR/CLVM preservation. Existing Rust replay performs Program validation and six finite VM comparisons."
REPLAY_SCOPE = "Three actual ordinary Rust policy mutations admitted through Cargo. Native tests and six CLVM runs distinguish the independent protocol relation. This is not an error Rust makes impossible or constructed-IR mutation."
ENTRY = "chialisp_oxide_puzzles::p2_singleton::puzzle"
require, sha, canonical, same = cost.require, cost.sha, cost.canonical, cost.same


def read(path, *, limit=4 * 1024 * 1024):
    with contained_file(ROOT, path).open("rb") as stream:
        payload = stream.read(limit + 1)
    require(len(payload) <= limit, "Oversized source-cost evidence: " + path)
    return payload


def digest(value):
    return type(value) is str and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def load(payload=None):
    payload = read(str(PATH.relative_to(ROOT))) if payload is None else payload
    require(sha(payload) == PIN, "Source-cost reviewed profile bytes changed")
    value = json.loads(payload, object_pairs_hook=unique_object)
    ledger.closed(value, ["schema_version", "protocol", "status", "scope", "baseline",
        "immutable_profiles", "retained", "source_transition", "reviewed_after"], "source-cost profile")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1
        and value["protocol"] == "chialisp-oxide-p2-source-cost/v1"
        and value["status"] in ("pending_root_final_capture", "reviewed_final_source_cost")
        and type(value["scope"]) is str and value["scope"]
        and value["baseline"] == BASELINE and value["immutable_profiles"] == IMMUTABLE,
        "Unsupported source-cost profile or signed93 baseline")
    require((value["reviewed_after"] is None) if value["status"] == "pending_root_final_capture"
            else type(value["reviewed_after"]) is dict, "Changed pending/final source-cost capture state")
    for path, pin in IMMUTABLE.items():
        require(sha(read(path,limit=16*1024*1024)) == pin, "Changed immutable predecessor: " + path)
    require(value["source_transition"] == {"path":SOURCE,"before_sha256":BEFORE_SOURCE,
        "after_sha256":AFTER_SOURCE,"changed_main_authoring_inputs":[SOURCE]},
        "Source-cost permits only the reviewed direct P2 source transition")
    ledger.closed(value["retained"], RETAINED, "source-cost retained inputs")
    for name, path in RETAINED.items():
        row = value["retained"][name]
        ledger.closed(row, ["path", "sha256"], "retained " + name)
        require(row["path"] == path and digest(row["sha256"])
                and sha(read(path)) == row["sha256"], "Changed retained source-cost baseline " + name)
    return value


def before_p2(profile=None):
    profile = load() if profile is None else profile
    value = json.loads(read(profile["retained"]["p2"]["path"]), object_pairs_hook=unique_object)
    require(value["slug"] == append.ADDED and sha(value["source"].encode()) == BEFORE_SOURCE
            and sha(canonical(value)) == "38060007922e623e3d8a5378f3815498b207b821cb793640091df846d544cecc",
            "Changed complete released93 P2 row")
    return value


def validate_before_controls(profile=None):
    profile = load() if profile is None else profile
    value = json.loads(read(profile["retained"]["controls"]["path"]),object_pairs_hook=unique_object)
    ledger.closed(value,["schema_version","protocol","baseline_commit","baseline_source_sha256","recipe_sha256",
        "recipe","private_sources","identity_checker_sha256","scope"],"released93 control retention")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1
        and value["protocol"] == "chialisp-oxide-p2-source-cost-before-controls/v1"
        and value["baseline_commit"] == "93e50c82e80d675f59d304b962f8a8828c69a041" and value["baseline_source_sha256"] == BEFORE_SOURCE
        and value["recipe_sha256"] == "531223d0ed304ec044df9b3017321244c14c1d30d2c2b06ff97b8358318bb5f7"
        and value["identity_checker_sha256"] == "486d09bc7da5386838ae445b6386193e754cdaaebbb9b4567847f8665465b14a",
        "Changed released93 source-control pins")
    recipe = value["recipe"]
    require(sha((json.dumps(recipe,indent=2)+"\n").encode()) == value["recipe_sha256"],
        "Retained old recipe does not reconstruct its original bytes")
    text = before_p2(profile)["source"]
    names = ("proper_struct","reversed_parameters","wrong_dollar")
    files = ["Cargo.toml","Cargo.lock","mutations.json","src/lib.rs",*("src/"+n+".rs" for n in names)]
    rows = value["private_sources"]
    require([r["path"] for r in rows] == sorted(files),"Changed old private seven-file closure")
    pins = {r["path"]:r["sha256"] for r in rows}
    for row in rows:
        ledger.closed(row,["path","sha256"],"old private input")
        require(digest(row["sha256"]),"Malformed old private input digest")
    require(pins["mutations.json"] == value["recipe_sha256"],"Retained old recipe/source pin differs")
    for row,name in zip(recipe["mutations"],names,strict=True):
        require(row["name"] == name and row["original_sha256"] == BEFORE_SOURCE
            and text.count(row["old"]) == 1
            and sha(text.replace(row["old"],row["new"],1).encode()) == pins["src/"+name+".rs"],
            "Retained old mutation cannot be reconstructed")
    return {"retained_private_inputs":7,"reconstructible_source_mutations":3,
        "scope":"Released93 data/recipe reconstruction only; no current freshness or new execution credit."}


def is_transition(report):
    rows = [p for p in report.get("puzzles", []) if p.get("slug") == append.ADDED]
    return bool(rows) and (len(rows) != 1 or type(rows[0].get("source")) is not str
                          or sha(rows[0]["source"].encode()) != BEFORE_SOURCE)


def reviewed_after(report):
    profile = load()
    require(profile["status"] == "reviewed_final_source_cost",
            "P2 source-cost profile pending root final capture/review; no promotion")
    after = profile["reviewed_after"]
    ledger.closed(after, ["report_sha256", "canonical_report_sha256", "p2_sha256", "protocol_sha256",
        "raw_ir_sha256", "metrics_sha256", "controls_sha256", "bundles"], "source-cost reviewed capture")
    for key in set(after) - {"bundles"}:
        require(digest(after[key]), "Missing final source-cost capture identity: " + key)
    ledger.closed(after["bundles"],["on","off"],"source-cost paired capture")
    for mode,name in ((True,"on"),(False,"off")):
        pin = after["bundles"][name]
        ledger.closed(pin,["manifest_sha256","optimize_lir"],"source-cost fresh mode")
        require(digest(pin["manifest_sha256"]) and pin["optimize_lir"] is mode,
                "Malformed source-cost paired capture mode/hash")
    require(sha(canonical(report)) == after["canonical_report_sha256"], "Unreviewed current source-cost report")
    require([p["slug"] for p in report["puzzles"]] == append.load()["preserved_slugs"] + [append.ADDED],
            "Source-cost requires exact22 order")
    p2 = report["puzzles"][-1]
    require(sha(p2["source"].encode()) == AFTER_SOURCE and sha(canonical(p2)) == after["p2_sha256"]
            and sha(canonical(p2["protocol_validation"])) == after["protocol_sha256"],
            "Changed reviewed source-cost P2 source/artifact/protocol")
    return profile, after


def check_report_bytes(payload, report):
    _, after = reviewed_after(report)
    require(sha(payload) == after["report_sha256"], "Current source-cost report bytes differ from final capture")


def project_validated(report):
    """PURE current22→whole signed93; current validation is a prerequisite."""
    profile, _ = reviewed_after(report)
    old = before_p2(profile)
    current = report["puzzles"][-1]
    restored = copy.deepcopy(current)
    restored["source"] = old["source"]
    require(len(current["artifacts"]) == len(old["artifacts"]) == 5, "Changed five-build artifacts")
    for index, (new, previous) in enumerate(zip(current["artifacts"], old["artifacts"], strict=True)):
        require((new["backend"],new["optimize_lir"]) == (previous["backend"],previous["optimize_lir"]),
                "Changed source-cost artifact mode")
        if new["backend"] == "rust_mir":
            cost.artifact_bytes(new)
            restored["artifacts"][index] = copy.deepcopy(previous)
    require(len(current["cases"]) == len(old["cases"]) == 50, "Changed source-cost named coverage")
    for new, previous, target in zip(current["cases"], old["cases"], restored["cases"], strict=True):
        require(len(new["runs"]) == len(previous["runs"]) == 5, "Changed named mode count")
        for run, before, output in zip(new["runs"], previous["runs"], target["runs"], strict=True):
            if run["backend"] == "rust_mir":
                require(same({k:v for k,v in run["execution"].items() if k != "cost"},
                    {k:v for k,v in before["execution"].items() if k != "cost"}),
                    "Source refactor changed named output or typed failure")
                output["execution"]["cost"] = before["execution"]["cost"]
    new_rows = current["protocol_validation"]["observations"]
    old_rows = old["protocol_validation"]["observations"]
    target_rows = restored["protocol_validation"]["observations"]
    require(len(new_rows) == len(old_rows) == 5, "Changed dedicated five-build coverage")
    for index, (new, previous, target) in enumerate(zip(new_rows, old_rows, target_rows, strict=True)):
        require((new["backend"],new["optimize_lir"]) == (previous["backend"],previous["optimize_lir"]),
                "Changed dedicated source-cost mode")
        if new["backend"] == "rust_mir":
            # Typed current reader derives own curry/coin/transaction identities.
            # The exact reviewed whole row binds the actually captured values.
            target_rows[index] = copy.deepcopy(previous)
        elif new["backend"] == "chialisp":
            before_negative, negative = previous["negatives"][16], new["negatives"][16]
            require(negative["name"] == before_negative["name"] == "different-language reveal submitted to consensus"
                and negative["consensus"] == before_negative["consensus"]
                and negative["consensus"]["code"] == "WrongPuzzleHash"
                and negative["full_simulator_equality"] is before_negative["full_simulator_equality"] is True
                and negative["transaction"]["spends"][1]["reveal_hex"] == new_rows[1]["curried_p2_reveal_hex"]
                and before_negative["transaction"]["spends"][1]["reveal_hex"] == old_rows[1]["curried_p2_reveal_hex"],
                "Unbound different-language submitted Rust reveal")
            target["negatives"][16]["transaction"]["spends"][1]["reveal_hex"] = before_negative["transaction"]["spends"][1]["reveal_hex"]
    require(same(restored, old), "Source-cost changed unreviewed P2 reference/Rue/source/ABI/native/outcome/metadata")
    result = copy.deepcopy(report)
    result["puzzles"][-1] = old
    rows = result["authoring"]["sources"]
    selected = [r for r in rows if r["path"] == SOURCE]
    require(len(selected) == 1 and selected[0] == {"path":SOURCE,"sha256":AFTER_SOURCE},
            "Changed source-cost declared authoring identity")
    selected[0]["sha256"] = BEFORE_SOURCE
    require(sha(canonical(result)) == BEFORE_CANONICAL,
            "Source-cost changed old21/full93 IR/artifacts/native/typed/ABI/coverage/ledger or unknown metadata")
    return result


def verify_fresh_bundles(report, on, off, *, compiler, driver_sha256, toolchain, cargo_options):
    _, after = reviewed_after(report)
    previous = append.load()
    expected = {r["entry"]:r["sha256"] for r in previous["raw_ir_before"]}
    expected[ENTRY] = after["raw_ir_sha256"]
    return append.check_fresh_bundles(report,on,off,expected_ir=expected,bundle_pins=after["bundles"],
        compiler=compiler,driver_sha256=driver_sha256,toolchain=toolchain,cargo_options=cargo_options)


def validate_metrics(report, metrics):
    """Bind recorded cost arithmetic to exact accepted bundles; no execution."""
    reference = json.loads(read(RETAINED["metrics"]), object_pairs_hook=unique_object)
    ledger.closed(metrics, reference, "conditions metrics")
    require({k:v for k,v in metrics.items() if k != "observations"}
            == {k:v for k,v in reference.items() if k != "observations"}, "Changed conditions VM/network/scope")
    rows = metrics["observations"]
    protocol = report["puzzles"][-1]["protocol_validation"]["observations"]
    require(type(rows) is list and len(rows) == len(protocol) == 5
        and [(r["backend"],r["optimize_lir"]) for r in rows] == ledger.MODES,
        "Missing conditions metric modes")
    total = 0
    for group, actual, old in zip(rows, protocol, reference["observations"], strict=True):
        ledger.closed(group, ["backend", "optimize_lir", "genesis", "joint", "lineage"], "cost mode")
        require(type(group["optimize_lir"]) is bool
            and (group["backend"],group["optimize_lir"]) == (actual["backend"],actual["optimize_lir"]),
            "Changed metric/backend order")
        for height, phase in enumerate(("genesis", "joint", "lineage")):
            value = group[phase]
            ledger.closed(value, old[phase], "conditions " + phase)
            for key, exemplar in old[phase].items():
                if type(exemplar) is int:
                    require(type(value[key]) is int and value[key] >= 0, "Malformed nonnegative metric: " + key)
                elif type(exemplar) is str and not key.endswith("_hex"):
                    require(value[key] == exemplar, "Changed metric source/claim boundary: " + key)
            require(value["previous_transaction_height"] == height and value["max_cost"] == 5_500_000_000
                    and value["cost_per_byte"] == 12_000 and value["excluded_quote_wrapper_bytes"] == 2,
                    "Changed pinned height/budget/quote/byte cost rule")
            transaction = actual["lifecycle"][phase]
            ledger.bundle(transaction, accepted=True)
            items = []
            for spend in transaction["spends"]:
                coin = spend["coin"]
                amount = coin["amount"]
                amount_atom = amount.to_bytes((amount.bit_length()+7)//8, "big")
                if amount_atom and amount_atom[0] >= 128:
                    amount_atom = b"\0" + amount_atom
                items.append(proper(bytes.fromhex(coin["parent_id"]), ledger.tree(spend["reveal_hex"]),
                                    amount_atom, ledger.tree(spend["solution_hex"])))
            # Pinned solution_generator prepends spends, then quotes one outer item.
            generator = serialize((b"\x01", proper(proper(*reversed(items)))))
            require(ledger.hex_bytes(value["generator_uncompressed_hex"]) == generator
                and value["generator_uncompressed_bytes"] == value["generator_bytes_calculated"] == len(generator)
                and value["charged_generator_bytes_pinned_source_rule"] == len(generator)-2
                and value["generator_backrefs_bytes"] == len(ledger.hex_bytes(value["generator_backrefs_hex"]))
                and value["total_reveal_bytes"] == sum(len(ledger.hex_bytes(s["reveal_hex"])) for s in transaction["spends"])
                and value["total_solution_bytes"] == sum(len(ledger.hex_bytes(s["solution_hex"])) for s in transaction["spends"]),
                "Recorded generator does not bind exact accepted lifecycle bundle")
            spends = value["per_spend"]
            require(type(spends) is list and len(spends) == len(transaction["spends"]), "Missing metric spend")
            ids = []
            for spend in spends:
                ledger.closed(spend,["coin_id","execution_cost","condition_cost"],"per-spend metric")
                require(type(spend["execution_cost"]) is int and spend["execution_cost"] >= 0
                    and type(spend["condition_cost"]) is int and spend["condition_cost"] >= 0,
                    "Malformed per-spend costs")
                ids.append(spend["coin_id"])
            require(len(ids) == len(set(ids)) and set(ids) == {s["coin"]["id"] for s in transaction["spends"]}
                and value["execution_cost"] == sum(s["execution_cost"] for s in spends)
                and value["condition_cost"] == sum(s["condition_cost"] for s in spends)
                and value["observed_byte_cost_component"] == 12_000*(len(generator)-2)
                and value["total_conditions_cost"] == value["execution_cost"] + value["condition_cost"] + value["observed_byte_cost_component"]
                and value["total_conditions_cost"] <= value["max_cost"], "Inconsistent conditions costs/coin bindings")
            total += 1
    return {"groups":5,"accepted_bundle_metrics":total,
        "scope":"Recorded exact accepted-bundle/generator and pinned cost arithmetic consistency only; no new signature verification, consensus admission, VM execution or cost theorem."}


def control_source_window(report):
    paths = {row["path"] for row in report["authoring"]["sources"]}
    paths.update(["Cargo.toml","Cargo.lock","rust-toolchain.toml",".cargo/config.toml"])
    paths.update("scripts/"+name for name in ("check-p2-source-controls.py","cargo_authoring.py","cargo_bundle.py"))
    members = ("experiments/backend","experiments/byte-proofs","crates/chialisp-oxide-ir",
        "crates/chialisp-oxide-reference","crates/chialisp-oxide-rue","crates/chialisp-oxide-driver",
        "crates/chialisp-oxide","crates/chialisp-oxide-macros","crates/cargo-chialisp-oxide","crates/chialisp-oxide-package",
        "crates/chialisp-oxide-simulator","crates/chialisp-oxide-verifier","crates/chialisp-oxide-corpus",
        "crates/chialisp-oxide-cli","crates/chialisp-oxide-vm","puzzles","vendor/rue-lir")
    require(tomllib.loads(read("Cargo.toml").decode())["workspace"]["members"] == list(members),
            "Control implementation closure requires the reviewed workspace packages")
    for package in members:
        path = package+"/Cargo.toml";paths.add(path)
        manifest = tomllib.loads(read(path).decode())
        build = manifest["package"].get("build")
        require(build is None or build is False or build == "build.rs", "Unsupported control compiler build path")
        entries = ([manifest["lib"]] if "lib" in manifest else []) + manifest.get("bin",[])
        for entry in entries:
            source = entry.get("path")
            require(source is None or (package == "puzzles" and source == "lib.rs")
                or (type(source) is str and source.startswith("src/") and ".." not in Path(source).parts),
                "Unsupported compiler/native source entry outside reviewed src layout")
        example_names = {"crates/chialisp-oxide-simulator":("p2_singleton_cost",),
            "crates/chialisp-oxide-corpus":("p2_singleton_controls","p2_singleton_probe")}.get(package,())
        if example_names:
            for example in manifest.get("example",[]):
                if example.get("name") in example_names:
                    require(example.get("path", "examples/"+example["name"]+".rs") == "examples/"+example["name"]+".rs",
                            "Unsupported P2 metric/control example path")
        if (ROOT/package/"build.rs").is_file():paths.add(package+"/build.rs")
        paths.update(p.relative_to(ROOT).as_posix() for p in (ROOT/package/"src").rglob("*.rs") if p.is_file())
    paths.add("crates/chialisp-oxide-simulator/examples/p2_singleton_cost.rs")
    paths.update("crates/chialisp-oxide-corpus/examples/"+name+".rs" for name in
        ("p2_singleton_controls","p2_singleton_probe"))
    paths.update("experiments/p2-singleton-controls/"+path for path in
        ("Cargo.toml","Cargo.lock","mutations.json","src/lib.rs","src/proper_struct.rs","src/reversed_parameters.rs","src/wrong_dollar.rs"))
    require(len(paths) <= 4096,"Control source count exceeds bound")
    total, rows = 0, []
    for path in sorted(paths):
        payload = read(path); total += len(payload)
        require(total <= 64*1024*1024,"Control source bytes exceed bound")
        rows.append({"path":path,"sha256":sha(payload)})
    return rows


def captured_absolute(value):
    require(type(value) is str and value.startswith("/") and not value.startswith("//"),
            "Captured path must be a normalized absolute path")
    path = Path(value)
    require(path.is_absolute() and str(path) == value and ".." not in path.parts,
            "Captured path must be normalized without traversal")
    return path


def captured_compiler(compiler, report):
    require(type(compiler) is str and len(compiler) <= 4096 and compiler.endswith("\n"),
            "Expected full captured rustc identity")
    lines = compiler.splitlines()
    require(len(lines) == 7 and lines[0] == report["toolchain"], "Captured compiler header differs from current report")
    fields = {}
    for line in lines[1:]:
        parts = line.split(": ",1)
        require(len(parts) == 2 and parts[0] not in fields and parts[1], "Duplicate/malformed rustc identity field")
        fields[parts[0]] = parts[1]
    require(set(fields) == {"binary","commit-hash","commit-date","host","release","LLVM version"}
        and fields["binary"] == "rustc" and re.fullmatch(r"[0-9a-f]{40}",fields["commit-hash"])
        and re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}",fields["commit-date"])
        and re.fullmatch(r"[A-Za-z0-9_.-]+",fields["host"])
        and re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+-nightly",fields["release"])
        and re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+",fields["LLVM version"])
        and lines[0] == f"rustc {fields['release']} ({fields['commit-hash'][:9]} {fields['commit-date']})",
        "Inconsistent captured full rustc fields/header/host")
    require({k:v for k,v in fields.items() if k != "host"} == {
        "binary":"rustc","commit-hash":"daf2e5e18b3abc4311d9da3d96fd64138f43d508",
        "commit-date":"2026-07-13","release":"1.99.0-nightly","LLVM version":"22.1.8"},
        "Captured compiler fields differ from the independently queried pinned nightly contract")
    return fields["host"]


def validate_controls(report, value):
    """Read actual control capture against current sources; never execute it."""
    ledger.closed(value,["schema_version","protocol","scope","identity","manifest","vm_replay","native","window"],"source-cost controls")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1
        and value["protocol"] == "chialisp-oxide-p2-source-cost-controls/v1"
        and value["scope"] == CONTROLS_SCOPE, "Unsupported source-cost controls")
    spec = importlib.util.spec_from_file_location("source_cost_control_identity",ROOT/"scripts/check-p2-source-controls.py")
    gate = importlib.util.module_from_spec(spec); spec.loader.exec_module(gate)
    gate.recipes(ROOT)
    identity = value["identity"]
    ledger.closed(identity,["schema_version","protocol","status","exports","manifest_path","manifest_sha256",
        "compiler","driver_sha256","toolchain","rue_revision","optimize_lir","cargo_options","private_authoring",
        "main_authoring","implementation_sources","baseline","recipes","scope"],"control identity")
    ledger.closed(value["window"],["source_root","compiler","driver_sha256","toolchain","sources","unchanged"],"control source window")
    captured_root = captured_absolute(value["window"]["source_root"])
    host = captured_compiler(value["window"]["compiler"],report)
    toolchain = tomllib.loads(read("rust-toolchain.toml").decode())["toolchain"]["channel"]
    require(toolchain == value["window"]["toolchain"] == "nightly-2026-07-14",
            "Captured controls require the current pinned dated toolchain")
    require(value["window"]["unchanged"] is True and digest(value["window"]["driver_sha256"])
        and value["window"]["sources"] == control_source_window(report), "Stale control source window")
    require(type(identity["schema_version"]) is int and identity["schema_version"] == 1 and identity["protocol"] == "chialisp-oxide-p2-source-controls/v1"
        and identity["status"] == "checked_bundle_identity_and_current_sources" and identity["scope"] == IDENTITY_SCOPE
        and identity["main_authoring"] == report["authoring"]
        and identity["private_authoring"] == gate.private_snapshot(ROOT)
        and identity["recipes"] == {"path":f"{gate.CONTROL}/mutations.json","sha256":gate.RECIPE_SHA,"mutations":list(gate.NAMES)}
        and identity["compiler"] == value["window"]["compiler"]
        and identity["driver_sha256"] == value["window"]["driver_sha256"]
        and identity["toolchain"] == value["window"]["toolchain"] and identity["optimize_lir"] is True
        and identity["baseline"] == {"path":SOURCE,"sha256":AFTER_SOURCE}
        and identity["cargo_options"] == gate.CARGO_OPTIONS and identity["rue_revision"] == gate.RUE
        and identity["implementation_sources"] == [{"path":"scripts/"+name,"sha256":sha(read("scripts/"+name))}
             for name in ("check-p2-source-controls.py","cargo_authoring.py","cargo_bundle.py")],
        "Stale control compiler/main/private/recipe identity capture")
    identity_manifest = captured_absolute(identity["manifest_path"])
    require(identity_manifest.name == "manifest.json" and type(value["manifest"]) is str
        and len(value["manifest"]) <= 1_000_000 and sha(value["manifest"].encode()) == identity["manifest_sha256"],
        "Retained actual manifest bytes differ from identity capture")
    manifest = json.loads(value["manifest"],object_pairs_hook=unique_object)
    ledger.closed(manifest,["analysis_target","cargo_options","compiler","driver_sha256","entries","manifest_path",
        "package","package_id","rue_revision","schema_version","status","toolchain","workspace_root"],"retained actual control manifest")
    workspace = captured_root/gate.CONTROL
    version = tomllib.loads(read(gate.CONTROL+"/Cargo.toml").decode())["package"]["version"]
    require(type(manifest["schema_version"]) is int and manifest["schema_version"] == 1
        and manifest["status"] == "wip_prototype_not_ready_for_use" and manifest["analysis_target"] == host
        and manifest["compiler"] == identity["compiler"] and manifest["driver_sha256"] == identity["driver_sha256"]
        and manifest["package"] == gate.PACKAGE and captured_absolute(manifest["workspace_root"]) == workspace
        and captured_absolute(manifest["manifest_path"]) == workspace/"Cargo.toml"
        and manifest["package_id"] == f"path+{workspace.as_uri()}#{gate.PACKAGE}@{version}"
        and manifest["toolchain"] == toolchain and manifest["cargo_options"] == gate.CARGO_OPTIONS
        and manifest["rue_revision"] == gate.RUE,"Captured actual manifest target/source/package/compiler/options differ")
    native = value["native"]
    ledger.closed(native,["argv","exit_code","tests","log_sha256","observations"],"native control receipt")
    require(native["argv"] == ["cargo","test","--locked","--offline","--manifest-path",f"{gate.CONTROL}/Cargo.toml","--","--nocapture"]
        and type(native["exit_code"]) is int and native["exit_code"] == 0
        and type(native["tests"]) is int and native["tests"] == 3 and digest(native["log_sha256"]),
        "Malformed native three-test command/result receipt")
    require(type(native["observations"]) is list and [r["name"] for r in native["observations"]] == list(gate.NAMES),
        "Missing native control observations")
    expected = report["puzzles"][-1]["cases"][0]
    replay = value["vm_replay"]
    ledger.closed(replay,["controls","manifest_path","manifest_sha256","scope","source_controls","witness"],"VM control capture")
    ledger.closed(replay["witness"],["label","solution","expected","input_hex"],"policy witness")
    require(replay["scope"] == REPLAY_SCOPE and replay["witness"]["input_hex"] == serialize(parse(expected["solution"])).hex()
        and replay["witness"]["label"] == expected["label"] and replay["witness"]["solution"] == expected["solution"]
        and replay["witness"]["expected"] == expected["native"] and digest(replay["manifest_sha256"])
        and type(replay["controls"]) is list and len(replay["controls"]) == 3,
        "Source-control independent witness differs")
    require(captured_absolute(replay["manifest_path"]) == identity_manifest
        and replay["manifest_sha256"] == identity["manifest_sha256"]
        and type(replay["source_controls"]) is list and len(replay["source_controls"]) == 3,
        "VM/identity bundle capture differs")
    expected_entries = [f"chialisp_oxide_p2_singleton_controls::{name}::puzzle" for name in gate.NAMES]
    require([r["entry"] for r in replay["controls"]] == sorted(expected_entries,key=lambda e:sha(e.encode()))
        and len(identity["exports"]) == 3 and type(manifest["entries"]) is list
        and [r["entry"] for r in manifest["entries"]] == sorted(expected_entries,key=lambda e:sha(e.encode())),
        "Missing/duplicate/reordered VM or actual manifest control entries")
    by_entry = {r["entry"]:r for r in replay["controls"]}
    manifest_entries = {r["entry"]:r for r in manifest["entries"]}
    count = 0
    for index, observation in enumerate(native["observations"]):
        name = gate.NAMES[index]
        control = by_entry[expected_entries[index]]
        ledger.closed(control,["entry","ir_path","ir_sha256","runs"],"VM policy control")
        export = identity["exports"][index]
        ledger.closed(export,["name","entry","source","ir_sha256"],"control identity export")
        require(export == {"name":name,"entry":f"chialisp_oxide_p2_singleton_controls::{name}::puzzle",
            "source":f"{gate.CONTROL}/src/{name}.rs","ir_sha256":control["ir_sha256"]},
            "VM/identity export or IR binding differs")
        require(replay["source_controls"][index] == {"name":name,"path":str(Path(value["window"]["source_root"])/f"{gate.CONTROL}/src/{name}.rs"),
            "sha256":sha(read(f"{gate.CONTROL}/src/{name}.rs"))},"Stale VM policy source")
        require(control["entry"] == f"chialisp_oxide_p2_singleton_controls::{name}::puzzle"
            and digest(control["ir_sha256"]) and len(control["runs"]) == 2
            and captured_absolute(control["ir_path"]).parent == identity_manifest.parent
            and captured_absolute(control["ir_path"]).name == sha(control["entry"].encode())+".ir.json",
            "Missing control entry/IR/modes")
        ledger.closed(observation,["name","digest","dollar"],"native policy observation")
        ledger.hex_bytes(observation["digest"],size=32);ledger.hex_bytes(observation["dollar"],size=1)
        expected_digest = ledger.tree(expected["native"]["output_hex"])[0][1][0].hex()
        require((observation["digest"] != expected_digest and observation["dollar"] == "24") if index < 2
                else (observation["digest"] == expected_digest and observation["dollar"] == "21"),
                "Recorded native witness does not expose the exact policy mistake")
        entry = manifest_entries[control["entry"]]
        ledger.closed(entry,["entry","ir","clvm","artifact"],"actual manifest export")
        stem = sha(control["entry"].encode())
        require(entry["ir"] == stem+".ir.json" and entry["clvm"] == stem+".clvm.hex",
                "Actual manifest IR/CLVM leaf differs from export")
        for optimize, run in zip((False,True),control["runs"],strict=True):
            ledger.closed(run,["artifact","distinguished","execution","independent_expected_output_hex","optimized"],"policy VM run")
            artifact = run["artifact"]
            require(run["optimized"] is optimize and run["distinguished"] is True
                and run["independent_expected_output_hex"] == expected["native"]["output_hex"],
                "False policy control distinction")
            require(artifact["optimize_lir"] is optimize and artifact["backend"] == "rust_source_policy_mutation",
                    "Changed control VM mode")
            cost.artifact_bytes(artifact)
            if optimize:
                expected_artifact = {**artifact,"backend":"rust_mir"}
                require(same(entry["artifact"],expected_artifact),"Actual manifest artifact differs from retained optimized VM program")
            outcome = cost.check_execution(run["execution"])
            require(type(run["execution"]["cost"]) is int and 0 <= run["execution"]["cost"] <= 1_000_000_000,
                    "Policy VM control exceeds the actual replay budget")
            require(outcome["status"] == "accepted" and outcome != expected["native"], "Inconclusive or indistinguishable policy control")
            ledger.tree(outcome["output_hex"])
            expected_output = ledger.tree(expected["native"]["output_hex"])
            # Native test prints the digest/dollar; bind both to exact VM output.
            encoded = serialize(proper(proper(b"\x3f",bytes.fromhex(observation["digest"])),
                proper(b"\x3c",bytes.fromhex(observation["dollar"])), expected_output[1][1][0]))
            require(encoded.hex() == outcome["output_hex"], "Native/VM policy witness differs")
            count += 1
    return {"native_test_receipts":3,"recorded_vm_comparisons":count,
        "scope":"Recorded native-command receipt, current sources and six conclusive policy-counterexample observations; reader adds no execution/authentication or generic security claim."}


def check_sidecars(report):
    _, after = reviewed_after(report)
    metrics_payload, controls_payload = read(METRICS_PATH), read(CONTROLS_PATH)
    require(sha(metrics_payload) == after["metrics_sha256"] and sha(controls_payload) == after["controls_sha256"],
            "Source-cost metric/control capture bytes differ")
    metrics = json.loads(metrics_payload,object_pairs_hook=unique_object)
    previous = json.loads(read(RETAINED["metrics"]),object_pairs_hook=unique_object)
    measured = validate_metrics(report,metrics)
    validate_metrics({"puzzles":[before_p2()]},previous)  # Pure old bundle arithmetic, no current-source check.
    for current, old in zip(metrics["observations"],previous["observations"],strict=True):
        if current["backend"] != "rust_mir":
            require(same(current,old),"Source-cost changed canonical/Rue accepted-bundle metrics")
        for phase in ("genesis","joint","lineage"):
            require(current[phase]["condition_cost"] == old[phase]["condition_cost"],
                    "Source refactor changed lifecycle condition charges")
    joint, old_joint = metrics["observations"][1]["joint"], previous["observations"][1]["joint"]
    require(joint["total_conditions_cost"] < old_joint["total_conditions_cost"],
            "Reviewed source-cost candidate lacks its measured optimized joint improvement")
    measured["optimized_joint_total_delta"] = joint["total_conditions_cost"]-old_joint["total_conditions_cost"]
    measured["optimized_joint_execution_delta"] = joint["execution_cost"]-old_joint["execution_cost"]
    measured["optimized_joint_charged_bytes_delta"] = joint["charged_generator_bytes_pinned_source_rule"]-old_joint["charged_generator_bytes_pinned_source_rule"]
    return {"metrics":measured,"retained_controls":validate_before_controls(),
        "controls":validate_controls(report,json.loads(controls_payload,object_pairs_hook=unique_object))}
