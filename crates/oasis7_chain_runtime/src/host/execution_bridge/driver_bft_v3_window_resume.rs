//! Resume only the authenticated original InstalledPending installation.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftSignerStateV3, BftSuccessorWindowAdoptedEvidenceV1, BftSuccessorWindowStoredRequestV1,
};
pub(in super::super) fn prepare_pending(
    records: &Path,
    store: LocalCasStore,
    base: &VerifiedPrivateSuccessorBaseV1,
    original: &BftSuccessorWindowStoredRequestV1,
    state: &BftSignerStateV3,
    deadline: Instant,
) -> Result<BftSuccessorWindowAdoptedEvidenceV1, String> {
    if base.original.as_ref() != Some(original) {
        return Err("pending private base original differs".into());
    }
    let records = canonical_storage_path(records)?;
    if !records.join("bft-format.lock").is_file()
        || !records
            .join(storage::DIRECTORY)
            .join("writer.lock")
            .is_file()
    {
        return Err("pending original carrier absent; initialization forbidden".into());
    }
    let _format = common_storage::lock_format(&records)?;
    let _writer = files::lock(&records.join(storage::DIRECTORY), false)?;
    let carriers = successor_carriers::InstallationRootV1::new(records.clone())?;
    let _original_carriers = carriers.verify_origin_carriers(&base.origin, deadline)?;
    let origin = layout::prepare(&records, store.clone(), base, deadline)?;
    layout::verify_ready(&records, store, base, &origin, true, deadline)?;
    live(deadline)?;
    let pointer = successor_install::publish_installed(&records, original, state, deadline)?;
    carriers.verify_origin_carriers(&base.origin, deadline)?;
    if carriers.read_installed(&base.origin, original, state, deadline)? != pointer {
        return Err("pending original installed pointer readback changed".into());
    }
    let mut evidence = pointer.evidence();
    evidence.active_segment_origin = Some(origin);
    Ok(evidence)
}
