import { puzzles } from '../../lib/data';
export function GET() {
  const evidence = puzzles.find(puzzle => puzzle.slug === 'p2-singleton')?.protocol_validation;
  if (!evidence) throw new Error('Missing actual recorded pay-to-singleton protocol validation');
  return new Response(JSON.stringify(evidence), { headers: { 'Content-Type': 'application/json; charset=utf-8' } });
}
