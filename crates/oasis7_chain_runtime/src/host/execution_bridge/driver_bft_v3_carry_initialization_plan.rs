//! C0 cost plan is usable only with the complete authenticated closure context.
use super::*;
use node_execution::CarryCopyDescriptorV1;
fn reserve(
    metadata: &PrivateClosureV1,
    descriptors: &[CarryCopyDescriptorV1],
) -> Result<(u64, u64), String> {
    let old = window_init_reserve(&metadata.pending_candidates)?;
    if metadata.publication_layout != Some(2) {
        return Ok(old);
    }
    if metadata.active_base.is_some() || metadata.protected_retention.is_none() {
        return Err("protected carry initializer namespace recipe differs".into());
    }
    let mut bytes = 8 * MAX_METADATA as u64;
    let mut objects = 16u64;
    let mut seen = BTreeSet::new();
    let mut heights = BTreeMap::<u64, usize>::new();
    for candidate in &metadata.pending_candidates {
        if !candidate.roles.contains(&PendingRoleV1::ProtectedOriginal)
            || !seen.insert(&candidate.header_id)
        {
            return Err("protected carry initializer role/duplicate differs".into());
        }
        let descriptor = descriptors
            .iter()
            .find(|d| d.header == candidate.header_id)
            .ok_or("protected carry initializer original missing")?;
        let archived = metadata
            .publications
            .iter()
            .find(|p| p.data == descriptor.data)
            .ok_or("protected carry initializer Data ref differs")?;
        if archived.reservation != descriptor.metadata
            || archived.original_data.is_some()
            || candidate.value.execution_commitment.height != descriptor.height
            || candidate.actions != descriptor.actions
            || node_execution::dependency_identity(
                candidate.dependencies.iter().map(|r| r.digest.as_str()),
            ) != descriptor.dependencies
        {
            return Err("protected carry initializer exact original differs".into());
        }
        let count = heights.entry(descriptor.height).or_default();
        *count += 1;
        if *count > 16 || seen.len() > 256 {
            return Err("protected carry initializer count capacity".into());
        }
        let received = !candidate.roles.contains(&PendingRoleV1::RetainedLocal);
        // Two Data images (CAS + canonical carrier) and two atomic pin images.
        // The same four objects are held separately for a future selection5.
        bytes = bytes
            .checked_add(descriptor.alias_bytes)
            .and_then(|n| {
                n.checked_add(if received {
                    descriptor.received_selection_bytes
                } else {
                    0
                })
            })
            .filter(|n| *n <= MAX_CLOSURE as u64)
            .ok_or("protected carry initialization capacity")?;
        objects = objects
            .checked_add(if received { 8 } else { 4 })
            .ok_or("protected carry initialization objects")?;
    }
    Ok((old.0.max(bytes), old.1.max(objects)))
}
pub(super) fn from_raw(
    metadata: &PrivateClosureV1,
    raw: &[node_execution::PublicationArchiveMaterialV1],
) -> Result<(u64, u64), String> {
    if metadata.publication_layout != Some(2) {
        return window_init_reserve(&metadata.pending_candidates);
    }
    let descriptors = raw
        .iter()
        .map(|p| p.carry_copy_descriptor())
        .collect::<Result<Vec<_>, _>>()?;
    reserve(metadata, &descriptors)
}
pub(in super::super) fn from_verified(
    metadata: &PrivateClosureV1,
    publications: &[node_execution::VerifiedPublicationOriginalV1],
) -> Result<(u64, u64), String> {
    if metadata.publication_layout != Some(2) {
        return window_init_reserve(&metadata.pending_candidates);
    }
    let descriptors = publications
        .iter()
        .map(|p| p.carry_copy_descriptor())
        .collect::<Result<Vec<_>, _>>()?;
    reserve(metadata, &descriptors)
}
pub(in super::super) fn validate_committed(
    old_charge: (u64, u64),
    old_init: (u64, u64),
    required: (u64, u64),
    committed: (u64, u64),
) -> Result<(), String> {
    let bytes = old_charge
        .0
        .checked_add(
            required
                .0
                .checked_sub(old_init.0)
                .ok_or("carry byte baseline")?,
        )
        .ok_or("carry charge byte overflow")?;
    let objects = old_charge
        .1
        .checked_add(
            required
                .1
                .checked_sub(old_init.1)
                .ok_or("carry object baseline")?,
        )
        .ok_or("carry charge object overflow")?;
    if bytes > committed.0 || objects > committed.1 {
        return Err("successor protected carry initialization undercharged".into());
    }
    Ok(())
}
#[cfg(test)]
#[path = "driver_bft_v3_carry_initialization_plan_tests.rs"]
mod tests;
