import { report } from '../../lib/data';
import { readStandardTutorialEvidence } from '../../lib/standard-tutorial.mjs';
export async function GET() {
  const evidence = await readStandardTutorialEvidence({ required: report.puzzles.some(puzzle => puzzle.slug === 'standard-delegated-hidden') });
  return new Response(JSON.stringify(evidence), { headers: { 'Content-Type': 'application/json; charset=utf-8' } });
}
