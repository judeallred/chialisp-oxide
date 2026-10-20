import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { createHighlighter } from 'shiki';
import { unified } from 'unified';
import rehypeParse from 'rehype-parse';
import { chialispGrammar, chialispThemeRules, palettes } from '../chialisp-highlighting.mjs';

const fixture = `; canonical forms and CLVM operators
(mod (AMOUNT)
  (defun-inline increment (X) (+ X 1))
  (if (> AMOUNT 0)
    (list (list CREATE_COIN 0x42 AMOUNT))
    (x "bad; still a string")))`;
async function checkSourceLanguages(directory) {
  for (const entry of await readdir(directory, {withFileTypes:true})) {
    const file = new URL(entry.name + (entry.isDirectory() ? '/' : ''), directory);
    if (entry.isDirectory()) await checkSourceLanguages(file);
    else if (/\.(?:md|mdx|astro)$/.test(entry.name)) assert(!/```clojure|lang="lisp"/.test(await readFile(file,'utf8')), `${file}: borrowed Chialisp grammar remains`);
  }
}
await checkSourceLanguages(new URL('../src/',import.meta.url));
const highlighter = await createHighlighter({ langs: [chialispGrammar], themes: ['dark', 'light'].map(type => ({
  name: `chialisp-${type}`, type, colors: { 'editor.background': type === 'dark' ? '#23262f' : '#f6f7f9', 'editor.foreground': palettes[type].plain },
  settings: chialispThemeRules(type),
})) });
try {
  for (const type of ['dark', 'light']) {
    const tokens = highlighter.codeToTokens(fixture, {lang:'clsp',theme:`chialisp-${type}`,includeExplanation:true}).tokens;
    assert.equal(tokens.map(row => row.map(token => token.content).join('')).join('\n'), fixture, 'Tokenization changed source');
    assert.deepEqual(highlighter.codeToTokens(fixture, {lang:'chialisp',theme:`chialisp-${type}`}).tokens,
      highlighter.codeToTokens(fixture, {lang:'clsp',theme:`chialisp-${type}`}).tokens, 'Language alias differs');
    for (const [spelling, category] of [['mod','keyword'], ['defun-inline','keyword'], ['increment','function'],
      ['+','builtin'], ['if','keyword'], ['>','builtin'], ['list','list'], ['CREATE_COIN','literal'], ['0x42','literal'], ['0','literal']]) {
      const token = tokens.flat().find(token => token.content === spelling);
      assert(token, `Missing ${spelling} token`);
      assert.equal(token.color.toLowerCase(), palettes[type][category], `${type}/${spelling}: wrong token category/color`);
    }
    const string = tokens.flat().find(token => token.content === '"bad; still a string"');
    assert(string?.explanation[0].scopes.some(scope => scope.scopeName === 'string.quoted.double.chialisp'));
    assert.equal(string.color.toLowerCase(), palettes[type].literal);
    const comment = tokens.flat().find(token => token.content.startsWith('; canonical'));
    assert.equal(comment.color.toLowerCase(), palettes[type].comment);
  }
  for (const sample of ['(sha256tree X)', '(my-sha256 X)', '(0x42suffix)', '(+ -1 0x0001)', '(q . (1 . 2))']) {
    const tokens = highlighter.codeToTokens(sample, {lang:'clsp',theme:'chialisp-dark',includeExplanation:true}).tokens.flat();
    assert.equal(tokens.map(token => token.content).join(''), sample);
    if (sample.includes('sha256')) assert(!tokens.some(token => token.explanation?.some(part => part.scopes.some(scope => scope.scopeName === 'support.function.builtin.chialisp'))), 'Builtin prefix misclassified');
    if (sample.includes('suffix')) assert(!tokens.some(token => token.explanation?.some(part => part.scopes.some(scope => scope.scopeName === 'constant.numeric.hex.chialisp'))), 'Hex prefix misclassified');
  }
} finally { highlighter.dispose(); }

const parse = unified().use(rehypeParse);
function all(node, predicate) { return [...(predicate(node) ? [node] : []), ...(node.children ?? []).flatMap(child => all(child,predicate))]; }
const luminance = hex => {
  const linear = hex.slice(1).match(/../g).map(pair => parseInt(pair,16) / 255).map(value => value <= .04045 ? value/12.92 : ((value+.055)/1.055)**2.4);
  return .2126*linear[0]+.7152*linear[1]+.0722*linear[2];
};
const contrast = (a,b) => { const x=luminance(a),y=luminance(b); return (Math.max(x,y)+.05)/(Math.min(x,y)+.05); };
const built = new URL('../dist/', import.meta.url);
const backgrounds = new Set();
for (const file of await readdir(new URL('_astro/',built))) if (file.endsWith('.css')) {
  const css = await readFile(new URL(`_astro/${file}`,built),'utf8');
  for (const match of css.matchAll(/--ec-codeBg:(#[a-fA-F0-9]{6})[;}]/g)) backgrounds.add(match[1]);
}
assert.equal(backgrounds.size, 2, 'Expected the actual dark and light code backgrounds');
const bg = [...backgrounds];
assert(luminance(bg[0]) < luminance(bg[1]), 'Theme order must match rendered --0/--1 colors');
let colors = 0;
let blocks = 0;
let minimum = Infinity;
for (const route of ['lab/capped-payment/', 'verification/error-study/', 'tutorials/delegated-conditions/']) {
  const ast = parse.parse(await readFile(new URL(`${route}index.html`,built),'utf8'));
  const snippets = all(ast,node=>node.tagName==='pre' && node.properties?.dataLanguage==='clsp');
  assert(snippets.length > 0, `${route}: no real clsp blocks`);
  for (const snippet of snippets) {
    blocks++;
    for (const span of all(snippet,node=>node.tagName==='span' && typeof node.properties?.style==='string')) {
      for (const match of span.properties.style.matchAll(/--([01]):(#[a-fA-F0-9]{6})(?:;|$)/g)) {
        const ratio = contrast(match[2],bg[Number(match[1])]);
        assert(ratio >= 4.5, `${route}: low-contrast token ${match[2]} on ${bg[Number(match[1])]}`);
        minimum = Math.min(minimum,ratio); colors++;
      }
    }
  }
}
assert(colors > 100, 'No real dual-theme token colors checked');
for (const file of ['chialisp-highlighting-notice.txt','chialisp-highlighting-apache-2.0.txt']) {
  const source = await readFile(new URL(`../public/licenses/${file}`,import.meta.url));
  assert.deepEqual(await readFile(new URL(`licenses/${file}`,built)),source,'Published highlighting attribution differs');
}
console.log(`PASS Chialisp highlighting: real Shiki categories/aliases, string/comment and literal boundaries; ${blocks} built blocks, ${colors} actual theme colors, minimum contrast ${minimum.toFixed(2)}:1; exact published attribution`);
