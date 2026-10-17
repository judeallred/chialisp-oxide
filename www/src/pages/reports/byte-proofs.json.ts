import type { APIRoute } from 'astro';
import evidence from '../../../../corpus/byte-proofs.json';

export const GET: APIRoute = () => new Response(JSON.stringify(evidence, null, 2), {
  headers: { 'Content-Type': 'application/json' },
});
