import type { APIRoute, GetStaticPaths } from 'astro';
import proof from '../../../../../corpus/tree-proofs.json';
import { readStructuralArtifact, structuralArtifacts } from '../../../lib/structural-artifacts.mjs';

const artifacts = structuralArtifacts(proof.artifacts);
export const prerender = true;

export const getStaticPaths: GetStaticPaths = () => artifacts.map(artifact => ({
  params: { artifact: artifact.basename },
}));

export const GET: APIRoute = async ({ params }) => {
  const artifact = artifacts.find(artifact => artifact.basename === params.artifact);
  if (!artifact) return new Response('Not found', { status: 404 });
  const bytes = await readStructuralArtifact(artifact);
  return new Response(new Uint8Array(bytes), {
    headers: {
      'Content-Type': artifact.basename.endsWith('.json') ? 'application/json' : 'text/plain; charset=utf-8',
    },
  });
};
