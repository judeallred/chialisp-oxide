# Generated agent documentation

The authoring entry point is [llms.md](llms.md); compiler contributors also read
[AGENTS.md](https://github.com/judeallred/chialisp-oxide/blob/main/AGENTS.md).

The site publishes `llms.txt`, `llms-full.txt`, per-page `index.md` alternatives,
a Markdown sitemap and a hash inventory under its `/chialisp-oxide/` base.
These are generated after Astro renders the pages. They include resolved Astro
components, three-language sources and recorded validation tables rather than
unexpanded MDX imports. HTML remains the destination for internal fragment
links; converting heading IDs to Markdown anchors could break those links.

## Integration choice

We evaluated [starlight-llms-txt 0.12.0](https://github.com/delucis/starlight-llms-txt)
against a small build integration. The plugin supports rendered content and
curated sets and is compatible with our pinned Astro/Starlight versions. The
local integration uses the same standard unified HTML-to-Markdown approach,
but consumes **all built pages**, including the generated laboratory, coverage
and published-source routes, and emits individual Markdown alternatives with
an exact HTML/Markdown hash inventory. It also avoids a second route-rendering
pass and keeps evidence-page inclusion explicit in our checks. No plugin code
is vendored. The chosen dependencies are exact versions in `www/package.json`
and `www/pnpm-lock.yaml`.

[llms.txt](https://llmstxt.org/) is a discovery convention, not a guarantee that
every agent tool will read it. The small index leads with current supported
authoring and validation. The full text is deliberately optional and large;
curated pages are usually the better starting context. Manifest hashes check
build consistency, not execution authenticity or source safety.

## Gates and remaining acceptance

`pnpm --dir www check` verifies the repository guide's Rust example against the
actual corpus source and checks the standard discovery entry points.
`pnpm --dir www test:site` checks generated-file freshness, component/source
preservation, discovery links, sitemap coverage and validation witness displays,
alongside the existing real Pagefind search and evidence checks.

Issue #25 requests discovery trials from fresh clones in **two actual agent
tools**. Deterministic file/link tests do not satisfy that manual acceptance
criterion. One actual Codex trial is recorded below; a second genuine tool trial
is still outstanding, so **#25 remains open**. An unavailable second tool is a
limitation, not a passing trial. The facade's Rust crate documentation has not
been changed by this documentation increment.

## Actual Codex discovery trial

On October 20, 2026, installed **Codex CLI 0.162.0-alpha.17.2** ran a new ephemeral,
read-only session against a fresh local clone of published commit
[`db73a30d0626b0350132b1a799249072ff1b3f18`](https://github.com/judeallred/chialisp-oxide/commit/db73a30d0626b0350132b1a799249072ff1b3f18).
The session did not resume or inherit a previous chat. The exact task prompt,
with no authoring-guide filename supplied, was:

> I want to write and test a Chia puzzle in this repository. Find the authoring workflow and explain where to start, what crates/commands to use, and the current safety limitations. Cite files you actually read.

The CLI completed with exit status 0. Its four recorded shell commands only
read files with `cat` and `rg`. It independently found `docs/llms.md`, read the
real Cargo consumer's manifest and Rust source, the facade manifest, the pinned
toolchain, quickstart, CLI reference and supported-subset page. Its answer cited
files it actually read and explained:

- The unpublished `chialisp-oxide` facade and `#[chialisp_oxide::puzzle]` exports
  in ordinary Cargo libraries.
- Native `cargo check/test`, separate CLVM subset checks, both optimization modes
  and immutable bundle selection.
- Malformed-input testing, simulator obligations and the limits of VM execution.
- WIP status, public fixture keys and the distinction between tests, models,
  exhaustive security and compiler preservation.

The before/after audit found all **1,437 tracked source-file hashes unchanged**,
a clean clone and the same pinned commit. No Cargo, compiler or test commands
were executed, and no puzzle was authored. Raw prompt, tool/version output,
events, answer, exit status and audit receipts are retained locally in the
ignored `target/agent-discovery-20261020/` directory; they are not corpus
execution evidence. An initial sandbox-blocked CLI launch produced no model
events and receives no discovery credit.

The trial exposed a setup omission: its suggested `chialisp-oxide run` command
needed `chialisp-oxide-cli`, while the copied setup built only the Cargo
subcommand and driver. The authoring guide, README, quickstart and CLI reference
now build all three packages. The recorded trial remains a discovery observation
at `db73a30`, not a rerun of that correction or an end-to-end authoring test.
