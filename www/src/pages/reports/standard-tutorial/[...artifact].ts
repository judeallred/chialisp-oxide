import type { APIRoute, GetStaticPaths } from 'astro';
import { report } from '../../../lib/data';
import { readStandardTutorialDownloads } from '../../../lib/standard-tutorial.mjs';
export const getStaticPaths: GetStaticPaths = async () => {
  const evidence = await readStandardTutorialDownloads({ required: report.puzzles.some(puzzle => puzzle.slug === 'standard-delegated-hidden') });
  return evidence.downloads.map((file: { path: string }) => ({ params: { artifact: file.path }, props: { bytes: evidence.payloads.get(file.path), json: file.path.endsWith('.json') } }));
};
export const GET: APIRoute = ({ props }) => new Response(Uint8Array.from(props.bytes).buffer, { headers: { 'Content-Type': props.json ? 'application/json; charset=utf-8' : 'application/octet-stream' } });
