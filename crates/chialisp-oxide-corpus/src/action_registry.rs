//! One complete registry/slot protocol record, produced after all fifteen artifacts are admitted.
//! The caller owns complete Cargo/source admission and bounded sidecar materialization.
//! This producer performs no runtime filesystem access and returns no host origins.
mod adversarial;
mod fixtures;
mod lifecycle;
mod source_controls;
mod wire;
pub use source_controls::{
    RegistryControlBuild, RegistryControlJournal, RegistryControlKind, RegistryControlReferences,
    RegistryControlSide, RegistrySourceControlRecord, validate_source_controls,
};

pub use adversarial::AdversarialJournal;
use anyhow::{Result, ensure};
use chialisp_oxide_simulator::SecretKey;
pub use lifecycle::LifecycleJournal;
use serde::Serialize;
use sha2::{Digest, Sha256};
use wire::{LIMIT, admit, hash};

const ACTIONS: [&str; 3] = ["initialize", "set_alpha", "set_beta"];
const SLUGS: [&str; 3] = [
    "registry-initialize",
    "registry-set-alpha",
    "registry-set-beta",
];
const PRIMARY_COST: u64 = 250_000_000;
// All fifteen module slots share the inherited fixed group ceiling. It is
// intentionally stricter than fifteen independent per-module allowances.
const MAX_MODULE_INVENTORY_BYTES: usize = 9 * LIMIT;
const MODES: [(&str, bool, &str); 5] = [
    ("rust_mir", false, "rust-off"),
    ("rust_mir", true, "rust-on"),
    ("chialisp", true, "chialisp"),
    ("rue_source", false, "rue-off"),
    ("rue_source", true, "rue-on"),
];
const REFERENCES: [&str; 3] = [
    include_str!("../../../puzzles/registry-initialize/reference.clsp"),
    include_str!("../../../puzzles/registry-set-alpha/reference.clsp"),
    include_str!("../../../puzzles/registry-set-beta/reference.clsp"),
];

/// Exactly three selected modules in one independently validated language/build mode.
/// Supply all five ordered builds only after complete all-export bundle admission.
#[derive(Clone, Copy)]
pub struct RegistryBuild<'a> {
    pub backend: &'a str,
    pub optimize_lir: bool,
    pub initialize: &'a [u8],
    pub set_alpha: &'a [u8],
    pub set_beta: &'a [u8],
}
impl<'a> RegistryBuild<'a> {
    fn modules(self) -> [&'a [u8]; 3] {
        [self.initialize, self.set_alpha, self.set_beta]
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryArtifactIdentity {
    pub slug: &'static str,
    pub bytes: usize,
    pub blob_sha256: String,
    pub module_tree_hash: String,
}
#[derive(Debug, Serialize)]
pub struct RegistryCanonicalComponent {
    pub name: &'static str,
    pub module_hex: String,
    pub bytes: usize,
    pub blob_sha256: String,
    pub module_tree_hash: String,
}
#[derive(Debug, Serialize)]
pub struct RegistryReferenceIdentity {
    pub artifact: RegistryArtifactIdentity,
    pub source_sha256: String,
}
#[derive(Debug, Serialize)]
pub struct RegistryPrimaryProfile {
    pub version: &'static str,
    pub failure_protocol: &'static str,
    pub flags: u32,
    pub max_cost: u64,
    pub allocator_limit_bytes: usize,
}
#[derive(Debug, Serialize)]
pub struct RegistrySdkProfile {
    pub simulator: &'static str,
    pub consensus: &'static str,
    pub vm: &'static str,
    pub network: &'static str,
    pub transaction_max_cost: u64,
    pub max_block_cost_clvm: u64,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RegistryOrdinaryCounts {
    pub scenario_runs: usize,
    pub sdk_transaction_attempts: usize,
    pub accepted: usize,
    pub typed_consensus_rejections: usize,
    pub component_primary_observations: usize,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RegistryAdversarialCounts {
    pub scenario_runs: usize,
    pub sdk_transaction_attempts: usize,
    pub accepted: usize,
    pub typed_consensus_rejections: usize,
    pub runtime_umbrellas: usize,
    pub component_primary_observations: usize,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RegistrySetupCounts {
    pub sdk_transaction_attempts: usize,
    pub accepted: usize,
    pub component_primary_observations: usize,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RegistryCausalCounts {
    pub primary_observations: usize,
    pub accepted: usize,
    pub typed_program_rejections: usize,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RegistryLaneCounts {
    pub ordinary: RegistryOrdinaryCounts,
    pub adversarial: RegistryAdversarialCounts,
    pub adversarial_setup: RegistrySetupCounts,
    pub causal_primary: RegistryCausalCounts,
}
impl RegistryLaneCounts {
    fn add(&mut self, other: Self) {
        self.ordinary.scenario_runs += other.ordinary.scenario_runs;
        self.ordinary.sdk_transaction_attempts += other.ordinary.sdk_transaction_attempts;
        self.ordinary.accepted += other.ordinary.accepted;
        self.ordinary.typed_consensus_rejections += other.ordinary.typed_consensus_rejections;
        self.ordinary.component_primary_observations +=
            other.ordinary.component_primary_observations;
        self.adversarial.scenario_runs += other.adversarial.scenario_runs;
        self.adversarial.sdk_transaction_attempts += other.adversarial.sdk_transaction_attempts;
        self.adversarial.accepted += other.adversarial.accepted;
        self.adversarial.typed_consensus_rejections += other.adversarial.typed_consensus_rejections;
        self.adversarial.runtime_umbrellas += other.adversarial.runtime_umbrellas;
        self.adversarial.component_primary_observations +=
            other.adversarial.component_primary_observations;
        self.adversarial_setup.sdk_transaction_attempts +=
            other.adversarial_setup.sdk_transaction_attempts;
        self.adversarial_setup.accepted += other.adversarial_setup.accepted;
        self.adversarial_setup.component_primary_observations +=
            other.adversarial_setup.component_primary_observations;
        self.causal_primary.primary_observations += other.causal_primary.primary_observations;
        self.causal_primary.accepted += other.causal_primary.accepted;
        self.causal_primary.typed_program_rejections +=
            other.causal_primary.typed_program_rejections;
    }
}

#[derive(Debug, Serialize)]
pub struct RegistryProtocolHeader {
    pub schema_version: u32,
    pub protocol: &'static str,
    pub status: &'static str,
    pub primary_vm: RegistryPrimaryProfile,
    pub sdk: RegistrySdkProfile,
    pub owner_public_key: String,
    pub canonical_components: Vec<RegistryCanonicalComponent>,
    pub independently_compiled_chialisp: Vec<RegistryReferenceIdentity>,
    pub totals: RegistryLaneCounts,
    pub source_control_scope: &'static str,
    pub scope: &'static str,
}
#[derive(Debug, Serialize)]
pub struct RegistryBuildRecord {
    pub backend: String,
    pub optimize_lir: bool,
    pub mode: &'static str,
    pub artifacts: [RegistryArtifactIdentity; 3],
    pub counts: RegistryLaneCounts,
    pub ordinary: LifecycleJournal,
    pub adversarial: AdversarialJournal,
}
#[derive(Debug, Serialize)]
pub struct RegistryProtocolRecord {
    pub header: RegistryProtocolHeader,
    pub builds: Vec<RegistryBuildRecord>,
}

fn identity(slug: &'static str, module: &[u8]) -> Result<RegistryArtifactIdentity> {
    Ok(RegistryArtifactIdentity {
        slug,
        bytes: module.len(),
        blob_sha256: hex::encode(Sha256::digest(module)),
        module_tree_hash: hex::encode(hash(module)?),
    })
}
fn canonical_components() -> Result<Vec<RegistryCanonicalComponent>> {
    use chia_sdk_types::puzzles::{ACTION_LAYER_PUZZLE, DEFAULT_FINALIZER_PUZZLE, SLOT_PUZZLE};
    let mut components = Vec::new();
    for (name, module, length, expected) in [
        (
            "action_layer",
            ACTION_LAYER_PUZZLE.as_slice(),
            630,
            "afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b",
        ),
        (
            "default_finalizer",
            DEFAULT_FINALIZER_PUZZLE.as_slice(),
            494,
            "9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b",
        ),
        (
            "singleton_launcher",
            chia_puzzles::SINGLETON_LAUNCHER.as_slice(),
            175,
            "eff07522495060c066f66f32acc2a77e3a3e737aca8baea4d1a64ea4cdc13da9",
        ),
        (
            "singleton_top_layer_v1_1",
            chia_puzzles::SINGLETON_TOP_LAYER_V1_1.as_slice(),
            967,
            "7faa3253bfddd1e0decb0906b2dc6247bbc4cf608f58345d173adb63e8b47c9f",
        ),
        (
            "slot",
            SLOT_PUZZLE.as_slice(),
            311,
            "8ab9f3b57a65d7f7a0810f79f7bc1e96bda680d16dd6f51d51e868e13fc7bbb3",
        ),
        (
            "standard_wallet",
            chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE.as_slice(),
            227,
            "e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52",
        ),
    ] {
        admit(module)?;
        let module_tree_hash = hex::encode(hash(module)?);
        ensure!(
            module.len() == length && module_tree_hash == expected,
            "canonical {name} identity"
        );
        components.push(RegistryCanonicalComponent {
            name,
            module_hex: hex::encode(module),
            bytes: module.len(),
            blob_sha256: hex::encode(Sha256::digest(module)),
            module_tree_hash,
        });
    }
    Ok(components)
}

/// Produce all ordinary/adversarial journals only after the complete fifteen-module inventory.
/// No host paths or sidecar destinations are accepted or emitted here.
pub fn validate(builds: &[RegistryBuild<'_>]) -> Result<RegistryProtocolRecord> {
    ensure!(
        builds.len() == MODES.len(),
        "registry needs exactly five builds"
    );
    let mut remaining = MAX_MODULE_INVENTORY_BYTES;
    for (build, (backend, optimize, _)) in builds.iter().zip(MODES) {
        ensure!(
            (build.backend, build.optimize_lir) == (backend, optimize),
            "registry build order/profile"
        );
        for module in build.modules() {
            ensure!(
                !module.is_empty() && module.len() <= LIMIT && module.len() <= remaining,
                "complete registry module inventory precharge"
            );
            remaining -= module.len();
        }
    }
    // Complete raw tree admission before copying/selecting any build.
    for build in builds {
        for module in build.modules() {
            admit(module)?;
        }
    }
    let mut references = Vec::new();
    for (i, source) in REFERENCES.iter().enumerate() {
        let module = chialisp_oxide_reference::compile(source)?;
        ensure!(
            module.as_slice() == builds[2].modules()[i],
            "independently compiled Chialisp {i} differs"
        );
        references.push(RegistryReferenceIdentity {
            artifact: identity(SLUGS[i], &module)?,
            source_sha256: hex::encode(Sha256::digest(source.as_bytes())),
        });
    }
    let canonical_components = canonical_components()?;
    let owner = SecretKey::from_seed(&fixtures::OWNER_SEED);
    let owner_public_key = hex::encode(owner.public_key().to_bytes());
    ensure!(
        owner_public_key == fixtures::OWNER_PUBLIC_KEY_HEX,
        "public simulator fixture identity"
    );
    let artifacts: Vec<_> = builds
        .iter()
        .zip(MODES)
        .flat_map(|(build, (_, _, mode))| {
            ACTIONS
                .into_iter()
                .zip(build.modules())
                .map(move |(action, module)| (action.to_owned(), mode.to_owned(), module.to_vec()))
        })
        .collect();
    let mut records = Vec::new();
    let mut totals = RegistryLaneCounts::default();
    for (build, (_, _, mode)) in builds.iter().zip(MODES) {
        let (ordinary, ordinary_counts) = lifecycle::run(&artifacts, mode, &owner)?;
        let (adversarial, adversarial_counts, setup_counts, causal_counts) =
            adversarial::run(&artifacts, mode, &owner)?;
        let counts = RegistryLaneCounts {
            ordinary: ordinary_counts,
            adversarial: adversarial_counts,
            adversarial_setup: setup_counts,
            causal_primary: causal_counts,
        };
        totals.add(counts);
        records.push(RegistryBuildRecord {
            backend: build.backend.to_owned(),
            optimize_lir: build.optimize_lir,
            mode,
            artifacts: [
                identity(SLUGS[0], build.initialize)?,
                identity(SLUGS[1], build.set_alpha)?,
                identity(SLUGS[2], build.set_beta)?,
            ],
            counts,
            ordinary,
            adversarial,
        });
    }
    ensure!(
        totals.ordinary.sdk_transaction_attempts == 55
            && totals.ordinary.accepted == 30
            && totals.ordinary.typed_consensus_rejections == 25
            && totals.ordinary.component_primary_observations == 95,
        "complete ordinary inventory"
    );
    ensure!(
        totals.adversarial.sdk_transaction_attempts == 50
            && totals.adversarial.accepted == 10
            && totals.adversarial.typed_consensus_rejections == 15
            && totals.adversarial.runtime_umbrellas == 25
            && totals.adversarial.component_primary_observations == 90,
        "complete adversarial inventory"
    );
    ensure!(
        totals.adversarial_setup.sdk_transaction_attempts == 10
            && totals.adversarial_setup.accepted == 10
            && totals.adversarial_setup.component_primary_observations == 15,
        "complete separate setup inventory"
    );
    ensure!(
        totals.causal_primary.primary_observations == 15
            && totals.causal_primary.accepted == 10
            && totals.causal_primary.typed_program_rejections == 5,
        "complete separate causal primary inventory"
    );
    Ok(RegistryProtocolRecord {
        header: RegistryProtocolHeader {
            schema_version: 1,
            protocol: "chialisp-oxide-action-registry/v1",
            status: "validated",
            primary_vm: RegistryPrimaryProfile {
                version: "clvmr 0.21.0",
                failure_protocol: chialisp_oxide_vm::FAILURE_PROTOCOL,
                flags: clvmr::MEMPOOL_MODE.bits(),
                max_cost: PRIMARY_COST,
                allocator_limit_bytes: 32 * LIMIT,
            },
            sdk: RegistrySdkProfile {
                simulator: "chia-sdk-test 0.36.0",
                consensus: "chia-consensus 0.36.1",
                vm: "clvmr 0.16.2",
                network: "TESTNET11",
                transaction_max_cost: 5_500_000_000,
                max_block_cost_clvm: chia_sdk_types::TESTNET11_CONSTANTS.max_block_cost_clvm,
            },
            owner_public_key,
            canonical_components,
            independently_compiled_chialisp: references,
            totals,
            source_control_scope: "No source controls are included in this base producer. Research Chialisp controls and actual Rust source-mutant observations belong to separate captures and receive no credit from this record.",
            scope: "Original fixed two-name teaching actions, all three selected artifacts in each of five builds. Exact primary outputs and typed SDK rejection/full in-memory Simulator equality. Runtime umbrellas have separate independent primary program failures; resources/infrastructure are inconclusive. Native calls, generic five-mode VM vectors, source controls and this composed SDK record remain distinct. No canonical registry port, BLS/general security/uniqueness/compiler-preservation proof, authenticated execution, production readiness, live Testnet11 or complete issue14 claim.",
        },
        builds: records,
    })
}
