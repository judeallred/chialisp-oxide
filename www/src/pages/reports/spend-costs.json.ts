import type { APIRoute } from 'astro';
import evidence from '../../../../experiments/spend-costs/results.json?raw';

export const GET: APIRoute = () => new Response(
  evidence,
  { headers: { 'Content-Type': 'application/json; charset=utf-8' } },
);
