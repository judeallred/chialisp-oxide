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

Issue #25 also requests discovery trials from fresh clones in **two actual agent
tools**. Deterministic file/link tests do not satisfy that manual acceptance
criterion. Record the tool names, versions, prompt, discovered entry points and
result after those trials; keep the issue open until they are complete. An
unavailable second tool is a limitation, not a passing trial. The facade's Rust
crate documentation has not been changed by this documentation increment.
