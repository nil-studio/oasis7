//! Profile-three original durability consumes complete provisional evidence.
//! Pages must separately be verified through that evidence's exact RootSet.
use super::provisional::ProvisionalCandidate;
use super::*;
use crate::incremental_page_store::objects::{IncrementalObjectStore, ObjectIo};
/// Store exact signed material without constructing profile-two snapshots or
/// cloning original payloads. Immutable object writes are safe to retry.
pub(super) fn persist_checked_originals(
    candidate: &FormalCandidateMaterialV1,
    checked: &ProvisionalCandidate,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    remaining: &mut u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormalError> {
    let mut record = [0u8; 32];
    hex::decode_to_slice(candidate.candidate.record.record_id()?, &mut record)
        .map_err(|_| invalid("IWS persistence candidate ID"))?;
    if record != checked.core.candidate_record_id() {
        return Err(invalid("IWS persistence exact provisional candidate"));
    }
    for object in candidate.material.objects.iter().chain(
        candidate
            .frozen_material
            .iter()
            .flat_map(|m| m.objects.iter()),
    ) {
        if cancelled() {
            return Err(invalid("IWS persistence cancelled"));
        }
        *remaining = remaining
            .checked_sub(1)
            .ok_or_else(|| invalid("IWS persisted original quota"))?;
        let mut expected = [0u8; 32];
        hex::decode_to_slice(&object.content_hash, &mut expected)
            .map_err(|_| invalid("IWS persisted original ID"))?;
        let length = u64::try_from(object.bytes.len())
            .map_err(|_| invalid("IWS persisted original length"))?;
        let mut input = object.bytes.as_slice();
        objects
            .write_object(&mut input, length, Some(expected), io)
            .map_err(page_store_formal_error)?;
    }
    Ok(())
}
