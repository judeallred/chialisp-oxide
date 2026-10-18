import baseline from '../../../../corpus/helper-inlining-baseline.json?raw';

export function GET() {
  return new Response(baseline, { headers: { 'Content-Type': 'application/json; charset=utf-8' } });
}
