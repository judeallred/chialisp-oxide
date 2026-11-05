/// Local host-tool admission limits, never consensus or puzzle-policy guards.
/// Only the caller selects limits; an imported envelope cannot raise them.
#[derive(Clone, Debug)]
pub struct Limits {
    pub max_blob_bytes: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_atom_bytes: usize,
    pub max_atom_heap_bytes: usize,
    pub max_work: usize,
    pub max_expanded_bytes: usize,
    pub max_envelope_bytes: usize,
    pub max_ir_bytes: usize,
    pub max_json_depth: usize,
    pub max_json_nodes: usize,
    pub max_sources: usize,
    pub max_source_bytes: usize,
    pub max_total_source_bytes: usize,
    pub max_objects: usize,
    pub max_lookup_bytes: usize,
    pub max_source_read_events: usize,
    pub max_source_read_bytes: usize,
    pub max_bundle_files: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_blob_bytes: 2 * 1024 * 1024,
            max_nodes: 262_144,
            max_depth: 4096,
            max_atom_bytes: 2 * 1024 * 1024,
            max_atom_heap_bytes: 4 * 1024 * 1024,
            max_work: 4_194_304,
            max_expanded_bytes: 8 * 1024 * 1024,
            max_envelope_bytes: 16 * 1024 * 1024,
            max_ir_bytes: 8 * 1024 * 1024,
            max_json_depth: 96,
            max_json_nodes: 262_144,
            max_sources: 4096,
            max_source_bytes: 2 * 1024 * 1024,
            max_total_source_bytes: 8 * 1024 * 1024,
            max_objects: 1024,
            max_lookup_bytes: 64 * 1024 * 1024,
            max_source_read_events: 16_384,
            max_source_read_bytes: 64 * 1024 * 1024,
            max_bundle_files: 16_384,
        }
    }
}
