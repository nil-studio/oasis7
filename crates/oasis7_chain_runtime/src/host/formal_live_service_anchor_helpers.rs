use super::*;

pub(super) fn anchor_ref(anchor: &VerifiedFormalReadAnchorV1) -> Result<FormalReadAnchorRefV1, FormalReadError> {
    Ok(FormalReadAnchorRefV1 {
        position: anchor.head().record().position,
        record_id: anchor.head().head().record_id.clone(),
        active_authority_id: anchor
            .authority()
            .authority_id()
            .map_err(FormalReadError::from)?,
    })
}

pub(super) fn validate_anchor_ref(
    expected: &FormalReadAnchorRefV1,
    anchor: &VerifiedFormalReadAnchorV1,
) -> Result<(), FormalReadError> {
    if &anchor_ref(anchor)? != expected {
        return Err(FormalReadError::Integrity("formal read origin differs from exact caller anchor".into()));
    }
    Ok(())
}

pub(super) fn same_anchor(
    first: &VerifiedFormalReadAnchorV1,
    second: &VerifiedFormalReadAnchorV1,
) -> Result<bool, FormalReadError> {
    Ok(anchor_ref(first)? == anchor_ref(second)?)
}
