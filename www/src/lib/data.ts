import validation from '../../../corpus/validation.json';
import inventory from '../../../corpus/inventory.json';
import tutorialInventory from '../../../corpus/tutorial-examples.json';

export type Outcome = { status: string; output_hex?: string };
export interface Artifact {
  backend: string;
  optimize_lir: boolean;
  bytes: number;
  tree_hash: string;
  clvm_hex: string;
  disassembly: string;
}
export interface LedgerScenario {
  name: string;
  passed: boolean;
  scope: 'ledger';
  rejections: { stage: 'consensus'; code: string }[];
}
export interface Execution {
  outcome: Outcome;
  cost: number | null;
  error: string | null;
  failure?: { kind: string; code: string };
}
export interface DeclaredDivergence {
  backends: string[];
  outcome: Outcome;
  reason: string;
}
export interface CaseReport {
  label: string;
  solution: string;
  native: Outcome;
  runs: {
    backend: string;
    optimize_lir: boolean;
    execution: Execution;
  }[];
  declared_divergence?: DeclaredDivergence;
}
export const boundOutcome = (test: CaseReport, backend: string): Outcome =>
  test.declared_divergence && test.declared_divergence.backends.includes(backend) ? test.declared_divergence.outcome : test.native;
export const declaredDivergences = (puzzle: { cases: CaseReport[] }) => puzzle.cases.filter(test => test.declared_divergence);
export interface InstanceState {
  created_height: number | null;
  spent_height: number | null;
  children: { coin_id: string; parent_coin_id: string; puzzle_hash: string; amount: number }[];
}
export interface InstanceAttempt {
  label: string;
  argument_hex: string;
  coin_id: string;
  coin_puzzle_hash: string;
  reveal_hex: string;
  reveal_tree_hash: string;
  solution: string;
  solution_hex: string;
  vm_021: Execution;
  consensus: {
    status: 'accepted' | 'consensus_rejected' | 'runtime_umbrella';
    code: string | null;
    error: string | null;
    semantic_rejection_credit: boolean;
  };
  before: InstanceState;
  after: InstanceState;
  full_state_unchanged: boolean;
}
export interface CurriedInstanceEvidence {
  protocol: string;
  constructor: string;
  source_role: string;
  scheme: string;
  source: string;
  revision: string;
  fixed_argument: string;
  artifacts: { module_tree_hash: string; artifact: Artifact }[];
  cases: CaseReport[];
  varying_arguments: {
    argument_hex: string;
    password_hex: string;
    solution: string;
    environment: string;
    native: Outcome;
    runs: { module_tree_hash: string; artifact: Artifact; execution: Execution }[];
  }[];
  ledger: {
    backend: string;
    optimize_lir: boolean;
    simulator: string;
    consensus: string;
    vm: string;
    network: string;
    module_tree_hash: string;
    instance_tree_hash: string;
    transaction_attempts: number;
    attempts: InstanceAttempt[];
  }[];
}
export interface CurriedExample {
  label: string; argument: string; argument_hex: string; argument_tree_hash: string;
  page_result?: string; page_result_hex?: string; scheme: string; constructor: string;
  instances: { backend: string; optimize_lir: boolean; module_tree_hash: string; artifact: Artifact;
    sdk_owned_curry_bytes_equal: boolean; sdk_hash_equal: boolean; page_result_equal?: boolean }[];
  cases: CaseReport[];
}
export interface ProtocolCoin {
  amount: number; id: string; parent_id: string; puzzle_hash: string;
}
export interface ProtocolBundle {
  signature_hex: string;
  spends: { coin: ProtocolCoin; reveal_hex: string; solution_hex: string }[];
}
export interface ProtocolRejection {
  name: string; expected: string; observed: string;
  stage: 'actual_consensus_submission'; full_simulator_equality: boolean;
  transaction: ProtocolBundle;
  consensus: { stage: 'consensus'; protocol: string; code: string; status: 'rejected'; semantic_rejection_credit: boolean };
}
export interface ProtocolValidation {
  schema_version: number; protocol: string; status: string; scope: string;
  simulator: string; consensus: string; ledger_VM: string; network: string; rejection_protocol: string;
  modules: { repository: string; revision: string; root_tree_sha1: string; scope: string;
    modules: { path: string; bytes: number; source_sha256: string; published_hex_sha256: string; tree_hash: string; independent_compilation_matches_published: boolean }[] };
  observations: { backend: string; optimize_lir: boolean; module_tree_hash: string; curried_p2_reveal_hex: string;
    curried_p2_hash: string; canonical_singleton_reveal_hex: string; inner_hash: string; negatives: ProtocolRejection[];
    lifecycle: { name: string; scope: string; funding: ProtocolCoin; launcher: ProtocolCoin; eve: ProtocolCoin;
      p2: ProtocolCoin; continuation: ProtocolCoin; next_singleton: ProtocolCoin;
      genesis: ProtocolBundle; joint: ProtocolBundle; lineage: ProtocolBundle; rejections: ProtocolRejection[];
      receipts: { name: string; stage: 'actual_consensus_submission'; status: 'accepted'; bundle_field: string }[];
    };
  }[];
}
export function protocolCounts(evidence: ProtocolValidation) {
  const accepted = evidence.observations.reduce((sum, group) => sum + group.lifecycle.receipts.length, 0);
  const rejected = evidence.observations.reduce((sum, group) => sum + group.negatives.length + group.lifecycle.rejections.length, 0);
  return { groups: evidence.observations.length, attempts: accepted + rejected, accepted, rejected };
}
export interface DelegatedSubmission {
  label: string;
  status: 'accepted' | 'consensus_rejected';
  transaction: ProtocolBundle;
  code?: string;
  full_simulator_equality?: boolean;
  scope?: string;
  spent_input_observed?: boolean;
  child?: ProtocolCoin;
}
export interface DelegatedProtocol {
  schema_version: number; protocol: 'chia-consensus-0.36.1/p2-delegated-conditions-v1';
  status: string; simulator: string; consensus: string; vm: string; network: string;
  transaction_attempts: number; accepted: number; typed_rejected_with_full_rollback: number; scope: string;
  builds: { backend: string; optimize_lir: boolean; status: string; module_tree_hash: string; module_hex: string;
    owner_public_key: string; attacker_public_key: string; owner_curried_reveal_hex: string; owner_instance_hash: string;
    transaction_attempts: number; accepted: number; typed_rejected_with_full_rollback: number;
    observations: DelegatedSubmission[]; scope: string }[];
  raw_validation: { max_cost: number; execution_vm: string; accepted: number; rejected: number;
    native_evaluations: number; VM_evaluations: number; scope: string;
    cases: (CaseReport & { environment_hex: string; expected: Outcome; canonical_failure_code: string | null })[] };
}
export interface PuzzleReport {
  slug: string;
  title: string;
  kind: string;
  security_scope: string;
  provenance: { classification: string; references: string[]; example_id?: string };
  environment?: 'ignored' | 'raw';
  reference_mode?: 'expression';
  reference_includes?: Record<string, string>;
  native_sources?: { path: string; sha256: string }[];
  validation_max_cost?: number;
  resource_controls?: { max_cost: number; cases: CaseReport[] };
  source: string;
  source_dependencies: { path: string; sha256: string; source: string }[];
  reference: string;
  rue: { path: string; sha256: string; source: string; dependencies: { path: string; sha256: string; source: string }[] };
  artifacts: Artifact[];
  cases: CaseReport[];
  generated_cases: number;
  simulator: LedgerScenario[];
  simulator_comparisons: { backend: string; optimize_lir: boolean; scenarios: LedgerScenario[] }[];
  factory_checks?: { backend:string; optimize_lir:boolean; environment:string; expected_output_hex:string; program_hex:string; relation?:string; application_runs:{solution:string;execution:{outcome:Outcome;cost:number|null;error:string|null};expected_outcome?:Outcome}[] }[];
  module_identity?: {upstream_hex:string;upstream_tree_hash:string;comparisons:{backend:string;optimize_lir:boolean;serialized_bytes_equal:boolean;tree_hash_equal:boolean}[]};
  curried_instances?: CurriedInstanceEvidence;
  curried_examples?: CurriedExample[];
  protocol_validation?: ProtocolValidation | DelegatedProtocol;
  validated: boolean;
}
export function singletonProtocol(puzzle: PuzzleReport): ProtocolValidation | undefined {
  if (puzzle.slug !== 'p2-singleton' || !puzzle.protocol_validation) return undefined;
  if (puzzle.protocol_validation.protocol !== 'chialisp-oxide-p2-singleton/v1') throw new Error('Wrong singleton protocol');
  return puzzle.protocol_validation as ProtocolValidation;
}
export function delegatedProtocol(puzzle: PuzzleReport): DelegatedProtocol | undefined {
  if (puzzle.slug !== 'p2-delegated-conditions' || !puzzle.protocol_validation) return undefined;
  if (puzzle.protocol_validation.protocol !== 'chia-consensus-0.36.1/p2-delegated-conditions-v1') throw new Error('Wrong delegated-conditions protocol');
  return puzzle.protocol_validation as DelegatedProtocol;
}
export interface Authoring {
  mode: string;
  package: string;
  manifest: string;
  library: string;
  exports: Record<string, { entry: string; source: string }>;
  sources: { path: string; sha256: string }[];
}

export interface TutorialExample {
  id: string;
  title: string;
  page_id: string;
  implementation_status: string;
  validation_status: string;
  source: { path: string; start_line: number; end_line: number; sha256: string; url: string };
  feasibility: string;
  semantic_gaps: string[];
  notes: string[];
  local_examples: string[];
  evidence: string[];
  variants: { id: string }[];
  source_adaptation?: { slug: string; reference: string; rust_source: string; operation: string };
}

export const report = validation;
export const authoring: Authoring = validation.authoring;
export const puzzles = validation.puzzles as PuzzleReport[];
export const entries = inventory.entries;
export const tutorialExamples: TutorialExample[] = tutorialInventory.examples;
export const canonicalExample = (puzzle: PuzzleReport) => tutorialExamples.find(example =>
  example.implementation_status === 'implemented' && example.validation_status === 'differential_only'
  && example.source_adaptation?.slug === puzzle.slug);
export const canonicalPuzzle = (puzzle: PuzzleReport) => puzzle.provenance.classification === 'canonical_puzzle_port';
export const canonicalHelper = (puzzle: PuzzleReport) => puzzle.provenance.classification === 'canonical_helper_port';
const helperContracts: Record<string, { source: string; adapter: string; generated: string }> = {
  'last-list-element': {
    source: 'The raw wrapper passes the entire environment as the single last_inner argument.',
    adapter: 'a raw module adapter: it passes the complete environment as one argument.',
    generated: 'over bounded atom and pair trees',
  },
  'prefix-list': {
    source: 'The strict wrapper passes exactly two input trees to prefix: the list and an arbitrary tail.',
    adapter: 'a strict two-argument module adapter: it passes the list and arbitrary tail as separate arguments.',
    generated: 'over bounded list spines, arbitrary tail trees and malformed argument lists',
  },
  'atom-not-equal': {
    source: 'The strict wrapper passes exactly two atom arguments to the published != helper.',
    adapter: 'a strict two-atom-argument module adapter: pair operands and malformed argument lists reject.',
    generated: 'over exact atom bytes, lengths, padding and malformed argument lists',
  },
  'insertion-sort-by-keys': {
    source: 'The strict wrapper passes exactly two input trees to insertion_sort_by_keys: the existing sequence and insertions.',
    adapter: 'a strict two-argument module adapter: it preserves key comparisons and permissive base cases.',
    generated: 'over byte keys, duplicate keys, untouched payload trees and malformed sequences',
  },
  'sha256tree': {
    source: 'The raw wrapper passes the entire environment as the single sha256tree argument.',
    adapter: 'a raw module adapter: it hashes the complete environment as one tree without numeric or list decoding.',
    generated: 'over exact atom payloads, arbitrary proper and improper trees, padding and hash-block boundaries',
  },
};
export function helperContract(puzzle: PuzzleReport) {
  if (!canonicalHelper(puzzle)) return undefined;
  const contract = helperContracts[puzzle.slug];
  if (!contract) throw new Error(`Missing helper adapter contract: ${puzzle.slug}`);
  return contract;
}
export const kindLabel = (kind: string) => kind === 'program_factory' ? 'program factory' : kind === 'pure_function' ? 'pure function' : kind === 'coin_puzzle' ? 'coin puzzle' : kind === 'protocol_component' ? 'protocol component' : kind.replaceAll('_', ' ');
export const base = import.meta.env.BASE_URL.replace(/\/$/, '');
export const href = (path: string) => `${base}/${path.replace(/^\//, '')}`;
export const format = (value: number) => new Intl.NumberFormat('en-US').format(value);
export const backendLabel = (backend: string) => ({ rust_mir: 'Rust', chialisp: 'Chialisp', rue_source: 'Rue' })[backend] ?? backend;
export const optimized = (puzzle: PuzzleReport) => puzzle.artifacts.find(a => a.backend === 'rust_mir' && a.optimize_lir)!;
export const acceptedCosts = (puzzle: PuzzleReport) => puzzle.cases.flatMap(c => c.runs.filter(r => r.backend === 'rust_mir' && r.optimize_lir && r.execution.outcome.status === 'accepted' && r.execution.cost !== null).map(r => r.execution.cost!));
const protocolEvidence = puzzles.flatMap(puzzle => { const evidence = singletonProtocol(puzzle); return evidence ? [evidence] : []; });
export const protocolTotals = protocolEvidence.reduce((totals, evidence) => {
  const count = protocolCounts(evidence);
  return { groups: totals.groups + count.groups, attempts: totals.attempts + count.attempts,
    accepted: totals.accepted + count.accepted, rejected: totals.rejected + count.rejected };
}, { groups: 0, attempts: 0, accepted: 0, rejected: 0 });
export const delegatedTotals = puzzles.flatMap(puzzle => { const evidence = delegatedProtocol(puzzle); return evidence ? [evidence] : []; }).reduce((totals,evidence) => ({
  groups: totals.groups + evidence.builds.length,
  attempts: totals.attempts + evidence.transaction_attempts,
  accepted: totals.accepted + evidence.accepted,
  rejected: totals.rejected + evidence.typed_rejected_with_full_rollback,
}), { groups: 0, attempts: 0, accepted: 0, rejected: 0 });
export const totals = {
  puzzles: puzzles.length,
  vectors: puzzles.reduce((sum, p) => sum + p.cases.length, 0),
  generated: puzzles.reduce((sum, p) => sum + p.generated_cases, 0),
  ledger: puzzles.reduce((sum, p) => sum + p.simulator.length, 0),
  ledgerRuns: puzzles.reduce((sum, p) => sum + p.simulator.length + p.simulator_comparisons.reduce((n, run) => n + run.scenarios.length, 0), 0),
  consensusRejections: puzzles.reduce((sum, puzzle) => sum
    + puzzle.simulator.reduce((count, scenario) => count + scenario.rejections.length, 0)
    + puzzle.simulator_comparisons.reduce((count, run) => count
      + run.scenarios.reduce((observations, scenario) => observations + scenario.rejections.length, 0), 0), 0),
  coinPuzzles: puzzles.filter(puzzle => puzzle.kind === 'coin_puzzle').length,
  pureFunctions: puzzles.filter(puzzle => puzzle.kind === 'pure_function').length,
  protocolComponents: puzzles.filter(puzzle => puzzle.kind === 'protocol_component').length,
  programFactories: puzzles.filter(puzzle => puzzle.kind === 'program_factory').length,
  canonicalPorts: puzzles.filter(puzzle => canonicalExample(puzzle)).length,
  canonicalExpressionPorts: puzzles.filter(puzzle => canonicalExample(puzzle) && puzzle.reference_mode === 'expression').length,
  canonicalPuzzlePorts: puzzles.filter(puzzle => canonicalPuzzle(puzzle)).length,
  canonicalHelperPorts: puzzles.filter(puzzle => canonicalHelper(puzzle)).length,
  originalExamples: puzzles.filter(puzzle => !canonicalExample(puzzle) && !canonicalPuzzle(puzzle) && !canonicalHelper(puzzle)).length,
};

if (validation.schema_version !== 1 || puzzles.length === 0 || puzzles.some(p => !p.validated)) {
  throw new Error('Documentation requires a successful, versioned corpus validation report.');
}
