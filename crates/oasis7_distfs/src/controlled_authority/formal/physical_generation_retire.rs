//! Bounded source retirement after the new immutable closure is selected.
use super::*;
#[path = "physical_generation_cleanup.rs"]
mod cleanup;
pub(super) fn cleanup_overlay_catalog(
    root: &Path,
    budget: GenerationBudget,
) -> Result<bool, FormalError> {
    cleanup::old_generation_batch(root, budget)
}
pub(super) fn retire(
    root: &Path,
    state: &mut Build,
    selected: &str,
    session: &mut GenerationSession,
    budget: GenerationBudget,
) -> Result<FormalGenerationProgress, FormalError> {
    let _fence = pin(root, true)?;
    if current(root)?.as_deref() != Some(selected) {
        return Err(invalid("generation retire lost selector"));
    }
    let target = manifest(root, selected)?;
    if target.nonce != state.nonce
        || target.catalogs != state.completed
        || state.objects != state.completed
    {
        return Err(invalid("generation retire target coverage"));
    }
    let dir = root.join(GENERATIONS).join(selected);
    let ids = catalog::page(
        &dir,
        &target.catalogs,
        state.retire_after.as_deref(),
        budget.objects,
    )?;
    let mut size = 0usize;
    let mut done = 0;
    for id in &ids {
        let length = super::super::super::business_pack::logical_length(&dir.join(id), id)?
            .ok_or_else(|| invalid("generation retirement target missing"))?;
        if done > 0 && (size as u64).saturating_add(length) > budget.bytes as u64 {
            break;
        }
        let target_bytes = crate::controlled_authority::formal::business_pack::read_object_durable(
            &dir.join(id),
            id,
        )?
        .ok_or_else(|| invalid("generation retirement target missing"))?;
        if done > 0 && size.saturating_add(target_bytes.len()) > budget.bytes {
            break;
        }
        size = size
            .checked_add(target_bytes.len())
            .ok_or_else(|| invalid("generation retirement byte overflow"))?;
        for path in [root.join(SELECTORS).join(id), root.join(id)] {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(e)),
            }
        }
        publication::fault(6)?;
        state.retire_after = Some(id.clone());
        done += 1;
        if budget.expired() {
            break;
        }
    }
    sync(&root.join(SELECTORS))?;
    sync(root)?;
    save(root, state)?;
    if done < ids.len() || ids.len() == budget.objects {
        return progress(root, state);
    }
    if let Some(previous) = &state.previous {
        if previous == selected {
            return Err(invalid("cannot retire selected generation"));
        }
        let old = state
            .previous_manifest
            .as_ref()
            .ok_or_else(|| invalid("generation original manifest absent"))?;
        if blake3::hash(&serde_cbor::to_vec(old).map_err(io)?)
            .to_hex()
            .as_str()
            != previous
        {
            return Err(invalid("generation original manifest checksum"));
        }
        let olddir = root.join(GENERATIONS).join(previous);
        if !state.old_verified {
            let original = catalog::page(
                &olddir,
                &old.catalogs,
                state.old_checked.as_deref(),
                budget.objects,
            )?;
            for id in &original {
                if !catalog::contains(&dir, &target.catalogs, id)? {
                    return Err(invalid("generation old closure not covered"));
                }
            }
            if let Some(last) = original.last() {
                state.old_checked = Some(last.clone());
            }
            state.old_verified = original.len() < budget.objects;
            save(root, state)?;
            return progress(root, state);
        }
        if !cleanup::old_generation_batch(&olddir, budget)? {
            return progress(root, state);
        }
    }
    if !overlay::advance(root, state, selected, &mut session.overlay, budget)? {
        save(root, state)?;
        return progress(root, state);
    }
    if !super::super::staging::retire_batch(root, budget.objects, budget.bytes, || {
        budget.expired()
    })? {
        save(root, state)?;
        return progress(root, state);
    }
    publication::fault(7)?;
    fs::remove_file(root.join(BUILD)).map_err(io)?;
    sync(root)?;
    let count = catalog::count(&target.catalogs)?;
    Ok(FormalGenerationProgress {
        building: false,
        selected: Some(selected.into()),
        copied_objects: count,
        total_objects: count,
    })
}
