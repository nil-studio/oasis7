//! Restore the exact qualified H independently of a ledger format. A prefix
//! writer must be able to start here before its first H+1 QC exists; the fixed
//! H+1 read ledger is not an origin requirement for the new canonical ledger.
use super::*;

pub(super) struct QualifiedFormalBftOriginV1 {
    pub(super) target: BftProfileTargetV1,
    pub(super) handoff: VerifiedBftHandoffTargetV1,
    pub(super) schedule: BftHeightProposerScheduleV2,
    pub(super) parent_record: CanonicalRecordV1,
    pub(super) parent_world: RuntimeWorld,
    pub(super) retained_parent: oasis7_runtime::VerifiedFormalRetainedHistoryParentV1,
    pub(super) business_source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    pub(super) parent_result_tree: FormalBusinessResultTreeV1,
    pub(super) business_reader: formal_business_package::SharedBusinessReader,
    pub(super) retained_nodes: Arc<formal_dependencies::RecordingBusinessIndexReaderV1>,
    pub(super) pin: formal_storage::FormalBftReadPinV1,
}

pub(super) fn restore(
    anchor: VerifiedFormalReadAnchorV1,
    target: BftProfileTargetV1,
    h_material: ClosedRecord,
    business_reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    seed: Vec<(String, Vec<u8>)>,
    strict: bool,
) -> Result<QualifiedFormalBftOriginV1, String> {
    let business_source = business_reader.clone();
    let retained_nodes = if strict {
        formal_dependencies::RecordingBusinessIndexReaderV1::new_strict(business_reader, seed)?
    } else {
        formal_dependencies::RecordingBusinessIndexReaderV1::new(business_reader, seed)?
    };
    let business_reader = formal_business_package::SharedBusinessReader(retained_nodes.clone());
    let parent_record = anchor.head().record().clone();
    let handoff = verify_formal_bft_handoff_v1(&anchor, target.clone())?;
    validate_handoff_material(&anchor, &handoff, &h_material)?;
    let restored = restore_handoff_world(&anchor, &h_material, &business_reader)?;
    let parent_world = restored.world;
    let retained_parent = restored.retained_parent;
    let parent_result_tree = restored.result_tree;
    let parent_semantic_root = parent_world
        .execution_state_identity()
        .map_err(|error| format!("read qualified H semantic root: {error:?}"))?
        .state_root_hash;
    let parent_journal_bytes =
        super::super::super::super::super::journal_to_cbor(parent_world.journal())?;
    let parent_journal_root = blake3_hex(&parent_journal_bytes);
    let parent_manifest_hash = parent_world
        .current_manifest_hash()
        .map_err(|error| format!("read qualified H manifest: {error:?}"))?;
    let parent_result_root = parent_result_tree.root()?;
    let parent_projection_root =
        oasis7::world_service::formal_projection::commitments_for_world(&parent_world)?.root()?;
    let closed_material_digest = closed_material_digest(&parent_record, &h_material)?;
    let pin = formal_storage::FormalBftReadPinV1::from_handoff(
        &handoff,
        parent_semantic_root.clone(),
        parent_manifest_hash.clone(),
        parent_journal_root,
        parent_result_root,
        parent_projection_root,
        closed_material_digest,
    )?;
    let schedule = handoff
        .initial_schedule_after_handoff_replay(&parent_semantic_root, &parent_manifest_hash)?;
    validate_schedule_against_handoff(&schedule, &handoff, &parent_world)?;
    Ok(QualifiedFormalBftOriginV1 {
        target,
        handoff,
        schedule,
        parent_record,
        parent_world,
        retained_parent,
        business_source,
        parent_result_tree,
        business_reader,
        retained_nodes,
        pin,
    })
}
