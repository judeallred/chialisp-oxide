# Chialisp Oxide documentation

Astro 7 + Starlight, with pnpm and Pagefind static full-text search. The site
uses a custom green/oxide editorial theme, accessible framework navigation,
code-copy controls, responsive Rust/Chialisp source comparisons, a filterable
puzzle laboratory, formal evidence, and a separately filterable migration
inventory.
Authoring guides lead with the real unpublished Cargo SDK and annotated exports.
Their consumer examples are imported directly from `../examples/cargo-puzzles`;
the laboratory reports annotated exports of the real `chialisp-oxide-puzzles` Cargo
library. Validation and the native oracle use that same library and SDK. Full recorded
dependency sources, including public fixture-key bytes, remain available in
expandable drawers. Comparisons stack at narrow content widths; wrapped code
keeps its original copied line breaks.
The laboratory also compares serialized bytes between optimization modes and
against Chialisp, with exact replacement offsets and explicit hash-compatibility
scope; byte similarity is not presented as a semantic proof.

```sh
pnpm install --frozen-lockfile
pnpm check
pnpm build
pnpm test:site
pnpm preview
```

Requires Node 24 or later. Run from `www/`. Build data comes from the actual
checked-in `../corpus/validation.json`, `../corpus/proofs.json`,
`../corpus/inventory.json`, and `../corpus/tutorial-examples.json`. The site refuses a validation report with no puzzles
or unvalidated examples. Regenerate compiler evidence from the repository root
before publishing changed puzzles; never edit displayed metrics by hand.
Both `check` and `build` compare report source strings, vectors, policy scope,
and provenance to the local fixtures, check backend agreement and ledger
results, and verify the formal report's source, IR, mutant IR, specifications,
and SMT query hashes. Stale evidence fails
the site build rather than silently displaying obsolete results.
Explicitly declared source dependencies are checked by path, content, and
SHA-256 as well. Their enumeration comes from the fixture manifest, not an
automatic compiler source-map claim.
Both proof and validation authoring records must match `puzzles/authoring.json`
and the current hashes of every declared local Cargo input, with the counter's
proof export identified explicitly.
The example inventory is displayed separately as unimplemented source research.
Its pinned pages, license, and exact source spans are hashed against the local
snapshot; they do not increase validated-puzzle counts.

The Pages configuration is `site: https://judeallred.github.io` and
`base: /chialisp-oxide`. Keep every internal route and static resource under
that base. `pnpm test:site` checks generated links, fragments, source/evidence
pages, report downloads, and Pagefind output. Full search is available in a
built preview; the development server does not prove search works.

Deployment is configured by the repository-level Pages workflow, with GitHub
Actions selected as the Pages source. This directory also builds as a
standalone static artifact for local review. Keep the documented site/base
settings aligned with the selected repository and hosting configuration.
