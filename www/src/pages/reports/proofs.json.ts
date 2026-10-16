import type { APIRoute } from 'astro';
import proof from '../../../../corpus/proofs.json';
export const GET: APIRoute = () => new Response(JSON.stringify(proof, null, 2), { headers: { 'Content-Type': 'application/json' } });
