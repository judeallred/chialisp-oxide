---
title: Hashing and the compiler bridge
description: Exact byte semantics, implementation identity, strict evaluation and explicit cryptographic trust assumptions.
---

**WIP prototype. Not ready for production or use with funds.**

`chialisp_oxide::sha256(&[u8]) -> [u8; 32]` and
`sha256_parts(&[u8], &[u8], &[u8]) -> [u8; 32]` are ordinary safe Rust functions.
They hash exact payload bytes and preserve every digest byte. The multipart
function hashes concatenation without separators or length framing.

The [tree-hash lesson](/chialisp-oxide/tutorials/sha256tree/) names the one-byte
atom/pair domains and compares the published helper with Rust and Rue. The
[launcher lesson](/chialisp-oxide/tutorials/singleton-launcher/) uses that
commitment while preserving arbitrary raw fields. It checks the original
Chialisp module's published hash separately from the Rust/Rue ports' hashes.

## A familiar name cannot grant primitive behavior

A function called `sha256_parts` could return zeros. The frontend must preserve
what Rust checked, so neither a package name, function spelling nor marker
attribute is enough to replace a call with SHA opcode 11.

The bridge selects the actual reserved SDK export as a candidate, requires its
exact Rust signature, and compares a closed canonical encoding of its typed MIR
implementation with independently reviewed target/profile pins. Changing a
reserved implementation fails with a diagnostic. Unrelated same-named
functions and the ordinary single-part wrapper retain their actual bodies.

The encoding includes statement/operand order, control flow, types/layouts,
literal constants and their bodies, supported calls, and compiler/target/profile
configuration. Unsupported constructs fail closed. Bounds precede type/constant
normalization and constant evaluation, including unused literal temporaries.
The initial kernel uses safe scalar arrays/loops and the actual installed core
slice-length operation. This exception does not admit general crypto crates,
trait dispatch, architecture-specific implementations or arbitrary loops.

Current pins cover `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`, with
the dated compiler, overflow checks and debug assertions enabled, and
`panic=unwind`. Other profiles receive a diagnostic. The standalone research
frontend's usual `panic=abort` is outside these hash pins; use the documented
Cargo authoring route and supported profile.

The [installed toolchain boundary](/chialisp-oxide/reference/compiler-trust/)
still applies. Compiler and standard-library bytes, metadata/native-object
consistency and stable build inputs remain trusted. A source fingerprint is not
authentication of native machine code, a compiler sandbox, or a proof that the
kernel implements SHA-256.

## Preserve evaluation and resource failures

Hashing uses version 4's strict runtime bindings. Arguments evaluate once in
Rust order, including when the result is unused. Unchosen branches remain
lazy. Malformed pair operands fail before later computations; a diverging
earlier computation cannot become immediate rejection merely because a later
operand is malformed. Both optimization modes have executed controls.

The SDK has three parts. The IR's nominal maximum of 64 parts does not override
global CLVM expansion limits or guarantee successful compilation of every
large expression. Native/VM resource limits are inconclusive failures, not
evidence that a policy rejects its input.

The native kernel uses wide counters and encodes the low 64-bit bit length.
[FIPS 180-4](https://csrc.nist.gov/pubs/fips/180-4/upd1/final) specifies SHA-256
for messages below 2^64 bits; the supported
bounded CLVM atoms are far below that limit. This is not a verified cryptographic
implementation or a universal resource/termination proof.

## Read the assurance claims separately

Native known-answer vectors, multipart/padding tests, compiled mutants and
independent OpenSSL checks test the actual SDK implementation. Native Linux
execution and Mac-host Linux-target MIR analysis are separate checks; the
local Linux worker can run under explicitly recorded CPU emulation.

The pinned VM also uses RustCrypto. Agreement with that same dependency alone
does not supply an independent crypto oracle. Published known answers and
separately executed OpenSSL implementations provide additional finite evidence.

The scalar, byte and structural-tree proof APIs reject SHA everywhere, including
dead branches and unused definitions. They introduce no collision-free,
injective or uninterpreted-hash axiom. A proof about a future hash-based policy
will need an explicit model and assumptions. Tests and identity controls do not
establish collision resistance, preimage secrecy, constant-time execution,
source-to-CLVM preservation or authorization of a spend.
