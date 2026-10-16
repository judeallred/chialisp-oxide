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
export interface PuzzleReport {
  slug: string;
  title: string;
  kind: string;
  security_scope: string;
  provenance: { classification: string; references: string[]; example_id?: string };
  environment?: 'ignored';
  reference_mode?: 'expression';
  source: string;
  source_dependencies: { path: string; sha256: string; source: string }[];
  reference: string;
  artifacts: Artifact[];
  cases: { label: string; solution: string; native: Outcome; runs: { backend: string; optimize_lir: boolean; execution: { outcome: Outcome; cost: number | null; error: string | null } }[] }[];
  generated_cases: number;
  simulator: { name: string; passed: boolean; scope: string }[];
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
  source_adaptation?: { slug: string; reference: string; rust_source: string };
}

export const report = validation;
export const authoring: Authoring = validation.authoring;
export const puzzles = validation.puzzles as PuzzleReport[];
export const entries = inventory.entries;
export const tutorialExamples: TutorialExample[] = tutorialInventory.examples;
export const canonicalExample = (puzzle: PuzzleReport) => tutorialExamples.find(example =>
  example.implementation_status === 'implemented' && example.validation_status === 'differential_only'
  && example.source_adaptation?.slug === puzzle.slug);
export const base = import.meta.env.BASE_URL.replace(/\/$/, '');
export const href = (path: string) => `${base}/${path.replace(/^\//, '')}`;
export const format = (value: number) => new Intl.NumberFormat('en-US').format(value);
export const optimized = (puzzle: PuzzleReport) => puzzle.artifacts.find(a => a.backend === 'rust_mir' && a.optimize_lir)!;
export const acceptedCosts = (puzzle: PuzzleReport) => puzzle.cases.flatMap(c => c.runs.filter(r => r.backend === 'rust_mir' && r.optimize_lir && r.execution.outcome.status === 'accepted' && r.execution.cost !== null).map(r => r.execution.cost!));
export const totals = {
  puzzles: puzzles.length,
  vectors: puzzles.reduce((sum, p) => sum + p.cases.length, 0),
  generated: puzzles.reduce((sum, p) => sum + p.generated_cases, 0),
  ledger: puzzles.reduce((sum, p) => sum + p.simulator.length, 0),
  coinPuzzles: puzzles.filter(puzzle => puzzle.kind === 'coin_puzzle').length,
  pureFunctions: puzzles.filter(puzzle => puzzle.kind === 'pure_function').length,
  canonicalPorts: puzzles.filter(puzzle => canonicalExample(puzzle)).length,
  originalExamples: puzzles.filter(puzzle => !canonicalExample(puzzle)).length,
};

if (validation.schema_version !== 1 || puzzles.length === 0 || puzzles.some(p => !p.validated)) {
  throw new Error('Documentation requires a successful, versioned corpus validation report.');
}
