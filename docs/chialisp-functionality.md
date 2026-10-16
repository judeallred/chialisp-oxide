# Chialisp functionality and the Rust compiler gap

Rust can express a CLVM interpreter and the algorithms used by Chialisp puzzles.
The existing [Rust CLVM implementation](https://github.com/Chia-Network/clvm_rs/tree/5692395a4ef0776a08c10aaaa66406e0408dd5f0)
is a concrete host-language witness. That does **not** prove that this project's
Rust-to-CLVM compiler implements every Chialisp feature, preserves every puzzle,
or produces an affordable or hash-compatible program. It currently does not.
The project remains a prototype and is not ready for use with funds.

The requested destination is broad functionality with strong assurance. Current
tests, bounded models, and source discovery are separate pieces of evidence;
none is a universal compiler-correctness or security proof.

## Published source discovery

[published-puzzles.json](../corpus/published-puzzles.json) records 3,582 selected
files from 98 complete, commit-pinned Git trees observed on 2026-10-16. Two
additional discovered repositories were empty and remain recorded as failures.
The sweep combines 14 seed repositories with all returned results of three
GitHub repository searches, each explicitly filtered with `is:public`.
Captured repository metadata independently records `visibility: public` and
`private: false` for every enumerated tree; the validator requires both.
Its scope is deliberately stated in the JSON.

The selected files include Chialisp libraries, compiler-negative fixtures,
generated solutions/expected values, serialized CLVM, Rue examples, and Rust
host tooling. **These are file counts, not counts of distinct, valid, deployed,
audited, ported, or verified puzzles.** Rust wallet builders and generated Rust
bindings for existing CLVM do not demonstrate compilation of Rust puzzle logic.

Important source families are pinned independently:

- [Standard Chia puzzles](https://github.com/Chia-Network/chia_puzzles/tree/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles):
  standard delegated puzzles, CATs, singletons, NFTs, DID, DAO, credentials,
  pools, MIPS, and consensus/full-node programs. The repository also publishes
  compiled artifacts and generated bindings; its stated continuity requirement
  makes module identity a separate interoperability obligation.
- [Chia wallet integration](https://github.com/Chia-Network/chia-blockchain/tree/9e54d9cb1d2bd5b1801a26f7f8fc73d156b6b925/chia/wallet/puzzles)
  and [Rust puzzle types](https://github.com/Chia-Network/chia_rs/tree/446ccb03952b5032db0e1460694c54a6a9ffa9a7/crates/chia-puzzle-types):
  remaining source libraries, custody policies, typed puzzle/solution codecs,
  and host consensus integration.
- [Rue](https://github.com/xch-dev/rue/tree/245c107a8e795229ea08097142f005a4c0d91075):
  every tracked `.rue` source, including standard-library modules, five puzzle
  examples, general language examples, and tests. Rue's Rust compiler source is
  inventoried as host tooling, not as a collection of Rust-authored puzzles.
- [Chia Wallet SDK](https://github.com/xch-dev/chia-wallet-sdk/tree/9f8a3bcc473f4731ce86f571baf7373cce81e1ac):
  Rust puzzle builders, layers, types, bindings, and the separately identified
  Chialisp/Rue example and compiler-test sources.
- [Chia gaming](https://github.com/Chia-Network/chia-gaming/tree/11e1c80ecd83d9a343fcd83fa2e7314c968c0ba4),
  [CHIP source assets](https://github.com/Chia-Network/chips/tree/6fa7625dfcc895177dbecac20c838498e6a60ceb/assets),
  compiler/VM repositories, developer tools, and the community repositories
  named in the discovery snapshot extend the standard-puzzle scope.

Each file has its repository, exact commit, path, Git blob SHA1, pinned link,
repository license metadata, and explicit triage category. A Git blob hash
identifies tracked file bytes; it is **not** the CLVM tree hash. Hex-file hashes
identify the hex text. Source-byte SHA256 is not fabricated from Git SHA1.
License metadata is not a per-file license review or permission to port code.

The local complete tree manifests include paths, modes, kinds, and object IDs.
The offline validator reconstructs every directory's Git tree hash and requires
the exact suffix-selected source set, so omitted catalog files cannot hide
behind a summary count. Run:

```sh
python3 corpus/validate_published_puzzles.py --self-test
```

Discovery still excludes private/deleted/unindexed repositories, other hosts,
future revisions, unrecorded linked repositories, programs embedded in other
file formats, and unrecognized filename extensions. Submodules and symlinks are
recorded but not followed. The separate
[tutorial page/span inventory](../corpus/tutorial-examples.json) handles pinned
documentation examples. Neither inventory is an all-internet completeness proof.

## What a preservation claim would mean

Choose a source puzzle, a CLVM dialect/activation context, and an input domain.
Then compare all of the following explicitly:

1. The environment: arbitrary atom/pair trees, integer encodings, argument
   projection, and any typed adapter restrictions.
2. Results and failures: exact output bytes, strict evaluation of discarded
   computations, branch laziness, and numeric/error behavior.
3. Resources: VM cost, serialization/condition costs, maximum input sizes,
   recursion, and the intended consensus budget.
4. Identity and composition: module/curried-instance hashes, coin IDs, delegated
   programs, announcements, signatures, and state transitions as applicable.
5. The translation chain: checked Rust MIR to typed IR, IR lowering, optimizer,
   serialization, and the selected VM's semantics.

Finite differential cases establish observations on those cases. Model-level
SMT results establish their stated formulas under their assumptions. A native
Rust interpreter demonstrates expressibility. Those statements must not be
substituted for a preservation proof across this entire chain.

## Functionality matrix

The Chialisp [operator reference](https://chialisp.com/operators/) defines the
source-language families. The exact execution semantics come from the pinned
[VM dialect](https://github.com/Chia-Network/clvm_rs/blob/5692395a4ef0776a08c10aaaa66406e0408dd5f0/src/chia_dialect.rs),
[numeric implementation](https://github.com/Chia-Network/clvm_rs/blob/5692395a4ef0776a08c10aaaa66406e0408dd5f0/src/number.rs),
and [operators](https://github.com/Chia-Network/clvm_rs/blob/5692395a4ef0776a08c10aaaa66406e0408dd5f0/src/more_ops.rs).
Do not assume every documented opcode is active under every network context.

“Native Rust route” means an implementation can be expressed and tested in Rust;
it does not mean an ordinary dependency is admitted by this compiler. Current
support is grounded in the [frontend subset](../crates/chialisp-oxide-driver/README.md),
[IR validation/lowering](../crates/chialisp-oxide-ir/src), and executable tests.

| Feature family | Native Rust route | Current compiler | Evidence still required for broader support |
| --- | --- | --- | --- |
| Atoms, pairs, proper and improper trees; `c`, `f`, `r`, `l` | An explicit atom/pair representation | Borrowed `Tree::Atom`/`Tree::Pair`, pattern matching and checked projections; native arena codec | Broader byte/number operations, recursive processing and per-operator proofs |
| Environment paths, `@`, argument patterns | Explicit tree projections | Exact typed lists by default; ignored nullary exports; explicit v3 raw whole-tree export | More argument patterns and deliberate typed adapters without weakening defaults |
| Quote, `q` | Data constructors | Scalar/product literals and `quote(&Tree)` constructing `(1 . value)` | General program execution/composition and compiler preservation |
| Apply, `a`; dynamic delegated puzzles | An interpreter or an explicit program value | No user-facing dynamic apply; internal backend applications are not a source API | A reviewed program/environment SDK and IR, dialect rules, failures, and cost bounds |
| Signed arbitrary-precision numeric atoms | A big-integer implementation with explicit atom codec | Fixed widths 8–128 only; canonical encodings required | Unbounded numeric SDK/IR semantics, padded/negative atoms, result encoding, and resource bounds |
| `+`, `-`, `*` | Primitive checked arithmetic or explicit big integers | Checked fixed-width operations implemented | Per-operation widening/overflow contract; never silently map Rust overflow to unbounded arithmetic |
| `/`, `divmod`, `%` | Explicit quotient/remainder algorithms | Rejected | CLVM floor rounding versus Rust signed truncation, remainder signs, zero, and signed minimum cases |
| `modpow` | Modular exponentiation | Rejected | Negative operands, exceptional domains, canonical results, and VM costs |
| Atom `=`; numeric `>`; byte `>s` | Separate byte and numeric comparisons | Typed scalar equality/order only | Raw-atom equality versus numeric equality; variable-byte ordering and padding cases |
| Truthiness; `not`, `all`, `any` | Explicit CLVM truth conversion and evaluation order | Canonical Rust `bool`; boolean negation/control flow | Nil versus nonempty atoms/pairs; eager operand effects versus Rust short-circuit operators |
| Lazy `if`; strict `i`; `x` | Branches, evaluated operands, and explicit failure | Rust lazy branches and plain assertions supported | General strict-conditional and raw failure-value semantics; discarded failures must remain observable |
| Byte `concat`, `strlen`, `substr` | Byte vectors/slices with specified bounds | Fixed bytes and static projections; no general byte operations | Dynamic lengths, invalid slices, allocation bounds, and byte-exact results |
| `logand`, `logior`, `logxor`, `lognot`, `ash`, `lsh` | Explicit signed-byte integer operations | Rejected | Unbounded sign extension, logical versus arithmetic shifts, shift limits, and costs |
| `sha256`, tree hashing | A reviewed hash library and tree encoding | Hash constants can be carried; computing hashes on-chain is unsupported | Exact concatenation/domain separation, variable inputs, vectors, and hash assumptions |
| `keccak256`, `coinid` | Reviewed algorithms with exact wire formats | Unsupported | Activation context, amount/identifier validation, padding, exception behavior, and cost vectors |
| BLS exponent/public-key and G1/G2 operations | Curve libraries with specified encodings | No on-chain crypto arithmetic/verification lowering | Point validation, infinity, subgroup rules, scalars, mapping DSTs, exceptional inputs, and costs |
| BLS pairing/signature verification | Reviewed native verification | Signature conditions exist in the small SDK; this is not a lowered BLS opcode | Separate opcode implementation/proof obligations from consensus verification of emitted conditions |
| secp256k1/secp256r1 verification | Reviewed curve verification | Unsupported | Exact key/signature/message encodings, failure behavior, activation, and cost |
| `softfork`, extension guards, unknown operators | The corresponding interpreter/dialect behavior | No explicit source support | Exact extension and cost semantics; no assumption that unknown operators are harmless |
| `mod`, `defun`, inline functions, constants | Cargo modules, functions, constants | Annotated free functions, nongeneric local/SDK helpers, scalar/fixed-byte constants | General aggregate constants, external-call semantics, and preserved module boundaries where observable |
| `include`, macros, quasiquote/unquote | Cargo dependencies and Rust compile-time expansion | Cargo expansion remains real Rust; marker macro exports functions | Reproducible input capture and semantics of generated code; not automatic Chialisp macro compatibility |
| Recursion, recursive lists, iteration | Ordinary Rust recursion/collections | Recursive borrowed tree data accepted; recursive calls, loops and allocation rejected | Runtime function/call IR or justified loop lowering, termination/resource policy, and strictness tests |
| Lambdas, closures, function values | Rust functions/closures | Reachable generic/closure/indirect calls rejected | Captured environment representation, indirect calls, trait/generic resolution, and function identity |
| Currying, tree hashes, singleton/inner-puzzle composition | Explicit program trees and curry/tree-hash algorithms | No general curry or nested-puzzle API | Exact canonical module and instance identity plus adversarial composition tests |
| Conditions and wallet protocols | Typed condition builders and transaction validation | Four named SDK constructors and selected complete-spend scenarios | Broader conditions/types, cross-coin invariants, replay/lineage/authorization checks, and consensus vectors |
| Costs and consensus acceptance | Pinned VM plus consensus validator | Runtime costs and selected ledger scenarios measured | Whole-spend costs, activation rules, resource-equivalence criteria, and realistic maximum inputs |
| Host I/O, clocks, randomness, network state | Native Rust can perform them | Not an implicit puzzle capability | Supply explicit committed inputs or return consensus conditions; no hidden nondeterministic dependency |

## Explicit pinned VM operator audit

This audit covers `clvmr 0.21.0`, used by the differential execution backend.
The locked crate's `chia_dialect.rs`, `run_program.rs`, `more_ops.rs`,
`number.rs`, `op_utils.rs`, and `bls_ops.rs` were compared by Git blob ID with
the cataloged `clvm_rs@5692395a4ef0776a08c10aaaa66406e0408dd5f0` files; all six
match. The dispatch in `chia_dialect.rs` and interpreter handling in
[run_program.rs](https://github.com/Chia-Network/clvm_rs/blob/5692395a4ef0776a08c10aaaa66406e0408dd5f0/src/run_program.rs)
define the audited surface, rather than an assumption about future operators.

These are runtime opcode names. Source-language aliases are a separate
compiler concern: the pinned Chialisp
[primitive map](https://github.com/Chia-Network/chialisp/blob/3b264eeef50af4ea5ad18ed655dee9d01c70e068/src/compiler/prims.rs)
uses `point_add` and the four-byte secp encodings; documentation calls opcode 29
`g1_add`. A documented name does not by itself prove a spelling is accepted by
every compiler version.

| Numeric opcode(s) | Names, in opcode order | Rust authoring coverage today |
| --- | --- | --- |
| 1, 2, 36 | `q`, `a`, `softfork` | Literals and raw-tree `quote`; no user-facing dynamic apply or softfork API |
| 3–10 | `i`, `c`, `f`, `r`, `l`, `x`, `=`, `>s` | Typed products, raw tree construction/projections/pair tests, lazy branches, assertions and scalar comparisons |
| 11–14 | `sha256`, `substr`, `strlen`, `concat` | No general source lowering; decoder-generated byte checks do not count as an authoring API |
| 16–18 | `+`, `-`, `*` | Checked fixed-width arithmetic only |
| 19–27 | `/`, `divmod`, `>`, `ash`, `lsh`, `logand`, `logior`, `logxor`, `lognot` | Typed numeric comparisons only; remaining operations rejected |
| 29, 30 | `point_add` (documentation: `g1_add`), `pubkey_for_exp` | Missing crypto-op lowering |
| 32–34 | `not`, `any`, `all` | `not` on Rust `bool`; no explicit strict variadic `any`/`all` or general truthiness API |
| 48–51 | `coinid`, `g1_subtract`, `g1_multiply`, `g1_negate` | Missing |
| 52–55 | `g2_add`, `g2_subtract`, `g2_multiply`, `g2_negate` | Missing |
| 56–59 | `g1_map`, `g2_map`, `bls_pairing_identity`, `bls_verify` | Missing |
| 60, 61 | `modpow`, `%` (`op_mod` implementation) | Missing; opcode 60 additionally has a disable/cost-model gate |
| 62, 63 | `keccak256`, `sha256tree` | Missing and flag/extension dependent |
| 64, 65 | `secp256k1_verify`, `secp256r1_verify` | Missing and gated by `ENABLE_SECP_OPS` |
| `0x13d61f00`, `0x1c3a8f00` | Four-byte `secp256k1_verify`, `secp256r1_verify` encodings | VM dispatch accepts these separately; no source lowering |
| Other/reserved operator bytes | Unknown-operator dispatch, defined costs or reserved failures | No source API; rejected by the execution backend's `NO_UNKNOWN_OPS` policy |

Unquoted program atoms also traverse environment paths; that interpreter
behavior is not a missing named opcode in the table. Raw-tree pattern matching
now provides checked first/rest access; general numeric path traversal is not a
separate SDK operation.

The pin's flags are `CANONICAL_INTS`, `NO_UNKNOWN_OPS`, `LIMIT_HEAP`,
`RELAXED_BLS`, `LIMIT_SOFTFORK`, `ENABLE_GC`, `LIMITS`,
`ENABLE_KECCAK_OPS_OUTSIDE_GUARD`, `DISABLE_OP`, `ENABLE_SHA256_TREE`,
`ENABLE_SECP_OPS`, `MALACHITE`, and `NEW_COST_MODEL`. Review their actual operand
checks; a flag name is not a proof that every integer operation rejects padding.

- The backend currently uses the pin's `MEMPOOL_MODE`: `NO_UNKNOWN_OPS`,
  `LIMIT_HEAP`, `DISABLE_OP`, `CANONICAL_INTS`, and `LIMIT_SOFTFORK`.
- `DISABLE_OP` blocks `modpow` unless `NEW_COST_MODEL` is also selected.
- Keccak is enabled inside the recognized extension guard or with its outside-
  guard flag. `sha256tree` and one-byte secp opcodes require their explicit flags.
- `RELAXED_BLS` changes some point-validation behavior. `NEW_COST_MODEL`
  changes costs and guard behavior. Neither can be silently assumed.
- `LIMITS` and `NEW_COST_MODEL` are mutually exclusive in this dialect;
  allocator/GC and big-integer implementation switches also need regression
  coverage when changing the execution configuration.

The complete-spend simulator separately pins `chia-consensus 0.36.1` and
`clvmr 0.16.2`. Its successful scenarios are evidence for that configuration;
they do not establish availability of every operator in the newer differential
VM. Actual network activation and any future opcode outside these pins require
a new explicit review, not a claim of automatic support.

## Source-only and modern Chialisp constructs

Runtime opcode coverage does not cover all source constructs. The pinned
[frontend](https://github.com/Chia-Network/chialisp/blob/3b264eeef50af4ea5ad18ed655dee9d01c70e068/src/compiler/frontend.rs)
and [preprocessor](https://github.com/Chia-Network/chialisp/blob/3b264eeef50af4ea5ad18ed655dee9d01c70e068/src/compiler/preprocessor/mod.rs)
also require distinct treatment:

- `mod`, destructured/dotted argument patterns, named helpers, `defun`,
  `defun-inline`, recursion, and function/lambda environments: Rust has
  functions and pattern matching. Borrowed recursive tree data is supported;
  recursive calls and runtime function values remain unsupported.
- `defconstant`, `defconst`, `let`, `let*`, `assign`, `assign-inline`, and
  `assign-lambda`: preserve binding, evaluation, staging, and failure order.
  Similar Rust syntax does not establish matching semantics.
- `defmacro`, preprocessor `defmac`, `qq`, `unquote`, and library macros such as
  `if` and `list`: these expand before runtime. Rust expansion can express an
  authoring route, but generated output must still be checked real Rust; the
  export annotation is not a Chialisp macro interpreter.
- `include`, modern `import`/`export`, `defalias`, namespaces, `embed-file`, and
  `compile-file`: these need reproducible source/dependency and embedding
  semantics. Cargo supports host expansion, but embedding compiled CLVM bytes
  must not be reported as translation of equivalent Rust puzzle logic.
- Dialect/strict-mode includes, atom/string/hex/integer literals, compiler
  optimizations, and library-defined constructs: specify the exact compiler
  version and expansions being compared. User-defined macros make the set of
  surface spellings open-ended; no fixed keyword list proves every library.

## Conditions are consensus data, not VM operators

The current SDK names `CREATE_COIN`, `AGG_SIG_ME`, `ASSERT_MY_AMOUNT`, and
`ASSERT_SECONDS_ABSOLUTE`. Other condition lists can sometimes be represented
as supported product data, but that is different from a reviewed typed builder,
complete spend validation, or an invariant proof.

The pinned consensus condition surface includes the `AGG_SIG_PARENT`,
`AGG_SIG_PUZZLE`, `AGG_SIG_AMOUNT`, `AGG_SIG_PUZZLE_AMOUNT`,
`AGG_SIG_PARENT_AMOUNT`, `AGG_SIG_PARENT_PUZZLE`, `AGG_SIG_UNSAFE`, and
`AGG_SIG_ME` family; `CREATE_COIN` and `RESERVE_FEE`; coin/puzzle announcement
creation/assertion; concurrent spend/puzzle assertions; `SEND_MESSAGE` and
`RECEIVE_MESSAGE`; coin/parent/puzzle-hash/amount/birth introspection and
`ASSERT_EPHEMERAL`; relative/absolute seconds/height and before-time assertions;
`REMARK`; and `SOFTFORK` condition extensions. Unknown/two-byte condition
encodings and their costs also belong to this consensus contract. The
[0.36.1 opcode/parser source](https://github.com/Chia-Network/chia_rs/blob/56f6b676a96de8659fe05d7e4f705df553f37fa5/crates/chia-consensus/src/opcodes.rs)
is separately pinned by the crate's VCS metadata; it is not the same revision
as the catalog's current `chia_rs` tree. The intended activation context must
also be reviewed. A condition named
`SOFTFORK` is distinct from VM operator 36. Constructing its tuple does not
establish extension or cost correctness.

## Concrete next compiler and corpus work

The smallest input-dependent tutorial step is the exact
[module-add reference](https://github.com/Chia-Network/chialisp-web/blob/dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8/docs/chialisp-primer/using-modules.md#L21-L23)
paired with a clearly labeled **bounded strict-ABI Rust adapter**. Ordinary
`fn puzzle(first: i64, second: i64) -> i64 { first + second }` already fits the
compiler. Its exact proper-list ABI and checked overflow differ from the
original. Test and record those differences instead of weakening default checks
or editing the original source to hide them.

The first raw-tree witness is the exact canonical `p2_conditions` factory.
It uses ordinary borrowed Rust enum matching and the SDK `quote` helper, with
an explicit whole-environment ABI. Its report checks the factory result, applies
the returned program, and separately compares published module identity.
The structural tree model proves its stated IR specification, not compiler
preservation. Broader numeric compatibility still needs native arbitrary-precision
semantics, reviewed SDK identity, and explicit target operations. A native Rust builder that embeds
precompiled CLVM, or an opaque extern intrinsic with no checked semantics, does
not establish that Rust puzzle logic was translated correctly. General recursion
and delegated-program execution remain additional work.

In parallel, conservative constant folding and sharing can reduce large closed
programs. Each optimization must preserve failures, overflow, and laziness;
compare optimization on/off and mutation controls before making cost claims.

Broader coverage should select concrete published puzzles from this catalog,
retain original source and hashes, enumerate required features, and attach
executed witnesses and proof obligations. Keep native expressibility,
implemented compiler support, protocol interoperability, and formal assurance
as independently reviewable results.
