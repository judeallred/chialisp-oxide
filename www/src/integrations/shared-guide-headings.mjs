import assert from 'node:assert/strict';
import { realpathSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const titles = new Map([
  [realpathSync(new URL('../../../docs/package-registry.md', import.meta.url)), 'Offline package registry'],
  [realpathSync(new URL('../../../docs/lifecycle.md', import.meta.url)), 'Local coin puzzle lifecycle'],
  [realpathSync(new URL('../../../docs/cat-genesis-tail.md', import.meta.url)), 'Genesis-by-coin-ID CAT TAIL: local Rust lifecycle'],
  [realpathSync(new URL('../../../docs/action-counter.md', import.meta.url)), 'Counter actions: state across singleton spends'],
  [realpathSync(new URL('../../../docs/action-registry.md', import.meta.url)), 'Registry actions and separately consumed slots'],
  [realpathSync(new URL('../../../docs/reveal-inspection.md', import.meta.url)), 'Inspect reveals and bind standard modules'],
  [realpathSync(new URL('../../../docs/roadmap.md', import.meta.url)), 'Current status'],
  [realpathSync(new URL('../../../docs/optional-lists.md', import.meta.url)), 'Optional output lists with standard Rust Option'],
  [realpathSync(new URL('../../../docs/llms.md', import.meta.url)), 'Writing puzzles with chialisp-oxide'],
]);

// Imported repository guides retain their own title in GitHub. Starlight
// supplies the page h1, so remove only these known leading Markdown titles.
function leadingGuideHeading({ fileURL }) {
  if (fileURL?.protocol !== 'file:') return false;
  let path;
  try { path = realpathSync(fileURLToPath(fileURL)); }
  catch { return false; } // Unrelated virtual Markdown inputs are untouched.
  const title = titles.get(path);
  if (!title) return false;
  return {
    name: 'shared-guide-leading-heading',
    before(root, context) {
      const heading = root.children[0];
      assert(heading?.type === 'heading' && heading.depth === 1
        && context.textContent(heading) === title,
      `${path}: expected the leading h1 "${title}" supplied by the shared guide`);
      context.removeChildAt(root, 0);
    },
  };
}

export default function sharedGuideHeadings() {
  return {
    name: 'shared-guide-headings',
    hooks: {
      'astro:config:setup'({ config }) {
        // Astro 7 and Starlight use Sätteri's MDAST hooks. Extend the existing
        // processor rather than switching Markdown engines or dependencies.
        const processor = config.markdown.processor;
        assert(processor.name === 'satteri' && Array.isArray(processor.options.mdastPlugins),
          'Shared guide headings require the configured Sätteri MDAST processor');
        const plugins = processor.options.mdastPlugins;
        if (!plugins.includes(leadingGuideHeading)) plugins.push(leadingGuideHeading);
      },
    },
  };
}
