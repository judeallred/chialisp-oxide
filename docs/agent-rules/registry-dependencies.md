# Registry dependencies

- Keep registry commands offline and dispatched before compiler or wrapper
  inspection. Building source and executing a retrieved artifact are separate
  operations; a host VM consumer is not typed on-chain dependency support.
- Pin exact envelope identity and the complete verified artifact identity.
  A matching tree hash can have multiple envelope variants; require explicit
  selection when ambiguous. Do not infer nominal bindings from names or erased
  structural ABI. Version-1 dependency pins require explicit `binding: null`.
- Resolve all pins with one complete bounded store scan, including unrelated
  corrupt objects. Retain each admitted package once and index aliases into
  that collection. Preserve the caller's registry limits without default-limit
  re-admission or per-alias byte/work amplification.
- Require an explicit consumer lock path. Protect Cargo manifests/lockfiles,
  registry descendants and nonregular/symlink/hardlink aliases. On Unix compare
  directory identities for containment; canonical path spelling alone does not
  close case aliases on macOS. Reserve Cargo basenames under Unicode lowercase
  comparison too: `Cargo.locK` aliases `Cargo.lock` on APFS. Preserve OS paths;
  do not lowercase the path used for file operations. Fail unsupported writer
  platforms before mutation.
- Serialize cooperating writers with a nonblocking empty regular sidecar;
  its inode cannot also be valid dependency content. Compare exact bytes or
  absence under that lock, re-admit a bounded complete stage and commit
  atomically. Preserve identical existing pins' original JSON bytes.
- Distinguish precommit preservation from postcommit I/O errors and orphan
  cleanup. Do not claim hostile-directory or power-loss safety. Run package
  tests, actual offline CLI controls and `scripts/check-registry-dependencies.py`
  with complete source-produced bundles and the prebuilt Rust consumer.
- Keep issue #21 open until its actual mainnet lookup/uncurry, reviewed typed
  bindings and dependency authoring requirements are fulfilled and verified.
- Generate local Rust only from actual admitted artifacts after complete exact
  lock resolution. Keep the generated helpers on the ordinary Rust/MIR path:
  names, comments, package metadata and generated status grant no primitive or
  nominal ABI. Preserve whole-tree environments and exact atom bytes; reject
  atoms beyond the current 256-byte source route rather than splitting them.
  Generation bounds do not prove acceptance by either compiler mode. Require
  native checks, actual paired builds, source receipts and typed VM comparisons.
  Keep new-or-identical source publication separate from lock replacement;
  `--check` must remain read-only and detect edited generated code.
- Reveal inspection verifies the full reveal hash before secondary module
  lookup. Preserve exact curry syntax/argument bytes, unknown-module opacity
  and explicit schema-invalid statuses. A Coinset observation is not chain
  authentication. The reviewed standard host binding uses retrieved module and
  inner bytes, an actual parsed synthetic-key role and the pinned SDK schema;
  it supplies no inferred inner ABI, signature or newly supported frontend call.
