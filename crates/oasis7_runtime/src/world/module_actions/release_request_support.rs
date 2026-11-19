use super::*;

impl World {
    #[expect(
        clippy::too_many_arguments,
        reason = "Release submission fields are the stable module governance transaction input set."
    )]
    pub(super) fn handle_module_release_submit_action(
        &mut self,
        action_id: ActionId,
        requester_agent_id: &str,
        manifest: &oasis7_wasm_abi::ModuleManifest,
        activate: bool,
        install_target: &ModuleInstallTarget,
        required_roles: &[String],
        profile_changes: &ModuleProfileChanges,
    ) -> Result<bool, WorldError> {
        use crate::world::incremental_normal_action_module_lifecycle::{
            release_legacy::LegacyModuleReleaseFactsV1,
            release_rules::{ModuleReleaseBusinessDecisionV1, prepare_module_release_submit_v1},
        };
        let event = match prepare_module_release_submit_v1(
            &mut LegacyModuleReleaseFactsV1::new(self),
            action_id,
            requester_agent_id,
            manifest,
            activate,
            install_target,
            required_roles,
            profile_changes,
        )? {
            ModuleReleaseBusinessDecisionV1::Rejected(reason) => {
                DomainEvent::ActionRejected { action_id, reason }
            }
            ModuleReleaseBusinessDecisionV1::Emit(event) => event,
        };
        self.append_event(
            WorldEventBody::Domain(event),
            Some(CausedBy::Action(action_id)),
        )?;
        Ok(true)
    }

    pub(super) fn handle_module_release_shadow_action(
        &mut self,
        action_id: ActionId,
        operator_agent_id: &str,
        request_id: u64,
    ) -> Result<bool, WorldError> {
        use crate::world::incremental_normal_action_module_lifecycle::{
            release_legacy::LegacyModuleReleaseFactsV1,
            release_rules::{ModuleReleaseBusinessDecisionV1, prepare_module_release_shadow_v1},
        };
        let event = match prepare_module_release_shadow_v1(
            &mut LegacyModuleReleaseFactsV1::new(self),
            action_id,
            operator_agent_id,
            request_id,
        )? {
            ModuleReleaseBusinessDecisionV1::Rejected(reason) => {
                DomainEvent::ActionRejected { action_id, reason }
            }
            ModuleReleaseBusinessDecisionV1::Emit(event) => event,
        };
        self.append_event(
            WorldEventBody::Domain(event),
            Some(CausedBy::Action(action_id)),
        )?;
        Ok(true)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Attestation submission fields mirror the persisted build-proof schema."
    )]
    pub(super) fn handle_module_release_submit_attestation_action(
        &mut self,
        action_id: ActionId,
        operator_agent_id: &str,
        request_id: u64,
        signer_node_id: &str,
        platform: &str,
        build_manifest_hash: &str,
        source_hash: &str,
        wasm_hash: &str,
        proof_cid: &str,
        builder_image_digest: &str,
        container_platform: &str,
        canonicalizer_version: &str,
    ) -> Result<bool, WorldError> {
        use crate::world::incremental_normal_action_module_lifecycle::{
            release_legacy::LegacyModuleReleaseFactsV1,
            release_rules::{
                ModuleReleaseBusinessDecisionV1, prepare_module_release_submit_attestation_v1,
            },
        };
        let event = match prepare_module_release_submit_attestation_v1(
            &mut LegacyModuleReleaseFactsV1::new(self),
            action_id,
            operator_agent_id,
            request_id,
            signer_node_id,
            platform,
            build_manifest_hash,
            source_hash,
            wasm_hash,
            proof_cid,
            builder_image_digest,
            container_platform,
            canonicalizer_version,
        )? {
            ModuleReleaseBusinessDecisionV1::Rejected(reason) => {
                DomainEvent::ActionRejected { action_id, reason }
            }
            ModuleReleaseBusinessDecisionV1::Emit(event) => event,
        };
        self.append_event(
            WorldEventBody::Domain(event),
            Some(CausedBy::Action(action_id)),
        )?;
        Ok(true)
    }

    pub(super) fn handle_module_release_approve_role_action(
        &mut self,
        action_id: ActionId,
        approver_agent_id: &str,
        request_id: u64,
        role: &str,
    ) -> Result<bool, WorldError> {
        use crate::world::incremental_normal_action_module_lifecycle::{
            release_legacy::LegacyModuleReleaseFactsV1,
            release_rules::{
                ModuleReleaseBusinessDecisionV1, prepare_module_release_approve_role_v1,
            },
        };
        let event = match prepare_module_release_approve_role_v1(
            &mut LegacyModuleReleaseFactsV1::new(self),
            action_id,
            approver_agent_id,
            request_id,
            role,
        )? {
            ModuleReleaseBusinessDecisionV1::Rejected(reason) => {
                DomainEvent::ActionRejected { action_id, reason }
            }
            ModuleReleaseBusinessDecisionV1::Emit(event) => event,
        };
        self.append_event(
            WorldEventBody::Domain(event),
            Some(CausedBy::Action(action_id)),
        )?;
        Ok(true)
    }

    pub(super) fn handle_module_release_bind_roles_action(
        &mut self,
        action_id: ActionId,
        operator_agent_id: &str,
        target_agent_id: &str,
        roles: &[String],
    ) -> Result<bool, WorldError> {
        use crate::world::incremental_normal_action_module_lifecycle::{
            release_legacy::LegacyModuleReleaseFactsV1,
            release_rules::{
                ModuleReleaseBusinessDecisionV1, prepare_module_release_bind_roles_v1,
            },
        };
        let event = match prepare_module_release_bind_roles_v1(
            &mut LegacyModuleReleaseFactsV1::new(self),
            action_id,
            operator_agent_id,
            target_agent_id,
            roles,
        )? {
            ModuleReleaseBusinessDecisionV1::Rejected(reason) => {
                DomainEvent::ActionRejected { action_id, reason }
            }
            ModuleReleaseBusinessDecisionV1::Emit(event) => event,
        };
        self.append_event(
            WorldEventBody::Domain(event),
            Some(CausedBy::Action(action_id)),
        )?;
        Ok(true)
    }

    pub(super) fn handle_module_release_reject_action(
        &mut self,
        action_id: ActionId,
        rejector_agent_id: &str,
        request_id: u64,
        reason: &str,
    ) -> Result<bool, WorldError> {
        use crate::world::incremental_normal_action_module_lifecycle::{
            release_legacy::LegacyModuleReleaseFactsV1,
            release_rules::{ModuleReleaseBusinessDecisionV1, prepare_module_release_reject_v1},
        };
        let event = match prepare_module_release_reject_v1(
            &mut LegacyModuleReleaseFactsV1::new(self),
            action_id,
            rejector_agent_id,
            request_id,
            reason,
        )? {
            ModuleReleaseBusinessDecisionV1::Rejected(reason) => {
                DomainEvent::ActionRejected { action_id, reason }
            }
            ModuleReleaseBusinessDecisionV1::Emit(event) => event,
        };
        self.append_event(
            WorldEventBody::Domain(event),
            Some(CausedBy::Action(action_id)),
        )?;
        Ok(true)
    }
}
