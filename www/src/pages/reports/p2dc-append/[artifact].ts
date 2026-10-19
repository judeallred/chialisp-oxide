import type { APIRoute, GetStaticPaths } from 'astro';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readCostInputs } from '../../../lib/p2-costs.mjs';
import { canonicalCheckpoint } from '../../../lib/runtime-checkpoints.mjs';
const definitions = [
  ['profile.json','corpus/p2-delegated-conditions-append-baseline.json',4*1024*1024],
  ['before-validation.json','corpus/checkpoints/runtime-composition-validation.json',16*1024*1024],
  ['p2-costs.json','corpus/p2dc-append-existing-p2-costs.json',4*1024*1024],
  ['p2-controls.json','corpus/p2dc-append-existing-p2-controls.json',4*1024*1024],
] as const;
const sha = (bytes: Uint8Array | string) => createHash('sha256').update(bytes).digest('hex');
async function artifacts() {
  const inputs = await readCostInputs(undefined,[
    ['active','corpus/validation.json',16*1024*1024],
    ['composition','corpus/runtime-composition-baseline.json',4*1024*1024],
    ...definitions,
  ],48*1024*1024) as Record<string,Uint8Array>;
  for(const value of Object.values(inputs)) assert(value instanceof Uint8Array);
  const profile = JSON.parse(Buffer.from(inputs['profile.json']).toString('utf8'));
  assert.equal(profile.schema_version,1);
  assert.equal(profile.protocol,'chialisp-oxide-p2dc-append/v1');
  assert.equal(profile.status,'reviewed_final_p2dc_append','Pending current23 evidence cannot publish raw append downloads');
  assert.equal(sha(inputs.active),profile.reviewed_after.report_sha256);
  assert.equal(sha(canonicalCheckpoint(JSON.parse(Buffer.from(inputs.active).toString('utf8')))),profile.reviewed_after.canonical_report_sha256);
  assert.equal(sha(inputs['before-validation.json']),profile.baseline.report_sha256);
  assert.equal(sha(inputs.composition),profile.baseline.authorizing_transition_sha256);
  assert.equal(sha(inputs['p2-costs.json']),profile.reviewed_after.metrics_sha256);
  assert.equal(sha(inputs['p2-controls.json']),profile.reviewed_after.controls_sha256);
  return definitions.map(([basename])=>({basename,bytes:inputs[basename]}));
}
const retained = artifacts();
export const prerender = true;
export const getStaticPaths: GetStaticPaths = async () => (await retained).map(row=>({params:{artifact:row.basename}}));
export const GET: APIRoute = async ({params}) => {
  const row=(await retained).find(row=>row.basename===params.artifact);
  if(!row)return new Response('Not found',{status:404});
  return new Response(new Uint8Array(row.bytes),{headers:{'Content-Type':'application/json; charset=utf-8'}});
};
