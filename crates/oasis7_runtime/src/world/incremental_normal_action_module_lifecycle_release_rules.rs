//! Unique six release-request Action business predicates. The only observers
//! are sealed Legacy World and genuine Current source implementations.
use super::*;
use super::instance_rules::ModuleBusinessErrorV1;
use crate::{ActionId, DomainEvent, ModuleManifest, ModuleProfileChanges, RejectReason};
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_world_state::state::{ModuleReleaseRequestState,ModuleReleaseRequestStatus};
use std::ops::Deref;
pub(in crate::world) mod sealed{pub trait Sealed{}}
pub(in crate::world) enum ModuleReleaseRequestSnapshotV1{Legacy(ModuleReleaseRequestState),Current(ReservedValue<ModuleReleaseRequestState>)}
impl Deref for ModuleReleaseRequestSnapshotV1{type Target=ModuleReleaseRequestState;fn deref(&self)->&Self::Target{match self{Self::Legacy(v)=>v,Self::Current(v)=>v}}}
pub(in crate::world) trait ModuleReleaseBusinessFactsV1:sealed::Sealed{
 type Error:ModuleBusinessErrorV1;
 fn agent_exists(&mut self,id:&str)->Result<bool,Self::Error>;
 fn artifact_exists(&mut self,hash:&str)->Result<bool,Self::Error>;
 fn owner(&mut self,hash:&str)->Result<Option<String>,Self::Error>;
 fn frozen_admission(&mut self,manifest:&ModuleManifest)->Result<(),Self::Error>;
 fn request(&mut self,id:u64)->Result<Option<ModuleReleaseRequestSnapshotV1>,Self::Error>;
 fn next_request_id(&mut self)->Result<u64,Self::Error>;
 fn node_identity_present(&mut self,id:&str)->Result<bool,Self::Error>;
 fn role_membership(&mut self,agent:&str,role:&str)->Result<oasis7_world_state::incremental_authorization_mapsets::AuthorizationMembershipV1,Self::Error>;
 fn profile_changes(&mut self,changes:&ModuleProfileChanges)->Result<Result<(),String>,Self::Error>;
 fn shadow_hash(&mut self,manifest:&ModuleManifest,activate:bool,changes:&ModuleProfileChanges)->Result<Result<String,String>,Self::Error>;
}
pub(in crate::world) enum ModuleReleaseBusinessDecisionV1{Rejected(RejectReason),Emit(DomainEvent)}
const MODULE_RELEASE_ATTESTATION_LIMIT:usize=128;
pub(in crate::world) fn prepare_module_release_submit_v1<S:ModuleReleaseBusinessFactsV1>(
        source:&mut S,
        action_id: ActionId,
        requester_agent_id: &str,
        manifest: &oasis7_wasm_abi::ModuleManifest,
        activate: bool,
        install_target: &ModuleInstallTarget,
        required_roles: &[String],
        profile_changes: &ModuleProfileChanges,
    ) -> Result<ModuleReleaseBusinessDecisionV1,S::Error> {
        if !source.agent_exists(requester_agent_id)? {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::AgentNotFound {
                        agent_id: requester_agent_id.to_string(),
                    }));

        }
        if manifest.module_id.trim().is_empty() {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![
                            "module release submit rejected: module_id is empty".to_string(),
                        ],
                    }));

        }
        if manifest.version.trim().is_empty() {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec!["module release submit rejected: version is empty".to_string()],
                    }));

        }
        if !source.artifact_exists(&manifest.wasm_hash)? {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release submit rejected: module artifact missing {}",
                            manifest.wasm_hash
                        )],
                    }));

        }
        if let Some(owner_agent_id) = source.owner(&manifest.wasm_hash)?
            && owner_agent_id != requester_agent_id
        {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release submit rejected: requester {} does not own {} (owner {})",
                            requester_agent_id, manifest.wasm_hash, owner_agent_id
                        )],
                    }));

        }
        if let Err(error)=source.frozen_admission(manifest){match error.into_business(){Ok(error)=>return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("{error:?}")]})),Err(error)=>return Err(error)}}

        let request_id = source.next_request_id()?;
        let normalized_roles = crate::world::World::normalize_module_release_required_roles(required_roles);
        return Ok(ModuleReleaseBusinessDecisionV1::Emit(DomainEvent::ModuleReleaseRequested {
                request_id,
                requester_agent_id: requester_agent_id.to_string(),
                manifest: manifest.clone(),
                activate,
                install_target: install_target.clone(),
                required_roles: normalized_roles,
                profile_changes: profile_changes.clone(),
            }));

    }

pub(in crate::world) fn prepare_module_release_shadow_v1<S:ModuleReleaseBusinessFactsV1>(
        source:&mut S,
        action_id: ActionId,
        operator_agent_id: &str,
        request_id: u64,
    ) -> Result<ModuleReleaseBusinessDecisionV1,S::Error> {
        if !source.agent_exists(operator_agent_id)? {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::AgentNotFound {
                        agent_id: operator_agent_id.to_string(),
                    }));

        }
        let Some(request) = source.request(request_id)? else {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release shadow rejected: request not found ({request_id})"
                        )],
                    }));

        };
        if !matches!(request.status, ModuleReleaseRequestStatus::Requested) {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release shadow rejected: invalid status {:?} for request {}",
                            request.status, request_id
                        )],
                    }));

        }
        if let Err(error)=source.frozen_admission(&request.manifest){match error.into_business(){Ok(error)=>return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("{error:?}")]})),Err(error)=>return Err(error)}}
        if let Err(reason) = source.profile_changes(&request.profile_changes)?
        {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!("module release shadow rejected: {reason}")],
                    }));

        }
        let shadow_manifest_hash = match source.shadow_hash(
            &request.manifest,
            request.activate,
            &request.profile_changes,
        )? {
            Ok(hash) => hash,
            Err(reason) => {
                return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                            notes: vec![reason],
                        }));

            }
        };
        return Ok(ModuleReleaseBusinessDecisionV1::Emit(DomainEvent::ModuleReleaseShadowed {
                request_id,
                operator_agent_id: operator_agent_id.to_string(),
                manifest_hash: shadow_manifest_hash,
            }));

    }

pub(in crate::world) fn prepare_module_release_submit_attestation_v1<S:ModuleReleaseBusinessFactsV1>(
        source:&mut S,
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
    ) -> Result<ModuleReleaseBusinessDecisionV1,S::Error> {
        if !source.agent_exists(operator_agent_id)? {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::AgentNotFound {
                        agent_id: operator_agent_id.to_string(),
                    }));

        }
        let Some(request) = source.request(request_id)? else {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release attestation rejected: request not found ({request_id})"
                        )],
                    }));

        };
        if matches!(
            request.status,
            ModuleReleaseRequestStatus::Rejected | ModuleReleaseRequestStatus::Applied
        ) {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release attestation rejected: invalid status {:?} for request {}",
                            request.status, request_id
                        )],
                    }));

        }
        let normalized_signer_node_id = signer_node_id.trim().to_string();
        if normalized_signer_node_id.is_empty() {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![
                            "module release attestation rejected: signer_node_id is empty"
                                .to_string(),
                        ],
                    }));

        }
        if !source.node_identity_present(normalized_signer_node_id.as_str())?
        {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release attestation rejected: signer_node_id is untrusted ({})",
                            normalized_signer_node_id
                        )],
                    }));

        }
        let Some(normalized_platform) =
            crate::world::World::normalize_module_release_attestation_platform(platform)
        else {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![
                            "module release attestation rejected: platform is empty".to_string(),
                        ],
                    }));

        };
        let normalized_build_manifest_hash = match crate::world::World::normalize_module_release_attestation_hash(
            build_manifest_hash,
            "build_manifest_hash",
        ) {
            Ok(hash) => hash,
            Err(note) => {
                return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied { notes: vec![note] }));

            }
        };
        let normalized_source_hash =
            match crate::world::World::normalize_module_release_attestation_hash(source_hash, "source_hash") {
                Ok(hash) => hash,
                Err(note) => {
                    return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied { notes: vec![note] }));

                }
            };
        let normalized_wasm_hash =
            match crate::world::World::normalize_module_release_attestation_hash(wasm_hash, "wasm_hash") {
                Ok(hash) => hash,
                Err(note) => {
                    return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied { notes: vec![note] }));

                }
            };
        if request.manifest.wasm_hash != normalized_wasm_hash {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release attestation rejected: wasm hash mismatch expected {} found {}",
                            request.manifest.wasm_hash, normalized_wasm_hash
                        )],
                    }));

        }
        let Some(normalized_proof_cid) =
            crate::world::World::normalize_module_release_attestation_proof_cid(proof_cid)
        else {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![
                            "module release attestation rejected: proof_cid is empty or too long"
                                .to_string(),
                        ],
                    }));

        };
        let normalized_builder_image_digest =
            match crate::world::World::normalize_module_release_attestation_builder_image_digest(
                builder_image_digest,
            ) {
                Ok(digest) => digest,
                Err(note) => {
                    return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied { notes: vec![note] }));

                }
            };
        let normalized_container_platform = match crate::world::World::normalize_module_release_attestation_label(
            container_platform,
            "container_platform",
        ) {
            Ok(value) => value,
            Err(note) => {
                return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied { notes: vec![note] }));

            }
        };
        let normalized_canonicalizer_version =
            match crate::world::World::normalize_module_release_attestation_label(
                canonicalizer_version,
                "canonicalizer_version",
            ) {
                Ok(value) => value,
                Err(note) => {
                    return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied { notes: vec![note] }));

                }
            };
        let attestation_key = crate::world::World::module_release_attestation_key(
            normalized_signer_node_id.as_str(),
            normalized_platform.as_str(),
        );
        if let Some(existing) = request.attestations.get(attestation_key.as_str()) {
            let same_payload = existing.build_manifest_hash == normalized_build_manifest_hash
                && existing.source_hash == normalized_source_hash
                && existing.wasm_hash == normalized_wasm_hash
                && existing.proof_cid == normalized_proof_cid
                && existing.builder_image_digest == normalized_builder_image_digest
                && existing.container_platform == normalized_container_platform
                && existing.canonicalizer_version == normalized_canonicalizer_version;
            if !same_payload {
                return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                            notes: vec![format!(
                                "module release attestation rejected: conflicting attestation already exists for signer={} platform={}",
                                normalized_signer_node_id, normalized_platform
                            )],
                        }));

            }
        } else if request.attestations.len() >= MODULE_RELEASE_ATTESTATION_LIMIT {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release attestation rejected: attestation limit exceeded for request {}",
                            request_id
                        )],
                    }));

        }
        return Ok(ModuleReleaseBusinessDecisionV1::Emit(DomainEvent::ModuleReleaseAttested {
                request_id,
                operator_agent_id: operator_agent_id.to_string(),
                signer_node_id: normalized_signer_node_id,
                platform: normalized_platform,
                build_manifest_hash: normalized_build_manifest_hash,
                source_hash: normalized_source_hash,
                wasm_hash: normalized_wasm_hash,
                proof_cid: normalized_proof_cid,
                builder_image_digest: normalized_builder_image_digest,
                container_platform: normalized_container_platform,
                canonicalizer_version: normalized_canonicalizer_version,
            }));

    }

pub(in crate::world) fn prepare_module_release_approve_role_v1<S:ModuleReleaseBusinessFactsV1>(
        source:&mut S,
        action_id: ActionId,
        approver_agent_id: &str,
        request_id: u64,
        role: &str,
    ) -> Result<ModuleReleaseBusinessDecisionV1,S::Error> {
        if !source.agent_exists(approver_agent_id)? {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::AgentNotFound {
                        agent_id: approver_agent_id.to_string(),
                    }));

        }
        let Some(request) = source.request(request_id)? else {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release approve_role rejected: request not found ({request_id})"
                        )],
                    }));

        };
        if !matches!(
            request.status,
            ModuleReleaseRequestStatus::Shadowed
                | ModuleReleaseRequestStatus::PartiallyApproved
                | ModuleReleaseRequestStatus::Approved
        ) {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release approve_role rejected: invalid status {:?} for request {}",
                            request.status, request_id
                        )],
                    }));

        }
        let Some(normalized_role) = crate::world::World::normalize_module_release_role(role) else {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![
                            "module release approve_role rejected: role is empty".to_string(),
                        ],
                    }));

        };
        if source.role_membership(approver_agent_id,&normalized_role)? != oasis7_world_state::incremental_authorization_mapsets::AuthorizationMembershipV1::Present {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release approve_role rejected: approver role binding missing for {} role {}",
                            approver_agent_id, normalized_role
                        )],
                    }));

        }
        if !request
            .required_roles
            .iter()
            .any(|required| required == &normalized_role)
        {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release approve_role rejected: role not required ({normalized_role})"
                        )],
                    }));

        }
        if let Some(existing_approver) = request.role_approvals.get(&normalized_role)
            && existing_approver != approver_agent_id
        {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release approve_role rejected: role {} already approved by {}",
                            normalized_role, existing_approver
                        )],
                    }));

        }
        return Ok(ModuleReleaseBusinessDecisionV1::Emit(DomainEvent::ModuleReleaseRoleApproved {
                request_id,
                approver_agent_id: approver_agent_id.to_string(),
                role: normalized_role,
            }));

    }

pub(in crate::world) fn prepare_module_release_bind_roles_v1<S:ModuleReleaseBusinessFactsV1>(
        source:&mut S,
        action_id: ActionId,
        operator_agent_id: &str,
        target_agent_id: &str,
        roles: &[String],
    ) -> Result<ModuleReleaseBusinessDecisionV1,S::Error> {
        if !source.agent_exists(operator_agent_id)? {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::AgentNotFound {
                        agent_id: operator_agent_id.to_string(),
                    }));

        }
        if !source.agent_exists(target_agent_id)? {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::AgentNotFound {
                        agent_id: target_agent_id.to_string(),
                    }));

        }
        let normalized_roles = crate::world::World::normalize_module_release_role_set(roles);
        return Ok(ModuleReleaseBusinessDecisionV1::Emit(DomainEvent::ModuleReleaseRolesBound {
                operator_agent_id: operator_agent_id.to_string(),
                target_agent_id: target_agent_id.to_string(),
                roles: normalized_roles,
            }));

    }

pub(in crate::world) fn prepare_module_release_reject_v1<S:ModuleReleaseBusinessFactsV1>(
        source:&mut S,
        action_id: ActionId,
        rejector_agent_id: &str,
        request_id: u64,
        reason: &str,
    ) -> Result<ModuleReleaseBusinessDecisionV1,S::Error> {
        if !source.agent_exists(rejector_agent_id)? {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::AgentNotFound {
                        agent_id: rejector_agent_id.to_string(),
                    }));

        }
        let Some(request) = source.request(request_id)? else {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release reject rejected: request not found ({request_id})"
                        )],
                    }));

        };
        if matches!(
            request.status,
            ModuleReleaseRequestStatus::Rejected | ModuleReleaseRequestStatus::Applied
        ) {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release reject rejected: invalid status {:?} for request {}",
                            request.status, request_id
                        )],
                    }));

        }
        if reason.trim().is_empty() {
            return Ok(ModuleReleaseBusinessDecisionV1::Rejected(RejectReason::RuleDenied {
                        notes: vec!["module release reject rejected: reason is empty".to_string()],
                    }));

        }
        return Ok(ModuleReleaseBusinessDecisionV1::Emit(DomainEvent::ModuleReleaseRejected {
                request_id,
                rejector_agent_id: rejector_agent_id.to_string(),
                reason: reason.trim().to_string(),
            }));

    }
