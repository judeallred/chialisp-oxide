import evidence from '../../../../corpus/integer-canonicality-proof.json?raw';

export function GET() {
  return new Response(evidence, { headers: { 'Content-Type': 'application/json; charset=utf-8' } });
}
