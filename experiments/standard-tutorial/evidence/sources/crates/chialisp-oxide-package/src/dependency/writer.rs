use super::{AddOutcome, AddStatus, DependencyPin, protocol, resolve};
use crate::{Digest32, Error, ErrorKind, Result, digest, store::Registry};
use std::{collections::BTreeMap, path::Path};

mod local;
use local::Location;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Phase {
    AfterResolution,
    AfterWriterLock,
    BeforeStageWrite,
    BeforeStageSync,
    BeforeCommit,
    AfterCommit,
}

pub(super) fn add(
    registry: &Registry,
    path: &Path,
    name: &str,
    module: Digest32,
    object: Option<Digest32>,
) -> Result<AddOutcome> {
    add_with(registry, path, name, module, object, &mut |_, _| Ok(()))
}

// Private fault/coordination seam. Production passes a no-op and exposes no
// external callback or retry; tests inject actual stale files and IO failures.
pub(super) fn add_with(
    registry: &Registry,
    path: &Path,
    name: &str,
    module: Digest32,
    object: Option<Digest32>,
    hook: &mut impl FnMut(Phase, &Path) -> Result<()>,
) -> Result<AddOutcome> {
    protocol::check_name(name)?;
    let location = Location::new(registry.root(), path)?;
    let snapshot = location.snapshot()?;
    let old = snapshot.as_deref().map(protocol::verify_lock).transpose()?;
    let pins = old.as_ref().map_or(&[][..], |lock| lock.dependencies());
    let existing = pins.iter().find(|pin| pin.name == name);
    if existing.is_none() && pins.len() >= super::MAX_DEPENDENCIES {
        return Err(protocol::resource("dependency count exceeds lock limit"));
    }
    if let Some(pin) = existing
        && object.is_some_and(|object| object != pin.object_id)
    {
        return Err(Error::new(
            ErrorKind::Conflict,
            "dependency alias already pins a different object",
        ));
    }
    let mut requested = resolve::requested_ids(pins);
    if let Some(object) = object {
        requested.insert(object);
    }
    let packages = registry.scan_for_dependencies(Some(module), &requested)?;
    let packages: BTreeMap<_, _> = packages.into_iter().map(|p| (p.object_id(), p)).collect();
    resolve::check_pins(pins, &packages)?;
    let selected = if let Some(object) = object {
        let package = packages.get(&object).ok_or_else(|| {
            Error::new(
                ErrorKind::UnknownObject,
                "requested dependency object is absent",
            )
        })?;
        if package.module_tree_hash() != module {
            return Err(Error::new(
                ErrorKind::Conflict,
                "requested object does not match dependency module hash",
            ));
        }
        package
    } else {
        let mut candidates = packages
            .values()
            .filter(|package| package.module_tree_hash() == module);
        let selected = candidates.next().ok_or_else(|| {
            Error::new(
                ErrorKind::UnknownObject,
                "requested dependency module hash is absent",
            )
        })?;
        if candidates.next().is_some() {
            return Err(Error::new(
                ErrorKind::Conflict,
                "dependency module hash has multiple objects; select an exact object ID",
            ));
        }
        selected
    };
    let pin = DependencyPin {
        name: name.into(),
        object_id: selected.object_id(),
        artifact: selected.artifact().identity().clone(),
        binding: (),
    };
    if existing.is_some_and(|existing| existing != &pin) {
        return Err(Error::new(
            ErrorKind::Conflict,
            "dependency alias already has a different full pin",
        ));
    }
    hook(Phase::AfterResolution, location.path())?;
    let guard = location.writer_lock()?;
    hook(Phase::AfterWriterLock, location.path())?;
    location.compare_snapshot(&snapshot)?;
    if existing.is_some() {
        guard.release()?;
        return Ok(AddOutcome {
            status: AddStatus::AlreadyPresent,
            pin,
            lock_sha256: digest(snapshot.as_ref().expect("existing pin requires lock bytes")),
            dependency_count: pins.len(),
        });
    }
    let mut updated = pins.to_vec();
    updated.push(pin.clone());
    updated.sort_by(|a, b| a.name.cmp(&b.name));
    let candidate = protocol::encode(&updated)?;
    let staged = location.stage(candidate.bytes(), hook)?;
    let checked = protocol::read_lock(staged.path())?;
    if checked.bytes() != candidate.bytes() {
        return Err(Error::new(
            ErrorKind::Conflict,
            "staged dependency lock changed",
        ));
    }
    hook(Phase::BeforeCommit, staged.path())?;
    location.compare_snapshot(&snapshot)?;
    staged.commit(location.path(), snapshot.is_some())?;
    hook(Phase::AfterCommit, location.path())?;
    guard.release()?;
    Ok(AddOutcome {
        status: AddStatus::Added,
        pin,
        lock_sha256: digest(candidate.bytes()),
        dependency_count: updated.len(),
    })
}
