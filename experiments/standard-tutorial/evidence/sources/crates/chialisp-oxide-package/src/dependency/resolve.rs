use super::protocol::{CheckedDependencyLock, DependencyPin};
use crate::{Digest32, Error, ErrorKind, Result, VerifiedPackage, store::Registry};
use std::collections::{BTreeMap, BTreeSet};

pub struct ResolvedDependencies {
    packages: BTreeMap<Digest32, VerifiedPackage>,
    aliases: BTreeMap<String, Digest32>,
}
impl ResolvedDependencies {
    pub fn get(&self, name: &str) -> Option<&VerifiedPackage> {
        self.aliases.get(name).and_then(|id| self.packages.get(id))
    }
    pub fn entries(&self) -> impl ExactSizeIterator<Item = (&str, &VerifiedPackage)> {
        self.aliases
            .iter()
            .map(|(name, id)| (name.as_str(), &self.packages[id]))
    }
}
impl CheckedDependencyLock {
    /// Perform one complete bounded registry scan, including unrelated objects.
    /// No writer sidecar, stage, destination or registry object is created.
    pub fn resolve(&self, registry: &Registry) -> Result<ResolvedDependencies> {
        let requested = self
            .dependencies()
            .iter()
            .map(|pin| pin.object_id)
            .collect();
        let packages = registry.scan_for_dependencies(None, &requested)?;
        let packages: BTreeMap<_, _> = packages.into_iter().map(|p| (p.object_id(), p)).collect();
        check_pins(self.dependencies(), &packages)?;
        let aliases = self
            .dependencies()
            .iter()
            .map(|pin| (pin.name.clone(), pin.object_id))
            .collect();
        Ok(ResolvedDependencies { packages, aliases })
    }
}

pub(super) fn requested_ids(pins: &[DependencyPin]) -> BTreeSet<Digest32> {
    pins.iter().map(|pin| pin.object_id).collect()
}
pub(super) fn check_pins(
    pins: &[DependencyPin],
    packages: &BTreeMap<Digest32, VerifiedPackage>,
) -> Result<()> {
    for pin in pins {
        let package = packages.get(&pin.object_id).ok_or_else(|| {
            Error::new(
                ErrorKind::UnknownObject,
                format!("missing locked registry object {}", pin.object_id),
            )
        })?;
        if package.artifact().identity() != &pin.artifact {
            return Err(Error::new(
                ErrorKind::DigestMismatch,
                format!("locked artifact differs for {}", pin.name),
            ));
        }
    }
    Ok(())
}
