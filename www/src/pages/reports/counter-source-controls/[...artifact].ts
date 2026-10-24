import type { APIRoute, GetStaticPaths } from 'astro';
import { readCounterSourceControls } from '../../../lib/counter-source-controls.mjs';

export const getStaticPaths: GetStaticPaths = async () => {
  const evidence = await readCounterSourceControls();
  if (!evidence) return [];
  // Physical payloads only. Logical repeat aliases remain solely in report.json.
  return evidence.physicalNames.map((name: string) => ({
    params: { artifact: name },
    props: { bytes: evidence.payloads.get(name), json: name.endsWith('.json') },
  }));
};

export const GET: APIRoute = ({ props }) => new Response(Uint8Array.from(props.bytes).buffer, {
  headers: { 'Content-Type': props.json ? 'application/json; charset=utf-8' : 'text/plain; charset=utf-8' },
});
