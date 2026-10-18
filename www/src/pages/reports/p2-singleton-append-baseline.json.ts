import baseline from '../../../../corpus/p2-singleton-append-baseline.json?raw';
export function GET() {
  return new Response(baseline, { headers: { 'Content-Type': 'application/json; charset=utf-8' } });
}
