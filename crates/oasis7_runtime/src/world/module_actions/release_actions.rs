use super::*;

impl World {
    pub(in crate::world) fn try_apply_runtime_module_action(
        &mut self,
        envelope: &ActionEnvelope,
    ) -> Result<bool, WorldError> {
        let action_id = envelope.id;
        use crate::world::incremental_normal_action_module_lifecycle::rules::{is_market_action_v1,evaluate_market_action_v1,LegacyModuleMarketFactsV1,ModuleMarketDecisionV1};
        if is_market_action_v1(&envelope.action) {
            let decision=evaluate_market_action_v1(&mut LegacyModuleMarketFactsV1(self),&envelope.action)?;
            match decision {
                ModuleMarketDecisionV1::Rejected(reason)=>{self.append_event(WorldEventBody::Domain(DomainEvent::ActionRejected{action_id,reason}),Some(CausedBy::Action(action_id)))?;},
                ModuleMarketDecisionV1::Event(event @ (DomainEvent::ModuleArtifactListed{..}|DomainEvent::ModuleArtifactBidPlaced{..}))=>self.append_module_marketplace_order(event,Some(CausedBy::Action(action_id)))?,
                ModuleMarketDecisionV1::Event(event @ DomainEvent::ModuleArtifactDestroyed{..})=>{
                    let DomainEvent::ModuleArtifactDestroyed{wasm_hash,..}=&event else{unreachable!()};
                    let retirement=self.prepare_module_artifact_retirement(wasm_hash.clone());
                    self.append_module_artifact_retirement(event,Some(CausedBy::Action(action_id)),retirement)?;
                },
                ModuleMarketDecisionV1::Event(event)=>{self.append_event(WorldEventBody::Domain(event),Some(CausedBy::Action(action_id)))?;},
            }
            return Ok(true);
        }
        match &envelope.action {
            Action::CompileModuleArtifactFromSource { .. } | Action::DeployModuleArtifact { .. } => {
                use crate::world::incremental_normal_action_module_lifecycle::deployment_rules::{
                    evaluate_module_deployment_v1, LegacyModuleDeploymentFactsV1, ModuleDeploymentDecisionV1,
                };
                let decision=evaluate_module_deployment_v1(&mut LegacyModuleDeploymentFactsV1(self),&envelope.action)?;
                match decision {
                    ModuleDeploymentDecisionV1::Rejected(reason)=>{
                        self.append_event(WorldEventBody::Domain(DomainEvent::ActionRejected{action_id,reason}),Some(CausedBy::Action(action_id)))?;
                    },
                    ModuleDeploymentDecisionV1::Ready{publisher,hash,bytes,fee}=>{
                        let bytes=bytes.bytes::<LegacyModuleDeploymentFactsV1<'_>>();
                        match self.prepare_module_artifact_registration(hash.clone(),bytes) {
                            Ok(registration)=>{
                                self.append_module_artifact_deployment(DomainEvent::ModuleArtifactDeployed{
                                    publisher_agent_id:publisher.to_owned(),wasm_hash:hash,bytes_len:bytes.len() as u64,
                                    fee_kind:ResourceKind::Electricity,fee_amount:fee,
                                },Some(CausedBy::Action(action_id)),registration)?;
                            },
                            Err(error)=>{
                                let note=if matches!(&envelope.action,Action::CompileModuleArtifactFromSource{..}) {
                                    format!("compile module source rejected: register artifact failed: {error:?}")
                                }else{format!("deploy module artifact rejected: {error:?}")};
                                self.append_event(WorldEventBody::Domain(DomainEvent::ActionRejected{
                                    action_id,reason:RejectReason::RuleDenied{notes:vec![note]},
                                }),Some(CausedBy::Action(action_id)))?;
                            },
                        }
                    },
                }
                Ok(true)
            }
            Action::InstallModuleFromArtifact {
                installer_agent_id,
                manifest,
                activate,
            } => self.apply_install_module_action(
                action_id,
                installer_agent_id.as_str(),
                manifest,
                *activate,
                ModuleInstallTarget::SelfAgent,
                None,
            ),
            Action::InstallModuleFromArtifactWithFinality {
                installer_agent_id,
                manifest,
                activate,
                finality_certificate,
            } => self.apply_install_module_action(
                action_id,
                installer_agent_id.as_str(),
                manifest,
                *activate,
                ModuleInstallTarget::SelfAgent,
                Some(finality_certificate),
            ),
            Action::InstallModuleToTargetFromArtifact {
                installer_agent_id,
                manifest,
                activate,
                install_target,
            } => self.apply_install_module_action(
                action_id,
                installer_agent_id.as_str(),
                manifest,
                *activate,
                install_target.clone(),
                None,
            ),
            Action::InstallModuleToTargetFromArtifactWithFinality {
                installer_agent_id,
                manifest,
                activate,
                install_target,
                finality_certificate,
            } => self.apply_install_module_action(
                action_id,
                installer_agent_id.as_str(),
                manifest,
                *activate,
                install_target.clone(),
                Some(finality_certificate),
            ),
            Action::UpgradeModuleFromArtifact {
                upgrader_agent_id,
                instance_id,
                from_module_version,
                manifest,
                activate,
            } => self.apply_upgrade_module_action(
                action_id,
                upgrader_agent_id.as_str(),
                instance_id.as_str(),
                from_module_version.as_str(),
                manifest,
                *activate,
                None,
            ),
            Action::UpgradeModuleFromArtifactWithFinality {
                upgrader_agent_id,
                instance_id,
                from_module_version,
                manifest,
                activate,
                finality_certificate,
            } => self.apply_upgrade_module_action(
                action_id,
                upgrader_agent_id.as_str(),
                instance_id.as_str(),
                from_module_version.as_str(),
                manifest,
                *activate,
                Some(finality_certificate),
            ),
            Action::RollbackModuleInstance {
                operator_agent_id,
                instance_id,
                target_module_version,
            } => self.apply_rollback_module_instance_action(
                action_id,
                operator_agent_id.as_str(),
                instance_id.as_str(),
                target_module_version.as_str(),
                None,
            ),
            Action::RollbackModuleInstanceWithFinality {
                operator_agent_id,
                instance_id,
                target_module_version,
                finality_certificate,
            } => self.apply_rollback_module_instance_action(
                action_id,
                operator_agent_id.as_str(),
                instance_id.as_str(),
                target_module_version.as_str(),
                Some(finality_certificate),
            ),
            action
                if matches!(
                    action,
                    Action::ModuleReleaseSubmit { .. }
                        | Action::ModuleReleaseShadow { .. }
                        | Action::ModuleReleaseSubmitAttestation { .. }
                        | Action::ModuleReleaseApproveRole { .. }
                        | Action::ModuleReleaseBindRoles { .. }
                        | Action::ModuleReleaseReject { .. }
                        | Action::ModuleReleaseApply { .. }
                        | Action::ModuleReleaseApplyWithFinality { .. }
                ) =>
            {
                Ok(self
                    .try_apply_module_release_action(action_id, action)?
                    .expect("release action must be handled"))
            }
            _ => Ok(false),
        }
    }
}
