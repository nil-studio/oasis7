//! Fixed normal Action owners for module artifacts, instances and releases.
//! Source views are borrowed DATA. Only the selected/Rule-bound fixed driver
//! may consume them; individual storage mutators grant no Action authority.
use crate::world::{
    incremental_action_execution::SelectedPendingActionV1,
    incremental_body::BodyMutationError,
    incremental_module_invocation::{RootModuleExecutionFamiliesV1, RootRulePhaseOutcomeV1},
    incremental_normal_action_factory_schedule::{FactoryScheduleSourcesV1, FactoryScheduleWorkV1},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
#[path = "incremental_normal_action_module_lifecycle_rules.rs"]
pub(in crate::world) mod rules;
/// Mandatory exact parent-qualified sources. The actual compiler loan is the
/// Host's configured capability, including its original explicit-unavailable
/// implementation when that is what the actual Host owns; no fallback is made.
pub(in crate::world) struct ModuleLifecycleSourcesV1<'a> {
    pub controls: &'a ReservedValue<oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsQualificationV1>,
    pub capability: &'a crate::world::ModuleCapabilityAdmissionSourcesV1<'a>,
    pub proposals: &'a ReservedValue<
        oasis7_world_state::incremental_runtime_proposal::RuntimeProposalsQualificationV1,
    >,
    pub families: RootModuleExecutionFamiliesV1<'a>,
    pub factory: &'a FactoryScheduleSourcesV1<'a>,
    pub governance: &'a ReservedValue<
        oasis7_world_state::incremental_runtime_governance::RuntimeGovernanceQualificationV1,
    >,
    pub compiler: &'a crate::module_source_compiler::RuntimeSourceCompiler,
}
/// Loans from the one normal bank. There is deliberately no default/new work
/// constructor; all original source/writer allowances stay continuous.
pub(in crate::world) struct ModuleLifecycleWorkV1<'a>{
 pub controls: &'a mut oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsWorkV1,
    pub capability_authorization: &'a mut crate::world::incremental_authorization_root_current::AuthorizationRootSourceReadWorkV1,
 pub config_write: &'a mut oasis7_world_state::incremental_runtime_artifact_config::ArtifactConfigWriteWorkV1,
 pub governance:&'a mut oasis7_world_state::incremental_runtime_governance::RuntimeGovernanceWorkV1,
 pub artifacts:&'a mut oasis7_world_state::incremental_runtime_artifact_resources::RuntimeArtifactResourcesWorkV1,
 pub content:&'a mut oasis7_world_state::incremental_runtime_artifact_content::RuntimeArtifactOriginalWorkV1,
 pub marketplace:&'a mut oasis7_world_state::incremental_module_marketplace::ModuleMarketplaceFamilyWorkV1,
 pub proposal_read:&'a mut oasis7_world_state::incremental_runtime_proposal::RuntimeProposalReadWorkV1,
 pub proposal_write:&'a mut oasis7_world_state::incremental_runtime_proposal::RuntimeProposalWriteWorkV1,
 pub proposal_scan:&'a mut StateScanBudget,
 pub marketplace_scan:&'a mut StateScanBudget,pub instance_scan:&'a mut StateScanBudget,
 pub instance_cells:&'a mut u64,pub pending_entries:&'a mut usize,
 pub max_selected_heap:usize,pub max_original_bytes:usize,
}
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("normal module lifecycle source")
}
fn pay(common: &mut FormalBoundedContextV1<'_>, bytes: usize) -> Result<(), BodyMutationError> {
    common
        .charge(1, u64::try_from(bytes).map_err(|_| invalid())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
/// The guard authenticates the genuine selected effective action. It is not a
/// prepared business result and cannot be submitted to a Domain event sink.
struct SelectedModuleLifecycleV1<'a> {
    selected: &'a SelectedPendingActionV1,
    outcome: &'a ReservedValue<RootRulePhaseOutcomeV1>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
impl SelectedModuleLifecycleV1<'_> {
    fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.selected.validate(tx)?;
        tx.validate_guard(&self.guard, live)?;
        pay(common, 128)?;
        if self.core != common.work().identity()
            || !self.outcome.uses_budget(common.memory())
            || !self.fee.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        Ok(())
    }
}

impl ModuleLifecycleWorkV1<'_> {
    pub(in crate::world) fn clamp_io_v1(&mut self, steps: u64, bytes: u64) {
        self.controls.records_remaining = self.controls.records_remaining.min(steps);
        self.controls.index_steps_remaining = self.controls.index_steps_remaining.min(steps);
        self.config_write.cells_remaining = self.config_write.cells_remaining.min(steps);
        self.config_write.json.cells_remaining = self.config_write.json.cells_remaining.min(steps);
        self.config_write.json.json_nodes_remaining =
            self.config_write.json.json_nodes_remaining.min(steps);
        self.proposal_read.cells_remaining = self.proposal_read.cells_remaining.min(steps);
        self.proposal_read.json.cells_remaining =
            self.proposal_read.json.cells_remaining.min(steps);
        self.proposal_read.json.json_nodes_remaining =
            self.proposal_read.json.json_nodes_remaining.min(steps);
        self.proposal_write.cells_remaining = self.proposal_write.cells_remaining.min(steps);
        self.proposal_write.json.cells_remaining =
            self.proposal_write.json.cells_remaining.min(steps);
        self.proposal_write.json.json_nodes_remaining =
            self.proposal_write.json.json_nodes_remaining.min(steps);
        self.proposal_scan.nodes_remaining = self.proposal_scan.nodes_remaining.min(steps);
        self.proposal_scan.leaves_remaining = self.proposal_scan.leaves_remaining.min(steps);
        self.proposal_scan.read_bytes_remaining =
            self.proposal_scan.read_bytes_remaining.min(bytes);
        self.content.objects_remaining = self.content.objects_remaining.min(steps);
        self.content.bytes_remaining = self.content.bytes_remaining.min(bytes);
        self.governance.cells_remaining = self.governance.cells_remaining.min(steps);
        self.artifacts.cells_remaining = self.artifacts.cells_remaining.min(steps);
        self.marketplace.outer_rows_remaining = self.marketplace.outer_rows_remaining.min(steps);
        self.marketplace.changed_cells_remaining =
            self.marketplace.changed_cells_remaining.min(steps);
        self.marketplace.membership_cells_remaining =
            self.marketplace.membership_cells_remaining.min(steps);
        self.marketplace_scan.nodes_remaining = self.marketplace_scan.nodes_remaining.min(steps);
        self.marketplace_scan.leaves_remaining = self.marketplace_scan.leaves_remaining.min(steps);
        self.instance_scan.nodes_remaining = self.instance_scan.nodes_remaining.min(steps);
        self.instance_scan.leaves_remaining = self.instance_scan.leaves_remaining.min(steps);
        *self.instance_cells = (*self.instance_cells).min(steps);
        *self.pending_entries =
            (*self.pending_entries).min(usize::try_from(steps).unwrap_or(usize::MAX));
        self.marketplace_scan.read_bytes_remaining =
            self.marketplace_scan.read_bytes_remaining.min(bytes);
        self.instance_scan.read_bytes_remaining =
            self.instance_scan.read_bytes_remaining.min(bytes);
    }
    pub(in crate::world) fn io_snapshot_v1(&self) -> ([u64; 25], [u64; 4]) {
        (
            [
                self.content.objects_remaining,
                self.governance.cells_remaining,
                self.artifacts.cells_remaining,
                self.marketplace.outer_rows_remaining,
                self.marketplace.changed_cells_remaining,
                self.marketplace.membership_cells_remaining,
                self.marketplace_scan.nodes_remaining,
                self.marketplace_scan.leaves_remaining,
                self.instance_scan.nodes_remaining,
                self.instance_scan.leaves_remaining,
                *self.instance_cells,
                *self.pending_entries as u64,
                self.proposal_read.cells_remaining,
                self.proposal_read.json.cells_remaining,
                self.proposal_read.json.json_nodes_remaining,
                self.proposal_write.cells_remaining,
                self.proposal_write.json.cells_remaining,
                self.proposal_write.json.json_nodes_remaining,
                self.proposal_scan.nodes_remaining,
                self.proposal_scan.leaves_remaining,
                self.config_write.cells_remaining,
                self.config_write.json.cells_remaining,
                self.config_write.json.json_nodes_remaining,
                self.controls.records_remaining,
                self.controls.index_steps_remaining,
            ],
            [
                self.content.bytes_remaining,
                self.marketplace_scan.read_bytes_remaining,
                self.instance_scan.read_bytes_remaining,
                self.proposal_scan.read_bytes_remaining,
            ],
        )
    }
}

#[path = "incremental_normal_action_module_lifecycle_commit.rs"]
mod commit;
#[path = "incremental_normal_action_module_lifecycle_current.rs"]
mod current;
#[path = "incremental_normal_action_module_lifecycle_event.rs"]
mod event;
pub(in crate::world) use event::{
    CompletedModuleLifecycleActionV1, PreparedNormalModuleLifecycleEventV1,
};

fn overlay_error(
    error: oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1,
) -> BodyMutationError {
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1;
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}

impl ModuleLifecycleWorkV1<'_> {
    pub(in crate::world) fn restore_io_v1(
        &mut self,
        original: ([u64; 25], [u64; 4]),
        before: ([u64; 25], [u64; 4]),
        after: ([u64; 25], [u64; 4]),
    ) -> Result<(), BodyMutationError> {
        let mut steps = [0u64; 25];
        let mut bytes = [0u64; 4];
        for i in 0..25 {
            steps[i] = original.0[i]
                .checked_sub(before.0[i].checked_sub(after.0[i]).ok_or_else(invalid)?)
                .ok_or_else(invalid)?
        }
        for i in 0..4 {
            bytes[i] = original.1[i]
                .checked_sub(before.1[i].checked_sub(after.1[i]).ok_or_else(invalid)?)
                .ok_or_else(invalid)?
        }
        let pending = usize::try_from(steps[11]).map_err(|_| invalid())?;
        self.content.objects_remaining = steps[0];
        self.governance.cells_remaining = steps[1];
        self.artifacts.cells_remaining = steps[2];
        self.marketplace.outer_rows_remaining = steps[3];
        self.marketplace.changed_cells_remaining = steps[4];
        self.marketplace.membership_cells_remaining = steps[5];
        self.marketplace_scan.nodes_remaining = steps[6];
        self.marketplace_scan.leaves_remaining = steps[7];
        self.instance_scan.nodes_remaining = steps[8];
        self.instance_scan.leaves_remaining = steps[9];
        *self.instance_cells = steps[10];
        *self.pending_entries = pending;
        self.content.bytes_remaining = bytes[0];
        self.marketplace_scan.read_bytes_remaining = bytes[1];
        self.instance_scan.read_bytes_remaining = bytes[2];
        self.proposal_scan.read_bytes_remaining = bytes[3];
        self.proposal_read.cells_remaining = steps[12];
        self.proposal_read.json.cells_remaining = steps[13];
        self.proposal_read.json.json_nodes_remaining = steps[14];
        self.proposal_write.cells_remaining = steps[15];
        self.proposal_write.json.cells_remaining = steps[16];
        self.proposal_write.json.json_nodes_remaining = steps[17];
        self.proposal_scan.nodes_remaining = steps[18];
        self.proposal_scan.leaves_remaining = steps[19];
        self.config_write.cells_remaining = steps[20];
        self.config_write.json.cells_remaining = steps[21];
        self.config_write.json.json_nodes_remaining = steps[22];
        self.controls.records_remaining = steps[23];
        self.controls.index_steps_remaining = steps[24];
        Ok(())
    }
}

impl ModuleLifecycleWorkV1<'_> {
    pub(in crate::world) fn reborrow(&mut self) -> ModuleLifecycleWorkV1<'_> {
        ModuleLifecycleWorkV1 {
            controls: &mut *self.controls,
            capability_authorization: &mut *self.capability_authorization,
            config_write: &mut *self.config_write,
            proposal_read: &mut *self.proposal_read,
            proposal_write: &mut *self.proposal_write,
            proposal_scan: &mut *self.proposal_scan,
            governance: &mut *self.governance,
            artifacts: &mut *self.artifacts,
            content: &mut *self.content,
            marketplace: &mut *self.marketplace,
            marketplace_scan: &mut *self.marketplace_scan,
            instance_scan: &mut *self.instance_scan,
            instance_cells: &mut *self.instance_cells,
            pending_entries: &mut *self.pending_entries,
            max_selected_heap: self.max_selected_heap,
            max_original_bytes: self.max_original_bytes,
        }
    }
}

#[path = "incremental_normal_action_module_lifecycle_execute.rs"]
mod execute;
#[path = "incremental_normal_action_module_lifecycle_facts.rs"]
mod facts;
pub(in crate::world) use execute::execute_market_after_accepted_v1;

#[path = "incremental_normal_action_module_lifecycle_registry_rules.rs"]
pub(in crate::world) mod registry_rules;

#[path = "incremental_normal_action_module_lifecycle_manifest_rules.rs"]
pub(in crate::world) mod manifest_rules;

#[path = "incremental_normal_action_module_lifecycle_m1_rules.rs"]
pub(in crate::world) mod m1_rules;

#[path = "incremental_normal_action_module_deployment_rules.rs"]
pub(in crate::world) mod deployment_rules;

#[path = "incremental_normal_action_module_lifecycle_instance_rules.rs"]
pub(in crate::world) mod instance_rules;

#[path = "incremental_normal_action_module_deployment_execute.rs"]
mod deployment_execute;
pub(in crate::world) use deployment_execute::{
    execute_deployment_after_accepted_v1, is_deployment_action_v1,
};
#[path = "incremental_normal_action_module_lifecycle_instance_current.rs"]
pub(in crate::world) mod instance_current;

#[path = "incremental_normal_action_module_lifecycle_governance_json.rs"]
pub(super) mod governance_json;
#[path = "incremental_normal_action_module_lifecycle_governance_manifest.rs"]
pub(super) mod governance_manifest;
#[path = "incremental_normal_action_module_lifecycle_governance_plan.rs"]
pub(super) mod governance_plan;
#[path = "incremental_normal_action_module_lifecycle_governance_state.rs"]
pub(super) mod governance_state;

#[path = "incremental_normal_action_module_lifecycle_governance_finality.rs"]
pub(super) mod governance_finality;
#[path = "incremental_normal_action_module_lifecycle_governance_shadow.rs"]
pub(super) mod governance_shadow;

#[path = "incremental_normal_action_module_lifecycle_governance_execute.rs"]
pub(super) mod governance_execute;

#[path = "incremental_normal_action_module_lifecycle_instance_execute.rs"]
pub(super) mod instance_execute;

#[path = "incremental_normal_action_module_lifecycle_governance_apply.rs"]
pub(super) mod governance_apply;

#[path = "incremental_normal_action_module_lifecycle_instance_plan.rs"]
pub(super) mod instance_plan;
#[path = "incremental_normal_action_module_lifecycle_instance_tail_rows.rs"]
pub(super) mod instance_tail_rows;

#[path = "incremental_normal_action_module_lifecycle_instance_tail_commit.rs"]
pub(super) mod instance_tail_commit;

#[path = "incremental_normal_action_module_lifecycle_install_driver.rs"]
pub(super) mod install_driver;
pub(in crate::world) use install_driver::execute_install_after_accepted_v1;

#[path = "incremental_normal_action_module_lifecycle_upgrade_rules.rs"]
pub(super) mod upgrade_rules;
pub(in crate::world) fn is_install_action_v1(action: &crate::Action) -> bool {
    matches!(
        action,
        crate::Action::InstallModuleFromArtifact { .. }
            | crate::Action::InstallModuleFromArtifactWithFinality { .. }
            | crate::Action::InstallModuleToTargetFromArtifact { .. }
            | crate::Action::InstallModuleToTargetFromArtifactWithFinality { .. }
    )
}

#[path = "incremental_normal_action_module_lifecycle_release_current.rs"]
pub(super) mod release_current;

#[path = "incremental_normal_action_module_lifecycle_upgrade_execute.rs"]
pub(super) mod upgrade_execute;
#[path = "incremental_normal_action_module_lifecycle_upgrade_legacy.rs"]
pub(super) mod upgrade_legacy;
pub(in crate::world) fn is_instance_action_v1(action: &crate::Action) -> bool {
    is_install_action_v1(action)
        || matches!(
            action,
            crate::Action::UpgradeModuleFromArtifact { .. }
                | crate::Action::UpgradeModuleFromArtifactWithFinality { .. }
                | crate::Action::RollbackModuleInstance { .. }
                | crate::Action::RollbackModuleInstanceWithFinality { .. }
        )
}

#[path = "incremental_normal_action_module_lifecycle_rollback_execute.rs"]
pub(super) mod rollback_execute;
#[path = "incremental_normal_action_module_lifecycle_rollback_legacy.rs"]
pub(super) mod rollback_legacy;
#[path = "incremental_normal_action_module_lifecycle_rollback_rules.rs"]
pub(super) mod rollback_rules;

#[path = "incremental_normal_action_module_lifecycle_release_commit.rs"]
pub(super) mod release_commit;
#[path = "incremental_normal_action_module_lifecycle_release_plan.rs"]
pub(super) mod release_plan;
#[path = "incremental_normal_action_module_lifecycle_release_roles_current.rs"]
pub(super) mod release_roles_current;

#[path = "incremental_normal_action_module_lifecycle_release_completion.rs"]
pub(super) mod release_completion;

#[path = "incremental_normal_action_module_lifecycle_release_facts.rs"]
pub(super) mod release_facts;

#[path = "incremental_normal_action_module_lifecycle_release_shadow_current.rs"]
pub(super) mod release_shadow_current;

#[path = "incremental_normal_action_module_lifecycle_release_profiles_current.rs"]
pub(in crate::world) mod release_profiles_current;

#[path = "incremental_normal_action_module_lifecycle_release_legacy.rs"]
pub(in crate::world) mod release_legacy;
#[path = "incremental_normal_action_module_lifecycle_release_rules.rs"]
pub(in crate::world) mod release_rules;

#[path = "incremental_normal_action_module_lifecycle_release_apply_legacy.rs"]
pub(in crate::world) mod release_apply_legacy;
#[path = "incremental_normal_action_module_lifecycle_release_apply_rules.rs"]
pub(in crate::world) mod release_apply_rules;

#[path = "incremental_normal_action_module_lifecycle_release_driver.rs"]
pub(super) mod release_driver;
#[path = "incremental_normal_action_module_lifecycle_release_rejection_event.rs"]
pub(super) mod release_rejection_event;

pub(in crate::world) use release_driver::execute_release_after_accepted_v1;
pub(in crate::world) fn is_release_action_v1(action: &crate::Action) -> bool {
    is_release_apply_action_v1(action)
        || matches!(
            action,
            crate::Action::ModuleReleaseSubmit { .. }
                | crate::Action::ModuleReleaseShadow { .. }
                | crate::Action::ModuleReleaseSubmitAttestation { .. }
                | crate::Action::ModuleReleaseApproveRole { .. }
                | crate::Action::ModuleReleaseBindRoles { .. }
                | crate::Action::ModuleReleaseReject { .. }
        )
}

#[path = "incremental_normal_action_module_lifecycle_release_apply_driver.rs"]
pub(super) mod release_apply_driver;
pub(in crate::world) use release_apply_driver::execute_release_apply_after_accepted_v1;
pub(in crate::world) fn is_release_apply_action_v1(action: &crate::Action) -> bool {
    matches!(
        action,
        crate::Action::ModuleReleaseApply { .. }
            | crate::Action::ModuleReleaseApplyWithFinality { .. }
    )
}
