import type { APIRoute, GetStaticPaths } from 'astro';
import { readXchandlesPremine, readXchandlesArtifact } from '../../../lib/xchandles-premine.mjs';

export const getStaticPaths: GetStaticPaths = async () => {
  const evidence = await readXchandlesPremine({ required: import.meta.env.PROD });
  if (!evidence) return [];
  // Only compact descriptors enter route props. GET reads and verifies one
  // bounded payload, rather than keeping the entire history packet in memory.
  return (['history', 'simulator'] as const).flatMap(lane =>
    evidence[lane].downloads.map((file: { path: string }) => ({
      params: { artifact: `${lane}/${file.path}` },
      props: { lane, name: file.path },
    })),
  );
};

export const GET: APIRoute = async ({ props }) => {
  const bytes = await readXchandlesArtifact(props.lane, props.name);
  return new Response(Uint8Array.from(bytes).buffer, {
    headers: { 'Content-Type': props.name.endsWith('.json') ? 'application/json; charset=utf-8' : 'application/octet-stream' },
  });
};
