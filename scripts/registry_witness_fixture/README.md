# Witness generation evidence fixture

WIP prototype, not ready for use. This is a template materialized by
`scripts/check-registry-witness.py --execute`; generated modules are intentionally
absent here. The runner creates a fresh isolated Cargo library, publishes and
pins exact canonical artifacts, and invokes the real offline
`registry generate-witness-rust` command for each local module.

The six annotated exports accept the complete raw environment
`(program_witness . complete_foreign_environment)`. Multiplication receives
`(2 . complete_raw_solution)` as its foreign environment; this is supplied by
the caller, never inferred or silently framed by the generated helper. Two
canonical quote modules return all 257 or 8192 bytes. Their large payloads exist
in caller witnesses and native test data, never generated on-chain literals.

The local generated helper uses the existing `puzzles/inner-puzzle` pattern:
logical-tree SHA-256, fixed commitment assertion, then strict foreign Apply.
The exact ordinary hash algorithm and Apache-2.0 source/license pins are emitted
by the generator and checked against `puzzles/sha256tree/puzzle.json`. No source
name, annotation or provenance comment grants compiler semantics.

Runtime commitment assumes SHA-256 collision/second-preimage resistance. It does
not bind original wire headers, package envelopes, source or authorship. The
witness changes funding ABI and puzzle hash; this is foreign-program composition,
not a Rust implementation of foreign arithmetic or all Chialisp. Quote programs
ignore their environments, so discarded quote calls do not establish preservation
of a matching-program failure. Multiplication provides that separate observation.

Native tests enable `vm` only as a dev dependency. Actual subset and paired Cargo
builds must retain default-only facade features, complete schema2 compiler reads,
every declared export, exact IR and artifact inventories. Finite replays, source
mutants, regeneration conflicts and compiler diagnostics are separate evidence;
none proves cryptography, absence of vulnerabilities, or compiler preservation.
