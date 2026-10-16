import type { APIRoute } from 'astro';
import report from '../../../../corpus/error-study.json';
export const GET: APIRoute = () => new Response(JSON.stringify(report, null, 2), { headers: { 'Content-Type': 'application/json' } });
