import type { APIRoute, GetStaticPaths } from 'astro';
import { readStatefulDownloads } from '../../../lib/stateful-evidence.mjs';

export const getStaticPaths: GetStaticPaths = async () => {
  const evidence = await readStatefulDownloads();
  return evidence.downloads.map((file: { path: string }) => ({
    params: { artifact: file.path },
    props: { bytes: evidence.payloads.get(file.path), json: file.path.endsWith('.json') },
  }));
};

export const GET: APIRoute = ({ props }) => new Response(Uint8Array.from(props.bytes).buffer, {
  headers: { 'Content-Type': props.json ? 'application/json; charset=utf-8' : 'application/octet-stream' },
});
