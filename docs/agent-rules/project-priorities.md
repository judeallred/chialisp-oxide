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
  the issue is closed. The next tooling slice is #21's offline package registry,
  with source-aware Cargo receipts and exact local dependency locks, followed by
  observed mainnet inspection, pinned bindings and #12's broader lifecycle;
  keep full issue acceptance and the later stateful-puzzle tracks explicit.
  Use issue dependencies to order larger work.
- Keep optimization (#6) and chain/node space studies (#22, #23) active at a lower
  priority. Preserve measured regressions and existing optimization checks.
- Publish coherent, verified increments. Do not close an issue until its actual
  acceptance criteria are met, including live deployment or external-tool checks
  when requested. A plan or source catalog is not an implemented puzzle.
- Puzzle authors should start at `docs/llms.md`; compiler contributors also follow
  `AGENTS.md` and the relevant semantic rules.
