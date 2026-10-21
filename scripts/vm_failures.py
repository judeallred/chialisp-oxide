"""Validate typed CLVM failures without interpreting human-readable diagnostics.

Native policy results have a separate wire shape. Historical reports may omit VM
failure metadata only when explicitly requested by a baseline comparator; current
proof and validation evidence must always use the shared, exhaustive protocol.
"""

import hashlib
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
PROTOCOL = "clvmr-0.21.0/v1"
PROTOCOL_PATH = "crates/chialisp-oxide-vm/failure-protocol.json"
HEX = re.compile(r"(?:[0-9a-f]{2})+")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load_protocol():
    table = json.loads((ROOT / PROTOCOL_PATH).read_text())
    require(set(table) == {"schema_version", "protocol", "failures"}
            and type(table["schema_version"]) is int and table["schema_version"] == 1
            and table["protocol"] == PROTOCOL, "Unsupported VM failure protocol")
    mapping = {}
    for row in table["failures"]:
        require(set(row) == {"kind", "code"} and row["kind"] in ("program", "resource", "infrastructure")
                and isinstance(row["code"], str) and re.fullmatch(r"[a-z][a-z0-9]*(?:_[a-z0-9]+)*", row["code"])
                and row["code"] not in mapping, "Malformed VM failure table")
        mapping[row["code"]] = row["kind"]
    require(bool(mapping), "Empty VM failure table")
    return mapping


FAILURES = load_protocol()


def check_failure(value, label="VM failure"):
    require(isinstance(value, dict) and set(value) == {"kind", "code"}, f"{label}: missing/malformed typed failure metadata")
    require(isinstance(value["code"], str) and value["code"] in FAILURES,
            f"{label}: unknown VM failure code")
    require(value["kind"] == FAILURES[value["code"]], f"{label}: inconsistent VM failure kind/code")
    return value["kind"]


def check_native_outcome(value, label="native policy"):
    require(isinstance(value, dict), f"{label}: malformed native outcome")
    status = value.get("status")
    if status == "accepted":
        require(set(value) == {"status", "output_hex"} and isinstance(value["output_hex"], str)
                and HEX.fullmatch(value["output_hex"]), f"{label}: malformed accepted native output")
    else:
        require(status == "rejected" and set(value) == {"status"}, f"{label}: malformed native rejection")
    return value


def check_execution(value, label="VM execution", *, require_conclusive=True, require_typed=True):
    require(isinstance(value, dict) and {"outcome", "cost", "error"} <= value.keys()
            and value.keys() <= {"outcome", "cost", "error", "failure"}, f"{label}: malformed execution envelope")
    result = value["outcome"]
    require(isinstance(result, dict), f"{label}: malformed VM outcome")
    status = result.get("status")
    if status == "accepted":
        check_native_outcome(result, label)
        require(type(value["cost"]) is int and value["cost"] > 0 and value["error"] is None
                and "failure" not in value, f"{label}: inconsistent successful execution metadata")
    else:
        require(status in ("rejected", "inconclusive") and set(result) == {"status"}, f"{label}: unknown VM outcome")
        require(value["cost"] is None and isinstance(value["error"], str) and value["error"],
                f"{label}: malformed VM failure diagnostics/cost")
        if "failure" not in value and not require_typed:
            require(status == "rejected", f"{label}: historical inconclusive outcome lacks classification")
        else:
            kind = check_failure(value.get("failure"), label)
            require((status == "rejected") == (kind == "program"),
                    f"{label}: resource/infrastructure failure cannot be a program rejection")
        require(not require_conclusive or status != "inconclusive", f"{label}: inconclusive VM execution is not validation evidence")
    return result


def check_replay(value, label="VM replay", *, expected_code=None):
    require(isinstance(value, dict), f"{label}: malformed replay")
    status = value.get("status")
    if status == "accepted":
        require(set(value) == {"status", "output_hex", "cost"}, f"{label}: inconsistent accepted replay fields")
        check_native_outcome({key: value[key] for key in ("status", "output_hex")}, label)
        require(type(value["cost"]) is int and value["cost"] > 0, f"{label}: invalid replay cost")
        result = {"status": "accepted", "output_hex": value["output_hex"]}
    else:
        require(status == "rejected" and set(value) == {"status", "error", "failure"}
                and isinstance(value["error"], str) and value["error"], f"{label}: missing/malformed rejected replay")
        require(check_failure(value["failure"], label) == "program", f"{label}: non-program failure is not a replay rejection")
        if expected_code is not None:
            require(value["failure"]["code"] == expected_code, f"{label}: expected program failure code {expected_code}")
        result = {"status": "rejected"}
    return result


def execution_paths():
    """Closed wire/ledger and current aggregate-reader dependency paths."""
    paths = [PROTOCOL_PATH, "scripts/vm_failures.py", "www/scripts/vm-failures.mjs",
             "crates/chialisp-oxide-simulator/rejection-protocol.json",
             "crates/chialisp-oxide-simulator/Cargo.toml", "crates/chialisp-oxide-simulator/src/lib.rs",
             "scripts/ledger_rejections.py", "www/scripts/ledger-rejections.mjs",
             "crates/chialisp-oxide-vm/Cargo.toml",
             "crates/chialisp-oxide-simulator/p2-singleton-rejection-protocol.json", "www/scripts/p2-protocol.mjs",
             "scripts/p2_singleton_evidence.py", "scripts/p2_append_migration.py", "scripts/check-p2-singleton.py",
             "scripts/password_evidence.py", "scripts/password_ledger.py", "scripts/password_provenance.py",
             "scripts/password_vectors.py", "scripts/hashing_vectors.py", "scripts/check-canonical-factories.py",
             "scripts/cargo_authoring.py", "scripts/cargo_bundle.py", "scripts/check-readable-puzzles.py",
             "scripts/check-hashing-puzzles.py", "scripts/hashing_provenance.py",
             "corpus/validate_published_puzzles.py",
             "scripts/runtime_cost_migration.py", "scripts/runtime_inline_migration.py",
             "scripts/runtime_refinement_migration.py", "scripts/runtime_password_migration.py",
             "corpus/cost-optimization-baseline.json", "corpus/helper-inlining-baseline.json",
             "corpus/runtime-refinement-baseline.json", "corpus/p2-singleton-append-baseline.json",
             "puzzles/p2-singleton/vectors.py", "puzzles/p2-singleton/vectors.json",
             "puzzles/p2-singleton/generated-cases.json"]
    paths += [
        "crates/chialisp-oxide-corpus/src/boomerang.rs",
        "crates/chialisp-oxide-corpus/src/boomerang/ledger.rs",
        "crates/chialisp-oxide-ir/src/lower/decoder_tests.rs",
        "crates/chialisp-oxide-simulator/tests/wrong_key_signature.rs",
        "puzzles/boomerang/puzzle.json", "puzzles/boomerang/puzzle.rs",
        "puzzles/boomerang/reference.clsp", "puzzles/boomerang/puzzle.rue",
        "scripts/boomerang_evidence.py", "scripts/boomerang_source_controls.py",
        "scripts/check-boomerang.py", "scripts/run-boomerang-source-controls.py",
        "scripts/record-controls-quiet-window.py",
        "scripts/test_boomerang.py",
        "experiments/boomerang-controls/mutations.json",
        "experiments/boomerang-controls/native.rs",
        "experiments/boomerang-controls/ledger.rs",
        "www/scripts/boomerang-protocol.mjs",
        "www/scripts/test-boomerang-protocol.mjs",
        "www/src/lib/boomerang-source-controls.mjs",
        "www/scripts/test-boomerang-source-controls.mjs",
    ]
    # The source-cost reader verifies retained native/control captures against
    # their closed Cargo/production source window before pure historical projection.
    # Bind those inputs too; these hashes establish freshness, not execution.
    paths += [
        ".cargo/config.toml",
        "Cargo.lock",
        "Cargo.toml",
        "corpus/p2-singleton-costs.json",
        "corpus/p2-source-cost-baseline.json",
        "corpus/p2-source-cost-before-controls.json",
        "corpus/p2-source-cost-before-metrics.json",
        "corpus/p2-source-cost-before-p2.json",
        "corpus/p2-source-cost-controls.json",
        "crates/cargo-chialisp-oxide/Cargo.toml",
        "crates/cargo-chialisp-oxide/src/lib.rs",
        "crates/cargo-chialisp-oxide/src/main.rs",
        "crates/cargo-chialisp-oxide/src/options.rs",
        "crates/chialisp-oxide-cli/Cargo.toml",
        "crates/chialisp-oxide-cli/src/batch.rs",
        "crates/chialisp-oxide-cli/src/main.rs",
        "crates/chialisp-oxide-corpus/Cargo.toml",
        "crates/chialisp-oxide-corpus/build.rs",
        "crates/chialisp-oxide-corpus/examples/p2_singleton/codec.rs",
        "crates/chialisp-oxide-corpus/examples/p2_singleton/modules.rs",
        "crates/chialisp-oxide-corpus/examples/p2_singleton_controls.rs",
        "crates/chialisp-oxide-corpus/examples/p2_singleton_ledger.rs",
        "crates/chialisp-oxide-corpus/examples/p2_singleton_probe.rs",
        "crates/chialisp-oxide-corpus/src/authoring.rs",
        "crates/chialisp-oxide-corpus/src/factories.rs",
        "crates/chialisp-oxide-corpus/src/lib.rs",
        "crates/chialisp-oxide-corpus/src/native.rs",
        "crates/chialisp-oxide-corpus/src/native_hash.rs",
        "crates/chialisp-oxide-corpus/src/native_owned_tests.rs",
        "crates/chialisp-oxide-corpus/src/native_owned_tree.rs",
        "crates/chialisp-oxide-corpus/src/native_tree.rs",
        "crates/chialisp-oxide-corpus/src/p2_singleton.rs",
        "crates/chialisp-oxide-corpus/src/p2_singleton/codec.rs",
        "crates/chialisp-oxide-corpus/src/p2_singleton/modules.rs",
        "crates/chialisp-oxide-corpus/src/password.rs",
        "crates/chialisp-oxide-corpus/src/password/ledger.rs",
        "crates/chialisp-oxide-corpus/src/spends.rs",
        "crates/chialisp-oxide-corpus/src/spends/singleton.rs",
        "crates/chialisp-oxide-driver/Cargo.toml",
        "crates/chialisp-oxide-driver/build.rs",
        "crates/chialisp-oxide-driver/src/cargo.rs",
        "crates/chialisp-oxide-driver/src/exports.rs",
        "crates/chialisp-oxide-driver/src/lower.rs",
        "crates/chialisp-oxide-driver/src/lower/byte_comparison.rs",
        "crates/chialisp-oxide-driver/src/lower/constants.rs",
        "crates/chialisp-oxide-driver/src/lower/empty_atoms.rs",
        "crates/chialisp-oxide-driver/src/lower/hashing/canonical/body.rs",
        "crates/chialisp-oxide-driver/src/lower/hashing/canonical/bounds.rs",
        "crates/chialisp-oxide-driver/src/lower/hashing/canonical/mod.rs",
        "crates/chialisp-oxide-driver/src/lower/hashing/canonical/types.rs",
        "crates/chialisp-oxide-driver/src/lower/hashing/canonical/values.rs",
        "crates/chialisp-oxide-driver/src/lower/hashing/mod.rs",
        "crates/chialisp-oxide-driver/src/lower/newtypes.rs",
        "crates/chialisp-oxide-driver/src/lower/owned.rs",
        "crates/chialisp-oxide-driver/src/lower/runtime.rs",
        "crates/chialisp-oxide-driver/src/lower/totality.rs",
        "crates/chialisp-oxide-driver/src/lower/trees.rs",
        "crates/chialisp-oxide-driver/src/main.rs",
        "crates/chialisp-oxide-driver/src/toolchain.rs",
        "crates/chialisp-oxide-ir/Cargo.toml",
        "crates/chialisp-oxide-ir/src/lib.rs",
        "crates/chialisp-oxide-ir/src/lower.rs",
        "crates/chialisp-oxide-ir/src/lower/limits.rs",
        "crates/chialisp-oxide-ir/src/lower/runtime.rs",
        "crates/chialisp-oxide-ir/src/optimize.rs",
        "crates/chialisp-oxide-ir/src/optimize/budget.rs",
        "crates/chialisp-oxide-ir/src/optimize/constants.rs",
        "crates/chialisp-oxide-ir/src/optimize/facts.rs",
        "crates/chialisp-oxide-ir/src/optimize/inline.rs",
        "crates/chialisp-oxide-ir/src/optimize/intervals.rs",
        "crates/chialisp-oxide-ir/src/validate.rs",
        "crates/chialisp-oxide-ir/src/wire.rs",
        "crates/chialisp-oxide-macros/Cargo.toml",
        "crates/chialisp-oxide-macros/src/lib.rs",
        "crates/chialisp-oxide-reference/Cargo.toml",
        "crates/chialisp-oxide-reference/src/includes.rs",
        "crates/chialisp-oxide-reference/src/lib.rs",
        "crates/chialisp-oxide-rue/Cargo.toml",
        "crates/chialisp-oxide-rue/src/lib.rs",
        "crates/chialisp-oxide-simulator/examples/p2_singleton_cost.rs",
        "crates/chialisp-oxide-verifier/Cargo.toml",
        "crates/chialisp-oxide-verifier/src/acyclic.rs",
        "crates/chialisp-oxide-verifier/src/bin/chialisp-oxide-tree-reference.rs",
        "crates/chialisp-oxide-verifier/src/lib.rs",
        "crates/chialisp-oxide-verifier/src/main.rs",
        "crates/chialisp-oxide-verifier/src/model_bindings.rs",
        "crates/chialisp-oxide-verifier/src/replay.rs",
        "crates/chialisp-oxide-verifier/src/scalar_bitwise.rs",
        "crates/chialisp-oxide-verifier/src/structural_boolean.rs",
        "crates/chialisp-oxide-verifier/src/tree.rs",
        "crates/chialisp-oxide-verifier/src/tree_witness.rs",
        "crates/chialisp-oxide/Cargo.toml",
        "crates/chialisp-oxide/src/domain.rs",
        "crates/chialisp-oxide/src/apply.rs",
        "crates/chialisp-oxide/src/curry.rs",
        "crates/chialisp-oxide/src/hashing.rs",
        "crates/chialisp-oxide/src/lib.rs",
        "crates/chialisp-oxide/src/owned_tree.rs",
        "crates/chialisp-oxide/src/tree.rs",
        "experiments/backend/Cargo.toml",
        "experiments/backend/src/lib.rs",
        "experiments/backend/src/main.rs",
        "experiments/byte-proofs/Cargo.toml",
        "experiments/byte-proofs/src/bin/native.rs",
        "experiments/byte-proofs/src/constant_false.rs",
        "experiments/byte-proofs/src/constant_true.rs",
        "experiments/byte-proofs/src/lib.rs",
        "experiments/byte-proofs/src/operator_eq.rs",
        "experiments/byte-proofs/src/self_inequality.rs",
        "experiments/byte-proofs/src/swap_operands.rs",
        "experiments/p2-singleton-controls/Cargo.lock",
        "experiments/p2-singleton-controls/Cargo.toml",
        "experiments/p2-singleton-controls/mutations.json",
        "experiments/p2-singleton-controls/src/lib.rs",
        "experiments/p2-singleton-controls/src/proper_struct.rs",
        "experiments/p2-singleton-controls/src/reversed_parameters.rs",
        "experiments/p2-singleton-controls/src/wrong_dollar.rs",
        "puzzles/Cargo.toml",
        "puzzles/atom-not-equal/generated-cases.json",
        "puzzles/atom-not-equal/puzzle.rs",
        "puzzles/authoring.json",
        "puzzles/boolean-gate/puzzle.rs",
        "puzzles/bounded-counter/puzzle.rs",
        "puzzles/capped-payment/puzzle.rs",
        "puzzles/fixed-payment/puzzle.rs",
        "puzzles/fixture_keys.rs",
        "puzzles/insertion-sort-by-keys/generated-cases.json",
        "puzzles/insertion-sort-by-keys/puzzle.rs",
        "puzzles/intro-add/puzzle.rs",
        "puzzles/intro-conditional/puzzle.rs",
        "puzzles/intro-list/puzzle.rs",
        "puzzles/last-list-element/puzzle.rs",
        "puzzles/lib.rs",
        "puzzles/p2-conditions/puzzle.rs",
        "puzzles/p2-singleton/puzzle.rs",
        "puzzles/password-coin/generated-cases.json",
        "puzzles/password-coin/puzzle.rs",
        "puzzles/prefix-list/generated-cases.json",
        "puzzles/prefix-list/puzzle.rs",
        "puzzles/recovery-payment/puzzle.rs",
        "puzzles/rightmost-tree/puzzle.rs",
        "puzzles/sha256tree/generated-cases.json",
        "puzzles/sha256tree/puzzle.rs",
        "puzzles/signature-payment/puzzle.rs",
        "puzzles/singleton-launcher/puzzle.rs",
        "puzzles/split-payment/puzzle.rs",
        "puzzles/timelocked-payment/puzzle.rs",
        "puzzles/two-signature-payment/puzzle.rs",
        "rust-toolchain.toml",
        "scripts/check-p2-source-controls.py",
        "scripts/p2_source_cost_migration.py",
        "scripts/runtime_guarded_projection_migration.py",
        "corpus/guarded-projection-baseline.json",
        "corpus/checkpoints/p2-source-cost-validation.json",
        "corpus/guarded-projection-costs.json",
        "corpus/guarded-projection-controls.json",
        "vendor/rue-lir/Cargo.toml",
        "vendor/rue-lir/src/atom.rs",
        "vendor/rue-lir/src/codegen.rs",
        "vendor/rue-lir/src/debug_dialect.rs",
        "vendor/rue-lir/src/error.rs",
        "vendor/rue-lir/src/lib.rs",
        "vendor/rue-lir/src/lir.rs",
        "vendor/rue-lir/src/optimize.rs",
        "vendor/rue-lir/src/optimize/arg_list.rs",
        "vendor/rue-lir/src/optimize/ops.rs",
        "vendor/rue-lir/src/optimize/truthy.rs",
        "vendor/rue-lir/src/path.rs",
    ]
    # Current23 aggregate readers, canonical provenance and the public control
    # runner's admitted source closure. These bind freshness, not translation.
    paths += [
        "corpus/checkpoints/runtime-composition-validation.json",
        "corpus/p2-delegated-conditions-append-baseline.json",
        "corpus/p2dc-append-existing-p2-controls.json",
        "corpus/p2dc-append-existing-p2-costs.json",
        "corpus/p2dc-source-controls/report.json",
        "corpus/runtime-composition-baseline.json",
        "corpus/runtime-composition-controls.json",
        "corpus/runtime-composition-costs.json",
        "corpus/upstream/chia-gaming/11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4/LICENSE.MD",
        "corpus/upstream/chia-gaming/11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4/clsp/std/last.clinc",
        "corpus/upstream/chia-gaming/11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4/clsp/std/relops.clinc",
        "corpus/upstream/chia-gaming/11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4/commit.git",
        "corpus/upstream/chia-gaming/11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4/last_inner.clinc",
        "corpus/upstream/chia-gaming/11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4/not_equal.clinc",
        "corpus/upstream/chia-gaming/11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4/prefix.clinc",
        "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/LICENSE",
        "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/commit.git",
        "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/insertion_sort_by_keys.clinc",
        "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/or_macro.clinc",
        "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles/p2_delegated_conditions.clsp",
        "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles/p2_delegated_conditions.clsp.hex",
        "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles/p2_singleton.clsp",
        "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles/p2_singleton.clsp.hex",
        "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles/sha256tree.clib",
        "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles/vc_puzzles/cr_puzzles/flag_proofs_checker.clsp",
        "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/sha256tree.clinc",
        "corpus/upstream/chialisp-web/dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8/LICENSE",
        "corpus/upstream/chialisp-web/dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8/docs/chialisp-primer/first-smart-coin.md",
        "corpus/upstream/published-puzzles/Chia-Network--chia-gaming/11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4/tree.json",
        "corpus/upstream/published-puzzles/Chia-Network--chia_puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/tree.json",
        "corpus/upstream/published-puzzles/Chia-Network--chialisp-web/dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8/tree.json",
        "corpus/validation.json",
        "crates/cargo-chialisp-oxide/tests/cargo_workflow.rs",
        "crates/chialisp-oxide-cli/tests/batch.rs",
        "crates/chialisp-oxide-corpus/src/p2_delegated_conditions.rs",
        "crates/chialisp-oxide-corpus/src/p2_delegated_conditions/ledger.rs",
        "crates/chialisp-oxide-driver/src/lower/cons.rs",
        "crates/chialisp-oxide-driver/tests/acyclic_fallback.rs",
        "crates/chialisp-oxide-driver/tests/bitwise.rs",
        "crates/chialisp-oxide-driver/tests/byte_equality.rs",
        "crates/chialisp-oxide-driver/tests/byte_ordering.rs",
        "crates/chialisp-oxide-driver/tests/cargo_mode.rs",
        "crates/chialisp-oxide-driver/tests/cons.rs",
        "crates/chialisp-oxide-driver/tests/constant_fallback.rs",
        "crates/chialisp-oxide-driver/tests/empty_atoms.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/acyclic-expansion/src/lib.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/acyclic-expansion/src/main.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/arithmetic.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/bitwise.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/borrow_error.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/branches.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/bytes.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/casts.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/constant_bytes.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/division.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/expression_blowup.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/floating.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/generic.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/guarded_payment.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/helpers.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/ignored_environment.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/ignored_parameter.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/lazy_arithmetic.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/loops.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_arithmetic.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_drop.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_external_inner.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_generic.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_macro.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_multifield.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_nested.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_plain.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_roundtrip.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_sdk.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_swap.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_time_swap.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_unsupported.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/newtype_zero_sized.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/overflowing.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/panic.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/raw_environment.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/recursion.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/references.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/sdk_readable.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/sdk_readable_baseline.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/signed.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/signed_overflow.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/type_blowup.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/type_error.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/unit.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/unsafe_fn.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/unused_arguments.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/unused_arithmetic.rs",
        "crates/chialisp-oxide-driver/tests/fixtures/wrapping.rs",
        "crates/chialisp-oxide-driver/tests/frontend.rs",
        "crates/chialisp-oxide-driver/tests/hashing.rs",
        "crates/chialisp-oxide-driver/tests/hashing/library.rs",
        "crates/chialisp-oxide-driver/tests/newtypes.rs",
        "crates/chialisp-oxide-driver/tests/owned_trees.rs",
        "crates/chialisp-oxide-driver/tests/owned_trees/library.rs",
        "crates/chialisp-oxide-driver/tests/owned_trees/native.rs",
        "crates/chialisp-oxide-driver/tests/owned_trees/oracle.rs",
        "crates/chialisp-oxide-driver/tests/raw_trees.rs",
        "crates/chialisp-oxide-driver/tests/readability.rs",
        "crates/chialisp-oxide-driver/tests/runtime_calls.rs",
        "crates/chialisp-oxide-driver/tests/support/mod.rs",
        "crates/chialisp-oxide-driver/tests/support/vm.rs",
        "crates/chialisp-oxide-driver/tests/toolchain.rs",
        "crates/chialisp-oxide-driver/tests/toolchain/alias.rs",
        "crates/chialisp-oxide-driver/tests/toolchain/alias_without_injected_core.rs",
        "crates/chialisp-oxide-driver/tests/toolchain/bytes.rs",
        "crates/chialisp-oxide-driver/tests/toolchain/dependency.rs",
        "crates/chialisp-oxide-driver/tests/toolchain/harness.rs",
        "crates/chialisp-oxide-driver/tests/toolchain/host.rs",
        "crates/chialisp-oxide-driver/tests/toolchain/transitive.rs",
        "crates/chialisp-oxide-ir/tests/atom_equality.rs",
        "crates/chialisp-oxide-ir/tests/atom_is_empty.rs",
        "crates/chialisp-oxide-ir/tests/atom_ordering.rs",
        "crates/chialisp-oxide-ir/tests/bitwise.rs",
        "crates/chialisp-oxide-ir/tests/compilation_limits.rs",
        "crates/chialisp-oxide-ir/tests/cons.rs",
        "crates/chialisp-oxide-ir/tests/constant_optimization.rs",
        "crates/chialisp-oxide-ir/tests/constructed_atoms.rs",
        "crates/chialisp-oxide-ir/tests/execution_facts.rs",
        "crates/chialisp-oxide-ir/tests/function_inlining.rs",
        "crates/chialisp-oxide-ir/tests/integer_decoder.rs",
        "crates/chialisp-oxide-ir/tests/intervals.rs",
        "crates/chialisp-oxide-ir/tests/optimizer_limits.rs",
        "crates/chialisp-oxide-ir/tests/runtime_aliases.rs",
        "crates/chialisp-oxide-ir/tests/runtime_atom_facts.rs",
        "crates/chialisp-oxide-ir/tests/runtime_evaluation.rs",
        "crates/chialisp-oxide-ir/tests/runtime_evaluation/support.rs",
        "crates/chialisp-oxide-ir/tests/runtime_optimization.rs",
        "crates/chialisp-oxide-ir/tests/runtime_pair_helper_facts.rs",
        "crates/chialisp-oxide-ir/tests/runtime_projection_aliases.rs",
        "crates/chialisp-oxide-ir/tests/runtime_unary_frames.rs",
        "crates/chialisp-oxide-ir/tests/runtime_validation.rs",
        "crates/chialisp-oxide-ir/tests/semantics.rs",
        "crates/chialisp-oxide-ir/tests/sha256.rs",
        "crates/chialisp-oxide-ir/tests/tree_optimization.rs",
        "crates/chialisp-oxide-ir/tests/trees.rs",
        "crates/chialisp-oxide-ir/tests/typed_bottom.rs",
        "crates/chialisp-oxide-ir/tests/wire.rs",
        "crates/chialisp-oxide-rue/tests/canonical_boolean_values.rs",
        "crates/chialisp-oxide-rue/tests/corpus.rs",
        "crates/chialisp-oxide-rue/tests/diagnostics.rs",
        "crates/chialisp-oxide-rue/tests/nested_bitwise_failure_order.rs",
        "crates/chialisp-oxide-simulator/examples/fixture-keys.rs",
        "crates/chialisp-oxide-verifier/tests/acyclic_scalar.rs",
        "crates/chialisp-oxide-verifier/tests/atom_is_empty.rs",
        "crates/chialisp-oxide-verifier/tests/bitwise.rs",
        "crates/chialisp-oxide-verifier/tests/bitwise_inputs.rs",
        "crates/chialisp-oxide-verifier/tests/cons.rs",
        "crates/chialisp-oxide-verifier/tests/existing_queries.rs",
        "crates/chialisp-oxide-verifier/tests/fixtures/acyclic-source/src/lib.rs",
        "crates/chialisp-oxide-verifier/tests/fixtures/acyclic-source/src/main.rs",
        "crates/chialisp-oxide-verifier/tests/ir_models.rs",
        "crates/chialisp-oxide-verifier/tests/replay_failures.rs",
        "crates/chialisp-oxide-verifier/tests/runtime_boundaries.rs",
        "crates/chialisp-oxide-verifier/tests/sha256_models.rs",
        "crates/chialisp-oxide-verifier/tests/solver_failures.rs",
        "crates/chialisp-oxide-verifier/tests/structural_boolean.rs",
        "crates/chialisp-oxide-verifier/tests/tree_models.rs",
        "crates/chialisp-oxide-verifier/tests/typed_bottom.rs",
        "crates/chialisp-oxide-vm/tests/real_vm_failures.rs",
        "crates/chialisp-oxide/src/cons.rs",
        "crates/chialisp-oxide/tests/hashing.rs",
        "experiments/backend/tests/differential.rs",
        "experiments/backend/tests/runtime_calls.rs",
        "experiments/p2dc-source-controls/native.rs",
        "puzzles/atom-not-equal/puzzle.json",
        "puzzles/atom-not-equal/puzzle.rue",
        "puzzles/atom-not-equal/reference.clsp",
        "puzzles/boolean-gate/puzzle.json",
        "puzzles/boolean-gate/puzzle.rue",
        "puzzles/boolean-gate/reference.clsp",
        "puzzles/bounded-counter/puzzle.json",
        "puzzles/bounded-counter/puzzle.rue",
        "puzzles/bounded-counter/reference.clsp",
        "puzzles/capped-payment/puzzle.json",
        "puzzles/capped-payment/puzzle.rue",
        "puzzles/capped-payment/reference.clsp",
        "puzzles/fixed-payment/puzzle.json",
        "puzzles/fixed-payment/puzzle.rue",
        "puzzles/fixed-payment/reference.clsp",
        "puzzles/insertion-sort-by-keys/puzzle.json",
        "puzzles/insertion-sort-by-keys/puzzle.rue",
        "puzzles/insertion-sort-by-keys/reference.clsp",
        "puzzles/intro-add/puzzle.json",
        "puzzles/intro-add/puzzle.rue",
        "puzzles/intro-add/reference.clsp",
        "puzzles/intro-conditional/puzzle.json",
        "puzzles/intro-conditional/puzzle.rue",
        "puzzles/intro-conditional/reference.clsp",
        "puzzles/intro-list/puzzle.json",
        "puzzles/intro-list/puzzle.rue",
        "puzzles/intro-list/reference.clsp",
        "puzzles/last-list-element/puzzle.json",
        "puzzles/last-list-element/puzzle.rue",
        "puzzles/last-list-element/reference.clsp",
        "puzzles/p2-conditions/puzzle.json",
        "puzzles/p2-conditions/puzzle.rue",
        "puzzles/p2-conditions/reference.clsp",
        "puzzles/p2-delegated-conditions/independent-vectors.json",
        "puzzles/p2-delegated-conditions/mutation-recipes.json",
        "puzzles/p2-delegated-conditions/puzzle.json",
        "puzzles/p2-delegated-conditions/puzzle.rs",
        "puzzles/p2-delegated-conditions/puzzle.rue",
        "puzzles/p2-delegated-conditions/reference.clsp",
        "puzzles/p2-singleton/puzzle.json",
        "puzzles/p2-singleton/puzzle.rue",
        "puzzles/p2-singleton/reference.clsp",
        "puzzles/password-coin/puzzle.json",
        "puzzles/password-coin/puzzle.rue",
        "puzzles/password-coin/reference.clsp",
        "puzzles/prefix-list/puzzle.json",
        "puzzles/prefix-list/puzzle.rue",
        "puzzles/prefix-list/reference.clsp",
        "puzzles/recovery-payment/puzzle.json",
        "puzzles/recovery-payment/puzzle.rue",
        "puzzles/recovery-payment/reference.clsp",
        "puzzles/rightmost-tree/puzzle.json",
        "puzzles/rightmost-tree/puzzle.rue",
        "puzzles/rightmost-tree/reference.clsp",
        "puzzles/sha256tree/puzzle.json",
        "puzzles/sha256tree/puzzle.rue",
        "puzzles/sha256tree/reference.clsp",
        "puzzles/signature-payment/puzzle.json",
        "puzzles/signature-payment/puzzle.rue",
        "puzzles/signature-payment/reference.clsp",
        "puzzles/singleton-launcher/puzzle.json",
        "puzzles/singleton-launcher/puzzle.rue",
        "puzzles/singleton-launcher/reference.clsp",
        "puzzles/split-payment/puzzle.json",
        "puzzles/split-payment/puzzle.rue",
        "puzzles/split-payment/reference.clsp",
        "puzzles/tests/policies.rs",
        "puzzles/timelocked-payment/puzzle.json",
        "puzzles/timelocked-payment/puzzle.rue",
        "puzzles/timelocked-payment/reference.clsp",
        "puzzles/two-signature-payment/puzzle.json",
        "puzzles/two-signature-payment/puzzle.rue",
        "puzzles/two-signature-payment/reference.clsp",
        "corpus/checkpoints/p2dc-append-validation.json",
        "corpus/corpus-append-existing-p2-controls.json",
        "corpus/corpus-append-existing-p2-costs.json",
        "corpus/corpus-append.json",
        "scripts/capture-p2-controls.py",
        "scripts/check-corpus-append.py",
        "scripts/corpus_append.py",
        "scripts/check-p2-delegated-conditions.py",
        "scripts/check-p2-source-cost.py",
        "scripts/check-p2dc-append.py",
        "scripts/check-p2dc-source-controls.py",
        "scripts/check-runtime-composition.py",
        "scripts/p2_cli.py",
        "scripts/p2_delegated_conditions_evidence.py",
        "scripts/p2_delegated_conditions_vectors.py",
        "scripts/p2dc_append_migration.py",
        "scripts/p2dc_source_controls.py",
        "scripts/run-p2dc-source-controls.py",
        "scripts/runtime_composition_migration.py",
        "scripts/test_p2dc_source_controls.py",
        "vendor/rue-lir/tests/canonical_boolean_values.rs",
        "vendor/rue-lir/tests/codegen_stack.rs",
        "vendor/rue-lir/tests/nested_bitwise_failure_order.rs",
        "vendor/rue-lir/tests/reconstruction_preserves_failure.rs",
        "vendor/rue-lir/tests/run_nil_environment.rs",
        "vendor/rue-lir/tests/truthiness_preserves_failure.rs",
        "www/scripts/check-evidence.mjs",
        "www/scripts/p2dc-protocol.mjs"
]
    paths += _source_control_artifact_paths()
    paths += _boomerang_control_artifact_paths()
    from p2_singleton_evidence import RUNTIME_INPUTS
    paths += list(RUNTIME_INPUTS)
    paths += [str(path.relative_to(ROOT)) for path in (ROOT / "crates/chialisp-oxide-vm/src").rglob("*.rs")]
    return sorted(set(paths))


def _source_control_artifact_paths():
    """Retained data only; no current validator, producer or external path IO.

    The public packet hashes this module's text, so packet hashes must never be
    embedded here. The separate recorded reader validates its full semantics.
    """
    import p2dc_source_controls as controls
    base = ROOT / "corpus/p2dc-source-controls"
    value = controls.load(base / "report.json")
    require(value.get("protocol") == controls.PROTOCOL
            and value.get("status") == "actual_source_controls_captured",
            "Actual public source-control packet required")
    rows = value.get("files")
    require(type(rows) is list and len(rows) == 86, "Closed86 retained control files required")
    paths = []
    for row in rows:
        require(type(row) is dict and set(row) == {"path", "sha256"}
                and type(row["path"]) is str and len(row["path"]) <= 4096
                and type(row["sha256"]) is str
                and re.fullmatch("[0-9a-f]{64}", row["sha256"]) is not None,
                "Malformed retained control file identity")
        path = controls.relative(base, row["path"])
        require(controls.sha(path) == row["sha256"], "Retained control data bytes changed")
        paths.append("corpus/p2dc-source-controls/" + row["path"])
    require(paths == sorted(set(paths)), "Sorted unique retained control files required")
    return paths


def execution_sources():
    return [{"path": path, "sha256": hashlib.sha256((ROOT / path).read_bytes()).hexdigest()}
            for path in execution_paths()]


def _boomerang_control_artifact_paths():
    """Bounded retained-file identities only; the independent gate checks semantics."""
    import boomerang_source_controls as controls
    base = ROOT / "corpus/boomerang-source-controls"
    value = controls.load(base / "report.json")
    require(value.get("protocol") == controls.PROTOCOL
            and value.get("status") == "actual_source_controls_captured",
            "Actual boomerang source-control packet required")
    rows = value.get("files")
    require(type(rows) is list and 1 <= len(rows) <= 512, "Bounded retained boomerang files required")
    paths = []
    for row in rows:
        require(type(row) is dict and set(row) == {"path", "sha256"}
                and type(row["path"]) is str and len(row["path"]) <= 4096
                and type(row["sha256"]) is str
                and re.fullmatch("[0-9a-f]{64}", row["sha256"]) is not None,
                "Malformed retained boomerang file identity")
        path = controls.relative(base, row["path"])
        require(controls.sha(path) == row["sha256"], "Retained boomerang data bytes changed")
        paths.append("corpus/boomerang-source-controls/" + row["path"])
    require(paths == sorted(set(paths)), "Sorted unique retained boomerang files required")
    return ["corpus/boomerang-source-controls/report.json", *paths]


class FailureProtocolTests(unittest.TestCase):
    def test_every_code_kind_and_envelope(self):
        for code, kind in FAILURES.items():
            value = {"outcome": {"status": "rejected" if kind == "program" else "inconclusive"},
                     "cost": None, "error": "arbitrary human diagnostic", "failure": {"kind": kind, "code": code}}
            with self.subTest(code=code):
                check_execution(value, require_conclusive=False)
                if kind != "program":
                    with self.assertRaises(ValueError): check_execution(value)

    def test_forged_missing_unknown_and_success_metadata(self):
        rejected = {"outcome": {"status": "rejected"}, "cost": None, "error": "Raise(spoofed)",
                    "failure": {"kind": "program", "code": "raise"}}
        import copy
        for mutation in ("missing", "unknown", "resource", "infrastructure", "forged", "null", "cost"):
            bad = copy.deepcopy(rejected)
            if mutation == "missing": del bad["failure"]
            elif mutation == "unknown": bad["failure"]["code"] = "future_code"
            elif mutation == "resource": bad["failure"] = {"kind": "resource", "code": "cost_exceeded"}
            elif mutation == "infrastructure": bad["failure"] = {"kind": "infrastructure", "code": "internal_error"}
            elif mutation == "forged": bad["failure"]["code"] = "cost_exceeded"
            elif mutation == "null": bad["failure"] = None
            else: bad["cost"] = 1
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): check_execution(bad)
        success = {"outcome": {"status": "accepted", "output_hex": "80"}, "cost": 1, "error": None}
        check_execution(success)
        for failure in (None, {"kind": "program", "code": "raise"}):
            with self.assertRaises(ValueError): check_execution({**success, "failure": failure})
        historical = {key: value for key, value in rejected.items() if key != "failure"}
        check_execution(historical, require_typed=False)
        with self.assertRaises(ValueError): check_execution(historical)

    def test_replay_and_native_are_distinct(self):
        check_native_outcome({"status": "rejected"})
        with self.assertRaises(ValueError): check_replay({"status": "rejected"})
        for code, kind in FAILURES.items():
            replay = {"status": "rejected", "error": "Raise(spoofed)", "failure": {"kind": kind, "code": code}}
            if kind == "program": check_replay(replay)
            else:
                with self.assertRaises(ValueError): check_replay(replay)
        with self.assertRaises(ValueError):
            check_replay({"status": "rejected", "error": "Raise(spoofed)", "failure": {"kind": "program", "code": "path_into_atom"}}, expected_code="raise")


if __name__ == "__main__":
    unittest.main()
