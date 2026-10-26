import type { APIRoute, GetStaticPaths } from 'astro';
import { readRegistryProtocol } from '../../../lib/registry-protocol.mjs';

export const getStaticPaths: GetStaticPaths = async () => {
  const evidence = await readRegistryProtocol();
  if (!evidence) return [];
  return evidence.downloads.map((file: { path: string; filename: string }) => ({
    params: { artifact: file.filename },
    props: { bytes: evidence.payloads.get(file.path) },
  }));
};
export const GET: APIRoute = ({ props }) => new Response(Uint8Array.from(props.bytes).buffer, {
  headers: { 'Content-Type': 'application/json; charset=utf-8' },
});
