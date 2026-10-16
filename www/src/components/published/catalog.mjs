export const pageSize = 24;

/** @type {Record<string, string>} */
export const languageLabels = {
  chialisp: 'Chialisp', clvm: 'CLVM assembly', clvm_hex: 'Serialized CLVM / hex',
  rue: 'Rue', rust: 'Rust',
};

export const roleOptions = [
  { value: 'programs', label: 'Puzzle / program source', kinds: ['consensus_program_source', 'puzzle_source', 'puzzle_or_program_source', 'rust_to_clvm_puzzle_source'] },
  { value: 'examples', label: 'Language / puzzle examples', kinds: ['language_example', 'puzzle_or_language_example'] },
  { value: 'libraries', label: 'Source libraries', kinds: ['chialisp_library', 'rue_library'] },
  { value: 'builders', label: 'Rust host puzzle builders', kinds: ['rust_puzzle_builder'] },
  { value: 'bindings', label: 'Rust types / compiled bindings', kinds: ['rust_puzzle_types_and_bindings', 'rust_compiled_puzzle_bindings'] },
  { value: 'host', label: 'Rust host / tool modules', kinds: ['rust_host_module'] },
  { value: 'tests', label: 'Tests, fuzzers, and test data', kinds: ['language_or_puzzle_test', 'rust_test_or_fuzzer', 'test_data'] },
  { value: 'compiled', label: 'Compiled program artifacts', kinds: ['compiled_clvm_artifact', 'compiled_test_artifact'] },
];

/** @type {Record<string, string>} */
const artifactLabels = {
  consensus_program_source: 'Consensus program source',
  puzzle_source: 'Puzzle source',
  puzzle_or_program_source: 'Puzzle / program source',
  rust_to_clvm_puzzle_source: 'Rust → CLVM source',
  language_example: 'Language example',
  puzzle_or_language_example: 'Puzzle / language example',
  chialisp_library: 'Chialisp library', rue_library: 'Rue library',
  rust_puzzle_builder: 'Rust host puzzle builder',
  rust_puzzle_types_and_bindings: 'Rust types / puzzle bindings',
  rust_compiled_puzzle_bindings: 'Rust compiled-puzzle bindings',
  rust_host_module: 'Rust host / tool module',
  language_or_puzzle_test: 'Language / puzzle test', rust_test_or_fuzzer: 'Rust test / fuzzer',
  test_data: 'Test data', compiled_clvm_artifact: 'Compiled CLVM artifact', compiled_test_artifact: 'Compiled test artifact',
};

export const artifactLabel = kind => artifactLabels[kind] ?? kind.replaceAll('_', ' ');

/**
 * @typedef {Object} PublishedSource
 * @property {string} id
 * @property {string} repository
 * @property {string} revision
 * @property {string} path
 * @property {string} language
 * @property {string} classification
 * @property {string} artifact_kind
 * @property {string} git_blob_sha1
 * @property {number} size_bytes
 * @property {string} source_url
 * @property {string} license
 */

/**
 * Filter whole records, then paginate; never discard matches before filtering.
 * @param {PublishedSource[]} sources
 * @param {{query?:string,language?:string,role?:string,repository?:string}} filters
 * @param {number} requestedPage
 */
export function selectSources(sources, filters = {}, requestedPage = 1) {
  const { query = '', language = 'all', role = 'programs', repository = 'all' } = filters;
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const kinds = roleOptions.find(option => option.value === role)?.kinds ?? [];
  const matches = sources.filter(source =>
    (language === 'all' || source.language === language)
    && (role === 'all' || kinds.includes(source.artifact_kind))
    && (repository === 'all' || source.repository === repository)
    && terms.every(term => `${source.repository} ${source.path} ${source.language} ${source.classification} ${source.artifact_kind} ${artifactLabel(source.artifact_kind)} ${source.license}`.toLowerCase().includes(term)));
  const pages = Math.max(1, Math.ceil(matches.length / pageSize));
  const page = Number.isFinite(requestedPage) ? Math.min(pages, Math.max(1, Math.trunc(requestedPage))) : 1;
  return {
    total: matches.length, pages, page,
    start: matches.length ? (page - 1) * pageSize + 1 : 0,
    end: Math.min(page * pageSize, matches.length),
    entries: matches.slice((page - 1) * pageSize, page * pageSize),
  };
}
