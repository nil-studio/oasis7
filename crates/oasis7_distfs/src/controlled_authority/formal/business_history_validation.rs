//! Contract validation against opaque native parent evidence, not caller roots.
use super::business_index::BusinessIndexLeafKindV1;
use super::endpoint::VerifiedFormalBusinessParentV1;
use super::*;
pub(super) fn verify_parent_history_cut<'a>(
    record: &CanonicalRecordV1,
    envelope: &FormalBusinessResultEnvelopeV1,
    parent: Option<&'a VerifiedFormalBusinessParentV1>,
) -> Result<Option<&'a str>, FormalError> {
    let mut history = envelope
        .delta
        .updates
        .iter()
        .filter(|leaf| leaf.kind == BusinessIndexLeafKindV1::HistoricalCut);
    match record.kind {
        FormalRecordKindV1::InitialActivation => {
            if parent.is_some() || history.next().is_some() {
                return Err(crypto::invalid(
                    "initial business envelope cannot insert a historical cut",
                ));
            }
            Ok(None)
        }
        FormalRecordKindV1::Execution | FormalRecordKindV1::RecoveryActivation { .. } => {
            let parent = parent.ok_or_else(|| {
                crypto::invalid("profile2 missing opaque authenticated parent checkpoint")
            })?;
            if parent.record().world != record.world
                || parent.record().record_id()? != record.parent_record_id
                || parent.record().position.checked_add(1) != Some(record.position)
            {
                return Err(crypto::invalid(
                    "profile2 business exact canonical predecessor",
                ));
            }
            let leaf = history.next().ok_or_else(|| {
                crypto::invalid("profile2 execution requires immutable parent HistoricalCut")
            })?;
            if history.next().is_some() || leaf != &parent.cut_leaf()? {
                return Err(crypto::invalid(
                    "profile2 HistoricalCut exact authenticated original parent proof",
                ));
            }
            Ok(Some(parent.record().result_root.as_str()))
        }
    }
}
