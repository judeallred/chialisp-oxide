import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import searchIndex from './search-index.mjs';

export default defineConfig({
  site: 'https://judeallred.github.io',
  base: '/chialisp-oxide',
  trailingSlash: 'always',
  integrations: [
    starlight({
      pagefind: false,
      title: 'chialisp / oxide',
      description: 'Real Rust to CLVM. Learn from executable puzzles, independent references, and reproducible validation.',
      logo: { src: './src/assets/oxide-mark.svg', replacesTitle: false },
      favicon: '/favicon.svg',
      customCss: ['./src/styles/custom.css'],
      expressiveCode: {
        defaultProps: {
          overridesByLang: { rust: { wrap: true, preserveIndent: true } },
        },
      },
      components: { Banner: './src/components/PrototypeBanner.astro', Search: './src/components/Search.astro' },
      social: [{ icon: 'github', label: 'GitHub repository', href: 'https://github.com/judeallred/chialisp-oxide' }],
      sidebar: [
        { label: 'Start here', items: [
          { label: 'Introduction', slug: 'start/introduction' },
          { label: 'Try the Cargo prototype', slug: 'start/quickstart' },
          { label: 'What works today', slug: 'reference/supported-rust' },
          { label: 'Chialisp functionality and Rust gaps', slug: 'reference/chialisp-functionality' },
        ] },
        { label: 'Learn by building', items: [
          { label: 'Port the canonical Intro', slug: 'tutorials/canonical-intro' },
          { label: 'Your first payment', slug: 'tutorials/first-payment' },
          { label: 'Validate untrusted inputs', slug: 'tutorials/input-validation' },
          { label: 'Use typed puzzle helpers', slug: 'tutorials/typed-sdk' },
          { label: 'Split a payment', slug: 'tutorials/split-payment' },
          { label: 'Require signatures', slug: 'tutorials/signatures' },
          { label: 'Time & recovery', slug: 'tutorials/recovery' },
          { label: 'Checked arithmetic', slug: 'tutorials/counter' },
          { label: 'Build a quoted program', slug: 'tutorials/p2-conditions' },
          { label: 'Follow recursive trees', slug: 'tutorials/recursive-trees' },
          { label: 'Port a published list helper', slug: 'tutorials/last-list-element' },
          { label: 'Construct an owned prefix list', slug: 'tutorials/prefix-list' },
          { label: 'Compare exact atom bytes', slug: 'tutorials/atom-not-equal' },
          { label: 'Order entries by byte keys', slug: 'tutorials/insertion-sort-by-keys' },
          { label: 'Hash exact bytes and trees', slug: 'tutorials/sha256tree' },
          { label: 'Bind a password hash to a coin', slug: 'tutorials/password-coin' },
          { label: 'Read the singleton launcher', slug: 'tutorials/singleton-launcher' },
        ] },
        { label: 'Inspect the evidence', items: [
          { label: 'Puzzle laboratory', link: '/lab/' },
          { label: 'Validation & security', slug: 'verification/method' },
          { label: 'Errors that Rust makes impossible', slug: 'verification/error-study' },
          { label: 'Proofs & model checking', slug: 'verification/formal' },
          { label: 'Prove a byte policy in its IR model', slug: 'verification/byte-proofs' },
          { label: 'Tutorial & CHIP inventory', link: '/coverage/' },
          { label: 'Published puzzle sources', link: '/published/' },
        ] },
        { label: 'Understand the compiler', items: [
          { label: 'Architecture & Rue', slug: 'reference/architecture' },
          { label: 'Optimization & measured costs', slug: 'reference/optimization' },
          { label: 'Compiler trust & standard libraries', slug: 'reference/compiler-trust' },
          { label: 'Hashing and the compiler bridge', slug: 'reference/hashing' },
          { label: 'Puzzle ABI', slug: 'reference/abi' },
          { label: 'Compiler commands', slug: 'reference/cli' },
          { label: 'Conditions & authorizations', slug: 'reference/conditions' },
          { label: 'Contributing', slug: 'reference/contributing' },
        ] },
      ],
    }),
    searchIndex(),
  ],
});
