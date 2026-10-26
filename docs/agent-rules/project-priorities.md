# Project priorities

- Follow the owner's October 20 direction: correctness, readable Rust authoring,
  validated puzzle coverage, security testing, and documentation take priority
  over further compiler cost optimization.
- Review the open GitHub issues alongside `docs/roadmap.md` before selecting the
  next milestone. Record dependencies and incomplete acceptance criteria honestly.
- Validation guides (#24), Chialisp syntax highlighting (#8) and agent
  authoring/discovery (#25) have shipped. Actual Codex and Claude Code
  fresh-clone trials are recorded in `docs/agent-documentation.md`.
- The original boomerang puzzle (#9) shipped with five-build committed-instance
  and separate source-mutation evidence, exact-commit CI, Pages and live checks;
  the issue is closed. #21 now includes source-aware Cargo receipts, exact local
  locks, observed mainnet inspection, reviewed standard/p2-puzzle-hash host
  bindings and a normal Cargo dependency-by-hash spend example. Broader nominal
  dependency support and registry integration remain open. Continue with #12's
  shared CLI/GUI lifecycle and real testnet evidence, then the later stateful
  puzzle tracks; keep full issue acceptance explicit. Use issue dependencies to
  order larger work.
- The local #12 companion shares fresh validated recipes, simulator journals and
  commands between CLI and GUI; see `docs/lifecycle.md`. Its read-only HTTPS
  observer remains separate from those adapters. Live funding, submission,
  monitoring and Testnet11 acceptance remain open; local fixtures need no wallet.
- Keep #14's counter and fixed two-name registry demonstrations separate from
  canonical protocol ports. The registry adds actual SLOT creator/current-spender
  bindings, isolated consumed-slot rejection and own-root Rust omission controls;
  see `docs/action-registry.md`. Count its base, native, template and mutation SDK
  lanes separately. Review exact-commit verification and publication before issue
  completion, then continue with #15 XCHandles and #16 authenticated continuations.
- Keep optimization (#6) and chain/node space studies (#22, #23) active at a lower
  priority. Preserve measured regressions and existing optimization checks.
- Publish coherent, verified increments. Do not close an issue until its actual
  acceptance criteria are met, including live deployment or external-tool checks
  when requested. A plan or source catalog is not an implemented puzzle.
- Puzzle authors should start at `docs/llms.md`; compiler contributors also follow
  `AGENTS.md` and the relevant semantic rules.
