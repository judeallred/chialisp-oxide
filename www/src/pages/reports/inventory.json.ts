import type { APIRoute } from 'astro';
import inventory from '../../../../corpus/inventory.json';
export const GET: APIRoute = () => new Response(JSON.stringify(inventory, null, 2), { headers: { 'Content-Type': 'application/json' } });
