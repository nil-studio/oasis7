//! Exact candidate restoration with operation-local original Snapshot bytes.
use super::*;

pub(super) fn restore(
    candidate: &FormalCandidateMaterialV1,
    release: &ReleaseSecurityPolicy,
) -> Result<World, String> {
    let expanded_snapshot =
        oasis7_distfs::controlled_authority::formal::verify_record_material_retaining_snapshot(
            &candidate.candidate.record,
            &candidate.material,
        )
        .map_err(|e| e.to_string())?;
    let snapshot: Snapshot = match expanded_snapshot {
        Some(bytes) => oasis7_world_state::codec::snapshot_from_cbor(&bytes)
            .map_err(|error| error.to_string())?,
        None => capture::decode_snapshot_role(&candidate.material, ArtifactRole::Snapshot)?,
    };
    let journal: Journal =
        capture::decode_journal_role(&candidate.material, ArtifactRole::Journal)?;
    let world = oasis7::runtime_host::world::from_snapshot(snapshot, journal)
        .map_err(|e| format!("{e:?}"))?
        .with_release_security_policy(release.clone());
    if blake3_hex(&snapshot_to_cbor(&world.snapshot())?)
        != candidate.candidate.record.after_state_root
    {
        return Err("formal writer restored candidate root mismatch".into());
    }
    super::super::formal_business_package::tree(
        &candidate.material,
        &candidate.candidate.record,
        &world,
        false,
    )?;
    Ok(world)
}
pub(super) fn restore_qualified(
    candidate: &FormalCandidateMaterialV1,
    release: &ReleaseSecurityPolicy,
    anchor: &VerifiedFormalReadAnchorV1,
    reader: std::sync::Arc<dyn business_index::BusinessIndexNodeReader + Send + Sync>,
) -> Result<World, String> {
    let expanded_snapshot =
        oasis7_distfs::controlled_authority::formal::verify_record_material_retaining_snapshot(
            &candidate.candidate.record,
            &candidate.material,
        )
        .map_err(|e| e.to_string())?;
    if &candidate.candidate.record != anchor.head().record() {
        return Err("formal restore candidate differs from qualified anchor".into());
    }
    let snapshot: Snapshot = match expanded_snapshot {
        Some(bytes) => oasis7_world_state::codec::snapshot_from_cbor(&bytes)
            .map_err(|error| error.to_string())?,
        None => capture::decode_snapshot_role(&candidate.material, ArtifactRole::Snapshot)?,
    };
    let journal: Journal =
        capture::decode_journal_role(&candidate.material, ArtifactRole::Journal)?;
    if snapshot.state.retained_history_root.is_none() {
        if anchor.authority().definition().profile_version == 2 {
            return Err("profile2 native restore requires retained-history descriptor".into());
        }
        return restore(candidate, release);
    }
    let bytes = snapshot_to_cbor(&snapshot)?;
    let parent = oasis7_runtime::VerifiedFormalRetainedHistoryParentV1::verify(anchor, &bytes)?;
    let world = oasis7::runtime_host::world::from_verified_formal_snapshot(
        snapshot, journal, parent, reader,
    )
    .map_err(|e| format!("{e:?}"))?
    .with_release_security_policy(release.clone());
    super::super::formal_business_package::tree(
        &candidate.material,
        &candidate.candidate.record,
        &world,
        false,
    )?;
    Ok(world)
}
