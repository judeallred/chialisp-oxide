import { readStatefulEvidence } from '../../lib/stateful-evidence.mjs';

export async function GET() {
  const evidence = await readStatefulEvidence();
  return new Response(JSON.stringify(evidence), {
    headers: { 'Content-Type': 'application/json; charset=utf-8' },
  });
}
