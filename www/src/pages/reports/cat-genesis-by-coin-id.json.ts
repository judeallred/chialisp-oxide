import { puzzles, catProtocol } from '../../lib/data';
export function GET() {
  const selected = puzzles.filter(puzzle => puzzle.slug === 'cat-genesis-by-coin-id');
  if (selected.length !== 1) throw new Error('Missing current CAT capture');
  const evidence = catProtocol(selected[0]);
  if (!evidence || evidence.status !== 'validated') throw new Error('Missing actual CAT lifecycle');
  return new Response(JSON.stringify(evidence), { headers: { 'Content-Type': 'application/json; charset=utf-8' } });
}
