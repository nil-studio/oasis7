//! Actual private-input World execution for the independently admitted wire 3.
//! No method here publishes a canonical head or grants signing authority.
use super::*;
use oasis7_consensus::formal_bft_payload_v3::{
    ValidatedBftPrivatePayloadV3, decode_private_actions_v3,
};
use oasis7_formal_protocol::bft::{
    BftHeaderHeightScheduleV3, BftProfileTargetV2, BoundFormalBftHeaderV3,
    FormalBftCertifiedHeaderV3, VerifiedBftHandoffTargetV2, VerifiedBftHeaderCertificateV3,
    verify_formal_bft_handoff_v2, verify_formal_bft_header_certificate_v3,
};
use oasis7_proto::distributed_bft::v3::{
    BFT_PROTOCOL_VERSION_V3, BftBlockHeaderV3, actions_cbor_digest_v3,
};

/// Public commitments only; private input is retained separately by the caller.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct FormalBftHeaderCandidateV3 {
    pub(crate) header: BftBlockHeaderV3,
    pub(crate) commitment: FormalBftExecutionCommitmentV1,
}

pub(crate) struct FormalBftHeaderExecutionV3 {
    handoff: VerifiedBftHandoffTargetV2,
    schedule: BftHeaderHeightScheduleV3,
    parent_record: CanonicalRecordV1,
    parent_world: RuntimeWorld,
    parent_result_tree: FormalBusinessResultTreeV1,
    business_reader: formal_business_package::SharedBusinessReader,
    retained_nodes: Arc<formal_dependencies::RecordingBusinessIndexReaderV1>,
    tip: Option<VerifiedBftHeaderCertificateV3>,
}

/// A replay result remains private. The durable publisher must reread and
/// reproduce every height before creating its own publication capability.
pub(crate) struct ExecutedFormalHeaderV3 {
    pub(crate) candidate: FormalBftHeaderCandidateV3,
    pub(crate) world: RuntimeWorld,
    pub(crate) snapshot_bytes: Vec<u8>,
    pub(crate) journal_bytes: Vec<u8>,
    pub(crate) result_tree: FormalBusinessResultTreeV1,
    pub(crate) result_delta:
        oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexDeltaV1,
}

impl FormalBftHeaderExecutionV3 {
    pub(crate) fn from_handoff(
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        material: &ClosedRecord,
        source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        seed: Vec<(String, Vec<u8>)>,
        strict: bool,
    ) -> Result<Self, String> {
        let handoff = verify_formal_bft_handoff_v2(&anchor, target)?;
        validate_handoff_material_for_scope(&anchor, &handoff.target().scope, material)?;
        let retained_nodes = if strict {
            formal_dependencies::RecordingBusinessIndexReaderV1::new_strict(source, seed)?
        } else {
            formal_dependencies::RecordingBusinessIndexReaderV1::new(source, seed)?
        };
        let business_reader = formal_business_package::SharedBusinessReader(retained_nodes.clone());
        let restored = restore_handoff_world(&anchor, material, &business_reader)?;
        let semantic = restored
            .world
            .execution_state_identity()
            .map_err(|error| format!("read wire 3 H semantic root: {error:?}"))?
            .state_root_hash;
        let manifest = restored
            .world
            .current_manifest_hash()
            .map_err(|error| format!("read wire 3 H manifest: {error:?}"))?;
        let schedule = handoff.initial_schedule_after_handoff_replay(&semantic, &manifest)?;
        // Validate the real H and parent policy immediately, independently of
        // any future QC. Keep the execution parent itself unattached.
        let mut checked = restored.world.clone();
        checked
            .attach_formal_bft_header_handoff_parent_v3(&handoff, &schedule)
            .map_err(|error| format!("bind actual wire 3 H: {error:?}"))?;
        Ok(Self {
            handoff,
            schedule,
            parent_record: anchor.head().record().clone(),
            parent_world: restored.world,
            parent_result_tree: restored.result_tree,
            business_reader,
            retained_nodes,
            tip: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn schedule(&self) -> &BftHeaderHeightScheduleV3 {
        &self.schedule
    }
    #[cfg(test)]
    pub(crate) fn parent_world(&self) -> &RuntimeWorld {
        &self.parent_world
    }

    pub(crate) fn build_candidate(
        &self,
        private_actions: &[u8],
    ) -> Result<FormalBftHeaderCandidateV3, String> {
        Ok(self.execute(private_actions)?.candidate)
    }

    pub(crate) fn verify_and_execute(
        &self,
        entry: &FormalBftCertifiedHeaderV3,
        private_actions: &[u8],
    ) -> Result<(VerifiedBftHeaderCertificateV3, ExecutedFormalHeaderV3), String> {
        let verified = verify_formal_bft_header_certificate_v3(&self.schedule, entry)?;
        ValidatedBftPrivatePayloadV3::decode(verified.bound_header().clone(), private_actions)?;
        let executed = self.execute(private_actions)?;
        if executed.candidate.header != entry.header
            || executed.candidate.commitment != entry.execution_commitment
        {
            return Err("wire 3 certificate differs from actual World execution".into());
        }
        Ok((verified, executed))
    }

    /// Advance only through actual, sequential H-anchored execution. This is
    /// replay state, not proof that the record was durably committed.
    pub(crate) fn replay_next(
        &mut self,
        entry: &FormalBftCertifiedHeaderV3,
        private_actions: &[u8],
    ) -> Result<ExecutedFormalHeaderV3, String> {
        let (verified, executed) = self.verify_and_execute(entry, private_actions)?;
        for node in &executed.result_delta.node_writes {
            self.business_reader
                .cache_content_node(&node.digest, &node.bytes)?;
        }
        self.schedule = verified.next_height_schedule().clone();
        self.parent_world = executed.world.clone();
        self.parent_result_tree = executed.result_tree.clone();
        self.tip = Some(verified);
        Ok(executed)
    }

    fn execute(&self, private_actions: &[u8]) -> Result<ExecutedFormalHeaderV3, String> {
        let actions_digest = actions_cbor_digest_v3(private_actions)?;
        let actions = decode_private_actions_v3(private_actions)?;
        let action_root = super::super::super::action_root(&actions)?;
        let schedule = &self.schedule;
        let parent = &self.parent_world;
        let identity = parent
            .execution_state_identity()
            .map_err(|error| format!("read wire 3 parent semantic root: {error:?}"))?;
        let manifest = parent
            .current_manifest_hash()
            .map_err(|error| format!("read wire 3 parent manifest: {error:?}"))?;
        if schedule.activation_digest() != self.handoff.activation_digest()
            || schedule.scope() != &self.handoff.target().scope
            || schedule.parent_tick() != parent.state().time
            || schedule.parent_state_root() != identity.state_root_hash
            || schedule.parent_manifest_hash() != manifest
            || schedule.height()
                != parent
                    .state()
                    .time
                    .checked_add(1)
                    .ok_or("wire 3 execution tick overflow")?
            || parent.pending_actions_len() != 0
            || parent.pending_effects_len() != 0
        {
            return Err("wire 3 execution differs from its exact replayed parent".into());
        }
        super::super::validate_world_scope_for_scope(parent, schedule.scope())?;
        let input_hash =
            execution_input_hash(&self.handoff, schedule, &action_root, &actions_digest)?;
        let commit_context = super::super::make_commit_context_for_binding(
            schedule.scope(),
            schedule.authority_epoch(),
            FORMAL_BFT_EXECUTION_TICK_AUTHORITY,
            schedule.height(),
            input_hash.clone(),
            &actions,
        )?;
        let runtime_actions = super::super::super::decode_closed_runtime_actions(&commit_context)?;
        super::super::governance::validate_validator_transition_action_schedule_v2(
            schedule,
            &runtime_actions,
        )?;
        let mut next_world = parent.clone();
        match &self.tip {
            Some(tip) => {
                if tip.next_height_schedule() != schedule {
                    return Err("wire 3 replay tip has a foreign successor schedule".into());
                }
                next_world.attach_formal_bft_header_successor_parent_v3(&self.handoff, tip)
            }
            None => next_world.attach_formal_bft_header_handoff_parent_v3(&self.handoff, schedule),
        }
        .map_err(|error| format!("attach actual wire 3 World parent: {error:?}"))?;
        let executed = super::super::execution::finish_prepared_world_transition(
            super::super::execution::PreparedWorldTransition {
                parent,
                next_world,
                scope: schedule.scope(),
                authority_epoch: schedule.authority_epoch(),
                schedule,
                height: schedule.height(),
                input_hash: &input_hash,
                action_root: &action_root,
                tick_authority: FORMAL_BFT_EXECUTION_TICK_AUTHORITY,
                commit_context,
                actions,
                runtime_actions,
            },
        )?;
        let projection_root =
            oasis7::world_service::formal_projection::commitments_for_world(&executed.world)?
                .root()?;
        let new_results = newly_recorded_results(parent, &executed.world)?;
        let parent_result_root = self.parent_result_tree.root()?;
        let result_update = FormalBusinessResultTreeV1::extend_bft_successor(
            oasis7::world_service::formal_result::FormalBftResultSuccessorInputV1 {
                world: &executed.world,
                identity: &self.parent_record.world,
                height: schedule.height(),
                parent_result_root: &parent_result_root,
                execution_state_root: &executed.execution_state_root,
                previous: &self.parent_result_tree,
                new_results: &new_results,
                reader: &self.business_reader,
            },
        )?;
        let commitment = FormalBftExecutionCommitmentV1 {
            domain: BFT_FORMAL_EXECUTION_DOMAIN_V1.to_owned(),
            schema_version: 1,
            profile: self.handoff.target().profile.clone(),
            profile_version: self.handoff.target().profile_version,
            activation_digest: self.handoff.activation_digest().to_owned(),
            scope: schedule.scope().clone(),
            height: schedule.height(),
            parent_commit_ref: schedule.parent_commit_ref().to_owned(),
            parent_tick: schedule.parent_tick(),
            parent_semantic_state_root: schedule.parent_state_root().to_owned(),
            authority_epoch: schedule.authority_epoch(),
            active_validator_set_hash: schedule.active_set().set_hash().to_owned(),
            schedule_policy_hash: schedule.policy_hash().to_owned(),
            manifest_hash: schedule.parent_manifest_hash().to_owned(),
            post_manifest_hash: executed.post_manifest_hash,
            action_root,
            event_root: executed.event_root,
            execution_state_root: executed.execution_state_root,
            state_root: executed.state_root,
            projection_root,
            result_root: result_update.tree.root()?,
            transition: executed.transition,
            transition_ref: executed.transition_ref,
        };
        commitment.validate()?;
        let header = BftBlockHeaderV3 {
            protocol_version: BFT_PROTOCOL_VERSION_V3,
            execution_commitment_hash: commitment.digest()?,
            actions_cbor_digest: actions_digest,
        };
        let bound = BoundFormalBftHeaderV3::bind(header.clone(), commitment.clone())?;
        ValidatedBftPrivatePayloadV3::decode(bound, private_actions)?;
        Ok(ExecutedFormalHeaderV3 {
            candidate: FormalBftHeaderCandidateV3 { header, commitment },
            world: executed.world,
            snapshot_bytes: executed.snapshot_bytes,
            journal_bytes: executed.journal_bytes,
            result_tree: result_update.tree,
            result_delta: result_update.delta,
        })
    }
}

fn execution_input_hash(
    handoff: &VerifiedBftHandoffTargetV2,
    schedule: &BftHeaderHeightScheduleV3,
    action_root: &str,
    actions_digest: &str,
) -> Result<String, String> {
    let payload = (
        "oasis7.formal.bft.execution_input.v3",
        handoff.activation_digest(),
        schedule.scope(),
        schedule.height(),
        schedule.parent_commit_ref(),
        schedule.parent_tick(),
        schedule.parent_state_root(),
        schedule.authority_epoch(),
        schedule.active_set().set_hash(),
        schedule.policy_hash(),
        schedule.parent_manifest_hash(),
        action_root,
        actions_digest,
    );
    Ok(blake3_hex(&serde_cbor::to_vec(&payload).map_err(
        |error| format!("encode wire 3 execution input: {error}"),
    )?))
}

#[path = "driver_bft_v3_formal_private.rs"]
mod private;
pub(crate) use private::{
    PRIVATE_HEADER_PREFIX_MAX_BYTES_V3, PrivateHeaderEntryV3, PrivateHeaderPrefixV3,
};

#[path = "driver_bft_v3_formal_storage.rs"]
mod storage;

#[path = "driver_bft_v3_formal_prefix.rs"]
mod prefix;
pub(crate) use prefix::{FormalBftHeaderLedgerExecutionV3, FormalBftHeaderNodeDriverV3};
