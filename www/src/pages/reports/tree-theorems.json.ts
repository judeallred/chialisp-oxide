import type { APIRoute } from 'astro';
import evidence from '../../../../experiments/tree-theorems/results.json';

export const GET: APIRoute = () => new Response(JSON.stringify(evidence, null, 2), {
  headers: { 'Content-Type': 'application/json' },
});
