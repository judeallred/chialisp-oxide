#!/usr/bin/env python3
"""Independent bounded retained Rust CAT report reader; no process execution.

Reuses the exact canonical CAT2 phase/coin/signature assertions, with explicit
Rust artifact policy identities. Does not authenticate source, state or builds.
"""
import argparse
import copy
import json
import re
from pathlib import Path
import sys
sys.dont_write_bytecode = True
import lifecycle
import rust_capture
from registry_codegen_evidence import require
import compiled_codec
from vectors import document as vector_specification
from compiled_codec import inspect as inspect_compiled_artifact
# Only this explicit Rust compiled-artifact reader selects the wider transport
# profile. Standalone canonical/source-vector readers retain their exact bounds.
lifecycle.decode=compiled_codec.decode
lifecycle.encode=compiled_codec.encode
lifecycle.identity=compiled_codec.identity

HERE=Path(__file__).resolve().parent


def validate_native(native, capture, project):
    """Independent finite native oracle and retained stdout/command binding.

    This reads records only. Coherent forged logs remain unauthenticated data.
    """
    specification = vector_specification()
    require(rust_capture.read(HERE / "vectors.json") ==
            (json.dumps(specification, indent=2) + "\n").encode(), "independent current vector specification")
    cases = specification["cases"]
    require(len(cases) == 37, "exact original native witness inventory")
    expected = [{"control": "original", "name": row["name"], "actual": row["expected"]} for row in cases]
    expected += [{"control": control, "name": name, "actual": {"status": "accepted", "output_hex": "80"}}
                 for control, name in (("drop_delta_guard", "non-nil-delta-one"),
                                       ("drop_genesis_equality", "mismatched-parent"))]
    require(native["status"] == "passed" and type(native["native_test_groups"]) is int
            and native["native_test_groups"] == 1 and native["observations"] == expected,
            "exact independent 39 native output witnesses")
    stdout_path = capture / "native-tests.stdout"
    command_path = capture / "native-tests.command.json"
    require(native["native_stdout"] == rust_capture.identity(stdout_path)
            and native["native_command"] == rust_capture.identity(command_path), "retained native stdout/command identity")
    stdout = rust_capture.read(stdout_path).decode()
    require(re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", stdout) == [("1", "0", "0")],
            "one retained actual native integration-test summary")
    observed = [json.loads(line.split("CAT_GENESIS_RUST_NATIVE ", 1)[1]) for line in stdout.splitlines()
                if "CAT_GENESIS_RUST_NATIVE " in line]
    require(observed == expected == native["observations"], "actual native stdout exact witness correspondence")
    command = json.loads(rust_capture.read(command_path))
    expected_argv = ["rustup", "run", rust_capture.TOOLCHAIN, "cargo", "test", "--manifest-path",
                     str(project / "Cargo.toml"), "--locked", "--test", "native", "--", "--nocapture"]
    target = Path(native["runner"]["path"]).parents[2]
    require(command["argv"] == expected_argv and command["cwd"] == str(rust_capture.ROOT)
            and command["target"] == str(target) and type(command["jobs"]) is int and command["jobs"] == 2
            and command["offline"] is True and type(command["exit_code"]) is int and command["exit_code"] == 0
            and command["status"] == "passed" and command["failure_kind"] is None and command["failure_detail"] is None
            and type(command["max_stream_bytes"]) is int and command["max_stream_bytes"] == rust_capture.MAX_FILE
            and command["timeout_seconds"] == rust_capture.COMMAND_TIMEOUT
            and command["cleanup_timeout_seconds"] == rust_capture.COMMAND_CLEANUP_TIMEOUT,
            "exact successful bounded native Cargo command")
    require(command["stdout"] == rust_capture.identity(stdout_path)
            and command["stderr"] == rust_capture.identity(capture / "native-tests.stderr"), "native command exact log correspondence")
    return expected


def validate(report, capture):
    require(report["protocol"]=="chialisp-oxide-rust-cat-genesis-lifecycle/v1" and report["status"]=="executed","explicit executed Rust lifecycle protocol")
    require(report["sdk_max_block_cost_clvm"]==11_000_000_000 and report["sdk_transaction_max_cost"]==5_500_000_000,"actual pinned SDK TESTNET11 block cost limit")
    require(report["raw_max_cost"]==1_000_000 and report["component_max_cost"]==250_000_000,"separate fixed VM budgets")
    artifacts=json.loads(rust_capture.read(capture/"artifacts.json"))
    require(report["artifacts"]==artifacts and artifacts["status"]=="admitted_fresh_bundles","exact separately retained admitted artifacts")
    require(report["canonical_tail_hex"]==(HERE/"upstream/genesis_by_coin_id.clsp.hex").read_text().strip(),"independently retained canonical bytes")
    require([lane["mode"] for lane in report["lanes"]]==["off","on"],"both modes exactly once")
    project=rust_capture.verify_prepared(capture)
    native=json.loads(rust_capture.read(capture/"native.json"))
    validate_native(native, capture, project)
    before=json.loads(rust_capture.read(capture/"extract-window-before.json"));after=json.loads(rust_capture.read(capture/"extract-window-after.json"))
    require(before==after and native["source_window"]==rust_capture.quiet(project),"complete native/LAST-host/extraction source-window consistency")
    require(before["tools"][3]==native["runner"],"frozen exact last-host runner")
    require(before==rust_capture.quiet(project,tuple(Path(row["path"]) for row in before["tools"]),before["compiler_root"]),"actual current compiler source/tool/runner window consistency")
    require(artifacts["compiler"]==rust_capture.read(capture/"compiler-query.stdout").decode()
            and artifacts["driver_declaration"]==rust_capture.read(capture/"driver-query.stdout").decode()
            and artifacts["driver_declaration"]==artifacts["compiler"].splitlines()[0]+"\n","retained independent compiler query consistency")
    original=rust_capture.read(rust_capture.POLICY_SOURCE).decode();recipe=json.loads((HERE/"mutations.json").read_text())
    sources={}
    for control in recipe["controls"]:
        source=original
        for edit in control["edits"]:
            require(source.count(edit["before"])==1,"exact unique actual source edit")
            source=source.replace(edit["before"],edit["after"])
        sources[control["name"]]=source
    for artifact in artifacts["artifacts"]:
        mode=artifact["mode"]
        authoring=rust_capture.snapshot(project)
        admitted=rust_capture.cargo_bundle.load_declared_export(capture/("bundle-"+mode),root=project,authoring=authoring,selected_entry=artifact["entry"],compiler=artifacts["compiler"],driver_sha256=before["tools"][0]["sha256"],toolchain=rust_capture.TOOLCHAIN,current_authoring=lambda:rust_capture.snapshot(project),optimize=mode=="on",cargo_options=["--manifest-path",str(project/"Cargo.toml"),"--locked"])
        selected=next(entry for entry in admitted.manifest["entries"] if entry["entry"]==artifact["entry"])
        require(admitted.manifest["schema_version"]==2 and admitted.program.get("environment")=="raw"
                and selected["artifact"]["clvm_hex"]==artifact["clvm_hex"]
                and admitted.source_receipt==artifact["source_receipt"],"independent complete retained bundle and raw ABI re-admission")
        require(rust_capture.identity(Path(artifact["manifest"]["path"]))==artifact["manifest"] and rust_capture.identity(Path(artifact["ir"]["path"]))==artifact["ir"],"retained exact bundle/IR bytes")
    return validate_observations(report, artifacts, sources)


def validate_observations(report, artifacts, sources):
    """Pure semantic checks; callers must first admit complete artifacts/sources/native data.

    This grants no source freshness, execution or native correspondence by itself.
    Ordinary validate() always performs its complete host-current checks first.
    """
    require(len(artifacts["artifacts"])==6,"six bounded complete Rust artifact records")
    vectors=json.loads((HERE/"vectors.json").read_text())["cases"]
    expected_raw=[("chialisp","canonical","original",row) for row in vectors]
    for lane in report["lanes"]:
        mode=lane["mode"]
        records=[a for a in artifacts["artifacts"] if a["mode"]==mode]
        require([a["control"] for a in records]==list(rust_capture.CONTROLS),"complete unique mode/control order")
        policy={a["control"]:a for a in records}
        for name,artifact in policy.items():
            raw=bytes.fromhex(artifact["clvm_hex"]);checked=inspect_compiled_artifact(raw)
            require(artifact["source_sha256"]==rust_capture.digest(sources[name].encode()) and checked==artifact["compiled_inspection"],"independent exact Rust source/module inspection")
            require(all(artifact[k]==v for k,v in checked["identity"].items()),"independent Rust compiled identities")
            require(artifact["entry"]==rust_capture.CRATE+"::"+name+"::puzzle","actual Cargo export identity")
            for row in vectors:
                if name=="original" or row["name"]==("non-nil-delta-one" if name=="drop_delta_guard" else "mismatched-parent"):
                    expected_raw.append(("rust",mode,name,row))
        require(lane["original_artifact"]==policy["original"],"main lifecycle selected unchanged Rust source")
        # Explicitly adapt only the retained-report envelope. The exact shared
        # CAT2 checker still checks every raw transaction and own-asset identity.
        canonical_envelope={"protocol":"chialisp-oxide-canonical-cat-genesis-lifecycle/v1","status":"executed",**{k:report[k] for k in ("sdk","consensus","sdk_vm","network","primary_vm","primary_protocol","cat_module_hex","owner_public_key","recipient_public_key")},"primary_max_cost":report["component_max_cost"],"cat_module_tree_hash":lifecycle.CAT,"tail_module_hex":policy["original"]["clvm_hex"],"lifecycle":lane["lifecycle"],"source_controls":lane["source_controls"],"sdk_transaction_attempts":23,"accepted":10,"typed_rejected":7,"runtime_umbrellas":6,"component_primary_vm_runs":25}
        lifecycle.validate(canonical_envelope,rust_policy=policy)
        canonical_asset=lifecycle.tree_hash(lifecycle.curry(bytes.fromhex(report["canonical_tail_hex"]),[bytes.fromhex(lane["lifecycle"]["genesis"]["coin_id"])]))
        require(lane["lifecycle"]["asset_id"]!=canonical_asset,"own Rust asset differs from canonical asset")
    runs=report["raw_tail_runs"]
    require(len(runs)==len(expected_raw)==115,"37canonical +74Rustoriginal +4actual mutant VM observations")
    for run,(language,mode,control,row) in zip(runs,expected_raw):
        require((run["language"],run["mode"],run["control"],run["name"])==(language,mode,control,row["name"]),"exact ordered independent raw vectors")
        expected=row["expected"] if control=="original" else {"status":"accepted","output_hex":"80"}
        execution=run["execution"];require(execution["outcome"]==expected,"independent whole-environment acceptance/exact output")
        if expected["status"]=="accepted":require(type(execution["cost"]) is int and 0<execution["cost"]<=1_000_000 and execution["failure"] is None,"conclusive raw success")
        else:
            code=row["reference_failure_specification"]["code"] if language=="chialisp" else "raise"
            require(execution["cost"] is None and execution["failure"]["kind"]=="program" and execution["failure"]["code"]==code,"separate actual canonical/Rust typed raw failure")
    require({k:report[k] for k in ("sdk_transaction_attempts","accepted","typed_rejected","runtime_umbrellas","component_primary_vm_runs")}=={"sdk_transaction_attempts":46,"accepted":20,"typed_rejected":14,"runtime_umbrellas":12,"component_primary_vm_runs":50},"distinct aggregate two-mode SDK/component counts")
    return {"status":"retained_consistency_checked","native_observations":39,"canonical_raw_vm":37,"rust_original_raw_vm":74,"rust_mutant_raw_vm":4,"rust_lifecycle_groups":2,"rust_source_control_groups":8,"sdk_transaction_attempts":46,"accepted_standard_funding_transactions":10,"accepted_cat_transitions":10,"typed_consensus_rejections":14,"runtime_umbrellas":12,"component_primary_vm_runs":50,"scope":"Retained exact source/artifact/phase/coin/typed-outcome consistency only; no new execution, build/state authentication, BLS theorem or compiler preservation."}


def self_test(report,capture):
    changes=[
        ("raw count",lambda r:r["raw_tail_runs"].pop()),
        ("missing mode",lambda r:r["lanes"].pop()),
        ("canonical asset",lambda r:r["lanes"][0]["lifecycle"].__setitem__("asset_id",lifecycle.TAIL)),
        ("mode alias",lambda r:r["lanes"][1].__setitem__("mode","off")),
        ("native exclusion",lambda r:r["artifacts"]["artifacts"].pop()),
        ("mutated source identity",lambda r:r["lanes"][0]["source_controls"][1]["artifact"].__setitem__("source_sha256","00"*32)),
        ("wrong typed raw code",lambda r:next(x for x in r["raw_tail_runs"] if x["language"]=="rust" and x["execution"]["failure"])["execution"]["failure"].__setitem__("code","path_into_atom")),
        ("resource rejection credit",lambda r:next(x for x in r["raw_tail_runs"] if x["execution"]["failure"])["execution"]["failure"].__setitem__("kind","resource")),
        ("empty recipient previous signature",lambda r:r["lanes"][0]["lifecycle"]["attempts"][12]["transaction"].__setitem__("signature_hex",lifecycle.EMPTY_SIGNATURE)),
        ("wrong consensus code",lambda r:r["lanes"][0]["lifecycle"]["attempts"][1]["outcome"].__setitem__("code","DoubleSpend")),
        ("state marker",lambda r:r["lanes"][0]["lifecycle"]["attempts"][1]["outcome"].__setitem__("full_simulator_equality",False)),
        ("asset reuse",lambda r:r["lanes"][0]["source_controls"][1].__setitem__("tail_reveal_hex",r["lanes"][0]["source_controls"][0]["tail_reveal_hex"])),
        ("missing subsidy",lambda r:r["lanes"][0]["source_controls"][3]["attempts"][1]["transaction"]["spends"].pop()),
    ]
    for name,change in changes:
        mutated=copy.deepcopy(report);change(mutated)
        try:validate(mutated,capture)
        except (ValueError,KeyError,IndexError):continue
        raise ValueError("corruption accepted: "+name)
    native = json.loads(rust_capture.read(capture / "native.json"))
    native_changes = [
        ("native original output", lambda n: n["observations"][0].__setitem__("actual", {"status":"rejected"})),
        ("native mutant output", lambda n: n["observations"][-1].__setitem__("actual", {"status":"rejected"})),
        ("native order", lambda n: n["observations"].reverse()),
        ("native boolean group", lambda n: n.__setitem__("native_test_groups", True)),
        ("native stdout identity", lambda n: n["native_stdout"].__setitem__("sha256", "00" * 32)),
        ("native command identity", lambda n: n["native_command"].__setitem__("sha256", "00" * 32)),
    ]
    for name, change in native_changes:
        mutated = copy.deepcopy(native); change(mutated)
        try: validate_native(mutated, capture, capture / "project")
        except (ValueError, KeyError, IndexError): continue
        raise ValueError("corruption accepted: " + name)
    return len(changes) + len(native_changes)


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("--report",type=Path,required=True);parser.add_argument("--capture",type=Path,required=True);parser.add_argument("--self-test",action="store_true");args=parser.parse_args()
    report=json.loads(rust_capture.read(args.report));result=validate(report,args.capture.resolve())
    if args.self_test:print("PASS",self_test(report,args.capture.resolve()),"constructed retained-reader controls and",compiled_codec.self_test(report["artifacts"]["artifacts"]),"compiled-codec controls; no execution credit")
    print(json.dumps(result,indent=2))
if __name__=="__main__":main()
