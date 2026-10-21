import type { APIRoute, GetStaticPaths } from 'astro';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { controlPacketRoot, readBoomerangControls } from '../../../lib/boomerang-source-controls.mjs';

export const getStaticPaths: GetStaticPaths = async () => {
  const evidence=await readBoomerangControls();
  if(!evidence)throw new Error('Actual admitted boomerang source-control packet required');
  return Promise.all(['report.json',...evidence.report.files.map((row:{path:string})=>row.path)].map(async artifact=>({
    params:{artifact},props:{bytes:await readFile(path.join(controlPacketRoot,artifact)),json:artifact.endsWith('.json')},
  })));
};
export const GET: APIRoute=({props})=>new Response(Uint8Array.from(props.bytes).buffer,{
  headers:{'Content-Type':props.json?'application/json; charset=utf-8':'text/plain; charset=utf-8'},
});
