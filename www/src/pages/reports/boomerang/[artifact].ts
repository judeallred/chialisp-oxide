import type { APIRoute, GetStaticPaths } from 'astro';
import { puzzles } from '../../../lib/data';
import { artifactFile } from '../../../lib/compiled-artifacts.mjs';

export const getStaticPaths: GetStaticPaths = () => {
  const puzzle = puzzles.find(puzzle => puzzle.slug === 'boomerang');
  if (!puzzle?.coin_instances || puzzle.coin_instances.examples.length !== 1) throw new Error('Missing boomerang instances');
  return puzzle.coin_instances.examples[0].instances.map(({ artifact }) => ({
    params: { artifact: artifactFile('boomerang', artifact, 'clvm.hex') },
    props: { text: artifact.clvm_hex + '\n' },
  }));
};
export const GET: APIRoute = ({ props }) => new Response(props.text, {
  headers: { 'Content-Type': 'text/plain; charset=utf-8' },
});
