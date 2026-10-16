import type { APIRoute } from 'astro';
import examples from '../../../../corpus/tutorial-examples.json';
export const GET: APIRoute = () => new Response(JSON.stringify(examples, null, 2), { headers: { 'Content-Type': 'application/json' } });
