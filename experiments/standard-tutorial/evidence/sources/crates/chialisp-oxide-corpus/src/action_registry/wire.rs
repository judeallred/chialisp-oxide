//! Bounded independent raw wire construction, checked against pinned SDK schemas.
use anyhow::{Result, ensure};
use chia_puzzle_types::Proof;
use chia_puzzle_types::singleton::{SingletonArgs, SingletonSolution};
use chia_sdk_types::{
    MerkleProof, MerkleTree,
    puzzles::{
        ACTION_LAYER_PUZZLE, ACTION_LAYER_PUZZLE_HASH, ActionLayerArgs, CompactLineageProof,
        DEFAULT_FINALIZER_PUZZLE, DefaultFinalizer1stCurryArgs, DefaultFinalizer2ndCurryArgs,
        RawActionLayerSolution, SLOT_PUZZLE, Slot1stCurryArgs, Slot2ndCurryArgs, SlotSolution,
    },
};
use chialisp_oxide_simulator::{Bytes32, puzzle_hash};
use clvm_traits::ToClvm;
use clvm_utils::CurriedProgram;
use clvmr_sdk::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes_limit},
};
use sha2::{Digest, Sha256};

pub const LIMIT: usize = 1024 * 1024;
pub fn admit(raw: &[u8]) -> Result<()> {
    chialisp_oxide_package::artifact::inspect_artifact(
        raw,
        chialisp_oxide_package::artifact::Encoding::Plain,
        &chialisp_oxide_package::Limits {
            max_blob_bytes: LIMIT,
            max_expanded_bytes: LIMIT,
            max_nodes: 65536,
            max_depth: 512,
            max_atom_bytes: LIMIT,
            max_atom_heap_bytes: 2 * LIMIT,
            max_work: 16 * LIMIT,
            ..Default::default()
        },
    )?;
    Ok(())
}
pub fn hash(raw: &[u8]) -> Result<Bytes32> {
    admit(raw)?;
    puzzle_hash(raw)
}
pub fn sha(parts: &[&[u8]]) -> Bytes32 {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    Bytes32::new(h.finalize().into())
}
pub fn atom(raw: &[u8]) -> Result<Vec<u8>> {
    ensure!(raw.len() < LIMIT - 8, "bounded atom before copy");
    let n = raw.len();
    let mut out = Vec::with_capacity(n + 8);
    if n == 1 && raw[0] < 128 {
    } else if n < 64 {
        out.push(128 + n as u8);
    } else if n < 8192 {
        out.extend([192 + (n >> 8) as u8, n as u8]);
    } else if n < 1048576 {
        out.extend([224 + (n >> 16) as u8, (n >> 8) as u8, n as u8]);
    } else {
        unreachable!()
    }
    out.extend(raw);
    Ok(out)
}
pub fn integer(n: u64) -> Result<Vec<u8>> {
    if n == 0 {
        return atom(&[]);
    }
    let bytes = n.to_be_bytes();
    let first = bytes.iter().position(|b| *b != 0).unwrap();
    let mut data = Vec::with_capacity(9);
    if bytes[first] & 128 != 0 {
        data.push(0);
    }
    data.extend(&bytes[first..]);
    atom(&data)
}
pub fn pair(a: &[u8], b: &[u8]) -> Result<Vec<u8>> {
    let n = a.len().checked_add(b.len()).and_then(|n| n.checked_add(1));
    ensure!(
        n.is_some_and(|n| n <= LIMIT),
        "pair expanded bytes before allocation"
    );
    let mut out = Vec::with_capacity(n.unwrap());
    out.push(255);
    out.extend(a);
    out.extend(b);
    Ok(out)
}
pub fn list(values: &[Vec<u8>]) -> Result<Vec<u8>> {
    ensure!(values.len() <= 64, "bounded fixed host list");
    let mut rest = atom(&[])?;
    for v in values.iter().rev() {
        rest = pair(v, &rest)?;
    }
    Ok(rest)
}
pub fn curry(program: &[u8], args: &[Vec<u8>]) -> Result<Vec<u8>> {
    ensure!(args.len() <= 8, "bounded curry arity");
    admit(program)?;
    let mut environment = integer(1)?;
    for arg in args.iter().rev() {
        admit(arg)?;
        environment = list(&[integer(4)?, pair(&integer(1)?, arg)?, environment])?;
    }
    let raw = list(&[integer(2)?, pair(&integer(1)?, program)?, environment])?;
    admit(&raw)?;
    Ok(raw)
}
pub fn allocator() -> Allocator {
    Allocator::new_limited(32 * 1024 * 1024)
}
pub fn encode<T: ToClvm<Allocator>>(a: &mut Allocator, t: &T) -> Result<Vec<u8>> {
    let p = t.to_clvm(a)?;
    let raw = node_to_bytes_limit(a, p, LIMIT)?;
    admit(&raw)?;
    Ok(raw)
}

pub const NONCE: u64 = 0x434f5852454731;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub initialized: bool,
    pub alpha: Bytes32,
    pub beta: Bytes32,
}
pub fn entry_hash(side: usize, value: &[u8; 32]) -> Bytes32 {
    let name = sha(&[if side == 0 {
        b"chialisp-oxide/registry/v1/alpha"
    } else {
        b"chialisp-oxide/registry/v1/beta"
    }]);
    let left = sha(&[&[1], name.as_ref()]);
    let right = sha(&[&[1], value]);
    sha(&[&[2], left.as_ref(), right.as_ref()])
}
impl State {
    pub fn initial() -> Self {
        Self {
            initialized: false,
            alpha: entry_hash(0, &[0; 32]),
            beta: entry_hash(1, &[0; 32]),
        }
    }
    pub fn encode(self) -> Result<Vec<u8>> {
        pair(
            &integer(u64::from(self.initialized))?,
            &pair(&atom(self.alpha.as_ref())?, &atom(self.beta.as_ref())?)?,
        )
    }
    pub fn set(self, side: usize, value: &[u8; 32]) -> Self {
        Self {
            initialized: true,
            alpha: if side == 0 {
                entry_hash(0, value)
            } else {
                self.alpha
            },
            beta: if side == 1 {
                entry_hash(1, value)
            } else {
                self.beta
            },
        }
    }
}
#[derive(Clone)]
pub struct Policy {
    pub actions: [Vec<u8>; 3],
    pub root: Bytes32,
    pub proofs: [MerkleProof; 3],
    pub finalizer: Vec<u8>,
    pub launcher: Bytes32,
    pub slot_first: Vec<u8>,
    pub slot_first_hash: Bytes32,
}
impl Policy {
    pub fn new(templates: [&[u8]; 3], owner: &[u8; 48], launcher: Bytes32) -> Result<Self> {
        for template in templates {
            admit(template)?;
        }
        ensure!(
            SLOT_PUZZLE.len() == 311
                && hex::encode(hash(&SLOT_PUZZLE)?)
                    == "8ab9f3b57a65d7f7a0810f79f7bc1e96bda680d16dd6f51d51e868e13fc7bbb3",
            "canonical SLOT identity"
        );
        let ss = pair(
            &atom(&chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH)?,
            &pair(
                &atom(launcher.as_ref())?,
                &atom(&chia_puzzles::SINGLETON_LAUNCHER_HASH)?,
            )?,
        )?;
        let packed = pair(
            &atom(&chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH)?,
            &atom(hash(&ss)?.as_ref())?,
        )?;
        let slot_first = curry(&SLOT_PUZZLE, &[packed, integer(NONCE)?])?;
        let mut a = allocator();
        let p = node_from_bytes(&mut a, &SLOT_PUZZLE)?;
        ensure!(
            slot_first
                == encode(
                    &mut a,
                    &CurriedProgram {
                        program: p,
                        args: Slot1stCurryArgs::new(launcher, NONCE)
                    }
                )?,
            "independent canonical SLOT first curry/nonce exact bytes"
        );
        let slot_first_hash = hash(&slot_first)?;
        let actions = [
            curry(templates[0], &[atom(slot_first_hash.as_ref())?])?,
            curry(
                templates[1],
                &[atom(owner)?, atom(slot_first_hash.as_ref())?],
            )?,
            curry(
                templates[2],
                &[atom(owner)?, atom(slot_first_hash.as_ref())?],
            )?,
        ];
        let leaves = [hash(&actions[0])?, hash(&actions[1])?, hash(&actions[2])?];
        let leaf_hashes = leaves.map(|leaf| sha(&[&[1], leaf.as_ref()]));
        let left = sha(&[&[2], leaf_hashes[0].as_ref(), leaf_hashes[1].as_ref()]);
        let root = sha(&[&[2], left.as_ref(), leaf_hashes[2].as_ref()]);
        let proofs = [
            MerkleProof::new(0, vec![leaf_hashes[1], leaf_hashes[2]]),
            MerkleProof::new(1, vec![leaf_hashes[0], leaf_hashes[2]]),
            MerkleProof::new(1, vec![left]),
        ];
        let tree = MerkleTree::new(&leaves);
        ensure!(tree.root() == root, "independent three-leaf Merkle root");
        for i in 0..3 {
            ensure!(
                tree.proof(leaves[i]) == Some(proofs[i].clone()),
                "independent three-leaf proof"
            );
        }
        let first = curry(
            &DEFAULT_FINALIZER_PUZZLE,
            &[
                atom(ACTION_LAYER_PUZZLE_HASH.as_ref())?,
                atom(launcher.as_ref())?,
            ],
        )?;
        let finalizer = curry(&first, &[atom(hash(&first)?.as_ref())?])?;
        let p = node_from_bytes(&mut a, &DEFAULT_FINALIZER_PUZZLE)?;
        ensure!(
            first
                == encode(
                    &mut a,
                    &CurriedProgram {
                        program: p,
                        args: DefaultFinalizer1stCurryArgs::new(launcher)
                    }
                )?,
            "finalizer first curry exact schema"
        );
        let p = node_from_bytes(&mut a, &first)?;
        ensure!(
            finalizer
                == encode(
                    &mut a,
                    &CurriedProgram {
                        program: p,
                        args: DefaultFinalizer2ndCurryArgs::new(launcher)
                    }
                )?,
            "finalizer second curry exact schema"
        );
        Ok(Self {
            actions,
            root,
            proofs,
            finalizer,
            launcher,
            slot_first,
            slot_first_hash,
        })
    }
    pub fn inner(&self, state: State) -> Result<Vec<u8>> {
        let encoded = state.encode()?;
        let raw = curry(
            &ACTION_LAYER_PUZZLE,
            &[
                self.finalizer.clone(),
                atom(self.root.as_ref())?,
                encoded.clone(),
            ],
        )?;
        let mut a = allocator();
        let p = node_from_bytes(&mut a, &ACTION_LAYER_PUZZLE)?;
        let f = node_from_bytes(&mut a, &self.finalizer)?;
        let s = node_from_bytes(&mut a, &encoded)?;
        ensure!(
            raw == encode(
                &mut a,
                &CurriedProgram {
                    program: p,
                    args: ActionLayerArgs::new(f, self.root, s)
                }
            )?,
            "independent action-layer state curry schema"
        );
        Ok(raw)
    }
    pub fn full(&self, state: State) -> Result<Vec<u8>> {
        let inner = self.inner(state)?;
        let ss = pair(
            &atom(&chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH)?,
            &pair(
                &atom(self.launcher.as_ref())?,
                &atom(&chia_puzzles::SINGLETON_LAUNCHER_HASH)?,
            )?,
        )?;
        let raw = curry(
            &chia_puzzles::SINGLETON_TOP_LAYER_V1_1,
            &[ss, inner.clone()],
        )?;
        let mut a = allocator();
        let p = node_from_bytes(&mut a, &chia_puzzles::SINGLETON_TOP_LAYER_V1_1)?;
        let inner = node_from_bytes(&mut a, &inner)?;
        ensure!(
            raw == encode(
                &mut a,
                &CurriedProgram {
                    program: p,
                    args: SingletonArgs::new(self.launcher, inner)
                }
            )?,
            "independent full singleton curry schema"
        );
        Ok(raw)
    }
    pub fn slot(&self, entry: Bytes32) -> Result<Vec<u8>> {
        let raw = curry(&self.slot_first, &[atom(entry.as_ref())?])?;
        let mut a = allocator();
        let p = node_from_bytes(&mut a, &self.slot_first)?;
        ensure!(
            raw == encode(
                &mut a,
                &CurriedProgram {
                    program: p,
                    args: Slot2ndCurryArgs { value_hash: entry }
                }
            )?,
            "independent canonical SLOT second curry schema"
        );
        Ok(raw)
    }
    pub fn solution(&self, action: usize, old: &[u8; 32], new: &[u8; 32]) -> Result<Vec<u8>> {
        ensure!(action < 3, "one known registry action");
        let action_solution = if action == 0 {
            atom(&[])?
        } else {
            list(&[atom(old)?, atom(new)?])?
        };
        let proof = &self.proofs[action];
        let mut proof_fields = vec![integer(u64::from(proof.path))?];
        for sibling in &proof.proof {
            proof_fields.push(atom(sibling.as_ref())?);
        }
        let raw = list(&[
            list(&[self.actions[action].clone()])?,
            list(&[pair(&integer(2)?, &list(&proof_fields)?)?])?,
            list(std::slice::from_ref(&action_solution))?,
        ])?;
        let mut a = allocator();
        let p = node_from_bytes(&mut a, &self.actions[action])?;
        let s = node_from_bytes(&mut a, &action_solution)?;
        ensure!(
            raw == encode(
                &mut a,
                &RawActionLayerSolution {
                    puzzles: vec![p],
                    selectors_and_proofs: vec![(2, Some(proof.clone()))],
                    solutions: vec![s],
                    finalizer_solution: NodePtr::NIL
                }
            )?,
            "independent single-action selector/proof/solution schema"
        );
        Ok(raw)
    }
    pub fn full_solution(
        &self,
        proof: Proof,
        action: usize,
        old: &[u8; 32],
        new: &[u8; 32],
    ) -> Result<Vec<u8>> {
        let inner = self.solution(action, old, new)?;
        let mut a = allocator();
        let p = node_from_bytes(&mut a, &inner)?;
        encode(
            &mut a,
            &SingletonSolution {
                lineage_proof: proof,
                amount: 1,
                inner_solution: p,
            },
        )
    }
    pub fn slot_solution(
        &self,
        creator: chialisp_oxide_simulator::Coin,
        creator_state: State,
        spender_state: State,
    ) -> Result<Vec<u8>> {
        let creator_hash = hash(&self.inner(creator_state)?)?;
        let spender_hash = hash(&self.inner(spender_state)?)?;
        let proof = pair(
            &atom(creator.parent_coin_info.as_ref())?,
            &pair(&atom(creator_hash.as_ref())?, &integer(creator.amount)?)?,
        )?;
        let raw = pair(&proof, &atom(spender_hash.as_ref())?)?;
        let mut a = allocator();
        ensure!(
            raw == encode(
                &mut a,
                &SlotSolution {
                    lineage_proof: CompactLineageProof {
                        parent_parent_coin_info: creator.parent_coin_info,
                        parent_inner_puzzle_hash: creator_hash,
                        parent_amount: creator.amount
                    },
                    spender_inner_puzzle_hash: spender_hash
                }
            )?,
            "independent compact creator proof/current spender SLOT wire"
        );
        Ok(raw)
    }
}
