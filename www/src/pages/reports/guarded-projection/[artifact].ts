import type { APIRoute, GetStaticPaths } from 'astro';
import { createHash } from 'node:crypto';
import { guardedArtifacts } from '../../../lib/guarded-costs.mjs';
import { readCostInputs } from '../../../lib/p2-costs.mjs';

// Exact historical candidate records. The old profile is still pending; this
// raw download route neither promotes it nor invokes it as current authority.
const pins: Record<string, string> = {
  'profile.json': '66dc316cf447bd123b11bb33c3a4568a7be28f64d31b7947ebedf9ed3e614b54',
  'costs.json': '878307b1082c5e6ae95ae8ffb5c5d31a1efb3c985dea50b378206a6c215934d4',
  'controls.json': 'c73b3cbde014d8ff6caf05a37bfacc939af5820c86ca58fb4944e3387828ba93',
};
const evidence = readCostInputs(undefined, guardedArtifacts.map(row => [row.basename, row.path, 4 * 1024 * 1024]));
export const prerender = true;
export const getStaticPaths: GetStaticPaths = async () => guardedArtifacts.map(artifact => ({ params: { artifact: artifact.basename } }));
export const GET: APIRoute = async ({ params }) => {
  const name = params.artifact;
  if (!name || !Object.hasOwn(pins, name)) return new Response('Not found', { status: 404 });
  const content = (await evidence as Record<string, Uint8Array>)[name];
  if (!(content instanceof Uint8Array)) throw new Error('Missing historical guarded download bytes');
  if (createHash('sha256').update(content).digest('hex') !== pins[name]) throw new Error('Historical guarded candidate bytes changed');
  return new Response(new Uint8Array(content), { headers: { 'Content-Type': 'application/json; charset=utf-8' } });
};
