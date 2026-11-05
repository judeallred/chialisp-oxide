#!/usr/bin/env python3
"""Fail closed on unknown, timeout, errors, or missing mutation detection."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[1]
result = subprocess.run(
    ["z3", str(root / "proofs/capped-payment.smt2")],
    text=True, capture_output=True, timeout=20, check=True,
)
statuses = [line for line in result.stdout.splitlines() if line in {"sat", "unsat", "unknown"}]
if statuses != ["unsat", "unsat", "sat"] or "(error" in result.stdout or result.stderr:
    raise SystemExit(f"Proof checks failed:\n{result.stdout}\n{result.stderr}")
print("PASS: integer-policy equivalence; range invariant; missing-guard mutation detected")
print(result.stdout.strip())
