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
export interface CaseReport {
  label: string;
  solution: string;
  native: Outcome;
  runs: {
    backend: string;
    optimize_lir: boolean;
    execution: Execution;
  }[];
}
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
  factory_checks?: { backend:string; optimize_lir:boolean; environment:string; expected_output_hex:string; program_hex:string; application_runs:{solution:string;execution:{outcome:Outcome;cost:number|null;error:string|null}}[] }[];
  module_identity?: {upstream_hex:string;upstream_tree_hash:string;comparisons:{backend:string;optimize_lir:boolean;serialized_bytes_equal:boolean;tree_hash_equal:boolean}[]};
  curried_instances?: CurriedInstanceEvidence;
  validated: boolean;
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
export const kindLabel = (kind: string) => kind === 'program_factory' ? 'program factory' : kind === 'pure_function' ? 'pure function' : kind === 'coin_puzzle' ? 'coin puzzle' : kind.replaceAll('_', ' ');
export const base = import.meta.env.BASE_URL.replace(/\/$/, '');
export const href = (path: string) => `${base}/${path.replace(/^\//, '')}`;
export const format = (value: number) => new Intl.NumberFormat('en-US').format(value);
export const backendLabel = (backend: string) => ({ rust_mir: 'Rust', chialisp: 'Chialisp', rue_source: 'Rue' })[backend] ?? backend;
export const optimized = (puzzle: PuzzleReport) => puzzle.artifacts.find(a => a.backend === 'rust_mir' && a.optimize_lir)!;
export const acceptedCosts = (puzzle: PuzzleReport) => puzzle.cases.flatMap(c => c.runs.filter(r => r.backend === 'rust_mir' && r.optimize_lir && r.execution.outcome.status === 'accepted' && r.execution.cost !== null).map(r => r.execution.cost!));
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
