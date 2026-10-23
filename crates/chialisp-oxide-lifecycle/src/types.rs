use chia_protocol::{Bytes32, Coin};
use serde::{Deserialize, Serialize};

pub type Digest = [u8; 32];

/// Explicit testnet identity supplied by the eventual node/wallet adapter.
/// A label and challenge are declarations, not chain authentication.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub testnet: String,
    pub genesis_challenge: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoinRecord {
    pub parent: Digest,
    pub puzzle_hash: Digest,
    pub amount: u64,
}

impl CoinRecord {
    pub fn coin(&self) -> Coin {
        Coin::new(
            Bytes32::new(self.parent),
            Bytes32::new(self.puzzle_hash),
            self.amount,
        )
    }

    /// Delegate canonical amount spelling and hashing to the pinned SDK.
    pub fn id(&self) -> Digest {
        self.coin().coin_id().to_bytes()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub puzzle_hash: Digest,
    pub amount: u64,
}

/// One input, complete outputs, implicit fee. Multi-input wallet operations
/// and change selection belong to a later separately validated adapter.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub network: Network,
    pub amount: u64,
    pub outputs: Vec<Output>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub reveal: Vec<u8>,
    pub solution: Vec<u8>,
}

/// The eventual executor supplies its interpretation and full public receipts.
/// Core verifies bindings/output equality, not execution or receipt authenticity.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRecord {
    pub artifact_digest: Digest,
    pub observed_outputs: Vec<Output>,
    pub native_receipt: Vec<u8>,
    pub clvm_receipt: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tip {
    pub height: u32,
    pub header_hash: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputState {
    Missing,
    Unspent {
        created_height: u32,
    },
    Spent {
        created_height: u32,
        spent_height: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChildObservation {
    pub coin: CoinRecord,
    pub created_height: u32,
}

/// A complete current observation for exactly one coin. No partial child list
/// can confirm a spend. The adapter owns chain/provider verification and must
/// serialize observations; the core cannot order independently queried tips.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub network: Network,
    pub tip: Tip,
    pub input_id: Digest,
    pub input: InputState,
    pub children: Vec<ChildObservation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignRequest {
    pub protocol: String,
    pub network: Network,
    pub artifact_digest: Digest,
    pub input: CoinRecord,
    pub unsigned_bundle: Vec<u8>,
    pub expected_children: Vec<CoinRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignReceipt {
    pub request_id: Digest,
    pub signed_bundle: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureValidation {
    pub request_id: Digest,
    pub signed_bundle_digest: Digest,
    /// Public executor receipt, not secret key material. Recording it is an
    /// explicit trusted-adapter assertion, not verification by this crate.
    pub receipt: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Built {
        artifact: Artifact,
    },
    Checked {
        record: CheckRecord,
    },
    Funded {
        coin: CoinRecord,
        observation: Observation,
    },
    SignatureReturned {
        receipt: SignReceipt,
    },
    SignatureValidated {
        validation: SignatureValidation,
    },
    Submitted {
        request_id: Digest,
        signed_bundle_digest: Digest,
        public_receipt: Vec<u8>,
    },
    Observed {
        observation: Observation,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    AwaitingBuild,
    AwaitingCheck,
    AwaitingFunding,
    AwaitingSignature,
    AwaitingSignatureValidation,
    ReadyToSubmit,
    AwaitingConfirmation,
    Confirmed,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub revision: u32,
    pub event: Event,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireJournal {
    pub protocol: String,
    pub plan: Plan,
    pub entries: Vec<Entry>,
}
