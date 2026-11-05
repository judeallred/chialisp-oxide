//! Bounded independent raw wire construction, checked against pinned SDK schemas.
use anyhow::{Result, ensure};
use chia_puzzle_types::Proof;
use chia_puzzle_types::singleton::{SingletonArgs, SingletonSolution};
use chia_sdk_types::{
    MerkleProof, MerkleTree,
    puzzles::{
        ACTION_LAYER_PUZZLE, ACTION_LAYER_PUZZLE_HASH, ActionLayerArgs, DEFAULT_FINALIZER_PUZZLE,
        DefaultFinalizer1stCurryArgs, DefaultFinalizer2ndCurryArgs, RawActionLayerSolution,
    },
};
use chialisp_oxide_simulator::{Bytes32, puzzle_hash};
use clvm_traits::ToClvm;
use clvm_utils::{CurriedProgram, TreeHash};
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
/// Replace one bounded physical pair path (false=first, true=rest).
pub fn rewrite(raw: &[u8], path: &[bool], replacement: &[u8]) -> Result<Vec<u8>> {
    ensure!(path.len() <= 64, "bounded fixture path");
    admit(raw)?;
    admit(replacement)?;
    let mut a = allocator();
    let mut node = node_from_bytes(&mut a, raw)?;
    let mut parents = Vec::new();
    for right in path {
        let clvmr_sdk::SExp::Pair(first, rest) = a.sexp(node) else {
            anyhow::bail!("fixture path requires pair")
        };
        parents.push((first, rest, *right));
        node = if *right { rest } else { first };
    }
    node = node_from_bytes(&mut a, replacement)?;
    for (first, rest, right) in parents.into_iter().rev() {
        node = if right {
            a.new_pair(first, node)?
        } else {
            a.new_pair(node, rest)?
        };
    }
    let out = node_to_bytes_limit(&a, node, LIMIT)?;
    admit(&out)?;
    Ok(out)
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

#[derive(Clone)]
pub struct Policy {
    pub increment: Vec<u8>,
    pub reset: Vec<u8>,
    pub root: Bytes32,
    pub proofs: [MerkleProof; 2],
    pub finalizer: Vec<u8>,
    pub launcher: Bytes32,
}
impl Policy {
    pub fn new(
        increment: Vec<u8>,
        reset_template: Vec<u8>,
        owner: &[u8; 48],
        launcher: Bytes32,
    ) -> Result<Self> {
        admit(&increment)?;
        admit(&reset_template)?;
        let reset = curry(&reset_template, &[atom(owner)?])?;
        let leaves = [hash(&increment)?, hash(&reset)?];
        let tree = MerkleTree::new(&leaves);
        let lh = sha(&[&[1], leaves[0].as_ref()]);
        let rh = sha(&[&[1], leaves[1].as_ref()]);
        let root = sha(&[&[2], lh.as_ref(), rh.as_ref()]);
        ensure!(root == tree.root(), "independent Merkle root");
        let proofs = [MerkleProof::new(0, vec![rh]), MerkleProof::new(1, vec![lh])];
        for i in 0..2 {
            ensure!(
                Some(proofs[i].clone()) == tree.proof(leaves[i]),
                "independent Merkle proof"
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
        let mut a = allocator();
        let d = node_from_bytes(&mut a, &DEFAULT_FINALIZER_PUZZLE)?;
        let sdk_first = encode(
            &mut a,
            &CurriedProgram {
                program: d,
                args: DefaultFinalizer1stCurryArgs::new(launcher),
            },
        )?;
        ensure!(
            first == sdk_first,
            "independent first finalizer curry exact bytes"
        );
        let f = node_from_bytes(&mut a, &first)?;
        ensure!(
            finalizer
                == encode(
                    &mut a,
                    &CurriedProgram {
                        program: f,
                        args: DefaultFinalizer2ndCurryArgs::new(launcher)
                    }
                )?,
            "independent second finalizer curry exact bytes"
        );
        ensure!(
            hash(&finalizer)?.to_bytes()
                == DefaultFinalizer2ndCurryArgs::curry_tree_hash(launcher).to_bytes(),
            "finalizer hash formula"
        );
        Ok(Self {
            increment,
            reset,
            root,
            proofs,
            finalizer,
            launcher,
        })
    }
    pub fn inner(&self, state: u64) -> Result<Vec<u8>> {
        let raw = curry(
            &ACTION_LAYER_PUZZLE,
            &[
                self.finalizer.clone(),
                atom(self.root.as_ref())?,
                integer(state)?,
            ],
        )?;
        let mut a = allocator();
        let program = node_from_bytes(&mut a, &ACTION_LAYER_PUZZLE)?;
        let finalizer = node_from_bytes(&mut a, &self.finalizer)?;
        ensure!(
            raw == encode(
                &mut a,
                &CurriedProgram {
                    program,
                    args: ActionLayerArgs::new(finalizer, self.root, state)
                }
            )?,
            "independent action-layer curry exact bytes"
        );
        ensure!(
            hash(&raw)?.to_bytes()
                == ActionLayerArgs::curry_tree_hash(
                    TreeHash::new(hash(&self.finalizer)?.to_bytes()),
                    self.root,
                    TreeHash::new(hash(&integer(state)?)?.to_bytes())
                )
                .to_bytes(),
            "action-layer curry hash formula"
        );
        Ok(raw)
    }
    pub fn full(&self, state: u64) -> Result<Vec<u8>> {
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
        let program = node_from_bytes(&mut a, &chia_puzzles::SINGLETON_TOP_LAYER_V1_1)?;
        let ip = node_from_bytes(&mut a, &inner)?;
        ensure!(
            raw == encode(
                &mut a,
                &CurriedProgram {
                    program,
                    args: SingletonArgs::new(self.launcher, ip)
                }
            )?,
            "independent singleton curry exact bytes"
        );
        ensure!(
            hash(&raw)?.to_bytes()
                == SingletonArgs::curry_tree_hash(
                    self.launcher,
                    TreeHash::new(hash(&inner)?.to_bytes())
                )
                .to_bytes(),
            "singleton hash formula"
        );
        Ok(raw)
    }
    pub fn solution(&self, actions: &[usize]) -> Result<Vec<u8>> {
        ensure!(
            !actions.is_empty() && actions.len() <= 8 && actions.iter().all(|i| *i < 2),
            "strict finite host batch"
        );
        let mut unique = Vec::new();
        for action in actions {
            if !unique.contains(action) {
                unique.push(*action);
            }
        }
        let puzzles: Vec<_> = unique
            .iter()
            .map(|i| {
                if *i == 0 {
                    self.increment.clone()
                } else {
                    self.reset.clone()
                }
            })
            .collect();
        let mut selector_entries = Vec::new();
        let mut proven = Vec::new();
        let mut sdk_entries = Vec::new();
        for action in actions.iter().rev() {
            let index = unique.iter().position(|i| i == action).unwrap();
            let selector = if index == 0 { 2 } else { 5 };
            let proof = if proven.contains(action) {
                None
            } else {
                proven.push(*action);
                Some(self.proofs[*action].clone())
            };
            let encoded_proof = if let Some(p) = &proof {
                list(&[integer(u64::from(p.path))?, atom(p.proof[0].as_ref())?])?
            } else {
                atom(&[])?
            };
            selector_entries.push(pair(&integer(selector)?, &encoded_proof)?);
            sdk_entries.push((selector as u32, proof));
        }
        let solutions = vec![atom(&[])?; actions.len()];
        let raw = list(&[list(&puzzles)?, list(&selector_entries)?, list(&solutions)?])?;
        let mut a = allocator();
        let pointers = puzzles
            .iter()
            .map(|b| node_from_bytes(&mut a, b))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let sdk = RawActionLayerSolution {
            puzzles: pointers,
            selectors_and_proofs: sdk_entries,
            solutions: vec![NodePtr::NIL; actions.len()],
            finalizer_solution: NodePtr::NIL,
        };
        ensure!(
            raw == encode(&mut a, &sdk)?,
            "independent reversed selector/proof/nil-reuse wire bytes"
        );
        admit(&raw)?;
        Ok(raw)
    }
    pub fn full_solution(&self, proof: Proof, actions: &[usize]) -> Result<Vec<u8>> {
        let inner = self.solution(actions)?;
        let mut a = allocator();
        let i = node_from_bytes(&mut a, &inner)?;
        let raw = encode(
            &mut a,
            &SingletonSolution {
                lineage_proof: proof,
                amount: 1,
                inner_solution: i,
            },
        )?;
        admit(&raw)?;
        Ok(raw)
    }
}
