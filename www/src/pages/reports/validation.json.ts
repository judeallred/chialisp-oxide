import type { APIRoute } from 'astro';
import validation from '../../../../corpus/validation.json?raw';
export const GET: APIRoute = () => new Response(validation, { headers: { 'Content-Type': 'application/json' } });
