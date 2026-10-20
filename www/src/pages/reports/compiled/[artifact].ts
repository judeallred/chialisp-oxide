import type { APIRoute, GetStaticPaths } from 'astro';
import { puzzles } from '../../../lib/data';
import { artifactFile } from '../../../lib/compiled-artifacts.mjs';

export const getStaticPaths: GetStaticPaths = () => puzzles.flatMap(puzzle => puzzle.artifacts.flatMap(artifact => [
  { params: { artifact: artifactFile(puzzle.slug,artifact,'clvm.txt') }, props: { text: artifact.disassembly + '\n' } },
  { params: { artifact: artifactFile(puzzle.slug,artifact,'clvm.hex') }, props: { text: artifact.clvm_hex + '\n' } },
]));
export const GET: APIRoute = ({ props }) => new Response(props.text, {
  headers: { 'Content-Type': 'text/plain; charset=utf-8' },
});
