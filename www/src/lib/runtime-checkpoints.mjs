// Presentation provenance only. Mandatory Python current-source validation and
// the reviewed profile pin remain separate from these recorded context checks.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';

const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const fields = (value, names, label) => {
  assert(value && typeof value === 'object' && !Array.isArray(value), `${label}: expected object`);
  assert.deepEqual(Object.keys(value).sort(), [...names].sort(), `${label}: missing/unknown fields`);
};
export const canonicalCheckpoint = value => {
  if (Array.isArray(value)) return `[${value.map(canonicalCheckpoint).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(key => `${canonicalCheckpoint(key)}:${canonicalCheckpoint(value[key])}`).join(',')}}`;
  return JSON.stringify(value).replace(/[\u007f-\uffff]/g, char => `\\u${char.charCodeAt(0).toString(16).padStart(4, '0')}`);
};
export function reviewedCompositionProfile(profile) {
  fields(profile, ['schema_version','protocol','status','scope','signed_anchor','reviewed_intermediate','immutable_profiles','implementation_transition','optimization_pipeline','reviewed_after'], 'Runtime composition profile');
  assert.equal(profile.schema_version, 1);
  assert.equal(profile.protocol, 'chialisp-oxide-runtime-composition/v1');
  assert.equal(profile.status, 'reviewed_final_runtime_composition', 'Runtime composition requires final root review; pending profiles cannot publish');
  const after = profile.reviewed_after;
  fields(after, ['report_sha256','canonical_report_sha256','p2_sha256','protocol_sha256','raw_ir_sha256','metrics_sha256','controls_sha256','bundles','delta'], 'Runtime composition capture');
  for (const key of Object.keys(after).filter(key => !['bundles','delta'].includes(key))) assert(typeof after[key] === 'string' && /^[0-9a-f]{64}$/.test(after[key]));
  fields(after.bundles, ['on','off'], 'Runtime composition modes');
  for (const [name, mode] of [['on',true],['off',false]]) {
    fields(after.bundles[name], ['manifest_sha256','optimize_lir'], 'Captured composition mode');
    assert.equal(after.bundles[name].optimize_lir, mode);
    assert(/^[0-9a-f]{64}$/.test(after.bundles[name].manifest_sha256));
  }
  fields(after.delta, ['leaf_count','inventory_sha256'], 'Complete composition delta');
  assert(Number.isSafeInteger(after.delta.leaf_count) && after.delta.leaf_count > 0 && after.delta.leaf_count <= 100_000);
  assert(/^[0-9a-f]{64}$/.test(after.delta.inventory_sha256));
  return profile;
}

/** M1 is unsigned content; E20 is a different, real signed Git checkpoint. */
export function compositionCheckpoint(profile, bytes) {
  reviewedCompositionProfile(profile);
  const intermediate = profile.reviewed_intermediate, anchor = profile.signed_anchor;
  fields(intermediate, ['kind','checkpoint_path','report_sha256','canonical_report_sha256','authorizing_transition_path','authorizing_transition_sha256'], 'Unsigned reviewed checkpoint');
  assert.deepEqual(intermediate, {
    kind: 'unsigned_reviewed_content_addressed_checkpoint',
    checkpoint_path: 'corpus/checkpoints/p2-source-cost-validation.json',
    report_sha256: '1a19cca4b2f1d840bf49c3119305f8872ff03e4ea91c5f74e93ad57ef4ad8b03',
    canonical_report_sha256: '1f8819d8c2ddab219b1277e2550381378c3e7b2a9e5933830b1bbdfe96370943',
    authorizing_transition_path: 'corpus/p2-source-cost-baseline.json',
    authorizing_transition_sha256: 'ee2689f1bdf3a1fd275f3d2374068b38b12d0f52c3135fe8ee5de455a5c8bb5a',
  });
  fields(anchor, ['kind','repository','commit','tree_sha1','report_path','report_git_blob_sha1','report_sha256','canonical_report_sha256'], 'Signed historical anchor');
  assert.deepEqual(anchor, {
    kind: 'signed_git_checkpoint', repository: 'https://github.com/judeallred/chialisp-oxide',
    commit: 'e20f6d126aedbf76a8c179701af654ca96550074', tree_sha1: '6f795121c9998515bb0e38c16cf6d00924254492',
    report_path: 'corpus/validation.json', report_git_blob_sha1: '4e739b7cd0f55130742ae44e8d0a91aac2cecf11',
    report_sha256: '104e4e706e1183cff556ee0a559b8410ff6a5cb6b76bc8a512c04eb3a00d31da',
    canonical_report_sha256: 'b95dfeb63fb1df2cc980135644b35c41cd50f04bad508dd71af5519c5b82c89b',
  });
  assert(bytes instanceof Uint8Array && bytes.length > 0 && bytes.length <= 16 * 1024 * 1024);
  assert.equal(sha(bytes), intermediate.report_sha256, 'Unsigned checkpoint bytes changed');
  assert.equal(sha(canonicalCheckpoint(JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)))), intermediate.canonical_report_sha256);
  return { label: 'Unsigned reviewed source-only checkpoint', kind: intermediate.kind,
    reportSHA: intermediate.report_sha256, anchor: { commit: anchor.commit, reportSHA: anchor.report_sha256, repository: anchor.repository } };
}
