use super::*;

impl World {
    pub(super) fn apply_rollback_module_instance_action(
        &mut self,
        action_id: u64,
        operator_agent_id: &str,
        instance_id: &str,
        target_module_version: &str,
        finality_certificate: Option<&GovernanceFinalityCertificate>,
    ) -> Result<bool, WorldError> {
        use crate::world::incremental_normal_action_module_lifecycle::{rollback_legacy::LegacyModuleRollbackFactsV1,rollback_rules::{prepare_module_rollback_prelude_v1,ModuleRollbackPreludeV1},upgrade_rules::ModuleInstanceRecordV1};
        let (instance,target_manifest,changes,from_wasm_hash,from_release_request_id,fee_kind,fee_amount)=match prepare_module_rollback_prelude_v1(
            &mut LegacyModuleRollbackFactsV1::new(self),operator_agent_id,instance_id,target_module_version,
        )?{
            ModuleRollbackPreludeV1::Rejected(reason)=>{
                self.append_event(WorldEventBody::Domain(DomainEvent::ActionRejected{action_id,reason}),Some(CausedBy::Action(action_id)))?;
                return Ok(true);
            },
            ModuleRollbackPreludeV1::Plan{instance,target_source:_,target_manifest,changes,from_wasm_hash,from_release_request_id,fee_kind,fee_amount}=>{
                let instance=match instance{ModuleInstanceRecordV1::Legacy(record)=>record.clone(),ModuleInstanceRecordV1::Current(_)=>unreachable!("fixed legacy rollback observer")};
                (instance,target_manifest,changes,from_wasm_hash,from_release_request_id,fee_kind,fee_amount)
            },
        };

        let target_module_version=target_module_version.trim();

        let module_changes_value = match serde_json::to_value(&changes) {
            Ok(value) => value,
            Err(err) => {
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("serialize module rollback changes failed: {err}")],
                        },
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                return Ok(true);
            }
        };
        let mut manifest_update = self.manifest.clone();
        manifest_update.version = manifest_update.version.saturating_add(1);
        let serde_json::Value::Object(content) = &mut manifest_update.content else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["current manifest content must be object".to_string()],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        content.insert("module_changes".to_string(), module_changes_value);
        let proposal_id =
            match self.propose_manifest_update(manifest_update, operator_agent_id.to_string()) {
                Ok(proposal_id) => proposal_id,
                Err(err) => {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!("propose module rollback rejected: {err:?}")],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
            };
        if let Err(err) = self.shadow_proposal(proposal_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("shadow module rollback rejected: {err:?}")],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if let Err(err) = self.approve_proposal(
            proposal_id,
            operator_agent_id.to_string(),
            ProposalDecision::Approve,
        ) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("approve module rollback rejected: {err:?}")],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let prepared =
            match self.prepare_module_governance_proposal(proposal_id, finality_certificate) {
                Ok(prepared) => prepared,
                Err(err) => {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!("apply module rollback rejected: {err:?}")],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
            };
        let manifest_hash = prepared.applied_hash().to_string();
        prepared.publish_lifecycle_tail(
            self,
            DomainEvent::ModuleRollbackApplied {
                operator_agent_id: operator_agent_id.to_string(),
                instance_id: instance.instance_id.clone(),
                module_id: instance.module_id.clone(),
                from_module_version: instance.module_version,
                to_module_version: target_module_version.to_string(),
                wasm_hash: target_manifest.wasm_hash,
                from_wasm_hash: Some(from_wasm_hash),
                from_release_request_id,
                install_target: instance.install_target,
                active: instance.active,
                proposal_id,
                manifest_hash,
                fee_kind,
                fee_amount,
            },
            Some(CausedBy::Action(action_id)),
        )?;
        Ok(true)
    }
}
