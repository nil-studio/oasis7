//! Durable undo of an unselected seal when a recovered writer added overlay.
use super::*;
pub(super) fn unseal(root: &Path, state: &mut Build) -> Result<(), FormalError> {
    let id = state
        .sealed
        .clone()
        .ok_or_else(|| invalid("generation unseal identity missing"))?;
    state.unsealing = Some(id);
    save(root, state)?;
    recover_unseal(root, state)
}
pub(super) fn recover_unseal(root: &Path, state: &mut Build) -> Result<(), FormalError> {
    let _fence = pin(root, true)?;
    let id = state
        .unsealing
        .clone()
        .ok_or_else(|| invalid("generation unseal intent missing"))?;
    if state.sealed.as_ref() != Some(&id) || current(root)?.as_ref() == Some(&id) {
        return Err(invalid("cannot unseal selected or unrelated generation"));
    }
    let source = root.join(GENERATIONS).join(id);
    let destination = root
        .join(GENERATIONS)
        .join(format!("building-{}", state.nonce));
    if source.try_exists().map_err(io)? {
        if destination.try_exists().map_err(io)? {
            return Err(invalid("unseal destination exists"));
        }
        fs::rename(source, &destination).map_err(io)?;
        sync(&root.join(GENERATIONS))?;
    }
    if !destination.is_dir() {
        return Err(invalid("unseal work directory missing"));
    }
    match fs::remove_file(destination.join(MANIFEST)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(e)),
    }
    sync(&destination)?;
    state.sealed = None;
    state.unsealing = None;
    save(root, state)
}
