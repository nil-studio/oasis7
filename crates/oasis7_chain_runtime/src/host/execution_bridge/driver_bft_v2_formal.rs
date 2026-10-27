//! The first BFT successor of a qualified formal-profile H record.
//!
//! This is a read/publication cut, not an engineering BFT genesis and not a
//! controlled-profile writer. It accepts one exact H+1 QC and derives the
//! committed World again from the authenticated H closure before returning a
//! publication cut.
use super::super::super::super::{
    controlled_capture, formal_business_package, formal_native_environment, formal_package,
};
use super::*;
use oasis7::world_service::{CanonicalIntentResultV1, formal_result::FormalBusinessResultTreeV1};
use oasis7_consensus::tendermint_v2::{
    BFT_FORMAL_EXECUTION_DOMAIN_V1, BftHeightProposerScheduleV2, BftProfileTargetV1,
    FormalBftExecutionCommitmentV1, VerifiedBftHandoffTargetV1, verify_commit_certificate_token_v2,
    verify_formal_bft_handoff_v1,
};
use oasis7_distfs::controlled_authority::{
    formal::{
        CanonicalRecordV1, FormalRecordKindV1, VerifiedFormalReadAnchorV1,
        business_index::BusinessIndexNodeReader, verify_record_material,
    },
    replicated_protocol::{ArtifactRole, ClosedRecord},
};
use oasis7_node::{BftBlockV2, BftCommitCertificateV2, BftScopeV2};
use oasis7_proto::distributed_bft::v2::{BFT_MAX_ACTIONS_CBOR_BYTES_V2, BFT_PROTOCOL_VERSION_V2};
use oasis7_runtime::{LocalCasStore, World as RuntimeWorld, blake3_hex};
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;

const FORMAL_BFT_EXECUTION_TICK_AUTHORITY: &str = "system:formal-bft-execution-v1";
const FORMAL_BFT_INPUT_DOMAIN_V1: &str = "oasis7.formal.bft.execution_input.v1";

#[derive(Serialize)]
struct FormalBftExecutionInputV1<'a> {
    domain: &'static str,
    activation_digest: &'a str,
    scope: &'a BftScopeV2,
    height: u64,
    parent_commit_ref: &'a str,
    parent_tick: u64,
    parent_semantic_state_root: &'a str,
    authority_epoch: u64,
    active_validator_set_hash: &'a str,
    schedule_policy_hash: &'a str,
    manifest_hash: &'a str,
    action_root: &'a str,
}

struct ExecutedFormalCandidateV1 {
    block: BftBlockV2,
    commitment: FormalBftExecutionCommitmentV1,
    world: RuntimeWorld,
    snapshot_bytes: Vec<u8>,
    journal_bytes: Vec<u8>,
    new_results: Vec<CanonicalIntentResultV1>,
    result_tree: FormalBusinessResultTreeV1,
    result_delta: oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexDeltaV1,
}

/// The only value passed to the formal read publisher after a durable reread.
/// Private fields prevent a caller from turning arbitrary roots into an
/// execution cut. The publisher still verifies the H handoff and all World
/// commitments independently.
pub struct FormalBftExecutionCutV1 {
    certificate: BftCommitCertificateV2,
    commitment: FormalBftExecutionCommitmentV1,
    world: RuntimeWorld,
    new_results: Vec<CanonicalIntentResultV1>,
    h_result_tree: FormalBusinessResultTreeV1,
    h_index_reader: formal_business_package::SharedBusinessReader,
    durable_record_digest: String,
}

impl FormalBftExecutionCutV1 {
    pub fn certificate(&self) -> &BftCommitCertificateV2 {
        &self.certificate
    }

    pub fn commitment(&self) -> &FormalBftExecutionCommitmentV1 {
        &self.commitment
    }

    pub fn world(&self) -> &RuntimeWorld {
        &self.world
    }

    pub fn new_results(&self) -> &[CanonicalIntentResultV1] {
        &self.new_results
    }

    pub fn h_result_tree(&self) -> &FormalBusinessResultTreeV1 {
        &self.h_result_tree
    }

    pub fn h_index_reader(&self) -> &dyn BusinessIndexNodeReader {
        &self.h_index_reader
    }

    pub fn durable_record_digest(&self) -> &str {
        &self.durable_record_digest
    }
}

pub(crate) struct FormalBftReadExecutionV1 {
    target: BftProfileTargetV1,
    handoff: VerifiedBftHandoffTargetV1,
    schedule: BftHeightProposerScheduleV2,
    parent_record: CanonicalRecordV1,
    parent_world: RuntimeWorld,
    parent_result_tree: FormalBusinessResultTreeV1,
    business_reader: formal_business_package::SharedBusinessReader,
    retained_nodes: Arc<formal_dependencies::RecordingBusinessIndexReaderV1>,
    store: LocalCasStore,
    ledger: formal_storage::FormalBftReadLedgerV1,
}

#[path = "driver_bft_v2_formal_dependencies.rs"]
mod formal_dependencies;
#[path = "driver_bft_v2_formal_storage.rs"]
mod formal_storage;

#[path = "driver_bft_v2_formal_replay.rs"]
mod formal_replay;

#[cfg(test)]
#[path = "driver_bft_v2_formal_tests.rs"]
mod tests;

impl NodeRuntimeExecutionDriver {
    /// Open the separate formal-read ledger from an independently qualified H
    /// and its complete closed material. No caller-provided World is trusted
    /// or used as a source of identity.
    pub(crate) fn open_formal_bft_read_v1(
        &mut self,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV1,
        h_material: ClosedRecord,
        business_reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        allow_initialize: bool,
    ) -> Result<FormalBftReadExecutionV1, String> {
        if self.bft_execution.is_some() || self.bft_execution_v2.is_some() || self.bft_mode_required
        {
            return Err("formal BFT read execution cannot share an engineering BFT driver".into());
        }
        let records_dir = self.records_dir.as_path();
        if super::super::storage::BftExecutionLedger::required_material_exists(records_dir)?
            || super::storage::BftV2ExecutionLedger::required_material_exists(records_dir)?
        {
            return Err("formal BFT read execution refuses an engineering BFT ledger".into());
        }
        FormalBftReadExecutionV1::open_from_handoff(
            records_dir,
            self.execution_store.clone(),
            anchor,
            target,
            h_material,
            business_reader,
            allow_initialize,
        )
    }
}

impl FormalBftReadExecutionV1 {
    pub(super) fn has_required_material(records_dir: &Path) -> Result<bool, String> {
        formal_storage::FormalBftReadLedgerV1::required_material_exists(records_dir)
    }

    pub(crate) fn open_from_handoff(
        records_dir: &Path,
        store: LocalCasStore,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV1,
        h_material: ClosedRecord,
        business_reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        allow_initialize: bool,
    ) -> Result<Self, String> {
        if super::super::storage::BftExecutionLedger::required_material_exists(records_dir)?
            || super::storage::BftV2ExecutionLedger::required_material_exists(records_dir)?
        {
            return Err("formal BFT read execution refuses an engineering BFT ledger".into());
        }
        let preloaded = formal_storage::FormalBftReadLedgerV1::preload_business_index_nodes(
            records_dir,
            &store,
        )?;
        let seed: Vec<_> = preloaded.nodes.into_iter().collect();
        let retained_nodes = if preloaded.record_present {
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
        let parent_result_tree = restored.result_tree;
        let parent_semantic_root = parent_world
            .execution_state_identity()
            .map_err(|error| format!("read qualified H semantic root: {error:?}"))?
            .state_root_hash;
        let parent_journal_bytes =
            super::super::super::super::journal_to_cbor(parent_world.journal())?;
        let parent_journal_root = blake3_hex(&parent_journal_bytes);
        let parent_manifest_hash = parent_world
            .current_manifest_hash()
            .map_err(|error| format!("read qualified H manifest: {error:?}"))?;
        let parent_result_root = parent_result_tree.root()?;
        let parent_projection_root =
            oasis7::world_service::formal_projection::commitments_for_world(&parent_world)?
                .root()?;
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
        let observed_h_refs: std::collections::BTreeSet<_> =
            retained_nodes.recorded_refs()?.into_iter().collect();
        let ledger = formal_storage::FormalBftReadLedgerV1::open(
            records_dir,
            pin,
            &store,
            allow_initialize,
            &preloaded.refs,
            preloaded.record_present,
            &observed_h_refs,
        )?;
        let execution = Self {
            target,
            handoff,
            schedule,
            parent_record,
            parent_world,
            parent_result_tree,
            business_reader,
            retained_nodes,
            store,
            ledger,
        };
        if execution.ledger.record().is_some() {
            execution.verified_cut(execution.height())?;
        }
        Ok(execution)
    }

    pub(crate) fn schedule(&self) -> &BftHeightProposerScheduleV2 {
        &self.schedule
    }

    pub(crate) fn height(&self) -> u64 {
        self.handoff.activation_height().unwrap_or_default()
    }

    pub(crate) fn build_candidate(
        &self,
        actions: Vec<NodeConsensusAction>,
    ) -> Result<BftBlockV2, String> {
        let actions_cbor = serde_cbor::to_vec(&actions)
            .map_err(|error| format!("encode formal BFT actions: {error}"))?;
        if actions_cbor.is_empty() || actions_cbor.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V2 {
            return Err("formal BFT actions exceed the protocol byte bound".into());
        }
        let action_root = super::super::action_root(actions.as_slice())?;
        let height = self.height();
        let parent_ref = self.handoff.handoff_head().record_id.as_str();
        let (input_transition, input_transition_ref) = super::transition_metadata_for_schedule(
            &self.schedule,
            actions.as_slice(),
            has_set_change_action_from_actions(&self.schedule, &actions)?,
            None,
        )?;
        let mut block = BftBlockV2 {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
            scope: self.target.scope().clone(),
            height,
            parent_commit_ref: parent_ref.to_owned(),
            authority_epoch: self.target.authority_epoch(),
            active_validator_set_hash: self.schedule.active_set().set_hash().to_owned(),
            schedule_policy_hash: self.schedule.policy_hash().to_owned(),
            manifest_hash: self
                .parent_world
                .current_manifest_hash()
                .map_err(|error| format!("read formal BFT H manifest: {error:?}"))?,
            post_manifest_hash: "00".repeat(32),
            actions_cbor,
            action_root,
            event_root: "00".repeat(32),
            execution_block_hash: "00".repeat(32),
            execution_state_root: "00".repeat(32),
            state_root: "00".repeat(32),
            transition: input_transition,
            transition_ref: input_transition_ref,
        };
        let executed = self.execute_candidate(&block)?;
        block.event_root = executed.commitment.event_root().to_owned();
        block.execution_block_hash = executed.commitment.digest()?;
        block.execution_state_root = executed.commitment.execution_state_root().to_owned();
        block.state_root = executed.commitment.state_root().to_owned();
        block.post_manifest_hash = executed.commitment.post_manifest_hash().to_owned();
        block.transition = executed.commitment.transition().cloned();
        block.transition_ref = executed.commitment.transition_ref().cloned();
        block.validate()?;
        executed
            .commitment
            .verify_block(&block, self.handoff.activation_digest())?;
        Ok(block)
    }

    pub(crate) fn preview_candidate(&self, candidate: &BftBlockV2) -> Result<(), String> {
        let executed = self.execute_candidate(candidate)?;
        if executed.block != *candidate {
            return Err("formal BFT candidate differs from isolated H+1 execution".into());
        }
        executed
            .commitment
            .verify_block(candidate, self.handoff.activation_digest())
    }

    pub(crate) fn commit_certificate(
        &mut self,
        certificate: BftCommitCertificateV2,
    ) -> Result<FormalBftExecutionCutV1, String> {
        let block = certificate.proposal.block.clone();
        let block = &block;
        let record = self.ledger.record().cloned();
        let commitment = match record.as_ref() {
            Some(record) => record.commitment.clone(),
            None => self.expected_commitment(block)?,
        };
        let schedule = self
            .handoff
            .initial_schedule_for_commitment(&commitment, block)?;
        if schedule != self.schedule {
            return Err("formal BFT certificate schedule differs from replayed H handoff".into());
        }
        let token = verify_commit_certificate_token_v2(&schedule, &certificate)?;
        if token.block_id() != block.block_id()?.as_str() {
            return Err("formal BFT certificate token does not identify its block".into());
        }
        if let Some(existing) = record {
            if existing.block_id != token.block_id() || existing.commitment != commitment {
                return Err("formal BFT H+1 already committed a different block".into());
            }
            return self.verified_cut(existing.height);
        }

        let executed = self.execute_candidate(block)?;
        if executed.commitment != commitment {
            return Err("formal BFT candidate commitment changed before durable commit".into());
        }
        let business_index_nodes = self
            .retained_nodes
            .recorded_nodes()?
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        self.ledger.commit(
            formal_storage::FormalBftReadCandidateV1 {
                certificate,
                commitment,
                block,
                block_id: token.block_id().to_owned(),
                snapshot_bytes: executed.snapshot_bytes.as_slice(),
                journal_bytes: executed.journal_bytes.as_slice(),
                business_index_nodes: &business_index_nodes,
                new_results: executed.new_results.as_slice(),
            },
            &self.store,
        )?;
        self.verified_cut(self.height())
    }

    /// Re-read the immutable H+1 record and replay its signed block from H.
    /// This is the only route to a publication cut, including after restart.
    pub(crate) fn verified_cut(&self, height: u64) -> Result<FormalBftExecutionCutV1, String> {
        if height != self.height() {
            return Err("formal BFT read cut is pinned to the exact H+1 height".into());
        }
        let record = self
            .ledger
            .reread_record(&self.store)?
            .ok_or("formal BFT H+1 has no durable committed record")?;
        let block = &record.certificate.proposal.block;
        if record.height != height {
            return Err("formal BFT durable record height differs from requested cut".into());
        }
        let schedule = self
            .handoff
            .initial_schedule_for_commitment(&record.commitment, block)?;
        if schedule != self.schedule {
            return Err("formal BFT durable record schedule differs from H handoff".into());
        }
        let token = verify_commit_certificate_token_v2(&schedule, &record.certificate)?;
        if token.block_id() != record.block_id {
            return Err("formal BFT durable QC block ID mismatch".into());
        }
        let executed = self.execute_candidate(block)?;
        if executed.block != *block
            || executed.commitment != record.commitment
            || executed.new_results != record.new_results
        {
            return Err("formal BFT durable record does not reproduce from qualified H".into());
        }
        let observed_refs: std::collections::BTreeSet<_> =
            self.retained_nodes.recorded_refs()?.into_iter().collect();
        self.ledger
            .validate_observed_business_index_refs(&observed_refs)?;
        let (snapshot_bytes, journal_bytes) =
            self.ledger.validate_cas_material(&record, &self.store)?;
        if snapshot_bytes != executed.snapshot_bytes || journal_bytes != executed.journal_bytes {
            return Err("formal BFT durable CAS material differs from replayed World".into());
        }
        let record_bytes = serde_cbor::to_vec(&record)
            .map_err(|error| format!("encode reread formal BFT record digest: {error}"))?;
        let durable_record_digest = blake3_hex(&record_bytes);
        Ok(FormalBftExecutionCutV1 {
            certificate: record.certificate,
            commitment: record.commitment,
            world: executed.world,
            new_results: executed.new_results,
            h_result_tree: self.parent_result_tree.clone(),
            h_index_reader: self.business_reader.clone(),
            durable_record_digest,
        })
    }

    fn expected_commitment(
        &self,
        block: &BftBlockV2,
    ) -> Result<FormalBftExecutionCommitmentV1, String> {
        let executed = self.execute_candidate(block)?;
        Ok(executed.commitment)
    }

    fn execute_candidate(&self, block: &BftBlockV2) -> Result<ExecutedFormalCandidateV1, String> {
        block.validate()?;
        validate_formal_block_binding(block, &self.schedule, &self.handoff, &self.parent_world)?;
        execute_formal_candidate(
            FormalCandidateContext {
                handoff: &self.handoff,
                schedule: &self.schedule,
                parent_world: &self.parent_world,
                parent_result_tree: &self.parent_result_tree,
                identity: &self.parent_record.world,
                business_reader: &self.business_reader,
                prefix: None,
            },
            block,
        )
    }
}

struct FormalCandidateContext<'a> {
    handoff: &'a VerifiedBftHandoffTargetV1,
    schedule: &'a BftHeightProposerScheduleV2,
    parent_world: &'a RuntimeWorld,
    parent_result_tree: &'a FormalBusinessResultTreeV1,
    identity: &'a oasis7_formal_protocol::controlled_authority::formal::FormalWorldIdentityV1,
    business_reader: &'a formal_business_package::SharedBusinessReader,
    prefix: Option<&'a oasis7_formal_protocol::bft::VerifiedFormalBftPrefixV1>,
}

fn execute_formal_candidate(
    context: FormalCandidateContext<'_>,
    block: &BftBlockV2,
) -> Result<ExecutedFormalCandidateV1, String> {
    let FormalCandidateContext {
        handoff,
        schedule,
        parent_world,
        parent_result_tree,
        identity,
        business_reader,
        prefix,
    } = context;
    block.validate()?;
    let actions = super::decode_actions_v2(block.actions_cbor.as_slice())?;
    if super::super::action_root(actions.as_slice())? != block.action_root {
        return Err("formal BFT action root differs from canonical action bytes".into());
    }
    let input_hash = formal_execution_input_hash(handoff, schedule, block)?;
    let executed =
        super::execution::execute_world_transition(super::execution::WorldTransitionRequest {
            parent: parent_world,
            scope: handoff.target().scope(),
            authority_epoch: schedule.authority_epoch(),
            schedule,
            block,
            expected_parent: schedule.parent_commit_ref(),
            input_hash: input_hash.as_str(),
            tick_authority: FORMAL_BFT_EXECUTION_TICK_AUTHORITY,
            formal_handoff: prefix.is_none().then_some(handoff),
            formal_prefix: prefix,
        })?;
    let projection_root =
        oasis7::world_service::formal_projection::commitments_for_world(&executed.world)?.root()?;
    let new_results = newly_recorded_results(parent_world, &executed.world)?;
    let result_update = FormalBusinessResultTreeV1::extend_bft_successor(
        oasis7::world_service::formal_result::FormalBftResultSuccessorInputV1 {
            world: &executed.world,
            identity,
            height: block.height,
            parent_result_root: parent_result_tree.root()?.as_str(),
            execution_state_root: executed.execution_state_root.as_str(),
            previous: parent_result_tree,
            new_results: new_results.as_slice(),
            reader: business_reader,
        },
    )?;
    let commitment = FormalBftExecutionCommitmentV1 {
        domain: BFT_FORMAL_EXECUTION_DOMAIN_V1.to_owned(),
        schema_version: 1,
        profile: handoff.target().profile().to_owned(),
        profile_version: handoff.target().profile_version(),
        activation_digest: handoff.activation_digest().to_owned(),
        scope: handoff.target().scope().clone(),
        height: block.height,
        parent_commit_ref: schedule.parent_commit_ref().to_owned(),
        parent_tick: schedule.parent_tick(),
        parent_semantic_state_root: schedule.parent_state_root().to_owned(),
        authority_epoch: schedule.authority_epoch(),
        active_validator_set_hash: schedule.active_set().set_hash().to_owned(),
        schedule_policy_hash: schedule.policy_hash().to_owned(),
        manifest_hash: schedule.parent_manifest_hash().to_owned(),
        post_manifest_hash: executed.post_manifest_hash,
        action_root: block.action_root.clone(),
        event_root: executed.event_root,
        execution_state_root: executed.execution_state_root,
        state_root: executed.state_root,
        projection_root,
        result_root: result_update.tree.root()?,
        transition: executed.transition,
        transition_ref: executed.transition_ref,
    };
    commitment.validate()?;
    let digest = commitment.digest()?;
    let mut completed_block = block.clone();
    completed_block.execution_block_hash = digest;
    completed_block.event_root = commitment.event_root().to_owned();
    completed_block.execution_state_root = commitment.execution_state_root().to_owned();
    completed_block.state_root = commitment.state_root().to_owned();
    completed_block.post_manifest_hash = commitment.post_manifest_hash().to_owned();
    completed_block.transition = commitment.transition().cloned();
    completed_block.transition_ref = commitment.transition_ref().cloned();
    completed_block.validate()?;
    commitment.verify_block(&completed_block, handoff.activation_digest())?;
    Ok(ExecutedFormalCandidateV1 {
        block: completed_block,
        commitment,
        world: executed.world,
        snapshot_bytes: executed.snapshot_bytes,
        journal_bytes: executed.journal_bytes,
        new_results,
        result_tree: result_update.tree,
        result_delta: result_update.delta,
    })
}

struct RestoredHandoffWorld {
    world: RuntimeWorld,
    result_tree: FormalBusinessResultTreeV1,
}

fn validate_handoff_material(
    anchor: &VerifiedFormalReadAnchorV1,
    handoff: &VerifiedBftHandoffTargetV1,
    material: &ClosedRecord,
) -> Result<(), String> {
    let record = anchor.head().record();
    verify_record_material(record, material).map_err(|error| error.to_string())?;
    if record.kind != FormalRecordKindV1::Execution
        || record.handoff.is_none()
        || record.record_id().map_err(|error| error.to_string())? != anchor.head().head().record_id
        || record.world != anchor.authority().definition().world
        || record.world.world_id != handoff.scope().world_id
        || record.world.chain_id != handoff.scope().chain_id
        || record.world.genesis_digest != handoff.scope().genesis_hash
        || material.roots != record.artifact_roots
    {
        return Err("qualified H record or closed material differs from formal BFT handoff".into());
    }
    let manifest: formal_package::FormalExecutionManifestV1 =
        controlled_capture::decode_role(material, ArtifactRole::ExecutionManifest)?;
    if !matches!(
        (manifest.schema_version, manifest.domain.as_str()),
        (1, "oasis7.formal.execution_input.v1") | (2, "oasis7.formal.execution_input.v2")
    ) || manifest.world != record.world
        || manifest.authority_id != record.original_authority_id
        || manifest.activation_ref != record.activation_ref
        || manifest.execution_policy_digest != record.execution_policy_digest
        || manifest.executable_digest != crate::controlled_execution_policy::executable_digest()?
        || manifest.parent.position.checked_add(1) != Some(record.position)
        || manifest.parent.record_id != record.parent_record_id
    {
        return Err("qualified H execution manifest does not match its record".into());
    }
    formal_native_environment::verify_current(material, &manifest)?;
    Ok(())
}

fn restore_handoff_world(
    anchor: &VerifiedFormalReadAnchorV1,
    material: &ClosedRecord,
    business_reader: &formal_business_package::SharedBusinessReader,
) -> Result<RestoredHandoffWorld, String> {
    let record = anchor.head().record();
    let snapshot_bytes = material
        .snapshot_bytes()
        .map_err(|error| error.to_string())?;
    if blake3_hex(snapshot_bytes.as_slice()) != record.after_state_root {
        return Err("qualified H expanded Snapshot differs from its after-state root".into());
    }
    let snapshot: oasis7_runtime::Snapshot =
        oasis7_world_state::codec::snapshot_from_cbor(snapshot_bytes.as_slice())
            .map_err(|error| format!("decode qualified H Snapshot: {error}"))?;
    if super::super::super::super::snapshot_to_cbor(&snapshot)? != snapshot_bytes {
        return Err("qualified H Snapshot is not canonical".into());
    }
    formal_package::validate_modules(&snapshot)?;
    let journal_ref = record
        .artifact_roots
        .get(&ArtifactRole::Journal)
        .ok_or("qualified H Journal role missing")?;
    let journal_object = material
        .objects
        .iter()
        .find(|object| object.content_hash == *journal_ref)
        .ok_or("qualified H Journal artifact missing")?;
    let journal: oasis7_runtime::Journal =
        oasis7_world_state::codec::journal_from_cbor(journal_object.bytes.as_slice())
            .map_err(|error| format!("decode qualified H Journal: {error}"))?;
    if super::super::super::super::journal_to_cbor(&journal)? != journal_object.bytes {
        return Err("qualified H Journal is not canonical".into());
    }
    let manifest: formal_package::FormalExecutionManifestV1 =
        controlled_capture::decode_role(material, ArtifactRole::ExecutionManifest)?;
    let retained =
        oasis7_runtime::VerifiedFormalRetainedHistoryParentV1::verify(anchor, &snapshot_bytes)?;
    let world = oasis7_runtime::World::from_verified_formal_snapshot(
        snapshot,
        journal,
        retained,
        business_reader.0.clone(),
    )
    .map_err(|error| format!("restore qualified H World: {error:?}"))?
    .with_release_security_policy(manifest.release_security_policy.clone());
    validate_handoff_world(anchor, &world)?;
    let tree = formal_business_package::tree(material, record, &world, true)?
        .ok_or("qualified H business result tree missing")?;
    if tree.root()? != record.result_root {
        return Err("qualified H result tree differs from the committed result root".into());
    }
    let projection_root =
        oasis7::world_service::formal_projection::commitments_for_world(&world)?.root()?;
    if projection_root != record.projection_root {
        return Err("qualified H World differs from its committed projection root".into());
    }
    Ok(RestoredHandoffWorld {
        world,
        result_tree: tree,
    })
}

fn validate_handoff_world(
    anchor: &VerifiedFormalReadAnchorV1,
    world: &RuntimeWorld,
) -> Result<(), String> {
    let record = anchor.head().record();
    if world.state().time != record.position {
        return Err(format!(
            "formal BFT H position/tick mapping is unsupported: H position {}, restored World tick {}",
            record.position,
            world.state().time
        ));
    }
    let manifest = world.chain_resource_manifest();
    let snapshot_bytes = super::super::super::super::snapshot_to_cbor(&world.snapshot())?;
    let journal_bytes = super::super::super::super::journal_to_cbor(world.journal())?;
    let semantic_root = world
        .execution_state_identity()
        .map_err(|error| format!("compute qualified H semantic state root: {error:?}"))?
        .state_root_hash;
    let governing_manifest = world
        .current_cognition_runtime_binding()
        .map_err(|error| format!("read qualified H governing binding: {error:?}"))?
        .runtime_manifest_hash
        .to_string();
    if blake3_hex(&snapshot_bytes) != record.after_state_root
        || governing_manifest.strip_prefix("blake3:")
            != Some(record.governing_manifest_digest.as_str())
        || semantic_root.trim().is_empty()
        || manifest.world_id != record.world.world_id
        || manifest.chain_id != record.world.chain_id
        || manifest.genesis_ref.as_deref() != Some(record.world.genesis_digest.as_str())
        || journal_bytes.is_empty() && !world.journal().events.is_empty()
        || world.pending_actions_len() != 0
        || world.pending_effects_len() != 0
    {
        return Err("restored World differs from exact qualified H identity or boundary".into());
    }
    Ok(())
}

fn validate_schedule_against_handoff(
    schedule: &BftHeightProposerScheduleV2,
    handoff: &VerifiedBftHandoffTargetV1,
    parent_world: &RuntimeWorld,
) -> Result<(), String> {
    let expected_height = handoff.activation_height()?;
    let expected_state_root = parent_world
        .execution_state_identity()
        .map_err(|error| format!("read formal H semantic root: {error:?}"))?
        .state_root_hash;
    let expected_manifest = parent_world
        .current_manifest_hash()
        .map_err(|error| format!("read formal H manifest: {error:?}"))?;
    if schedule.height() != expected_height
        || schedule.parent_tick() != handoff.handoff_position()
        || schedule.parent_commit_ref() != handoff.handoff_head().record_id
        || schedule.parent_state_root() != expected_state_root
        || schedule.parent_manifest_hash() != expected_manifest
        || schedule.scope() != handoff.scope()
        || schedule.authority_epoch() != handoff.target().authority_epoch()
        || schedule.active_set().set_hash() != handoff.initial_validator_set_hash()
        || schedule.policy_hash() != handoff.schedule_policy_hash()
    {
        return Err("formal BFT H+1 schedule differs from the replayed H World".into());
    }
    Ok(())
}

fn validate_formal_block_binding(
    block: &BftBlockV2,
    schedule: &BftHeightProposerScheduleV2,
    handoff: &VerifiedBftHandoffTargetV1,
    parent_world: &RuntimeWorld,
) -> Result<(), String> {
    let parent_manifest = parent_world
        .current_manifest_hash()
        .map_err(|error| format!("read formal BFT parent manifest: {error:?}"))?;
    if block.protocol_version != BFT_PROTOCOL_VERSION_V2
        || block.scope != *handoff.scope()
        || block.height != handoff.activation_height()?
        || block.parent_commit_ref != handoff.handoff_head().record_id
        || block.authority_epoch != handoff.target().authority_epoch()
        || block.active_validator_set_hash != schedule.active_set().set_hash()
        || block.schedule_policy_hash != schedule.policy_hash()
        || block.manifest_hash != parent_manifest
        || parent_manifest != schedule.parent_manifest_hash()
        || block.actions_cbor.is_empty()
        || block.actions_cbor.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V2
    {
        return Err("formal BFT block differs from exact H+1 handoff context".into());
    }
    Ok(())
}

fn formal_execution_input_hash(
    handoff: &VerifiedBftHandoffTargetV1,
    schedule: &BftHeightProposerScheduleV2,
    block: &BftBlockV2,
) -> Result<String, String> {
    let payload = FormalBftExecutionInputV1 {
        domain: FORMAL_BFT_INPUT_DOMAIN_V1,
        activation_digest: handoff.activation_digest(),
        scope: handoff.scope(),
        height: block.height,
        parent_commit_ref: block.parent_commit_ref.as_str(),
        parent_tick: schedule.parent_tick(),
        parent_semantic_state_root: schedule.parent_state_root(),
        authority_epoch: block.authority_epoch,
        active_validator_set_hash: block.active_validator_set_hash.as_str(),
        schedule_policy_hash: block.schedule_policy_hash.as_str(),
        manifest_hash: block.manifest_hash.as_str(),
        action_root: block.action_root.as_str(),
    };
    let bytes = serde_cbor::to_vec(&payload)
        .map_err(|error| format!("encode formal BFT execution input: {error}"))?;
    Ok(blake3_hex(&bytes))
}

fn newly_recorded_results(
    parent: &RuntimeWorld,
    after: &RuntimeWorld,
) -> Result<Vec<CanonicalIntentResultV1>, String> {
    let old = &parent.capability_revocation_state().world_service_results;
    let current = &after.capability_revocation_state().world_service_results;
    let mut results = Vec::new();
    for (key, value) in current {
        if old.get(key) == Some(value) {
            continue;
        }
        let result: CanonicalIntentResultV1 = serde_json::from_value(value.clone())
            .map_err(|error| format!("decode new formal BFT World result: {error}"))?;
        results.push(result);
    }
    if results.len() > 1024 {
        return Err("formal BFT candidate has too many new World results".into());
    }
    Ok(results)
}

fn has_set_change_action_from_actions(
    schedule: &BftHeightProposerScheduleV2,
    actions: &[NodeConsensusAction],
) -> Result<bool, String> {
    let context = super::make_commit_context_for_binding(
        schedule.scope(),
        schedule.authority_epoch(),
        FORMAL_BFT_EXECUTION_TICK_AUTHORITY,
        schedule.height(),
        "00".repeat(32),
        actions,
    )?;
    let decoded = super::super::decode_closed_runtime_actions(&context)?;
    Ok(super::has_validator_set_change_action_v2(&decoded))
}

fn closed_material_digest(
    record: &CanonicalRecordV1,
    material: &ClosedRecord,
) -> Result<String, String> {
    let bytes = serde_cbor::to_vec(&(record, material))
        .map_err(|error| format!("encode qualified H closure digest: {error}"))?;
    if bytes.len()
        > oasis7_distfs::controlled_authority::replicated_protocol::package::MAX_RECORD_BYTES
            + 1024 * 1024
    {
        return Err("qualified H closure exceeds formal BFT input byte bound".into());
    }
    Ok(blake3_hex(&bytes))
}
