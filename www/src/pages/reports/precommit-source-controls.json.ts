import type { APIRoute } from 'astro';
import { readPrecommitControls } from '../../lib/precommit-source-controls.mjs';
export const GET: APIRoute = async () => {
  const evidence = await readPrecommitControls();
  return new Response(new Uint8Array(evidence.bytes), { headers: { 'Content-Type': 'application/json' } });
};
