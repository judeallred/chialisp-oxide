import type { APIRoute, GetStaticPaths } from 'astro';
import evidence from '../../../../../corpus/byte-proofs.json';
import { byteProofArtifacts, readByteProofArtifact } from '../../../lib/byte-proof-artifacts.mjs';

const artifacts = byteProofArtifacts(evidence);
export const prerender = true;
export const getStaticPaths: GetStaticPaths = () => artifacts.map(artifact => ({
  params: { artifact: artifact.basename },
}));

export const GET: APIRoute = async ({ params }) => {
  const artifact = artifacts.find(artifact => artifact.basename === params.artifact);
  if (!artifact) return new Response('Not found', { status: 404 });
  const bytes = await readByteProofArtifact(artifact);
  return new Response(new Uint8Array(bytes), {
    headers: {
      'Content-Type': artifact.basename.endsWith('.json') ? 'application/json' : 'text/plain; charset=utf-8',
    },
  });
};
