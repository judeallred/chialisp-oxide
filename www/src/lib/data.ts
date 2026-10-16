import validation from '../../../corpus/validation.json';
import inventory from '../../../corpus/inventory.json';

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
  provenance: { classification: string; references: string[] };
  source: string;
  reference: string;
  artifacts: Artifact[];
  cases: { label: string; solution: string; native: Outcome; runs: { backend: string; optimize_lir: boolean; execution: { outcome: Outcome; cost: number | null; error: string | null } }[] }[];
  generated_cases: number;
  simulator: { name: string; passed: boolean; scope: string }[];
  validated: boolean;
}

export const report = validation;
export const puzzles = validation.puzzles as PuzzleReport[];
export const entries = inventory.entries;
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
};

if (validation.schema_version !== 1 || puzzles.length === 0 || puzzles.some(p => !p.validated)) {
  throw new Error('Documentation requires a successful, versioned corpus validation report.');
}
