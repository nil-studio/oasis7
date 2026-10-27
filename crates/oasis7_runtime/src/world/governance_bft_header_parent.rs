//! Wire-3 formal BFT parent attachment and schedule binding.
use super::*;
use oasis7_formal_protocol::bft::{
    BftHeaderHeightScheduleV3, VerifiedBftHandoffTargetV2, VerifiedBftHeaderCertificateV3,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FormalBftHeaderHandoffBindingV2 {
    target_digest: String,
    activation_digest: String,
    head: FormalHeadRefV1,
    scope: BftScopeV2,
    raw_snapshot_root: String,
    governing_manifest_digest: String,
    authority_epoch: u64,
    initial_validator_set_hash: String,
    schedule_policy_hash: String,
    successor: Option<FormalBftSuccessorBindingV1>,
}

impl FormalBftHeaderHandoffBindingV2 {
    pub(super) fn from_capability(
        capability: &VerifiedBftHandoffTargetV2,
    ) -> Result<Self, WorldError> {
        let target = capability.target();
        Ok(Self {
            target_digest: capability.target_digest().to_owned(),
            activation_digest: capability.activation_digest().to_owned(),
            head: capability.handoff_head().clone(),
            scope: target.scope.clone(),
            raw_snapshot_root: capability.raw_snapshot_root().to_owned(),
            governing_manifest_digest: capability.governing_manifest_digest().to_owned(),
            authority_epoch: target.authority_epoch,
            initial_validator_set_hash: target
                .initial_validator_set_hash()
                .map_err(|error| invalid(format!("derive BFT initial validator set: {error}")))?,
            schedule_policy_hash: target
                .schedule_policy_hash()
                .map_err(|error| invalid(format!("derive BFT schedule policy: {error}")))?,
            successor: None,
        })
    }

    pub(super) fn matches_retained_parent(
        &self,
        parent: &crate::VerifiedFormalRetainedHistoryParentV1,
    ) -> bool {
        self.target_digest.len() == 64
            && self.activation_digest.len() == 64
            && parent.head().is_some_and(|head| head == &self.head)
            && parent.after_snapshot_root() == self.raw_snapshot_root
            && parent.effect_world().is_some_and(|world| {
                world.world_id == self.scope.world_id
                    && world.chain_id == self.scope.chain_id
                    && world.genesis_digest == self.scope.genesis_hash
            })
    }

    pub(super) fn matches_schedule(&self, schedule: &BftGovernanceSchedule) -> bool {
        let Some(schedule) = schedule.header_v3() else {
            return false;
        };
        let Some(activation_height) = self.head.position.checked_add(1) else {
            return false;
        };
        if let Some(successor) = &self.successor {
            return schedule.scope() == &self.scope
                && schedule.activation_digest() == self.activation_digest
                && schedule.height() > activation_height
                && schedule
                    .digest()
                    .is_ok_and(|digest| digest == successor.schedule_digest);
        }
        schedule.scope() == &self.scope
            && schedule.activation_digest() == self.activation_digest
            && schedule.parent_commit_ref() == self.head.record_id
            && schedule.parent_tick() == self.head.position
            && schedule.height() == activation_height
            && schedule.authority_epoch() == self.authority_epoch
            && schedule.active_set().set_hash() == self.initial_validator_set_hash
            && schedule.policy_hash() == self.schedule_policy_hash
    }

    pub(super) fn matches_controller_registry(
        &self,
        retained: &crate::VerifiedFormalRetainedHistoryParentV1,
        captured: Option<&GovernanceMainTokenControllerRegistry>,
    ) -> bool {
        captured.is_some()
            && (self.successor.is_some() || retained.controller_registry() == captured)
    }

    pub(super) fn parent_snapshot_root(&self) -> &str {
        self.successor
            .as_ref()
            .map_or(&self.raw_snapshot_root, |next| &next.raw_snapshot_root)
    }

    pub(super) fn parent_governing_manifest(&self) -> &str {
        self.successor
            .as_ref()
            .map_or(&self.governing_manifest_digest, |next| {
                &next.governing_manifest_digest
            })
    }
}

impl World {
    /// Attach the exact wire-3 activation schedule to the independently
    /// retained H World. The schedule must be derived from this capability
    /// after the caller has replayed H; this method checks that replayed H
    /// against the actual World but does not manufacture a V2 capability.
    pub fn attach_formal_bft_header_handoff_parent_v3(
        &mut self,
        capability: &VerifiedBftHandoffTargetV2,
        schedule: &BftHeaderHeightScheduleV3,
    ) -> Result<(), WorldError> {
        self.ensure_formal_bft_attachable()?;
        if self.pending_actions_len() != 0 || self.pending_effects_len() != 0 {
            return Err(invalid(
                "formal BFT header handoff cannot attach with uncommitted queues",
            ));
        }
        let retained = self
            .formal_history_reader
            .as_ref()
            .and_then(|reader| reader.parent.as_ref())
            .ok_or_else(|| invalid("formal BFT header handoff requires retained H"))?;
        let head = retained
            .head()
            .ok_or_else(|| invalid("retained formal H commit reference is missing"))?;
        let effect_world = retained
            .effect_world()
            .ok_or_else(|| invalid("retained formal H World identity is missing"))?;
        let retained_registry = retained
            .controller_registry()
            .ok_or_else(|| invalid("retained formal H controller policy is missing"))?;
        let current_registry = self
            .governance_main_token_controller_registry()
            .ok_or_else(|| invalid("formal BFT header controller policy is missing"))?;
        let current_binding = self
            .current_cognition_runtime_binding()
            .map_err(|error| invalid(format!("read formal H governing binding: {error:?}")))?;
        let actual_governing = current_binding
            .runtime_manifest_hash
            .to_string()
            .strip_prefix("blake3:")
            .ok_or_else(|| invalid("formal H governing binding is not a canonical digest"))?
            .to_owned();
        let (retained_view, _) = self
            .attached_formal_history()
            .map_err(|error| invalid(format!("validate retained formal H reader: {error}")))?;
        let raw_snapshot = crate::codec::snapshot_cbor(&self.snapshot())
            .map_err(|error| invalid(format!("encode formal H snapshot: {error}")))?;
        let semantic_root = self
            .execution_state_identity()
            .map_err(|error| invalid(format!("compute formal H semantic root: {error:?}")))?
            .state_root_hash;
        let actual_manifest = self
            .current_manifest_hash()
            .map_err(|error| invalid(format!("read actual formal H World manifest: {error:?}")))?;
        let target = capability.target();
        let activation_height = capability
            .activation_height()
            .map_err(|error| invalid(format!("derive formal BFT activation height: {error}")))?;
        let expected_set_hash = target
            .initial_validator_set_hash()
            .map_err(|error| invalid(format!("derive BFT initial set hash: {error}")))?;
        let expected_policy_hash = target
            .schedule_policy_hash()
            .map_err(|error| invalid(format!("derive BFT schedule policy hash: {error}")))?;
        let mismatches = [
            ("H position", head.position != capability.handoff_position()),
            (
                "H commit reference",
                head.record_id != capability.handoff_head().record_id,
            ),
            (
                "World identity",
                effect_world.world_id != target.scope.world_id
                    || effect_world.chain_id != target.scope.chain_id
                    || effect_world.genesis_digest != target.scope.genesis_hash,
            ),
            (
                "raw Snapshot root",
                retained.after_snapshot_root() != capability.raw_snapshot_root()
                    || crate::blake3_hex(raw_snapshot.as_slice()) != capability.raw_snapshot_root(),
            ),
            (
                "retained-history descriptor",
                self.state.retained_history_root.as_ref() != Some(retained_view.root()),
            ),
            (
                "World tick",
                self.state.time != capability.handoff_position(),
            ),
            (
                "semantic state root",
                semantic_root != schedule.parent_state_root(),
            ),
            (
                "World manifest",
                actual_manifest != schedule.parent_manifest_hash(),
            ),
            (
                "governing manifest digest",
                actual_governing != capability.governing_manifest_digest(),
            ),
            ("controller registry", retained_registry != current_registry),
            ("schedule scope", schedule.scope() != &target.scope),
            (
                "activation digest",
                schedule.activation_digest() != capability.activation_digest(),
            ),
            (
                "schedule parent",
                schedule.parent_commit_ref() != capability.handoff_head().record_id
                    || schedule.parent_tick() != capability.handoff_position(),
            ),
            ("activation height", schedule.height() != activation_height),
            (
                "authority epoch",
                schedule.authority_epoch() != target.authority_epoch,
            ),
            (
                "active validator set",
                schedule.active_set().set_hash() != expected_set_hash,
            ),
            (
                "schedule policy hash",
                schedule.policy_hash() != expected_policy_hash,
            ),
            (
                "schedule policy",
                schedule.policy() != &target.schedule_policy,
            ),
            (
                "epoch length",
                self.governance_execution_policy().epoch_length_ticks
                    != schedule.policy().epoch_length_ticks,
            ),
        ]
        .into_iter()
        .filter_map(|(name, mismatch)| mismatch.then_some(name))
        .collect::<Vec<_>>();
        if !mismatches.is_empty() {
            return Err(invalid(format!(
                "formal wire-3 BFT handoff, retained H, schedule, or actual World mismatch: {}",
                mismatches.join(", ")
            )));
        }
        let registry = Some(Self::validate_governance_main_token_controller_registry(
            current_registry.clone(),
        )?);
        let binding = FormalBftHeaderHandoffBindingV2::from_capability(capability)?;
        self.bft_governance_parent = Some(Box::new(BftGovernanceParentV2 {
            schedule: BftGovernanceSchedule::HeaderV3(schedule.clone()),
            controller_registry: registry,
            formal_handoff: Some(FormalBftHandoffBinding::HeaderV3(binding)),
            execution: None,
        }));
        Ok(())
    }

    /// Attach a wire-3 certificate's opaque successor schedule only when the
    /// actual World is exactly that certificate's committed parent. The
    /// caller must obtain this token by replaying every certificate from the
    /// same qualified H; a tip certificate alone is not an H-to-tip execution
    /// proof.
    pub fn attach_formal_bft_header_successor_parent_v3(
        &mut self,
        capability: &VerifiedBftHandoffTargetV2,
        certificate: &VerifiedBftHeaderCertificateV3,
    ) -> Result<(), WorldError> {
        self.ensure_formal_bft_attachable()?;
        if self.pending_actions_len() != 0 || self.pending_effects_len() != 0 {
            return Err(invalid(
                "formal BFT header successor cannot attach with uncommitted queues",
            ));
        }
        let retained = self
            .formal_history_reader
            .as_ref()
            .and_then(|reader| reader.parent.as_ref())
            .ok_or_else(|| invalid("formal BFT header successor requires retained H"))?;
        let mut binding = FormalBftHeaderHandoffBindingV2::from_capability(capability)?;
        if !binding.matches_retained_parent(retained)
            || self.state.retained_history_root.as_ref() != Some(retained.root())
        {
            return Err(invalid(
                "formal BFT header successor differs from the retained H authority",
            ));
        }
        self.attached_formal_history().map_err(|error| {
            invalid(format!(
                "validate formal BFT header history reader: {error}"
            ))
        })?;

        let certified_schedule = certificate.schedule();
        let schedule = certificate.next_height_schedule();
        let commitment = certificate.bound_header().commitment();
        let target = capability.target();
        let snapshot = crate::codec::snapshot_cbor(&self.snapshot())
            .map_err(|error| invalid(format!("encode formal BFT successor World: {error}")))?;
        let semantic_root = self
            .execution_state_identity()
            .map_err(|error| invalid(format!("compute formal BFT successor root: {error:?}")))?
            .state_root_hash;
        let actual_manifest = self
            .current_manifest_hash()
            .map_err(|error| invalid(format!("read formal BFT successor manifest: {error:?}")))?;
        let current_binding = self
            .current_cognition_runtime_binding()
            .map_err(|error| invalid(format!("read formal BFT successor binding: {error:?}")))?;
        let governing = current_binding
            .runtime_manifest_hash
            .to_string()
            .strip_prefix("blake3:")
            .ok_or_else(|| invalid("formal BFT successor governing binding is not canonical"))?
            .to_owned();
        let initial_policy_hash = target
            .schedule_policy_hash()
            .map_err(|error| invalid(format!("derive BFT schedule policy hash: {error}")))?;
        let expected_next_height = commitment
            .height()
            .checked_add(1)
            .ok_or_else(|| invalid("formal BFT successor height exhausted"))?;
        let mismatches = [
            (
                "activation digest",
                certified_schedule.activation_digest() != capability.activation_digest()
                    || schedule.activation_digest() != capability.activation_digest()
                    || commitment.activation_digest() != capability.activation_digest(),
            ),
            (
                "scope",
                certified_schedule.scope() != &target.scope
                    || schedule.scope() != &target.scope
                    || commitment.scope() != &target.scope,
            ),
            (
                "schedule target policy",
                certified_schedule.policy_hash() != initial_policy_hash
                    || schedule.policy_hash() != initial_policy_hash,
            ),
            (
                "authority epoch",
                certified_schedule.authority_epoch() != target.authority_epoch
                    || schedule.authority_epoch() != target.authority_epoch
                    || commitment.authority_epoch() != target.authority_epoch,
            ),
            (
                "certified height",
                certified_schedule.height() != commitment.height()
                    || self.state.time != commitment.height(),
            ),
            (
                "actual raw Snapshot root",
                crate::blake3_hex(&snapshot) != commitment.execution_state_root(),
            ),
            (
                "actual semantic state root",
                semantic_root != commitment.state_root(),
            ),
            (
                "actual World manifest",
                actual_manifest != commitment.post_manifest_hash(),
            ),
            (
                "successor parent",
                schedule.parent_commit_ref() != certificate.block_id()
                    || schedule.parent_tick() != commitment.height()
                    || schedule.parent_state_root() != commitment.state_root()
                    || schedule.parent_manifest_hash() != commitment.post_manifest_hash()
                    || schedule.height() != expected_next_height,
            ),
            (
                "epoch length",
                self.governance_execution_policy().epoch_length_ticks
                    != schedule.policy().epoch_length_ticks,
            ),
        ]
        .into_iter()
        .filter_map(|(name, mismatch)| mismatch.then_some(name))
        .collect::<Vec<_>>();
        if !mismatches.is_empty() {
            return Err(invalid(format!(
                "formal wire-3 BFT certificate, successor schedule, or actual World mismatch: {}",
                mismatches.join(", ")
            )));
        }
        let registry = Some(Self::validate_governance_main_token_controller_registry(
            self.governance_main_token_controller_registry()
                .cloned()
                .ok_or_else(|| invalid("formal BFT successor controller policy is missing"))?,
        )?);
        binding.successor = Some(FormalBftSuccessorBindingV1 {
            schedule_digest: schedule.digest().map_err(invalid)?,
            raw_snapshot_root: commitment.execution_state_root().to_owned(),
            governing_manifest_digest: governing,
        });
        self.bft_governance_parent = Some(Box::new(BftGovernanceParentV2 {
            schedule: BftGovernanceSchedule::HeaderV3(schedule.clone()),
            controller_registry: registry,
            formal_handoff: Some(FormalBftHandoffBinding::HeaderV3(binding)),
            execution: None,
        }));
        Ok(())
    }

    fn ensure_formal_bft_attachable(&self) -> Result<(), WorldError> {
        if self.formal_execution_tick.is_some()
            || self.bft_governance_parent.is_some()
            || self.bft_governance_execution_in_progress()
            || self.formal_history_candidate_installed()
        {
            return Err(invalid(
                "formal BFT parent cannot replace an active or installed execution",
            ));
        }
        Ok(())
    }
}
