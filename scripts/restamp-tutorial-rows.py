#!/usr/bin/env python3
"""Restamp promoted tutorial rows in corpus/tutorial-examples.json after a capture.

Every promoted row pins the exact bytes of corpus/validation.json plus its
source files, so each regeneration must restamp them before
corpus/validate_tutorial_examples.py runs. Usage: python3 scripts/restamp-tutorial-rows.py .
This rewrites metadata only; it adds no execution credit and the validator still
checks every claim against the report.
"""
import hashlib, json, sys
ROOT = sys.argv[1] if len(sys.argv) > 1 else "."
CONTRACTS = {
    "module-add": {"solution": "(2 3)", "expected_output": "5", "expected_output_hex": "05",
                   "feasibility": "Implemented as a real Cargo export with two signed 64-bit parameters through the default strict typed ABI; every original/adapter difference is declared per case."},
    "module-times-ten": {"solution": "(5)", "expected_output": "50", "expected_output_hex": "32",
                         "feasibility": "Implemented as a real Cargo export keeping the named constant and helper function with signed 64-bit checked multiplication; every original/adapter difference is declared per case."},
    "module-factorial": {"solution": "(5)", "expected_output": "120", "expected_output_hex": "78",
                         "feasibility": "Implemented as a real Cargo export with checked signed 64-bit recursion through the closed runtime function table; 21 and above reject where the original grows, and every difference is declared per case."},
    "currying-multiply": {"solution": "(2 5)", "expected_output": "10", "expected_output_hex": "0a",
                          "feasibility": "Implemented as a real Cargo export with two signed 64-bit parameters through the default strict typed ABI; every original/adapter difference is declared per case, and each build's standard curried instance with the page arguments is built independently, cross-checked against the SDK curry helpers and executed in five builds.",
                          "curried": {"argument": "2", "page_result": "(a (q 18 2 5) (c (q . 2) 1))", "solution": "(5)", "expected_output": "10", "expected_output_hex": "0a"}},
}
CURRY_SCHEME = "(a (q . M) (c (q . A) 1))"
NOTES = [
    "The complete fenced module span is compiled unchanged by the independent Chialisp compiler; no wrapper or argument guard is added.",
    "Rust reads canonical signed 64-bit atoms through the strict typed ABI. Padded atoms, 0x00 for zero, extra list elements, improper tails, out-of-range values and checked-arithmetic overflow reject in Rust; each such case declares the original's independently specified result and the authored Rue translation shares it.",
    "Named cases are compared through native Cargo Rust, optimized/unoptimized Rust CLVM, independently compiled original Chialisp and Rue in both modes. These finite observations are not a compiler-preservation or universal equivalence proof.",
]
def sha(path):
    return hashlib.sha256(open(f"{ROOT}/{path}", "rb").read()).hexdigest()
inventory_path = f"{ROOT}/corpus/tutorial-examples.json"
raw = open(inventory_path).read()
indent = len(raw.splitlines()[1]) - len(raw.splitlines()[1].lstrip(" "))
inventory = json.loads(raw)
# Every promoted row pins the exact report bytes; restamp them all.
for row in inventory["examples"]:
    if "source_adaptation" in row and "report_sha256" in row["source_adaptation"]:
        row["source_adaptation"]["report_sha256"] = hashlib.sha256(open(f"{ROOT}/corpus/validation.json", "rb").read()).hexdigest()
report = json.load(open(f"{ROOT}/corpus/validation.json"))
for row in inventory["examples"]:
    slug = row["id"].split(":", 1)[1]
    if slug not in CONTRACTS:
        continue
    contract = CONTRACTS[slug]
    puzzle = next(p for p in report["puzzles"] if p["slug"] == slug)
    divergent = sum("declared_divergence" in c for c in puzzle["cases"])
    paths = {"rust_source": f"puzzles/{slug}/puzzle.rs", "reference": f"puzzles/{slug}/reference.clsp",
             "rue_source": f"puzzles/{slug}/puzzle.rue", "manifest": f"puzzles/{slug}/puzzle.json"}
    row["feasibility"] = contract["feasibility"]
    row["notes"] = NOTES
    row["implementation_status"] = "implemented"
    row["validation_status"] = "differential_only"
    row["local_examples"] = [paths["rust_source"], paths["reference"], paths["rue_source"]]
    row["evidence"] = ["corpus/validation.json"]
    row["proposed_observations"] = [{
        "solution": contract["solution"], "expected_output": contract["expected_output"],
        "basis": "Independently specified page result, checked against the linked executed differential cases.",
        "validation_status": "differential_only"}]
    adaptation = {"operation": "extract_complete_chialisp_module", "source_span_sha256": row["source"]["sha256"],
                  "report": "corpus/validation.json", "report_sha256": sha("corpus/validation.json"), "slug": slug}
    for key, path in paths.items():
        adaptation[key] = path
        adaptation[key + "_sha256"] = sha(path)
    row["source_adaptation"] = adaptation
    row["validation_contract"] = {
        "abi": "strict_typed_i64_list", "environment": "strict", "reference_mode": "module",
        "expected_solution": contract["solution"], "expected_output": contract["expected_output"],
        "expected_output_hex": contract["expected_output_hex"],
        "integer_domain": "signed_64_bit_checked_adapter_of_unbounded_original",
        "declared_divergences": divergent,
        "claim_scope": "executed_differential_cases_with_declared_original_divergences_only",
        "ledger": "not_applicable_pure_function", "compiler_preservation": "not_claimed"}
    if "curried" in contract:
        curried = contract["curried"]
        examples = puzzle["curried_examples"]
        row["validation_contract"]["curried_instances"] = {
            "scheme": CURRY_SCHEME, "page_argument": curried["argument"], "page_result": curried["page_result"],
            "expected_solution": curried["solution"], "expected_output": curried["expected_output"],
            "expected_output_hex": curried["expected_output_hex"], "examples": len(examples),
            "instance_cases": sum(len(example["cases"]) for example in examples),
            "claim_scope": "executed_curried_instance_cases_in_five_builds_only",
            "instance_hash_identity": "chialisp_build_equals_page_result_only"}
        row["notes"] = NOTES + ["Each build's artifact is curried with the page arguments in the standard form (a (q . M) (c (q . A) 1)), built independently with clvmr, cross-checked against the SDK's owned constructor and hash arithmetic, and executed on page and adversarial solutions in five builds. Only the Chialisp build's instance is compared with the page's spelled curried result; the Rust instances are different programs with different hashes, and no identity is claimed for the Rue instances even where their bytes coincide."]
inventory["scope"]["description"] = ("Source-level program inventory of eight pinned Chialisp primer/concept tutorials. "
    "Three closed Intro expressions, the password module and the three Using Modules programs have executed "
    "Cargo/native/original-reference/CLVM evidence; the module programs run through declared signed 64-bit typed "
    "adapters with every original difference recorded per case. The other five programs, one naming variant and "
    "supporting host workflows remain unimplemented or unrun. The ten original teaching fixtures are a separate corpus.")
with open(inventory_path, "w") as handle:
    json.dump(inventory, handle, indent=indent)
    handle.write("\n")
print("stamped", list(CONTRACTS))
