use super::*;

impl World {
    pub(super) fn try_apply_module_release_action(
        &mut self,
        action_id: u64,
        action: &Action,
    ) -> Result<Option<bool>, WorldError> {
        match action {
            Action::ModuleReleaseSubmit {
                requester_agent_id,
                manifest,
                activate,
                install_target,
                required_roles,
                profile_changes,
            } => self
                .handle_module_release_submit_action(
                    action_id,
                    requester_agent_id.as_str(),
                    manifest,
                    *activate,
                    install_target,
                    required_roles.as_slice(),
                    profile_changes,
                )
                .map(Some),
            Action::ModuleReleaseShadow {
                operator_agent_id,
                request_id,
            } => self
                .handle_module_release_shadow_action(
                    action_id,
                    operator_agent_id.as_str(),
                    *request_id,
                )
                .map(Some),
            Action::ModuleReleaseSubmitAttestation {
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
            } => self
                .handle_module_release_submit_attestation_action(
                    action_id,
                    operator_agent_id.as_str(),
                    *request_id,
                    signer_node_id.as_str(),
                    platform.as_str(),
                    build_manifest_hash.as_str(),
                    source_hash.as_str(),
                    wasm_hash.as_str(),
                    proof_cid.as_str(),
                    builder_image_digest.as_str(),
                    container_platform.as_str(),
                    canonicalizer_version.as_str(),
                )
                .map(Some),
            Action::ModuleReleaseApproveRole {
                approver_agent_id,
                request_id,
                role,
            } => self
                .handle_module_release_approve_role_action(
                    action_id,
                    approver_agent_id.as_str(),
                    *request_id,
                    role.as_str(),
                )
                .map(Some),
            Action::ModuleReleaseBindRoles {
                operator_agent_id,
                target_agent_id,
                roles,
            } => self
                .handle_module_release_bind_roles_action(
                    action_id,
                    operator_agent_id.as_str(),
                    target_agent_id.as_str(),
                    roles.as_slice(),
                )
                .map(Some),
            Action::ModuleReleaseReject {
                rejector_agent_id,
                request_id,
                reason,
            } => self
                .handle_module_release_reject_action(
                    action_id,
                    rejector_agent_id.as_str(),
                    *request_id,
                    reason.as_str(),
                )
                .map(Some),
            Action::ModuleReleaseApply {
                operator_agent_id,
                request_id,
            } => self
                .apply_module_release_request_action(
                    action_id,
                    operator_agent_id.as_str(),
                    *request_id,
                    None,
                )
                .map(Some),
            Action::ModuleReleaseApplyWithFinality {
                operator_agent_id,
                request_id,
                finality_certificate,
            } => self
                .apply_module_release_request_action(
                    action_id,
                    operator_agent_id.as_str(),
                    *request_id,
                    Some(finality_certificate),
                )
                .map(Some),
            _ => Ok(None),
        }
    }

    fn apply_module_release_request_action(
        &mut self,
        action_id: u64,
        operator_agent_id: &str,
        request_id: u64,
        finality_certificate: Option<&GovernanceFinalityCertificate>,
    ) -> Result<bool, WorldError> {
        use crate::world::incremental_normal_action_module_lifecycle::{release_apply_legacy::LegacyModuleReleaseApplyFactsV1,release_apply_rules::{prepare_module_release_apply_v1,ModuleReleaseApplyDecisionV1}};
        let request=match prepare_module_release_apply_v1(&mut LegacyModuleReleaseApplyFactsV1::new(self),operator_agent_id,request_id)?{
            ModuleReleaseApplyDecisionV1::Ready(request)=>request,
            ModuleReleaseApplyDecisionV1::Rejected(reason)=>{
                self.append_event(WorldEventBody::Domain(DomainEvent::ActionRejected{action_id,reason}),Some(CausedBy::Action(action_id)))?;
                return Ok(true);
            },
        };

        let installer_agent_id = request.requester_agent_id.clone();
        self.apply_install_module_action_with_release(
            action_id,
            installer_agent_id.as_str(),
            &request.manifest,
            request.activate,
            request.install_target.clone(),
            finality_certificate,
            Some(
                super::super::module_release_publication::ModuleReleaseCompletion {
                    request_id,
                    operator_agent_id: operator_agent_id.to_string(),
                    profile_changes: request.profile_changes.clone(),
                },
            ),
        )?;
        Ok(true)
    }

    pub(in crate::world) fn validate_module_release_profile_changes(
        &self,
        changes: &ModuleProfileChanges,
    ) -> Result<(), String> {
        crate::world::incremental_normal_action_module_lifecycle::release_profiles_current::validate_legacy_release_profiles_v1(self,changes)
    }
}
