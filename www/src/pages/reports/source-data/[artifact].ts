import type { APIRoute, GetStaticPaths } from 'astro';
import validation from '../../../../../corpus/validation.json';
import { sourceDataArtifacts, readSourceDataArtifact } from '../../../lib/source-data.mjs';

const artifacts = sourceDataArtifacts(validation);
export const prerender = true;
export const getStaticPaths: GetStaticPaths = () => artifacts.map(artifact => ({
  params: { artifact: artifact.basename },
}));
export const GET: APIRoute = async ({ params }) => {
  const artifact = artifacts.find(entry => entry.basename === params.artifact);
  if (!artifact) return new Response('Not found', { status: 404 });
  const bytes = await readSourceDataArtifact(artifact);
  return new Response(new Uint8Array(bytes), {
    headers: { 'Content-Type': 'application/json; charset=utf-8' },
  });
};
