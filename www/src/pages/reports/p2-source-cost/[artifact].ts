import type { APIRoute, GetStaticPaths } from 'astro';
import { loadP2CostEvidence } from '../../../lib/p2-costs.mjs';

// Six original raw inputs stay byte-exact; validation.json adds the shared whole source-only checkpoint.
// A pending guarded profile prevents publication before signed checkpoint review.
const evidence = loadP2CostEvidence();
export const prerender = true;
export const getStaticPaths: GetStaticPaths = async () => (await evidence).downloads.map(artifact => ({
  params: { artifact: artifact.basename },
}));
export const GET: APIRoute = async ({ params }) => {
  const artifact = (await evidence).downloads.find(row => row.basename === params.artifact);
  if (!artifact) return new Response('Not found', { status: 404 });
  return new Response(new Uint8Array(artifact.content), {
    headers: { 'Content-Type': 'application/json; charset=utf-8' },
  });
};
