"""Constructed reader/oracle corruption controls; no execution or BLS credit.

The fixtures deliberately use quoted marker modules and synthetic signature
bytes. They test the recorded-evidence reader, never pretend to be compiled
boomerang policies, native observations or actual SDK transactions.
"""
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import boomerang_evidence as gate


def artifact(program, mode):
    payload = gate.raw_tree.serialize(program)
    return {"backend": mode[0], "optimize_lir": mode[1], "bytes": len(payload),
            "tree_hash": gate.raw_tree.tree_hash(program).hex(), "clvm_hex": payload.hex(),
            "disassembly": "constructed reader fixture; not a compiled puzzle"}


def execution(outcome, failure="raise"):
    if outcome["status"] == "accepted":
        return {"outcome": outcome, "cost": 100, "error": None}
    return {"outcome": outcome, "cost": None, "error": "constructed Raise fixture",
            "failure": {"kind": "program", "code": failure}}


def cases(recipes, argument=None):
    rows = []
    for recipe in recipes:
        solution = gate.parse(recipe["solution"])
        desired = gate.relation(solution if argument is None else (argument, solution))
        rows.append({"label": recipe["label"], "solution": recipe["solution"], "native": desired,
                     "runs": [{"backend": mode[0], "optimize_lir": mode[1], "execution": execution(copy.deepcopy(desired), gate.expected_failure(mode, recipe["label"]))}
                              for mode in gate.MODES]})
    return rows


def coin(parent, puzzle_hash, amount=1000):
    return {"coin_id": hashlib.sha256(bytes.fromhex(parent + puzzle_hash) + gate.integer(amount)).hexdigest(),
            "parent_coin_id": parent, "puzzle_hash": puzzle_hash, "amount": amount}


def signature_context(kind, input_coin):
    intent = b"\xcd" * 32 if kind == "wrong_intent" else gate.INTENT
    key = gate.OTHER_KEY if kind == "wrong_key" else gate.OWNER
    return {"public_key": key.hex(), "intent_hex": intent.hex(), "coin_id": input_coin["coin_id"],
            "additional_data": gate.ADDITIONAL_DATA.hex(),
            "message_hex": (intent + bytes.fromhex(input_coin["coin_id"]) + gate.ADDITIONAL_DATA).hex()}


def ledger(module, instance):
    module_tree = gate.tree(module["clvm_hex"])
    original = gate.tree(instance["clvm_hex"])
    rows, previous = [], {}
    recipes = gate.ATTEMPTS + [(label, 100, None, "empty", "GeneratorRuntimeError") for label, _ in gate.MALFORMED]
    malformed = dict(gate.MALFORMED)
    for label, timestamp, claim, signing, code in recipes:
        terms = gate.selected_terms(label)
        reveal = gate.curry(module_tree, terms)
        amount = 999 if label == "refund_amount_too_small" else 1001 if label == "refund_amount_too_large" else 1000
        input_coin = coin(gate.PARENT, instance["tree_hash"], amount)
        solution = malformed[label] if claim is None else "(1)" if claim else "(())"
        before = {"height": 0, "next_timestamp": timestamp, "created_height": 0, "spent_height": None, "children": []}
        replay = {"refund_replay": "refund_replay_setup", "claim_then_refund": "claim_then_refund_setup"}.get(label)
        if replay is not None:
            before = copy.deepcopy(previous[replay]["after"])
        after = copy.deepcopy(before)
        if code is None:
            destination = gate.RECIPIENT if claim else gate.REFUND
            after.update(height=1, next_timestamp=timestamp + 1, spent_height=0,
                         children=[coin(input_coin["coin_id"], destination.hex())])
        context, origin = None, None
        signature = gate.EMPTY_SIGNATURE
        if signing != "empty":
            signing_coin = input_coin
            if signing == "other_coin":
                signing_coin = coin(gate.OTHER_PARENT, instance["tree_hash"])
            context = signature_context(signing, signing_coin)
            # Deliberately synthetic test bytes; this reader checks bindings,
            # not the BLS equation or authenticated SDK execution.
            signature = hashlib.sha256(json.dumps(context, sort_keys=True).encode()).digest() * 3
            if signing in ["wrong_intent", "other_coin"]:
                origin_reveal = original
                if signing == "wrong_intent":
                    conditions = gate.fields(gate.tree(gate.relation(gate.proper(gate.public_terms(), b"\1"))["output_hex"]), 4)
                    conditions[3] = gate.proper(gate.integer(50), gate.OWNER, b"\xcd" * 32)
                    origin_reveal = b"\1", gate.proper(*conditions)
                origin = {"coin": signing_coin, "reveal_hex": gate.raw_tree.serialize(origin_reveal).hex(),
                          "reveal_tree_hash": gate.raw_tree.tree_hash(origin_reveal).hex(), "solution": "(1)", "solution_hex": "ff0180"}
        desired = gate.relation((terms, gate.parse(solution)))
        attempt = {"label": label, "timestamp": timestamp, "argument_hex": gate.raw_tree.serialize(terms).hex(),
                   "coin": input_coin, "reveal_hex": gate.raw_tree.serialize(reveal).hex(),
                   "reveal_tree_hash": gate.raw_tree.tree_hash(reveal).hex(), "solution": solution,
                   "solution_hex": gate.raw_tree.serialize(gate.parse(solution)).hex(), "signature_kind": signing,
                   "signature_hex": signature.hex(), "signature_context": context, "signature_origin": origin,
                   "vm_021": execution(desired, gate.expected_failure((module["backend"], module["optimize_lir"]), label)),
                   "consensus": {"status": "accepted" if code is None else "runtime_umbrella" if claim is None else "consensus_rejected",
                                 "code": code, "error": None if code is None else f"Validation({code})",
                                 "semantic_rejection_credit": code is not None and claim is not None},
                   "before": before, "after": after, "full_state_unchanged": code is not None}
        rows.append(attempt)
        previous[label] = attempt
    return {"backend": module["backend"], "optimize_lir": module["optimize_lir"], "instance_label": "public_demo_payment",
            "module_tree_hash": module["tree_hash"], "instance_tree_hash": instance["tree_hash"],
            "simulator": "chia-sdk-test 0.36.0", "consensus": "chia-consensus 0.36.1", "vm": "clvmr 0.16.2",
            "network": "TESTNET11", "transaction_attempts": 29, "attempts": rows}


def fixture():
    manifest = gate.source_contract()
    argument = gate.public_terms()
    modules = [artifact((b"\1", b"constructed unit module " + bytes([index])), mode) for index, mode in enumerate(gate.MODES)]
    instances = [artifact(gate.curry(gate.tree(module["clvm_hex"]), argument), mode)
                 for module, mode in zip(modules, gate.MODES, strict=True)]
    dependencies = [{"path": path, "source": gate.read_source(path).decode(),
                     "sha256": hashlib.sha256(gate.read_source(path)).hexdigest()}
                    for path in manifest["source_dependencies"]]
    rue = gate.read_source(gate.DIRECTORY + "/puzzle.rue")
    puzzle = {key: copy.deepcopy(manifest[key]) for key in ["slug", "title", "kind", "security_scope", "provenance"]}
    puzzle.update(validated=True, source=gate.read_source(gate.DIRECTORY + "/puzzle.rs").decode(),
                  reference=gate.read_source(gate.DIRECTORY + "/reference.clsp").decode(),
                  rue={"path": gate.DIRECTORY + "/puzzle.rue", "source": rue.decode(), "sha256": hashlib.sha256(rue).hexdigest(), "dependencies": []},
                  source_dependencies=dependencies, artifacts=modules, cases=cases(manifest["cases"]), generated_cases=0,
                  simulator=[], simulator_comparisons=[])
    if manifest.get("native_sources"):
        puzzle["native_sources"] = [{"path": path, "sha256": hashlib.sha256(gate.read_source(path)).hexdigest()}
                                    for path in manifest["native_sources"]]
    spec = manifest["coin_instances"][0]
    example = {"label": spec["label"], "argument": spec["argument"], "argument_hex": gate.raw_tree.serialize(argument).hex(),
               "argument_tree_hash": gate.raw_tree.tree_hash(argument).hex(), "scheme": gate.SCHEME, "constructor": gate.CONSTRUCTOR,
               "instances": [{"backend": mode[0], "optimize_lir": mode[1], "module_tree_hash": module["tree_hash"],
                              "artifact": instance, "sdk_owned_curry_bytes_equal": True, "sdk_hash_equal": True}
                             for mode, module, instance in zip(gate.MODES, modules, instances, strict=True)],
               "cases": cases(spec["solutions"], argument)}
    puzzle["coin_instances"] = {"protocol": gate.PROTOCOL, "role": gate.ROLE, "scheme": gate.SCHEME,
                                "examples": [example], "ledger": [ledger(module, instance) for module, instance in zip(modules, instances, strict=True)],
                                "scope": gate.SCOPE}
    return puzzle


def control_fixture(base):
    """Synthetic closed packet for testing the portable reader, never a capture."""
    import boomerang_source_controls as common
    def write(path, value):
        payload = value if type(value) is bytes else (json.dumps(value, indent=2) + "\n").encode()
        destination = base / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(payload)
        return hashlib.sha256(payload).hexdigest()
    source, package, capture = gate.ROOT, base / "private", base
    files, recipe = common.package_files(source)
    files["Cargo.lock"] = b"version = 4\n"
    for path, payload in files.items(): write("package/" + path, payload)
    prepared = {"recipe": recipe, "witnesses": common.witnesses(),
                "source_files": [{"path": path, "sha256": hashlib.sha256(payload).hexdigest()} for path, payload in sorted(files.items())]}
    native_text = "".join("BOOMERANG_MUTATION_NATIVE " + json.dumps({"name": row["name"], "witness": row["witness"], "actual": row["expected"]}) + "\n" for row in common.witnesses())
    native_log = (native_text + "test result: ok. 1 passed; 0 failed;\n").encode()
    native_commands = []
    phases = [(["generate-lockfile", "--offline"], []), (["fmt"], ["--all", "--check"]),
              (["check"], ["--all-targets", "--locked", "--offline"]),
              (["test"], ["--locked", "--offline", "--", "--nocapture"]),
              (["clippy"], ["--all-targets", "--locked", "--offline", "--", "-D", "warnings"]),
              (["build"], ["--example", "ledger", "--locked", "--offline"])]
    for index, (flags, tail) in enumerate(phases):
        log = f"native-logs/{index}.log"
        native_commands.append({"argv": ["rustup", "run", "nightly-2026-07-14", "cargo", *flags, "--manifest-path", str(package / "Cargo.toml"), *tail],
                                "exit_code": 0, "log": log, "log_sha256": write(log, native_log if index == 3 else b"constructed native command\n")})
    native = {"argv": native_commands[3]["argv"], "exit_code": 0, "native_test_groups": 1, "native_policy_observations": 9,
              "log": "native.log", "log_sha256": write("native.log", native_log),
              "ledger_binary": {"path": str(base / "native/debug/examples/ledger"), "sha256": "1" * 64}}
    window = common.source_window()
    compiler = "rustc 1.99.0-nightly (daf2e5e18 2026-07-13)\nbinary: rustc\ncommit-hash: daf2e5e18b3abc4311d9da3d96fd64138f43d508\ncommit-date: 2026-07-13\nhost: aarch64-apple-darwin\nrelease: 1.99.0-nightly\nLLVM version: 22.1.8\n"
    tools = {name: {"path": str(base / "host/debug" / name), "sha256": "2" * 64} for name in common.TOOLS}
    context = dict(source_root=str(source), package_root=str(package), capture_root=str(capture), compiler=compiler,
                   toolchain="nightly-2026-07-14", source_window=window, tools=tools)
    native_receipt = dict(schema_version=1, protocol="chialisp-oxide-boomerang-controls-native/v1", status="native_phase_passed_before_LAST_host_build",
                          **{key: context[key] for key in ["source_root", "package_root", "toolchain", "source_window"]}, prepared=prepared, native=native, commands=native_commands)
    native_hash = write("native-receipt.json", native_receipt)
    quiet = dict(schema_version=1, protocol="chialisp-oxide-boomerang-controls-quiet-window/v1", status="LAST_host_build_terminal_tools_quiet",
                 **context, native_receipt_sha256=native_hash, scope="Constructed unit receipt, not a host build.")
    quiet.pop("capture_root")
    context["last_host_window"] = {"path": str(base / "outside-quiet.json"), "sha256": write("last-host-window.json", quiet)}
    commands, bundles, instances, runs, sdk_inputs, sdk_attempts = [], {}, {}, [], [], []
    def command(role, argv, result=None):
        index = len(commands)
        stdout, stderr = f"logs/{index:02}.stdout", f"logs/{index:02}.stderr"
        commands.append(dict(role=role, argv=argv, exit_code=0, stdout=stdout,
                             stdout_sha256=write(stdout, b"constructed export\n" if result is None else result),
                             stderr=stderr, stderr_sha256=write(stderr, b"")))
    u64 = {"kind": "int", "bits": 64, "signed": False}
    def byte_type(n): return {"kind": "bytes", "len": n}
    def tuple_type(*items): return {"kind": "tuple", "items": list(items)}
    result_type = {"kind": "optional", "item": tuple_type(u64, byte_type(48), byte_type(32))}
    for first in [tuple_type(u64, byte_type(32), u64), tuple_type(u64, u64), tuple_type(u64, u64)]:
        result_type = {"kind": "pair", "first": first, "rest": result_type}
    for mode in ["off", "on"]:
        entries, recompiled = [], []
        instances[mode] = {}
        command("export-" + mode, [tools["cargo-chialisp-oxide"]["path"], "build", *common.options(package), "--output-dir", str(capture / ("fresh-" + mode))] + (["--no-optimize"] if mode == "off" else []))
        for name in common.NAMES:
            program = b"\1", b"constructed source-control module " + name.encode()
            retained = artifact(program, ("rust_mir", mode == "on"))
            entry_name = f"chialisp_oxide_boomerang_controls::{name}::puzzle"
            envelope = dict(ir_version=6, environment="strict", program=dict(name=entry_name,
                parameters=[dict(name="terms", ty=tuple_type(byte_type(48), byte_type(32), byte_type(32), u64, u64)), dict(name="claim", ty={"kind": "bool"})],
                result=result_type, body=dict(op="reject", message="constructed typed IR fixture; never compiled"), functions=[]))
            ir, clvm = name + ".json", name + ".clvm.hex"
            ir_hash = write(f"bundles/{mode}/" + ir, envelope)
            write(f"bundles/{mode}/" + clvm, (retained["clvm_hex"] + "\n").encode())
            entries.append(dict(entry=entry_name, ir=ir, clvm=clvm, artifact=retained))
            compiled = [dict(optimize=optimize, **{key: retained[key] for key in ["bytes", "tree_hash", "clvm_hex"]}) for optimize in [False, True]]
            recompiled.append(dict(entry=entry_name, ir_sha256=ir_hash, artifacts=compiled))
            command(f"recompile-{mode}-{name}", [tools["chialisp-oxide-verifier"]["path"], str(capture / f"bundles/{mode}" / ir), "--artifacts"], compiled)
            instance = gate.raw_tree.serialize(gate.curry(program, gate.public_terms()))
            path = f"instances/{mode}/{name}.clvm.hex"
            write(path, (instance.hex() + "\n").encode())
            instances[mode][name] = dict(path=path, bytes=len(instance), clvm_hex=instance.hex(), tree_hash=gate.raw_tree.tree_hash(gate.tree(instance.hex())).hex(), module_tree_hash=retained["tree_hash"])
        manifest = dict(schema_version=1, status="wip_prototype_not_ready_for_use", package=common.PACKAGE,
                        workspace_root=str(package), manifest_path=str(package / "Cargo.toml"), package_id=f"path+{package.as_uri()}#{common.PACKAGE}@0.1.0",
                        analysis_target="aarch64-apple-darwin", compiler=compiler, toolchain=context["toolchain"],
                        driver_sha256=tools["chialisp-oxide-driver"]["sha256"], cargo_options=common.options(package), rue_revision="245c107a8e795229ea08097142f005a4c0d91075", entries=entries)
        manifest_path = f"bundles/{mode}/manifest.json"
        bundles[mode] = dict(manifest=manifest_path, manifest_sha256=write(manifest_path, manifest), recompiled=recompiled)
        for witness in common.witnesses():
            name = witness["name"]
            instance = instances[mode][name]
            selected = next(entry["artifact"] for entry in entries if entry["entry"].split("::")[-2] == name)
            observed = execution(gate.control_output(name, witness["claim"]))
            runs.append(dict(mode=mode, name=name, witness=witness["witness"], solution=witness["instance_solution"], instance_tree_hash=instance["tree_hash"],
                             instance_sha256=hashlib.sha256(bytes.fromhex(instance["clvm_hex"])).hexdigest(), execution=observed))
            solution_path = f"inputs/{mode}-{name}-{witness['witness']}.json"
            write(solution_path, [witness["instance_solution"]])
            command(f"vm-{mode}-{name}-{witness['witness']}", [tools["chialisp-oxide"]["path"], "run-batch", str(capture / instance["path"]), "--solutions", str(capture / solution_path), "--max-cost", "1000000"], [observed])
            sdk_inputs.append(dict(mode=mode, name=name, witness=witness["witness"], module_hex=selected["clvm_hex"], instance_hex=instance["clvm_hex"]))
            input_coin = coin(gate.PARENT, instance["tree_hash"])
            outcome = dict(status="consensus_rejected", code=witness["ledger_expected"].get("code"), semantic_rejection_credit=True, full_state_unchanged=True)
            if name not in common.NAMES[:2]:
                outcome = dict(status="accepted", input_spent=True, child_count=1, child=coin(input_coin["coin_id"], (gate.RECIPIENT if witness["claim"] else gate.REFUND).hex()))
            sdk_attempts.append(dict(mode=mode, name=name, witness=witness["witness"], claim=witness["claim"], timestamp=witness["timestamp"], signing=witness["signing"],
                module_tree_hash=selected["tree_hash"], instance_tree_hash=instance["tree_hash"], instance_hex=instance["clvm_hex"], solution_hex=gate.raw_tree.serialize(gate.parse(witness["instance_solution"])).hex(),
                signature_hex=(gate.EMPTY_SIGNATURE if witness["signing"] == "none" else b"synthetic signature; not BLS proof".ljust(96, b"!")).hex(), input=input_coin, outcome=outcome))
    sdk = dict(protocol=common.LEDGER_PROTOCOL, simulator="chia-sdk-test 0.36.0", consensus="chia-consensus 0.36.1", vm="clvmr 0.16.2", network="TESTNET11", scenario_runs=18, transaction_attempts=18, attempts=sdk_attempts)
    request_hash = write("ledger-request.json", dict(protocol=common.LEDGER_PROTOCOL, runs=sdk_inputs))
    output_hash = write("ledger.json", sdk)
    command("sdk-ledger", [native["ledger_binary"]["path"], str(capture / "ledger-request.json")], sdk)
    inventory = [{"path": path.relative_to(base).as_posix(), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in sorted(base.rglob("*")) if path.is_file()]
    return dict(schema_version=1, protocol=common.PROTOCOL, status="actual_source_controls_captured", scope=common.SCOPE, context=context,
                prepared=prepared, native=native, bundles=bundles, instances=instances, runs=runs,
                ledger=dict(request="ledger-request.json", request_sha256=request_hash, output="ledger.json", output_sha256=output_hash, result=sdk), commands=commands, files=inventory)


class IndependentOracle(unittest.TestCase):
    def test_declared_recipes_match_independent_exact_tree_oracle(self):
        manifest = gate.source_contract()
        self.assertEqual(len(manifest["cases"]), 101)
        self.assertEqual(len(manifest["coin_instances"][0]["solutions"]), 14)

    def test_complete_u64_domain_edges_and_no_implicit_positivity_guard(self):
        for value in [0, 1, 127, 128, 2**63 - 1, 2**63, 2**64 - 1]:
            self.assertEqual(gate.uint(gate.integer(value)), value)
            terms = gate.proper(b"\0" * 48, gate.RECIPIENT, gate.REFUND, gate.integer(value), gate.integer(value))
            for claim in [b"", b"\1"]:
                outcome = gate.relation(gate.proper(terms, claim))
                self.assertEqual(outcome["status"], "accepted")
                conditions = gate.fields(gate.tree(outcome["output_hex"]), 4 if claim else 3)
                self.assertEqual(conditions[0], gate.proper(bytes([85 if claim else 81]), gate.integer(value)))
                self.assertEqual(conditions[2], gate.proper(b"3", gate.RECIPIENT if claim else gate.REFUND, gate.integer(value)))

    def test_canonical_integer_and_strict_bool_rejections(self):
        for value in [b"\0", b"\0\0", b"\0\1", b"\0\0\x80", b"\xff", b"\x80", gate.integer(2**64), (b"\1", b"")]:
            self.assertIsNone(gate.uint(value))
        for claim in [b"\0", b"\0\1", b"\2", b"\xff", (b"\1", b"")]:
            self.assertEqual(gate.relation(gate.proper(gate.public_terms(), claim)), {"status": "rejected"})

    def test_outputs_have_exact_conditional_optional_signature_tail(self):
        claim = gate.tree(gate.relation(gate.proper(gate.public_terms(), b"\1"))["output_hex"])
        refund = gate.tree(gate.relation(gate.proper(gate.public_terms(), b""))["output_hex"])
        claim_items, refund_items = gate.fields(claim, 4), gate.fields(refund, 3)
        self.assertEqual(claim_items[-1], gate.proper(b"2", gate.OWNER, gate.INTENT))
        self.assertEqual(refund_items[-1], gate.proper(b"3", gate.REFUND, b"\x03\xe8"))
        self.assertIsNone(gate.fields(claim, 3))
        self.assertIsNone(gate.fields(refund, 4))


class RecordedControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original = fixture()

    def rejected(self, mutate, reader=gate.read_source):
        changed = copy.deepcopy(self.original)
        mutate(changed)
        with self.assertRaises((ValueError, KeyError, TypeError, IndexError)):
            gate.validate_coin(changed, reader=reader)

    def test_constructed_fixture_checks_all_five_modes_and_exact_counts(self):
        result = gate.validate_coin(self.original)
        self.assertEqual({key: result[key] for key in ["ledger_groups", "transaction_attempts", "accepted_spends", "semantic_rejections", "runtime_umbrellas_without_semantic_credit"]},
                         {"ledger_groups": 5, "transaction_attempts": 145, "accepted_spends": 25,
                          "semantic_rejections": 80, "runtime_umbrellas_without_semantic_credit": 40})

    def test_missing_unknown_duplicate_and_reordered_modes_fail(self):
        for key in ["artifacts", "ledger", "instances", "runs"]:
            def rows(p):
                if key == "ledger": return p["coin_instances"]["ledger"]
                if key == "instances": return p["coin_instances"]["examples"][0]["instances"]
                if key == "runs": return p["cases"][0]["runs"]
                return p[key]
            for mutation in ["missing", "reverse", "numeric", "unknown", "duplicate"]:
                def change(p):
                    values = rows(p)
                    if mutation == "missing": values.pop(0)
                    elif mutation == "reverse": values.reverse()
                    elif mutation == "numeric": values[0]["optimize_lir"] = 0
                    elif mutation == "unknown": values[0]["backend"] = "unknown"
                    else: values[0] = copy.deepcopy(values[1])
                with self.subTest(key=key, mutation=mutation): self.rejected(change)

    def test_each_ledger_attempt_is_mandatory_in_each_mode(self):
        for mode in range(5):
            for attempt in range(29):
                with self.subTest(mode=mode, attempt=attempt):
                    self.rejected(lambda p: p["coin_instances"]["ledger"][mode]["attempts"].pop(attempt))

    def test_each_template_and_instance_vector_is_mandatory(self):
        for key, count in [("template", 101), ("instance", 14)]:
            for index in range(count):
                def change(p):
                    rows = p["cases"] if key == "template" else p["coin_instances"]["examples"][0]["cases"]
                    rows.pop(index)
                with self.subTest(key=key, index=index): self.rejected(change)

    def test_argument_module_and_instance_bindings_fail_when_rebound(self):
        mutations = [
            lambda p: p["coin_instances"].update(protocol="unknown"),
            lambda p: p["coin_instances"].update(role="canonical_clawback"),
            lambda p: p["coin_instances"].update(scheme="other"),
            lambda p: p["coin_instances"].update(scope="Production-safe proof"),
            lambda p: p["coin_instances"]["examples"][0].update(argument_hex="80"),
            lambda p: p["coin_instances"]["examples"][0].update(argument_tree_hash="0" * 64),
            lambda p: p["coin_instances"]["examples"][0].update(page_result="()"),
            lambda p: p["coin_instances"]["examples"][0]["instances"][0].update(module_tree_hash="0" * 64),
            lambda p: p["coin_instances"]["examples"][0]["instances"][0].update(sdk_hash_equal=1),
            lambda p: p["coin_instances"]["examples"][0]["instances"][0].update(sdk_owned_curry_bytes_equal=False),
            lambda p: p["coin_instances"]["ledger"][0].update(instance_tree_hash="0" * 64),
            lambda p: p["simulator"].append({"passed": True}),
            lambda p: p.update(kind="pure_function"),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(mutation=index): self.rejected(mutate)
        def different_self_consistent_instance(p):
            row = p["coin_instances"]["examples"][0]["instances"][0]
            module = gate.tree(p["artifacts"][0]["clvm_hex"])
            row["artifact"] = artifact(gate.curry(module, b""), gate.MODES[0])
        self.rejected(different_self_consistent_instance)

    def test_clock_codes_amount_children_and_rollback_corruptions_fail(self):
        def attempt(p, index=0): return p["coin_instances"]["ledger"][0]["attempts"][index]
        mutations = [
            lambda p: attempt(p).update(timestamp=100),
            lambda p: attempt(p)["before"].update(next_timestamp=100),
            lambda p: attempt(p, 3)["consensus"].update(code="BadAggregateSignature", error="Validation(BadAggregateSignature)"),
            lambda p: attempt(p, 21)["consensus"].update(semantic_rejection_credit=True),
            lambda p: attempt(p, 6).update(full_state_unchanged=1),
            lambda p: attempt(p, 6)["after"].update(height=1),
            lambda p: attempt(p)["after"]["children"][0].update(amount=999),
            lambda p: attempt(p)["after"]["children"][0].update(parent_coin_id="0" * 64),
            lambda p: attempt(p)["after"]["children"][0].update(puzzle_hash=gate.REFUND.hex()),
            lambda p: attempt(p, 18)["before"].update(spent_height=None),
            lambda p: attempt(p, 20)["before"]["children"][0].update(puzzle_hash=gate.REFUND.hex()),
            lambda p: attempt(p, 10)["coin"].update(amount=1000),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(mutation=index): self.rejected(mutate)

    def test_substitutions_and_signature_contexts_cannot_be_conflated(self):
        def attempt(p, index): return p["coin_instances"]["ledger"][0]["attempts"][index]
        mutations = [
            lambda p: attempt(p, 7).update(signature_hex=gate.EMPTY_SIGNATURE.hex()),
            lambda p: attempt(p, 7)["signature_context"].update(public_key=gate.OWNER.hex()),
            lambda p: attempt(p, 7)["signature_context"].update(message_hex="00"),
            lambda p: attempt(p, 8)["signature_context"].update(intent_hex=gate.INTENT.hex()),
            lambda p: attempt(p, 8).update(signature_origin=None),
            lambda p: attempt(p, 8)["signature_origin"].update(reveal_hex=attempt(p, 0)["reveal_hex"]),
            lambda p: attempt(p, 9)["signature_origin"].update(coin=copy.deepcopy(attempt(p, 9)["coin"])),
            lambda p: attempt(p, 9)["signature_context"].update(coin_id=attempt(p, 9)["coin"]["coin_id"]),
            lambda p: attempt(p, 12).update(reveal_hex=attempt(p, 0)["reveal_hex"]),
            lambda p: attempt(p, 16).update(argument_hex=attempt(p, 0)["argument_hex"]),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(mutation=index): self.rejected(mutate)

    def test_resource_infrastructure_and_wrong_program_failures_are_not_rejection(self):
        for failure in [{"kind": "resource", "code": "cost_exceeded"},
                        {"kind": "infrastructure", "code": "internal_error"},
                        {"kind": "program", "code": "path_into_atom"}]:
            def change(p):
                value = p["coin_instances"]["ledger"][0]["attempts"][21]["vm_021"]
                value["failure"] = failure
                value["outcome"]["status"] = "rejected" if failure["kind"] == "program" else "inconclusive"
            with self.subTest(failure=failure): self.rejected(change)

    def test_extra_pair_tail_failure_code_is_exactly_mode_and_witness_scoped(self):
        for label in ["extra outer argument", "extra terms field"]:
            index = next(index for index, case in enumerate(self.original["cases"]) if case["label"] == label)
            for mode, wrong in [(0, "raise"), (1, "invalid_op_arg")]:
                with self.subTest(label=label, mode=mode):
                    self.rejected(lambda p: p["cases"][index]["runs"][mode]["execution"]["failure"].update(code=wrong))
        index = next(index for index, case in enumerate(self.original["coin_instances"]["examples"][0]["cases"]) if case["label"] == "extra path argument")
        self.rejected(lambda p: p["coin_instances"]["examples"][0]["cases"][index]["runs"][0]["execution"]["failure"].update(code="raise"))
        self.rejected(lambda p: p["coin_instances"]["ledger"][0]["attempts"][27]["vm_021"]["failure"].update(code="raise"))

    def test_stale_sources_manifest_recipes_and_false_expectations_fail(self):
        for path in [gate.DIRECTORY + "/puzzle.rs", gate.DIRECTORY + "/reference.clsp", gate.DIRECTORY + "/puzzle.rue",
                     "crates/chialisp-oxide/src/cons.rs"]:
            reader = lambda candidate, path=path: gate.read_source(candidate) + (b"\n" if candidate == path else b"")
            with self.subTest(path=path): self.rejected(lambda p: None, reader)
        for mutation in ["missing_case", "changed_case", "false_output", "terms", "page_result"]:
            manifest = gate.source_contract()
            if mutation == "missing_case": manifest["cases"].pop()
            elif mutation == "changed_case": manifest["cases"][0]["solution"] = "()"
            elif mutation == "false_output": manifest["cases"][0]["expected"] = {"status": "rejected"}
            elif mutation == "terms": manifest["coin_instances"][0]["argument"] = "()"
            else: manifest["coin_instances"][0]["page_result"] = "()"
            reader = lambda candidate, manifest=manifest: json.dumps(manifest).encode() if candidate == gate.DIRECTORY + "/puzzle.json" else gate.read_source(candidate)
            with self.subTest(mutation=mutation): self.rejected(lambda p: None, reader)

    def test_reader_never_launches_external_processes(self):
        with patch("subprocess.run", side_effect=AssertionError("recorded reader executed a tool")):
            gate.validate_coin(self.original)

    def test_replay_uses_exact_retained_bytes_and_all575_captured_observations(self):
        puzzle = self.original
        example = puzzle["coin_instances"]["examples"][0]
        expected = {}
        for artifacts, rows in [(puzzle["artifacts"], puzzle["cases"]),
                                ([row["artifact"] for row in example["instances"]], example["cases"])]:
            for mode, retained in enumerate(artifacts):
                expected[retained["clvm_hex"]] = (rows, mode)
        calls = []
        def run_batch(cli, program, solutions, count, *, max_cost):
            retained = program.read_text().strip()
            self.assertIn(retained, expected)
            rows, mode = expected[retained]
            self.assertEqual(json.loads(solutions.read_text()), [row["solution"] for row in rows])
            self.assertEqual(count, len(rows))
            self.assertEqual(max_cost, gate.MAX_COST)
            calls.append(retained)
            return [copy.deepcopy(row["runs"][mode]["execution"]) for row in rows]
        with patch("port_evidence.run_batch", side_effect=run_batch), \
                patch("subprocess.run", side_effect=AssertionError("constructed replay launched actual tool")):
            result = gate.replay_retained(puzzle, "unused-constructed-cli")
        self.assertEqual(len(calls), 10)
        self.assertEqual(result["retained_primary_vm_replays"], 575)

    def test_replay_resource_exhaustion_is_not_accepted_as_a_negative(self):
        def run_batch(cli, program, solutions, count, *, max_cost):
            return [{"outcome": {"status": "inconclusive"}, "cost": None,
                     "error": "constructed host resource limit", "failure": {"kind": "resource", "code": "cost_exceeded"}}] * count
        with patch("port_evidence.run_batch", side_effect=run_batch), self.assertRaises(ValueError):
            gate.replay_retained(self.original, "unused-constructed-cli")


class ActualSourcePacketReader(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix="constructed-boomerang-packet-")
        cls.base = Path(cls.directory.name)
        cls.original = control_fixture(cls.base)

    @classmethod
    def tearDownClass(cls):
        cls.directory.cleanup()

    def validate(self, report):
        # Keep constructed metadata tests deterministic while other agents edit
        # unrelated compiler sources. Actual capture gates recompute this window.
        with patch("boomerang_source_controls.source_window", return_value=self.original["context"]["source_window"]), \
                patch("subprocess.run", side_effect=AssertionError("recorded packet reader executed a tool")):
            return gate.validate_source_controls(report, self.base)

    def rejected(self, change):
        report = copy.deepcopy(self.original)
        change(report)
        wire_hash = lambda value: hashlib.sha256(json.dumps(value, sort_keys=True).encode()).digest()
        self.assertTrue(wire_hash(report) != wire_hash(self.original), "corruption must change its constructed wire fixture")
        with self.assertRaises((ValueError, KeyError, TypeError, IndexError)):
            self.validate(report)

    def test_complete_constructed_packet_checks18VM_and18SDK_separately(self):
        result = self.validate(self.original)
        self.assertEqual(result["exports"], 5)
        self.assertEqual(result["compiler_export_modes"], 10)
        self.assertEqual(result["native_policy_observations"], 9)
        self.assertEqual(result["retained_primary_VM_observations"], 18)
        self.assertEqual(result["SDK_transaction_attempts"], 18)
        self.assertEqual(result["accepted_policy_counterexamples"], 6)
        self.assertEqual(result["exact_typed_rejections"], 12)

    def test_every_actual_source_VM_andSDK_observation_is_required(self):
        for index in range(18):
            with self.subTest(kind="VM", index=index): self.rejected(lambda p: p["runs"].pop(index))
            with self.subTest(kind="SDK", index=index): self.rejected(lambda p: p["ledger"]["result"]["attempts"].pop(index))

    def test_closed_input_modes_recipes_native_and_file_receipts(self):
        mutations = [
            lambda p: p.update(schema_version=True),
            lambda p: p.update(scope="production-ready security proof"),
            lambda p: p["context"]["source_window"]["sources"][0].update(sha256="0" * 64),
            lambda p: p["prepared"]["recipe"]["controls"][0].update(source_sha256="0" * 64),
            lambda p: p["prepared"]["witnesses"][0].update(timestamp=101),
            lambda p: p["prepared"]["witnesses"][0].update(claim=1),
            lambda p: p["native"].update(native_test_groups=True),
            lambda p: p["native"].update(native_policy_observations=18),
            lambda p: p["bundles"].pop("off"),
            lambda p: p["instances"]["on"].pop("early_refund"),
            lambda p: p["instances"]["off"]["drop_claim_signature"].update(clvm_hex="80", bytes=1),
            lambda p: p["runs"][0].update(instance_tree_hash="0" * 64),
            lambda p: p["commands"][0].update(exit_code=False),
            lambda p: p["commands"][3]["argv"].append("unreviewed"),
            lambda p: p["files"].pop(),
        ]
        for index, mutation in enumerate(mutations):
            with self.subTest(mutation=index): self.rejected(mutation)

    def test_source_witness_resource_failures_are_inconclusive(self):
        def change(p):
            p["runs"][0]["execution"] = dict(outcome=dict(status="inconclusive"), cost=None,
                error="constructed host limit", failure=dict(kind="resource", code="cost_exceeded"))
        self.rejected(change)

    def test_independent_mutation_oracle_keeps_original_and_equivalent_equal(self):
        import boomerang_source_controls as common
        rows = gate.control_witnesses(common)
        self.assertEqual(len(rows), 9)
        for claim in [False, True]:
            baseline = gate.relation(gate.proper(gate.public_terms(), b"\1" if claim else b""))
            self.assertEqual(gate.control_output("original", claim), baseline)
            self.assertEqual(gate.control_output("equivalent", claim), baseline)
        self.assertNotEqual(gate.control_output("drop_claim_signature", True), gate.control_output("original", True))
        self.assertNotEqual(gate.control_output("late_claim", True), gate.control_output("original", True))
        self.assertNotEqual(gate.control_output("early_refund", False), gate.control_output("original", False))


if __name__ == "__main__":
    unittest.main()
