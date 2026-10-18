import type { APIRoute } from 'astro';
import { loadAcyclicProofEvidence } from '../../lib/acyclic-proof-artifacts.mjs';

export const prerender = true;
const { raw } = await loadAcyclicProofEvidence();
export const GET: APIRoute = () => new Response(new Uint8Array(raw), {
  headers: { 'Content-Type': 'application/json' },
});
