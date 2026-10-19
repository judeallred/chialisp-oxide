import { puzzles, delegatedProtocol } from '../../lib/data';
export function GET() {
  const selected = puzzles.filter(puzzle => puzzle.slug === 'p2-delegated-conditions');
  if (selected.length !== 1) throw new Error('Missing actual current delegated-conditions capture; pending data cannot publish');
  const evidence = delegatedProtocol(selected[0]);
  if (!evidence || evidence.status !== 'validated') throw new Error('Missing actual delegated-conditions evidence');
  return new Response(JSON.stringify(evidence), { headers: { 'Content-Type': 'application/json; charset=utf-8' } });
}
