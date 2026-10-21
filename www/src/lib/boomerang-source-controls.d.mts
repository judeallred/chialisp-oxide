/** Typed view of records admitted by the independent Python packet reader. */
export type ControlName = 'original' | 'equivalent' | 'drop_claim_signature' | 'late_claim' | 'early_refund';
export type Witness = 'timely_unsigned_claim' | 'signed_late_claim' | 'early_unsigned_refund';
export type Mode = 'off' | 'on';
export interface CoinRecord {
  coin_id: string;
  parent_coin_id: string;
  puzzle_hash: string;
  amount: number;
}
export interface AcceptedOutcome { status: 'accepted'; output_hex: string }
export interface AcceptedExecution { outcome: AcceptedOutcome; cost: number; error: null }
export interface AcceptedLedgerOutcome {
  status: 'accepted';
  input_spent: true;
  child_count: 1;
  child: CoinRecord;
}
export interface RejectedLedgerOutcome {
  status: 'consensus_rejected';
  code: string;
  semantic_rejection_credit: true;
  full_state_unchanged: true;
}
export interface Instance {
  path: string;
  bytes: number;
  clvm_hex: string;
  tree_hash: string;
  module_tree_hash: string;
}
export interface SourceEdit { before: string; after: string }
export interface SourceControl {
  name: ControlName;
  role: string;
  source_sha256: string;
  edits: SourceEdit[];
}
export interface ControlSpec {
  name: ControlName;
  witness: Witness;
  claim: boolean;
  timestamp: number;
  signing: 'none' | 'recipient';
  code: string | null;
}
export interface LedgerObservation {
  mode: Mode;
  name: ControlName;
  witness: Witness;
  claim: boolean;
  timestamp: number;
  signing: 'none' | 'recipient';
  module_tree_hash: string;
  instance_tree_hash: string;
  instance_hex: string;
  solution_hex: string;
  signature_hex: string;
  input: CoinRecord;
  outcome: AcceptedLedgerOutcome | RejectedLedgerOutcome;
}
export interface ControlRow extends ControlSpec {
  mode: Mode;
  native: AcceptedOutcome;
  execution: AcceptedExecution;
  ledger: LedgerObservation;
  instance: Instance;
}
export interface ControlCounts {
  nativeGroups: number;
  native: number;
  exports: number;
  exportModes: number;
  vm: number;
  attempts: number;
  accepted: number;
  rejected: number;
}
export interface Projection {
  protocol: string;
  scope: string;
  controls: SourceControl[];
  rows: ControlRow[];
  counts: ControlCounts;
}
export interface DisplayEvidence extends Projection { reportSHA: string }
export interface Packet {
  files: { path: string; sha256: string }[];
  [field: string]: unknown;
}
export interface AdmittedEvidence extends DisplayEvidence { report: Packet }
export const controlsProtocol: 'chialisp-oxide-boomerang-source-controls/v1';
export const controlNames: ControlName[];
export const controlSpecs: ControlSpec[];
export const controlPacketRoot: string;
export function controlOutput(name: ControlName, claim: boolean): string;
export function projectBoomerangControls(report: unknown, nativeRows: unknown): Projection;
export function readBoomerangControls(): Promise<AdmittedEvidence | undefined>;
export function checkBoomerangControlsDisplay(markup: string, evidence: DisplayEvidence): ControlCounts;
