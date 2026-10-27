//! BFT governance uses its own verified parent and execution phase. It never
//! fabricates a controlled-mode retained-history parent or qualification.

use super::{World, WorldError};
use crate::{GovernanceMainTokenControllerRegistry, RuntimeCommittedTickContext};
use header_parent::FormalBftHeaderHandoffBindingV2;
use oasis7_consensus::tendermint_v2::{BftHeightProposerScheduleV2, VerifiedBftHandoffTargetV1};
use oasis7_formal_protocol::bft::BftHeaderHeightScheduleV3;
use oasis7_formal_protocol::controlled_authority::formal::FormalHeadRefV1;
use oasis7_proto::distributed_bft::v2::BftScopeV2;

#[cfg(test)]
#[path = "governance_bft_parent_tests.rs"]
mod tests;

#[path = "governance_bft_prefix_parent.rs"]
mod prefix_parent;

#[path = "governance_bft_header_parent.rs"]
mod header_parent;

#[derive(Debug, Clone)]
pub(super) struct BftGovernanceParentV2 {
    pub(super) schedule: BftGovernanceSchedule,
    pub(super) controller_registry: Option<GovernanceMainTokenControllerRegistry>,
    pub(super) formal_handoff: Option<FormalBftHandoffBinding>,
    execution: Option<(RuntimeCommittedTickContext, bool)>,
}

/// A process-local typed view over schedule versions. This enum deliberately
/// has no conversion to V2: the version-specific capability is retained until
/// the private World execution phase ends.
#[derive(Debug, Clone)]
pub(super) enum BftGovernanceSchedule {
    V2(BftHeightProposerScheduleV2),
    HeaderV3(BftHeaderHeightScheduleV3),
}

impl BftGovernanceSchedule {
    pub(super) fn scope(&self) -> &BftScopeV2 {
        match self {
            Self::V2(schedule) => schedule.scope(),
            Self::HeaderV3(schedule) => schedule.scope(),
        }
    }

    pub(super) fn authority_epoch(&self) -> u64 {
        match self {
            Self::V2(schedule) => schedule.authority_epoch(),
            Self::HeaderV3(schedule) => schedule.authority_epoch(),
        }
    }

    pub(super) fn height(&self) -> u64 {
        match self {
            Self::V2(schedule) => schedule.height(),
            Self::HeaderV3(schedule) => schedule.height(),
        }
    }

    pub(super) fn parent_commit_ref(&self) -> &str {
        match self {
            Self::V2(schedule) => schedule.parent_commit_ref(),
            Self::HeaderV3(schedule) => schedule.parent_commit_ref(),
        }
    }

    pub(super) fn parent_tick(&self) -> u64 {
        match self {
            Self::V2(schedule) => schedule.parent_tick(),
            Self::HeaderV3(schedule) => schedule.parent_tick(),
        }
    }

    pub(super) fn parent_state_root(&self) -> &str {
        match self {
            Self::V2(schedule) => schedule.parent_state_root(),
            Self::HeaderV3(schedule) => schedule.parent_state_root(),
        }
    }

    pub(super) fn parent_manifest_hash(&self) -> &str {
        match self {
            Self::V2(schedule) => schedule.parent_manifest_hash(),
            Self::HeaderV3(schedule) => schedule.parent_manifest_hash(),
        }
    }

    pub(super) fn policy(&self) -> &oasis7_consensus::tendermint_v2::BftSchedulePolicyV2 {
        match self {
            Self::V2(schedule) => schedule.policy(),
            Self::HeaderV3(schedule) => schedule.policy(),
        }
    }

    fn digest(&self) -> Result<String, String> {
        match self {
            Self::V2(schedule) => schedule.digest(),
            Self::HeaderV3(schedule) => schedule.digest(),
        }
    }

    fn v2(&self) -> Option<&BftHeightProposerScheduleV2> {
        match self {
            Self::V2(schedule) => Some(schedule),
            Self::HeaderV3(_) => None,
        }
    }

    fn header_v3(&self) -> Option<&BftHeaderHeightScheduleV3> {
        match self {
            Self::V2(_) => None,
            Self::HeaderV3(schedule) => Some(schedule),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FormalBftHandoffBindingV1 {
    pub(super) target_digest: String,
    pub(super) activation_digest: String,
    pub(super) head: FormalHeadRefV1,
    pub(super) scope: BftScopeV2,
    pub(super) raw_snapshot_root: String,
    pub(super) governing_manifest_digest: String,
    successor: Option<FormalBftSuccessorBindingV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FormalBftHandoffBinding {
    V1(FormalBftHandoffBindingV1),
    HeaderV3(FormalBftHeaderHandoffBindingV2),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FormalBftSuccessorBindingV1 {
    schedule_digest: String,
    raw_snapshot_root: String,
    governing_manifest_digest: String,
}

impl FormalBftHandoffBindingV1 {
    fn from_capability(capability: &VerifiedBftHandoffTargetV1) -> Self {
        Self {
            target_digest: capability.target_digest().to_owned(),
            activation_digest: capability.activation_digest().to_owned(),
            head: capability.handoff_head().clone(),
            scope: capability.scope().clone(),
            raw_snapshot_root: capability.raw_snapshot_root().to_owned(),
            governing_manifest_digest: capability.governing_manifest_digest().to_owned(),
            successor: None,
        }
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

    fn matches_schedule(&self, schedule: &BftGovernanceSchedule) -> bool {
        let Some(schedule) = schedule.v2() else {
            return false;
        };
        if let Some(successor) = &self.successor {
            return schedule.scope() == &self.scope
                && schedule.height() > self.head.position.saturating_add(1)
                && schedule
                    .digest()
                    .is_ok_and(|digest| digest == successor.schedule_digest);
        }
        schedule.scope() == &self.scope
            && schedule.parent_commit_ref() == self.head.record_id
            && schedule.parent_tick() == self.head.position
            && schedule.height() == self.head.position.saturating_add(1)
    }

    fn matches_controller_registry(
        &self,
        retained: &crate::VerifiedFormalRetainedHistoryParentV1,
        captured: Option<&GovernanceMainTokenControllerRegistry>,
    ) -> bool {
        captured.is_some()
            && (self.successor.is_some() || retained.controller_registry() == captured)
    }

    fn parent_snapshot_root(&self) -> &str {
        self.successor
            .as_ref()
            .map_or(&self.raw_snapshot_root, |next| &next.raw_snapshot_root)
    }

    fn parent_governing_manifest(&self) -> &str {
        self.successor
            .as_ref()
            .map_or(&self.governing_manifest_digest, |next| {
                &next.governing_manifest_digest
            })
    }
}

impl FormalBftHandoffBinding {
    fn matches_retained_parent(
        &self,
        parent: &crate::VerifiedFormalRetainedHistoryParentV1,
    ) -> bool {
        match self {
            Self::V1(binding) => binding.matches_retained_parent(parent),
            Self::HeaderV3(binding) => binding.matches_retained_parent(parent),
        }
    }

    fn matches_schedule(&self, schedule: &BftGovernanceSchedule) -> bool {
        match self {
            Self::V1(binding) => binding.matches_schedule(schedule),
            Self::HeaderV3(binding) => binding.matches_schedule(schedule),
        }
    }

    fn matches_controller_registry(
        &self,
        retained: &crate::VerifiedFormalRetainedHistoryParentV1,
        captured: Option<&GovernanceMainTokenControllerRegistry>,
    ) -> bool {
        match self {
            Self::V1(binding) => binding.matches_controller_registry(retained, captured),
            Self::HeaderV3(binding) => binding.matches_controller_registry(retained, captured),
        }
    }

    fn parent_snapshot_root(&self) -> &str {
        match self {
            Self::V1(binding) => binding.parent_snapshot_root(),
            Self::HeaderV3(binding) => binding.parent_snapshot_root(),
        }
    }

    fn parent_governing_manifest(&self) -> &str {
        match self {
            Self::V1(binding) => binding.parent_governing_manifest(),
            Self::HeaderV3(binding) => binding.parent_governing_manifest(),
        }
    }

    fn attach_successor(
        &mut self,
        schedule: &BftGovernanceSchedule,
        raw_snapshot_root: String,
        governing_manifest_digest: String,
    ) -> Result<(), WorldError> {
        let successor = FormalBftSuccessorBindingV1 {
            schedule_digest: schedule.digest().map_err(invalid)?,
            raw_snapshot_root,
            governing_manifest_digest,
        };
        match self {
            Self::V1(binding) => binding.successor = Some(successor),
            Self::HeaderV3(binding) => binding.successor = Some(successor),
        }
        Ok(())
    }
}

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::GovernancePolicyInvalid {
        reason: reason.into(),
    }
}

impl World {
    /// Attach only after the caller has pinned the genesis and independently
    /// recovered this exact cursor. The opaque cursor validates protocol
    /// history; it does not authenticate arbitrary caller-supplied genesis.
    /// This method additionally binds the actual World and immutable policy.
    pub fn attach_bft_governance_parent_v2(
        &mut self,
        schedule: &BftHeightProposerScheduleV2,
    ) -> Result<(), WorldError> {
        if self
            .formal_history_reader
            .as_ref()
            .and_then(|reader| reader.parent.as_ref())
            .is_some()
            || self.formal_execution_tick.is_some()
            || self.bft_governance_execution_in_progress()
        {
            return Err(invalid(
                "BFT parent cannot replace an active qualified execution",
            ));
        }
        self.validate_bft_parent_world(&BftGovernanceSchedule::V2(schedule.clone()))?;
        let registry = self
            .governance_main_token_controller_registry()
            .cloned()
            .map(Self::validate_governance_main_token_controller_registry)
            .transpose()?;
        self.bft_governance_parent = Some(Box::new(BftGovernanceParentV2 {
            schedule: BftGovernanceSchedule::V2(schedule.clone()),
            controller_registry: registry,
            formal_handoff: None,
            execution: None,
        }));
        Ok(())
    }

    /// Attach BFT execution only when an opaque profile-handoff capability
    /// names the exact retained formal H already attached to this World.
    /// Unlike the engineering attach path, this keeps the H reader available
    /// for historical reads while using H's captured controller policy for
    /// BFT successor authorization.
    pub fn attach_formal_bft_handoff_parent_v1(
        &mut self,
        capability: &VerifiedBftHandoffTargetV1,
        schedule: &BftHeightProposerScheduleV2,
    ) -> Result<(), WorldError> {
        if self.formal_execution_tick.is_some()
            || self.bft_governance_parent.is_some()
            || self.bft_governance_execution_in_progress()
            || self.formal_history_candidate_installed()
        {
            return Err(invalid(
                "formal BFT handoff cannot replace an active or installed execution",
            ));
        }
        let attached = self
            .formal_history_reader
            .as_ref()
            .and_then(|reader| reader.parent.as_ref())
            .ok_or_else(|| invalid("formal BFT handoff requires the exact retained H parent"))?;
        let head = attached
            .head()
            .ok_or_else(|| invalid("retained formal H commit reference is missing"))?;
        let effect_world = attached
            .effect_world()
            .ok_or_else(|| invalid("retained formal H World identity is missing"))?;
        let retained_registry = attached
            .controller_registry()
            .ok_or_else(|| invalid("retained formal H controller policy is missing"))?;
        let current_registry = self
            .governance_main_token_controller_registry()
            .ok_or_else(|| invalid("formal BFT handoff controller policy is missing"))?;
        let current_binding = self
            .current_cognition_runtime_binding()
            .map_err(|error| invalid(format!("read formal H governing binding: {error:?}")))?;
        let governing_value = current_binding.runtime_manifest_hash.to_string();
        let actual_governing = governing_value
            .strip_prefix("blake3:")
            .ok_or_else(|| invalid("formal H governing binding is not a canonical digest"))?;
        let (retained_view, _) = self
            .attached_formal_history()
            .map_err(|error| invalid(format!("validate retained formal H reader: {error}")))?;
        let raw_snapshot = crate::codec::snapshot_cbor(&self.snapshot())
            .map_err(|error| invalid(format!("encode formal H snapshot: {error}")))?;
        let semantic_root = self
            .execution_state_identity()
            .map_err(|error| invalid(format!("compute formal H semantic root: {error:?}")))?
            .state_root_hash;
        let target = capability.target();
        let activation_height = capability
            .activation_height()
            .map_err(|error| invalid(format!("derive formal BFT activation height: {error}")))?;
        let expected_set_hash = capability.initial_validator_set_hash();
        let expected_policy_hash = capability.schedule_policy_hash();
        let actual_manifest = self
            .current_manifest_hash()
            .map_err(|error| invalid(format!("read actual formal H World manifest: {error:?}")))?;
        let mismatches = [
            ("H position", head.position != capability.handoff_position()),
            (
                "H commit reference",
                head.record_id != capability.handoff_head().record_id,
            ),
            (
                "World identity",
                effect_world.world_id != capability.scope().world_id
                    || effect_world.chain_id != capability.scope().chain_id
                    || effect_world.genesis_digest != capability.scope().genesis_hash,
            ),
            (
                "raw Snapshot root",
                attached.after_snapshot_root() != capability.raw_snapshot_root()
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
            ("schedule scope", schedule.scope() != capability.scope()),
            (
                "schedule parent",
                schedule.parent_commit_ref() != capability.handoff_head().record_id
                    || schedule.parent_tick() != capability.handoff_position(),
            ),
            ("activation height", schedule.height() != activation_height),
            (
                "authority epoch",
                schedule.authority_epoch() != target.authority_epoch(),
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
                schedule.policy() != target.schedule_policy(),
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
                "formal BFT handoff capability, retained H, schedule, or actual World mismatch: {}",
                mismatches.join(", ")
            )));
        }
        let registry = Some(Self::validate_governance_main_token_controller_registry(
            current_registry.clone(),
        )?);
        self.bft_governance_parent = Some(Box::new(BftGovernanceParentV2 {
            schedule: BftGovernanceSchedule::V2(schedule.clone()),
            controller_registry: registry,
            formal_handoff: Some(FormalBftHandoffBinding::V1(
                FormalBftHandoffBindingV1::from_capability(capability),
            )),
            execution: None,
        }));
        Ok(())
    }

    fn validate_bft_parent_world(
        &self,
        schedule: &BftGovernanceSchedule,
    ) -> Result<(), WorldError> {
        if self.state.time != schedule.parent_tick()
            || self.current_state_root_hash()? != schedule.parent_state_root()
            || self.current_manifest_hash()? != schedule.parent_manifest_hash()
            || self.governance_execution_policy().epoch_length_ticks
                != schedule.policy().epoch_length_ticks
            || schedule.height()
                != schedule
                    .parent_tick()
                    .checked_add(1)
                    .ok_or_else(|| invalid("BFT governance parent height exhausted"))?
        {
            return Err(invalid(
                "BFT governance parent World/root/time/manifest/policy mismatch",
            ));
        }
        Ok(())
    }

    /// Begin a detached candidate's exact successor execution. This does not
    /// publish its World or make the candidate a committed result.
    pub fn begin_bft_execution_tick(
        &mut self,
        context: &RuntimeCommittedTickContext,
    ) -> Result<(), WorldError> {
        if self.formal_execution_tick.is_some() || self.bft_governance_execution_in_progress() {
            return Err(invalid("BFT execution tick already begun or conflicting"));
        }
        let parent = self
            .bft_governance_parent
            .as_ref()
            .ok_or_else(|| invalid("BFT execution requires its verified parent"))?;
        self.validate_formal_handoff_parent(parent)?;
        self.validate_bft_parent_world(&parent.schedule)?;
        if context.height != parent.schedule.height()
            || context.epoch != parent.schedule.authority_epoch()
            || context.node_block_hash.trim().is_empty()
            || context.action_root.trim().is_empty()
        {
            return Err(invalid(
                "BFT execution tick does not match its exact parent",
            ));
        }
        self.bft_governance_parent
            .as_mut()
            .expect("checked parent")
            .execution = Some((context.clone(), false));
        Ok(())
    }

    pub(super) fn bft_governance_execution_in_progress(&self) -> bool {
        self.bft_governance_parent
            .as_ref()
            .is_some_and(|parent| parent.execution.is_some())
    }

    pub(super) fn formal_bft_handoff_attached(&self) -> bool {
        self.bft_governance_parent
            .as_ref()
            .is_some_and(|parent| parent.formal_handoff.is_some())
    }

    pub(super) fn governance_execution_is_qualified(&self) -> bool {
        self.formal_execution_tick.is_some()
            || self.bft_governance_parent.as_ref().is_some_and(|parent| {
                parent
                    .execution
                    .as_ref()
                    .is_some_and(|(context, finished)| {
                        !finished && context.height == self.state.time
                    })
            })
    }

    pub(super) fn validate_bft_execution_step(
        &self,
        context: &RuntimeCommittedTickContext,
        resume: bool,
    ) -> Result<(), WorldError> {
        let Some(parent) = &self.bft_governance_parent else {
            return Ok(());
        };
        if resume || parent.execution.as_ref() != Some(&(context.clone(), false)) {
            return Err(invalid(
                "BFT successor requires its exact begun execution context",
            ));
        }
        // The full H snapshot is checked before beginning the tick. By this
        // point the World may contain queued candidate actions, so validate
        // only the immutable retained-H/schedule binding here.
        self.validate_formal_handoff_authorization_parent(parent)?;
        self.validate_bft_parent_world(&parent.schedule)
    }

    pub(super) fn validate_formal_handoff_parent(
        &self,
        parent: &BftGovernanceParentV2,
    ) -> Result<(), WorldError> {
        let Some(binding) = parent.formal_handoff.as_ref() else {
            return Ok(());
        };
        let formal = self
            .formal_history_reader
            .as_ref()
            .and_then(|reader| reader.parent.as_ref())
            .ok_or_else(|| invalid("formal BFT retained H reader was detached"))?;
        let current_binding = self
            .current_cognition_runtime_binding()
            .map_err(|error| invalid(format!("read formal H governing binding: {error:?}")))?;
        let governing_value = current_binding.runtime_manifest_hash.to_string();
        let actual_governing = governing_value
            .strip_prefix("blake3:")
            .ok_or_else(|| invalid("formal H governing binding is not a canonical digest"))?;
        let raw_snapshot = crate::codec::snapshot_cbor(&self.snapshot())
            .map_err(|error| invalid(format!("encode formal BFT parent snapshot: {error}")))?;
        let semantic_root = self
            .execution_state_identity()
            .map_err(|error| invalid(format!("compute formal BFT parent root: {error:?}")))?
            .state_root_hash;
        let actual_manifest = self
            .current_manifest_hash()
            .map_err(|error| invalid(format!("read formal BFT parent manifest: {error:?}")))?;
        let mismatches = [
            (
                "retained H identity",
                !binding.matches_retained_parent(formal),
            ),
            (
                "schedule binding",
                !binding.matches_schedule(&parent.schedule),
            ),
            (
                "controller registry",
                !binding.matches_controller_registry(formal, parent.controller_registry.as_ref()),
            ),
            (
                "retained-history descriptor",
                self.state.retained_history_root.as_ref() != Some(formal.root()),
            ),
            (
                "retained candidate",
                self.formal_history_candidate_installed(),
            ),
            (
                "raw Snapshot root",
                crate::blake3_hex(raw_snapshot.as_slice()) != binding.parent_snapshot_root(),
            ),
            (
                "governing manifest digest",
                actual_governing != binding.parent_governing_manifest(),
            ),
            (
                "semantic state root",
                semantic_root != parent.schedule.parent_state_root(),
            ),
            (
                "World manifest",
                actual_manifest != parent.schedule.parent_manifest_hash(),
            ),
        ]
        .into_iter()
        .filter_map(|(name, mismatch)| mismatch.then_some(name))
        .collect::<Vec<_>>();
        if !mismatches.is_empty() {
            return Err(invalid(format!(
                "formal BFT retained H binding changed before successor execution: {}",
                mismatches.join(", ")
            )));
        }
        Ok(())
    }

    /// Recheck only the immutable H capability while processing actions in an
    /// already-begun successor tick. The World has advanced by then, so the
    /// full H snapshot/root comparison belongs at attach and before mutation.
    pub(super) fn validate_formal_handoff_authorization_parent(
        &self,
        parent: &BftGovernanceParentV2,
    ) -> Result<(), WorldError> {
        let formal = self
            .formal_history_reader
            .as_ref()
            .and_then(|history| history.parent.as_ref());
        match (&parent.formal_handoff, formal) {
            (Some(binding), Some(formal))
                if binding.matches_retained_parent(formal)
                    && binding.matches_schedule(&parent.schedule)
                    && binding.matches_controller_registry(
                        formal,
                        parent.controller_registry.as_ref(),
                    )
                    && self.state.retained_history_root.as_ref() == Some(formal.root())
                    && !self.formal_history_candidate_installed() =>
            {
                Ok(())
            }
            (None, None) => Ok(()),
            (Some(_), _) => Err(invalid(
                "formal BFT parent no longer matches its retained H capability",
            )),
            (None, Some(_)) => Err(invalid("ambiguous governance parent profiles")),
        }
    }

    pub(super) fn complete_bft_execution_step(&mut self, context: &RuntimeCommittedTickContext) {
        if let Some(parent) = self.bft_governance_parent.as_mut()
            && parent.execution.as_ref() == Some(&(context.clone(), false))
        {
            parent.execution = Some((context.clone(), true));
        }
    }

    /// The committed-context step already recorded its Tick. Finish only the
    /// private phase; a second finish cannot append another Tick or authorize.
    pub fn finish_bft_execution_tick(
        &mut self,
        context: &RuntimeCommittedTickContext,
    ) -> Result<(), WorldError> {
        let parent = self
            .bft_governance_parent
            .as_ref()
            .ok_or_else(|| invalid("BFT execution parent is absent"))?;
        if parent.execution.as_ref() != Some(&(context.clone(), true))
            || self.state.time != context.height
            || self.governance_execution_policy().epoch_length_ticks
                != parent.schedule.policy().epoch_length_ticks
        {
            return Err(invalid(
                "BFT execution has not completed its exact successor tick",
            ));
        }
        self.verify_tick_consensus_chain()?;
        self.bft_governance_parent = None;
        Ok(())
    }
}
