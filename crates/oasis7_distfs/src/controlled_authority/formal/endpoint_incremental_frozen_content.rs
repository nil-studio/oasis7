//! Recheck original Frozen content in the actual candidate closure.
//! This is post-execution validation, never a Frozen-start permit.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    IncrementalFrozenMaterialWorkV1, verify_incremental_frozen_material_v1,
};
pub(super) fn verify_candidate_frozen_content(
    endpoint: &FileFormalEndpoint,
    candidate: &FormalCandidateMaterialV1,
    memory: &MemoryBudget,
    work: &mut IncrementalFrozenMaterialWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormalError> {
    if cancelled() {
        return Err(invalid("IWS Frozen content cancelled"));
    }
    let (certificate, material) = match (&candidate.frozen, &candidate.frozen_material) {
        (None, None) => return Ok(()), // GEN is checked by the actual structural core.
        (Some(certificate), Some(material)) => (certificate, material),
        _ => return Err(invalid("IWS Frozen content certificate/material pairing")),
    };
    let parent = endpoint
        .parent
        .as_ref()
        .ok_or_else(|| invalid("IWS Frozen content verified parent missing"))?;
    let reservation =
        verify_formal_reservation(&certificate.reservation, &endpoint.authority, parent)?;
    let mut originals = ClosedRecordOriginalReader {
        record: material,
        memory,
    };
    let checked = verify_incremental_frozen_material_v1(
        &reservation,
        parent,
        material,
        &mut originals,
        memory,
        work,
        &mut *cancelled,
    )
    .map_err(|error| invalid(format!("IWS original Frozen content: {error}")))?;
    let capability = endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("IWS Frozen accepted parent store missing"))?;
    let accepted_slot = capability.jobs.parent()?;
    let accepted = accepted_slot
        .as_ref()
        .ok_or_else(|| invalid("IWS Frozen accepted parent missing"))?;
    // Keep the accepted cache borrow alive through the final observation.
    let parent_material = accepted.frozen_parent_material(&checked, memory, cancelled)?;
    let path_bytes = endpoint
        .store
        .path
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add("formal-objects-v1".len() + 256)
        .ok_or_else(|| invalid("IWS Frozen native directory admission bound"))?;
    let _path_fee = memory
        .reserve(path_bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid("IWS Frozen native directory admission"))?;
    let directory = endpoint.store.path.join("formal-objects-v1");
    let parent_lease = closure::CandidateExecutionMaterialV1::check_frozen_parent_lease_v1(
        parent_material,
        &checked,
        &directory,
        &capability.pages,
        memory,
        cancelled,
    )?;
    // All immutable original evidence and its fee remain live through the
    // final stop observation. This consumer cannot mint an endpoint receipt.
    if cancelled() {
        return Err(invalid("IWS Frozen content final cancellation"));
    }
    drop(parent_lease);
    drop(checked);
    Ok(())
}
