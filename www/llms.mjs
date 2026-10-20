import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { unified } from 'unified';
import rehypeParse from 'rehype-parse';
import rehypeRemark from 'rehype-remark';
import remarkGfm from 'remark-gfm';
import remarkStringify from 'remark-stringify';

export const siteURL = 'https://judeallred.github.io/chialisp-oxide/';
export const warning = 'WIP prototype. Not ready for use. Do not use for real funds or production puzzles.';
export const curated = [
  ['reference/agent-guide/', 'Puzzle authoring for agents', 'Cargo workflow, typed policy and testing.'],
  ['start/quickstart/', 'Cargo quickstart', 'Build the actual unpublished consumer library.'],
  ['reference/supported-rust/', 'Supported Rust', 'Implemented constructs and explicit rejections.'],
  ['reference/abi/', 'Puzzle ABI', 'Exact typed, raw and ignored environment contracts.'],
  ['reference/conditions/', 'Conditions', 'Named constructors and authorization boundaries.'],
  ['validation/inputs/', 'Input validation', 'Untrusted trees, commitments and actual witnesses.'],
  ['validation/outputs/', 'Output validation', 'Signatures, fees, delegation and composition.'],
  ['validation/review/', 'Review and testing', 'Adversarial vectors, mutations and simulator negatives.'],
  ['tutorials/signatures/', 'Signature tutorial', 'A real export with fixed owner and recipient.'],
  ['reference/cli/', 'Compiler commands', 'Subset checking and immutable artifact bundles.'],
  ['reference/chialisp-functionality/', 'Functionality and gaps', 'Implemented features versus outstanding protocol work.'],
  ['verification/method/', 'Evidence scope', 'Tests, model checks and their limitations.'],
];
export const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const parser = unified().use(rehypeParse);
const converter = unified().use(rehypeRemark).use(remarkGfm)
  .use(remarkStringify, { fences: true, bullet: '-', listItemIndent: 'one' });
const text = node => node.type === 'text' ? node.value : (node.children ?? []).map(text).join('');
const classes = node => node.properties?.className ?? [];
function find(node, predicate) {
  if (predicate(node)) return node;
  for (const child of node.children ?? []) { const match = find(child, predicate); if (match) return match; }
}

// Markdown follows the built HTML, so Astro components and evidence tables have
// already resolved. Never expose raw MDX imports or discard component content.
export async function renderMarkdown(html, pageURL) {
  const ast = parser.parse(html);
  const main = find(ast, node => node.tagName === 'main');
  assert(main, `${pageURL}: missing document main`);
  const heading = find(main, node => node.tagName === 'h1');
  const content = find(main, node => classes(node).includes('sl-markdown-content'));
  assert(heading && content, `${pageURL}: missing title/content`);
  const title = text(heading).trim();
  assert(title, `${pageURL}: empty title`);
  const excluded = new Set(['script', 'style', 'svg', 'button', 'nav', 'footer', 'template']);
  function clean(node) {
    if (excluded.has(node.tagName) || node.properties?.ariaHidden === 'true'
      || node.properties?.ariaHidden === true || classes(node).includes('sr-only')) return null;
    if (node.tagName === 'a' && node.properties?.href) {
      const href = String(node.properties.href);
      // Preserve HTML destinations/fragments. Markdown alternatives are explicit
      // discovery links; converting fragment IDs could silently break them.
      if (!/^(?:mailto:|tel:)/.test(href)) {
        const target = new URL(href, pageURL);
        assert(['http:', 'https:'].includes(target.protocol), `${pageURL}: unsafe content link`);
        node.properties.href = target.href;
      }
    }
    if (node.tagName === 'pre') {
      const code = find(node, child => child.tagName === 'code');
      assert(code, `${pageURL}: code block without code`);
      const lines = code.children?.filter(child => classes(child).includes('ec-line')) ?? [];
      const value = lines.length ? lines.map(line => text(line).replace(/\n$/, '')).join('\n') : text(code).replace(/\n$/, '');
      // Expressive Code line wrappers carry syntax spans and copy UI. Retain
      // exact line text, including indentation and blank lines.
      code.children = [{ type: 'text', value: `${value}\n` }];
      const language = node.properties?.dataLanguage;
      if (typeof language === 'string') code.properties = { ...code.properties, className: [`language-${language}`] };
      node.children = [code];
      return node;
    }
    if (node.children) node.children = node.children.map(clean).filter(Boolean);
    return node;
  }
  const root = { type: 'root', children: [structuredClone(heading), structuredClone(content)].map(clean) };
  const markdown = converter.stringify(converter.runSync(root));
  return { title, markdown: `> ${warning}\n\nSource: ${pageURL}\n\n${markdown}` };
}

async function htmlFiles(directory, prefix = '') {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const name = path.posix.join(prefix, entry.name);
    if (entry.isDirectory()) result.push(...await htmlFiles(path.join(directory, entry.name), name));
    else if (entry.isFile() && entry.name === 'index.html') result.push(name);
  }
  return result.sort();
}

export async function generateMarkdown(directory) {
  const files = await htmlFiles(directory);
  assert(files.length > 0, 'No rendered documentation pages');
  const pages = [];
  for (const htmlPath of files) {
    const route = htmlPath.replace(/index\.html$/, '');
    const html = await readFile(path.join(directory, htmlPath), 'utf8');
    const pageURL = new URL(route, siteURL).href;
    const { title, markdown } = await renderMarkdown(html, pageURL);
    const markdownPath = htmlPath.replace(/\.html$/, '.md');
    pages.push({ route, title, htmlPath, markdownPath, htmlSHA256: sha256(html),
      markdownSHA256: sha256(markdown), markdownBytes: Buffer.byteLength(markdown), markdown });
  }
  const byRoute = new Map(pages.map(page => [page.route, page]));
  for (const [route] of curated) assert(byRoute.has(route), `Missing curated page ${route}`);
  const links = curated.map(([route, title, description]) =>
    `- [${title}](${new URL(byRoute.get(route).markdownPath, siteURL)}): ${description}`).join('\n');
  const index = `# chialisp-oxide\n\n> ${warning}\n\nCompile a supported subset of real Rust into CLVM through a Cargo-driven MIR frontend and Rue backend. All project crates are unpublished. Native Rust checks, CLVM subset admission, runtime decoding, simulator tests and model proofs establish different claims.\n\n## Write and validate puzzles\n\n${links}\n\n## Optional\n\n- [All rendered documentation](${siteURL}llms-full.txt): ${pages.length} pages, including actual component-rendered source comparisons and recorded evidence; large context.\n- [Markdown inventory](${siteURL}llms-manifest.json): Per-page source and Markdown hashes for build freshness, not authentication.\n- [Markdown sitemap](${siteURL}llms-sitemap.xml): All Markdown page URLs.\n- [Repository authoring guide](https://github.com/judeallred/chialisp-oxide/blob/main/docs/llms.md): Start here when working from a checkout.\n`;
  const order = [...curated.map(([route]) => byRoute.get(route)), ...pages.filter(page => !curated.some(([route]) => route === page.route))];
  const full = `# chialisp-oxide: rendered documentation\n\n> ${warning}\n\n${pages.length} current pages. This is a generated reading format, not an independently verified security report. Exact report downloads and source links remain authoritative for bytes and provenance.\n\n${order.map(page => `---\n\n${page.markdown}`).join('\n')}`;
  const escape = value => value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('"', '&quot;');
  const urls = ['llms.txt', 'llms-full.txt', ...pages.map(page => page.markdownPath)];
  const sitemap = `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${urls.map(file => `<url><loc>${escape(new URL(file, siteURL).href)}</loc></url>`).join('')}</urlset>\n`;
  const manifest = { schema_version: 1, protocol: 'chialisp-oxide-rendered-markdown/v1', site: siteURL,
    scope: 'Build freshness of rendered reading formats, not execution authentication or compiler preservation.',
    pages: pages.map(({ markdown, ...page }) => page), llmsSHA256: sha256(index), fullSHA256: sha256(full), sitemapSHA256: sha256(sitemap) };
  return { pages, index, full, sitemap, manifest };
}

export default function markdownDocumentation() {
  return { name: 'chialisp-oxide:markdown-docs', hooks: {
    'astro:build:done': async ({ dir, logger }) => {
      const directory = fileURLToPath(dir);
      const output = await generateMarkdown(directory);
      for (const page of output.pages) await writeFile(path.join(directory, page.markdownPath), page.markdown);
      for (const [name, value] of [['llms.txt', output.index], ['llms-full.txt', output.full], ['llms-sitemap.xml', output.sitemap],
        ['llms-manifest.json', JSON.stringify(output.manifest, null, 2) + '\n']]) await writeFile(path.join(directory, name), value);
      logger.info(`Generated ${output.pages.length} Markdown pages, curated index, full text and sitemap from rendered HTML.`);
    },
  } };
}
