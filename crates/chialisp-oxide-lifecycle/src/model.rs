use crate::types::*;
use anyhow::{Context, Result, ensure};
use chia_protocol::{CoinSpend, Program, SpendBundle};
use chia_traits::Streamable;
use chialisp_oxide_package::{
    Limits,
    artifact::{Encoding, VerifiedArtifact, inspect_artifact},
};
use sha2::{Digest as _, Sha256};

const PROTOCOL: &str = "chialisp-oxide-lifecycle-journal/v1";
const SIGN_PROTOCOL: &str = "chialisp-oxide-lifecycle-sign/v1";
const MAX_JOURNAL: usize = 16 * 1024 * 1024;
const MAX_BLOB: usize = 1024 * 1024;
const MAX_RECEIPT: usize = 64 * 1024;
const MAX_OUTPUTS: usize = 256;
const MAX_EVENTS: usize = 1024;

pub fn digest(bytes: &[u8]) -> Digest {
    Sha256::digest(bytes).into()
}

pub fn artifact_digest(artifact: &Artifact) -> Result<Digest> {
    artifact.validate()?;
    Ok(digest(&serde_json::to_vec(&(SIGN_PROTOCOL, artifact))?))
}

fn public_receipt(bytes: &[u8]) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_RECEIPT,
        "public receipt byte bound"
    );
    Ok(())
}

fn inspect(bytes: &[u8]) -> Result<VerifiedArtifact> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_BLOB,
        "CLVM object byte bound"
    );
    let limits = Limits {
        max_blob_bytes: MAX_BLOB,
        max_nodes: 65_536,
        max_depth: 512,
        max_atom_bytes: MAX_BLOB,
        max_atom_heap_bytes: MAX_BLOB * 2,
        max_work: MAX_BLOB * 16,
        max_expanded_bytes: MAX_BLOB,
        ..Limits::default()
    };
    Ok(inspect_artifact(bytes, Encoding::Plain, &limits)?)
}

fn program(bytes: &[u8]) -> Result<Program> {
    let verified = inspect(bytes)?;
    Ok(Program::from(verified.bytes()))
}

impl Artifact {
    fn validate(&self) -> Result<()> {
        inspect(&self.reveal)?;
        inspect(&self.solution)?;
        Ok(())
    }

    pub fn puzzle_hash(&self) -> Result<Digest> {
        let reveal = inspect(&self.reveal)?;
        inspect(&self.solution)?;
        Ok(*reveal.identity().module_tree_hash.as_bytes())
    }
}

impl Plan {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.network.testnet == "testnet11" && self.network.genesis_challenge != [0; 32],
            "explicit testnet11 identity required"
        );
        ensure!(
            self.amount > 0 && self.outputs.len() <= MAX_OUTPUTS,
            "plan amount/output bound"
        );
        let mut outputs = self
            .outputs
            .iter()
            .map(|output| (output.puzzle_hash, output.amount))
            .collect::<Vec<_>>();
        outputs.sort();
        ensure!(
            outputs.windows(2).all(|pair| pair[0] != pair[1]),
            "duplicate output coin"
        );
        let total = self
            .outputs
            .iter()
            .try_fold(0u64, |sum, output| sum.checked_add(output.amount));
        ensure!(
            total.is_some_and(|sum| sum <= self.amount),
            "output amount overflow or minting"
        );
        Ok(())
    }
}

impl SignRequest {
    pub fn id(&self) -> Result<Digest> {
        ensure!(
            self.protocol == SIGN_PROTOCOL
                && self.unsigned_bundle.len() <= MAX_BLOB * 3
                && self.expected_children.len() <= MAX_OUTPUTS
                && self.network.testnet.len() <= 32,
            "signing request bound"
        );
        Ok(digest(&serde_json::to_vec(self)?))
    }
}

#[derive(Clone)]
struct State {
    stage: Stage,
    artifact: Option<Artifact>,
    funding: Option<CoinRecord>,
    signature: Option<SignReceipt>,
    validation: Option<SignatureValidation>,
    observation: Option<Observation>,
}

impl State {
    fn new() -> Self {
        Self {
            stage: Stage::AwaitingBuild,
            artifact: None,
            funding: None,
            signature: None,
            validation: None,
            observation: None,
        }
    }

    fn request(&self, plan: &Plan) -> Result<SignRequest> {
        let artifact = self.artifact.as_ref().context("artifact missing")?;
        let coin = self.funding.as_ref().context("funding missing")?;
        let spend = CoinSpend::new(
            coin.coin(),
            program(&artifact.reveal)?,
            program(&artifact.solution)?,
        );
        let unsigned_bundle = SpendBundle::new(vec![spend], Default::default()).to_bytes()?;
        let mut expected_children = plan
            .outputs
            .iter()
            .map(|output| CoinRecord {
                parent: coin.id(),
                puzzle_hash: output.puzzle_hash,
                amount: output.amount,
            })
            .collect::<Vec<_>>();
        expected_children.sort();
        Ok(SignRequest {
            protocol: SIGN_PROTOCOL.into(),
            network: plan.network.clone(),
            artifact_digest: artifact_digest(artifact)?,
            input: coin.clone(),
            unsigned_bundle,
            expected_children,
        })
    }

    fn apply(&mut self, plan: &Plan, event: &Event) -> Result<()> {
        match event {
            Event::Built { artifact } => {
                ensure!(
                    self.stage == Stage::AwaitingBuild,
                    "build is immutable once recorded"
                );
                artifact.validate()?;
                self.artifact = Some(artifact.clone());
                self.stage = Stage::AwaitingCheck;
            }
            Event::Checked { record } => {
                ensure!(self.stage == Stage::AwaitingCheck, "check requires a build");
                ensure!(
                    record.artifact_digest
                        == artifact_digest(self.artifact.as_ref().context("artifact missing")?)?,
                    "check binds a different artifact"
                );
                ensure!(
                    record.observed_outputs == plan.outputs,
                    "check outputs differ from complete plan outputs"
                );
                public_receipt(&record.native_receipt)?;
                public_receipt(&record.clvm_receipt)?;
                self.stage = Stage::AwaitingFunding;
            }
            Event::Funded { coin, observation } => {
                ensure!(
                    self.stage == Stage::AwaitingFunding,
                    "funding requires checked artifact"
                );
                ensure!(
                    coin.amount == plan.amount
                        && coin.puzzle_hash
                            == self
                                .artifact
                                .as_ref()
                                .context("artifact missing")?
                                .puzzle_hash()?,
                    "funding coin does not match actual artifact/amount"
                );
                ensure!(
                    matches!(observation.input, InputState::Unspent { .. }),
                    "funding requires observed unspent coin"
                );
                self.funding = Some(coin.clone());
                self.observe(plan, observation)?;
            }
            Event::SignatureReturned { receipt } => {
                ensure!(
                    matches!(
                        self.stage,
                        Stage::AwaitingSignature | Stage::AwaitingSignatureValidation
                    ),
                    "signature receipt out of order"
                );
                let request = self.request(plan)?;
                ensure!(
                    receipt.request_id == request.id()?,
                    "signature request mismatch"
                );
                ensure!(
                    !receipt.signed_bundle.is_empty()
                        && receipt.signed_bundle.len() <= MAX_BLOB * 3,
                    "signed bundle byte bound"
                );
                // Pinned chia-protocol 0.36.1 streams coin_spends followed by
                // one fixed 96-byte G2Element. Compare our own complete prefix
                // before decoding anything supplied by the signer: no claimed
                // vector/program lengths can cause attacker-selected allocation.
                let prefix = request
                    .unsigned_bundle
                    .len()
                    .checked_sub(96)
                    .context("unsigned bundle lacks signature")?;
                ensure!(
                    receipt.signed_bundle.len() == request.unsigned_bundle.len()
                        && receipt.signed_bundle[..prefix] == request.unsigned_bundle[..prefix],
                    "external signer changed exact coin spends/framing"
                );
                self.signature = Some(receipt.clone());
                self.validation = None;
                self.stage = Stage::AwaitingSignatureValidation;
            }
            Event::SignatureValidated { validation } => {
                ensure!(
                    self.stage == Stage::AwaitingSignatureValidation,
                    "signature validation out of order"
                );
                self.match_signed(plan, validation.request_id, validation.signed_bundle_digest)?;
                public_receipt(&validation.receipt)?;
                let signed = &self
                    .signature
                    .as_ref()
                    .context("signature missing")?
                    .signed_bundle;
                let signature =
                    chia_bls::Signature::from_bytes(signed[signed.len() - 96..].try_into()?)?;
                ensure!(
                    signature.to_bytes().as_slice() == &signed[signed.len() - 96..],
                    "noncanonical signature point"
                );
                self.validation = Some(validation.clone());
                self.stage = Stage::ReadyToSubmit;
            }
            Event::Submitted {
                request_id,
                signed_bundle_digest,
                public_receipt: receipt,
            } => {
                ensure!(
                    self.stage == Stage::ReadyToSubmit && self.validation.is_some(),
                    "submission requires explicit matching external validation"
                );
                self.match_signed(plan, *request_id, *signed_bundle_digest)?;
                public_receipt(receipt)?;
                self.stage = Stage::AwaitingConfirmation;
            }
            Event::Observed { observation } => self.observe(plan, observation)?,
        }
        Ok(())
    }

    fn match_signed(&self, plan: &Plan, request_id: Digest, bundle_digest: Digest) -> Result<()> {
        ensure!(
            request_id == self.request(plan)?.id()?,
            "request binding mismatch"
        );
        ensure!(
            self.signature
                .as_ref()
                .is_some_and(|receipt| digest(&receipt.signed_bundle) == bundle_digest),
            "signed bundle binding mismatch"
        );
        Ok(())
    }

    fn observe(&mut self, plan: &Plan, observation: &Observation) -> Result<()> {
        let coin = self
            .funding
            .as_ref()
            .context("observation requires selected funding coin")?;
        ensure!(
            observation.network == plan.network && observation.input_id == coin.id(),
            "observation network/input mismatch"
        );
        ensure!(
            observation.children.len() <= MAX_OUTPUTS,
            "observed child count bound"
        );
        match observation.input {
            InputState::Missing => {
                ensure!(observation.children.is_empty(), "missing coin has children");
                self.funding = None;
                self.signature = None;
                self.validation = None;
                self.stage = Stage::AwaitingFunding;
            }
            InputState::Unspent { created_height } => {
                ensure!(
                    created_height <= observation.tip.height && observation.children.is_empty(),
                    "inconsistent unspent observation"
                );
                self.stage = if self.validation.is_some() {
                    Stage::ReadyToSubmit
                } else if self.signature.is_some() {
                    Stage::AwaitingSignatureValidation
                } else {
                    Stage::AwaitingSignature
                };
            }
            InputState::Spent {
                created_height,
                spent_height,
            } => {
                ensure!(
                    created_height <= spent_height && spent_height <= observation.tip.height,
                    "inconsistent spent heights"
                );
                let mut observed = observation
                    .children
                    .iter()
                    .map(|child| child.coin.clone())
                    .collect::<Vec<_>>();
                observed.sort();
                ensure!(
                    observed.windows(2).all(|pair| pair[0] != pair[1]),
                    "duplicate observed child"
                );
                ensure!(
                    observation
                        .children
                        .iter()
                        .all(|child| child.coin.parent == coin.id()
                            && child.created_height == spent_height),
                    "child parent/creation height mismatch"
                );
                self.stage = if observed == self.request(plan)?.expected_children {
                    Stage::Confirmed
                } else {
                    Stage::Conflict
                };
            }
        }
        self.observation = Some(observation.clone());
        Ok(())
    }
}

/// Append-only public record; state is always reconstructed from validated
/// events, never trusted from a serialized cached stage. Persistence is owned
/// by the caller. A revision precondition catches stale CLI/GUI updates.
pub struct Journal {
    wire: WireJournal,
    state: State,
    charged_bytes: usize,
}

impl Journal {
    pub fn new(plan: Plan) -> Result<Self> {
        plan.validate()?;
        let wire = WireJournal {
            protocol: PROTOCOL.into(),
            plan,
            entries: vec![],
        };
        let charged_bytes = serde_json::to_vec(&wire)?.len();
        Ok(Self {
            wire,
            state: State::new(),
            charged_bytes,
        })
    }

    pub fn plan(&self) -> &Plan {
        &self.wire.plan
    }
    pub fn stage(&self) -> Stage {
        self.state.stage
    }
    pub fn revision(&self) -> u32 {
        self.wire.entries.len() as u32
    }
    pub fn entries(&self) -> &[Entry] {
        &self.wire.entries
    }
    pub fn last_observation(&self) -> Option<&Observation> {
        self.state.observation.as_ref()
    }
    pub fn signing_request(&self) -> Result<SignRequest> {
        self.state.request(&self.wire.plan)
    }

    /// Only explicitly validated bytes are offered to an eventual submitter.
    pub fn submission_bytes(&self) -> Result<&[u8]> {
        ensure!(
            self.state.stage == Stage::ReadyToSubmit && self.state.validation.is_some(),
            "not ready for submission"
        );
        Ok(&self
            .state
            .signature
            .as_ref()
            .context("signature receipt missing")?
            .signed_bundle)
    }

    pub fn append(&mut self, expected_revision: u32, event: Event) -> Result<()> {
        ensure!(
            expected_revision == self.revision(),
            "stale journal revision"
        );
        ensure!(self.wire.entries.len() < MAX_EVENTS, "journal event bound");
        // Check individual input bounds before state/entry cloning. The JSON
        // charge is retained across replay and precedes journal growth.
        match &event {
            Event::Built { artifact } => artifact.validate()?,
            Event::Checked { record } => {
                ensure!(
                    record.observed_outputs.len() <= MAX_OUTPUTS,
                    "check output bound"
                );
                public_receipt(&record.native_receipt)?;
                public_receipt(&record.clvm_receipt)?;
            }
            Event::SignatureReturned { receipt } => ensure!(
                receipt.signed_bundle.len() <= MAX_BLOB * 3,
                "signed bundle byte bound"
            ),
            Event::SignatureValidated { validation } => public_receipt(&validation.receipt)?,
            Event::Submitted {
                public_receipt: receipt,
                ..
            } => public_receipt(receipt)?,
            Event::Funded { observation, .. } | Event::Observed { observation } => {
                ensure!(
                    observation.children.len() <= MAX_OUTPUTS
                        && observation.network.testnet.len() <= 32,
                    "observation bound"
                );
            }
        }
        let entry = Entry {
            revision: expected_revision + 1,
            event,
        };
        let next_bytes = self
            .charged_bytes
            .checked_add(serde_json::to_vec(&entry)?.len() + 1)
            .context("journal size overflow")?;
        ensure!(next_bytes <= MAX_JOURNAL, "journal byte bound");
        let mut candidate = self.state.clone();
        candidate.apply(&self.wire.plan, &entry.event)?;
        self.wire.entries.push(entry);
        self.state = candidate;
        self.charged_bytes = next_bytes;
        Ok(())
    }

    pub fn to_json(&self) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec(&self.wire)?;
        ensure!(bytes.len() <= MAX_JOURNAL, "journal byte bound");
        Ok(bytes)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_JOURNAL, "journal byte bound");
        let wire: WireJournal = serde_json::from_slice(bytes)?;
        ensure!(
            wire.protocol == PROTOCOL && wire.entries.len() <= MAX_EVENTS,
            "journal protocol/event bound"
        );
        let mut journal = Self::new(wire.plan)?;
        for entry in wire.entries {
            ensure!(
                entry.revision == journal.revision() + 1,
                "noncontiguous journal revision"
            );
            journal.append(journal.revision(), entry.event)?;
        }
        Ok(journal)
    }
}
