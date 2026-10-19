import type { APIRoute, GetStaticPaths } from 'astro';
import { loadCompositionCostEvidence } from '../../../lib/composition-costs.mjs';
const evidence = loadCompositionCostEvidence();
export const prerender = true;
export const getStaticPaths: GetStaticPaths = async () => (await evidence).downloads.map(artifact => ({ params: { artifact: artifact.basename } }));
export const GET: APIRoute = async ({ params }) => {
  const artifact = (await evidence).downloads.find(row => row.basename === params.artifact);
  if (!artifact) return new Response('Not found', { status: 404 });
  return new Response(new Uint8Array(artifact.content), { headers: { 'Content-Type': 'application/json; charset=utf-8' } });
};
