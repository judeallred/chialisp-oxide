import type { APIRoute } from 'astro';
import validation from '../../../../corpus/validation.json';
export const GET: APIRoute = () => new Response(JSON.stringify(validation, null, 2), { headers: { 'Content-Type': 'application/json' } });
