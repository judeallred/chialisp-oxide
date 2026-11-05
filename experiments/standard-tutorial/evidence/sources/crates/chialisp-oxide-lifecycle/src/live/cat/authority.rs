//! Sealed source capability. Production construction accepts only opaque
//! actual-process Cargo/native material and remains internal until SDK gates pass.
use super::{CatError, MAX_CAT_MODULE_BYTES, Result};
use clvm_utils::tree_hash;
use clvmr_sdk::{
    Allocator,
    serde::{node_from_bytes, node_to_bytes},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatMode {
    RustOff,
    RustOn,
}
pub struct CatTailAuthority {
    module: Vec<u8>,
    hash: [u8; 32],
    mode: Option<CatMode>,
    max_cost: u64,
    source_identity: Option<[u8; 32]>,
}
impl CatTailAuthority {
    pub fn mode(&self) -> Option<CatMode> {
        self.mode
    }
    pub fn source_identity(&self) -> Option<[u8; 32]> {
        self.source_identity
    }
    pub(super) fn checked_candidate(
        material: &crate::fresh::cat::CheckedCatSourceMaterial,
        mode: Option<CatMode>,
    ) -> Result<Self> {
        let module = match mode {
            Some(mode) => material.selected_module(mode),
            None => material.canonical_module(),
        };
        let (module, hash) = checked_module(module)?;
        Ok(Self {
            module,
            hash,
            mode,
            max_cost: super::MAX_CAT_BUNDLE_COST,
            source_identity: Some(material.source_identity()),
        })
    }
    pub fn module_hash(&self) -> [u8; 32] {
        self.hash
    }
    pub fn module_bytes(&self) -> &[u8] {
        &self.module
    }
    pub(super) fn max_cost(&self) -> u64 {
        self.max_cost
    }
    #[cfg(test)]
    pub(in crate::live) fn fixture_recorded(module: &[u8], mode: CatMode) -> Result<Self> {
        if module.is_empty() || module.len() > MAX_CAT_MODULE_BYTES {
            return Err(CatError::Module);
        }
        let mut allocator = Allocator::new_limited(super::MAX_CODEC_HEAP);
        let node = node_from_bytes(&mut allocator, module).map_err(|_| CatError::Module)?;
        if node_to_bytes(&allocator, node).map_err(|_| CatError::Module)? != module {
            return Err(CatError::Module);
        }
        Ok(Self {
            module: module.to_vec(),
            hash: tree_hash(&allocator, node).to_bytes(),
            mode: Some(mode),
            max_cost: super::MAX_CAT_BUNDLE_COST,
            source_identity: None,
        })
    }
    #[cfg(test)]
    pub(in crate::live) fn fixture_reference(module: &[u8]) -> Result<Self> {
        let (module, hash) = checked_module(module)?;
        Ok(Self {
            module,
            hash,
            mode: None,
            max_cost: super::MAX_CAT_BUNDLE_COST,
            source_identity: None,
        })
    }
}

fn checked_module(module: &[u8]) -> Result<(Vec<u8>, [u8; 32])> {
    if module.is_empty() || module.len() > MAX_CAT_MODULE_BYTES {
        return Err(CatError::Module);
    }
    let mut allocator = Allocator::new_limited(super::MAX_CODEC_HEAP);
    let node = node_from_bytes(&mut allocator, module).map_err(|_| CatError::Module)?;
    if node_to_bytes(&allocator, node).map_err(|_| CatError::Module)? != module {
        return Err(CatError::Module);
    }
    Ok((module.to_vec(), tree_hash(&allocator, node).to_bytes()))
}
