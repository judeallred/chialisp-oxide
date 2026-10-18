import type { APIRoute, GetStaticPaths } from 'astro';
import { loadAcyclicProofEvidence } from '../../../lib/acyclic-proof-artifacts.mjs';

export const prerender = true;
const { artifacts } = await loadAcyclicProofEvidence();
export const getStaticPaths: GetStaticPaths = () => artifacts.map(artifact => ({
  params: { artifact: artifact.basename },
}));

export const GET: APIRoute = ({ params }) => {
  const artifact = artifacts.find(item => item.basename === params.artifact);
  if (!artifact) return new Response('Not found', { status: 404 });
  return new Response(new Uint8Array(artifact.bytes), {
    headers: { 'Content-Type': artifact.basename.endsWith('.json') ? 'application/json' : 'text/plain; charset=utf-8' },
  });
};
