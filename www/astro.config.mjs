import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://judeallred.github.io',
  base: '/chialisp-oxide',
  trailingSlash: 'always',
  integrations: [
    starlight({
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
      components: { Banner: './src/components/PrototypeBanner.astro' },
      social: [{ icon: 'github', label: 'GitHub repository', href: 'https://github.com/judeallred/chialisp-oxide' }],
      sidebar: [
        { label: 'Start here', items: [
          { label: 'Introduction', slug: 'start/introduction' },
          { label: 'Try the Cargo prototype', slug: 'start/quickstart' },
          { label: 'What works today', slug: 'reference/supported-rust' },
        ] },
        { label: 'Learn by building', items: [
          { label: 'Your first payment', slug: 'tutorials/first-payment' },
          { label: 'Validate untrusted inputs', slug: 'tutorials/input-validation' },
          { label: 'Use typed puzzle helpers', slug: 'tutorials/typed-sdk' },
          { label: 'Split a payment', slug: 'tutorials/split-payment' },
          { label: 'Require signatures', slug: 'tutorials/signatures' },
          { label: 'Time & recovery', slug: 'tutorials/recovery' },
          { label: 'Checked arithmetic', slug: 'tutorials/counter' },
        ] },
        { label: 'Inspect the evidence', items: [
          { label: 'Puzzle laboratory', link: '/lab/' },
          { label: 'Validation & security', slug: 'verification/method' },
          { label: 'Errors & guarantees', slug: 'verification/error-study' },
          { label: 'Proofs & model checking', slug: 'verification/formal' },
          { label: 'Tutorial & CHIP inventory', link: '/coverage/' },
        ] },
        { label: 'Understand the compiler', items: [
          { label: 'Architecture & Rue', slug: 'reference/architecture' },
          { label: 'Puzzle ABI', slug: 'reference/abi' },
          { label: 'Compiler commands', slug: 'reference/cli' },
          { label: 'Conditions & authorizations', slug: 'reference/conditions' },
          { label: 'Contributing', slug: 'reference/contributing' },
        ] },
      ],
    }),
  ],
});
