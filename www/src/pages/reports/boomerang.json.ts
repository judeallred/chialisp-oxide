import { puzzles } from '../../lib/data';
export function GET() {
  const selected = puzzles.filter(puzzle => puzzle.slug === 'boomerang');
  if (selected.length !== 1 || selected[0].coin_instances?.protocol !== 'chialisp-oxide-boomerang-instances/v1') {
    throw new Error('Missing actual current boomerang capture; pending data cannot publish');
  }
  return new Response(JSON.stringify(selected[0].coin_instances), {
    headers: { 'Content-Type': 'application/json; charset=utf-8' },
  });
}
