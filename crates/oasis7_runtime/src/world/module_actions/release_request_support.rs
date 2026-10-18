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
        if !self.state.agents.contains_key(requester_agent_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: requester_agent_id.to_string(),
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if manifest.module_id.trim().is_empty() {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "module release submit rejected: module_id is empty".to_string(),
                        ],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if manifest.version.trim().is_empty() {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["module release submit rejected: version is empty".to_string()],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if !self.module_artifacts.contains(&manifest.wasm_hash) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release submit rejected: module artifact missing {}",
                            manifest.wasm_hash
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if let Some(owner_agent_id) = self.state.module_artifact_owners.get(&manifest.wasm_hash)
            && owner_agent_id != requester_agent_id
        {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release submit rejected: requester {} does not own {} (owner {})",
                            requester_agent_id, manifest.wasm_hash, owner_agent_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if self.reject_frozen_module_admission(action_id, manifest)? {
            return Ok(true);
        }

        let request_id = self.peek_next_module_release_request_id();
        let normalized_roles = Self::normalize_module_release_required_roles(required_roles);
        self.append_event(
            WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested {
                request_id,
                requester_agent_id: requester_agent_id.to_string(),
                manifest: manifest.clone(),
                activate,
                install_target: install_target.clone(),
                required_roles: normalized_roles,
                profile_changes: profile_changes.clone(),
            }),
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
        if !self.state.agents.contains_key(operator_agent_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: operator_agent_id.to_string(),
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let Some(request) = self.state.module_release_requests.get(&request_id).cloned() else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release shadow rejected: request not found ({request_id})"
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        if !matches!(request.status, ModuleReleaseRequestStatus::Requested) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release shadow rejected: invalid status {:?} for request {}",
                            request.status, request_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if self.reject_frozen_module_admission(action_id, &request.manifest)? {
            return Ok(true);
        }
        if let Err(reason) = self.validate_module_release_profile_changes(&request.profile_changes)
        {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("module release shadow rejected: {reason}")],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let shadow_manifest_hash = match self.evaluate_module_release_shadow_hash(
            &request.manifest,
            request.activate,
            &request.profile_changes,
        ) {
            Ok(hash) => hash,
            Err(reason) => {
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![reason],
                        },
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                return Ok(true);
            }
        };
        self.append_event(
            WorldEventBody::Domain(DomainEvent::ModuleReleaseShadowed {
                request_id,
                operator_agent_id: operator_agent_id.to_string(),
                manifest_hash: shadow_manifest_hash,
            }),
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
        if !self.state.agents.contains_key(operator_agent_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: operator_agent_id.to_string(),
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let Some(request) = self.state.module_release_requests.get(&request_id).cloned() else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release attestation rejected: request not found ({request_id})"
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        if matches!(
            request.status,
            ModuleReleaseRequestStatus::Rejected | ModuleReleaseRequestStatus::Applied
        ) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release attestation rejected: invalid status {:?} for request {}",
                            request.status, request_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let normalized_signer_node_id = signer_node_id.trim().to_string();
        if normalized_signer_node_id.is_empty() {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "module release attestation rejected: signer_node_id is empty"
                                .to_string(),
                        ],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if self
            .node_identity_public_key(normalized_signer_node_id.as_str())
            .is_none()
        {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release attestation rejected: signer_node_id is untrusted ({})",
                            normalized_signer_node_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let Some(normalized_platform) =
            Self::normalize_module_release_attestation_platform(platform)
        else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "module release attestation rejected: platform is empty".to_string(),
                        ],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        let normalized_build_manifest_hash = match Self::normalize_module_release_attestation_hash(
            build_manifest_hash,
            "build_manifest_hash",
        ) {
            Ok(hash) => hash,
            Err(note) => {
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied { notes: vec![note] },
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                return Ok(true);
            }
        };
        let normalized_source_hash =
            match Self::normalize_module_release_attestation_hash(source_hash, "source_hash") {
                Ok(hash) => hash,
                Err(note) => {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied { notes: vec![note] },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
            };
        let normalized_wasm_hash =
            match Self::normalize_module_release_attestation_hash(wasm_hash, "wasm_hash") {
                Ok(hash) => hash,
                Err(note) => {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied { notes: vec![note] },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
            };
        if request.manifest.wasm_hash != normalized_wasm_hash {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release attestation rejected: wasm hash mismatch expected {} found {}",
                            request.manifest.wasm_hash, normalized_wasm_hash
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let Some(normalized_proof_cid) =
            Self::normalize_module_release_attestation_proof_cid(proof_cid)
        else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "module release attestation rejected: proof_cid is empty or too long"
                                .to_string(),
                        ],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        let normalized_builder_image_digest =
            match Self::normalize_module_release_attestation_builder_image_digest(
                builder_image_digest,
            ) {
                Ok(digest) => digest,
                Err(note) => {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied { notes: vec![note] },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
            };
        let normalized_container_platform = match Self::normalize_module_release_attestation_label(
            container_platform,
            "container_platform",
        ) {
            Ok(value) => value,
            Err(note) => {
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied { notes: vec![note] },
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                return Ok(true);
            }
        };
        let normalized_canonicalizer_version =
            match Self::normalize_module_release_attestation_label(
                canonicalizer_version,
                "canonicalizer_version",
            ) {
                Ok(value) => value,
                Err(note) => {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied { notes: vec![note] },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
            };
        let attestation_key = Self::module_release_attestation_key(
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
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "module release attestation rejected: conflicting attestation already exists for signer={} platform={}",
                                normalized_signer_node_id, normalized_platform
                            )],
                        },
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                return Ok(true);
            }
        } else if request.attestations.len() >= MODULE_RELEASE_ATTESTATION_LIMIT {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release attestation rejected: attestation limit exceeded for request {}",
                            request_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        self.append_event(
            WorldEventBody::Domain(DomainEvent::ModuleReleaseAttested {
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
            }),
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
        if !self.state.agents.contains_key(approver_agent_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: approver_agent_id.to_string(),
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let Some(request) = self.state.module_release_requests.get(&request_id).cloned() else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release approve_role rejected: request not found ({request_id})"
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        if !matches!(
            request.status,
            ModuleReleaseRequestStatus::Shadowed
                | ModuleReleaseRequestStatus::PartiallyApproved
                | ModuleReleaseRequestStatus::Approved
        ) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release approve_role rejected: invalid status {:?} for request {}",
                            request.status, request_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let Some(normalized_role) = Self::normalize_module_release_role(role) else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "module release approve_role rejected: role is empty".to_string(),
                        ],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        let bound_roles = self
            .state
            .module_release_role_bindings
            .get(approver_agent_id)
            .cloned()
            .unwrap_or_default();
        if !bound_roles.contains(&normalized_role) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release approve_role rejected: approver role binding missing for {} role {}",
                            approver_agent_id, normalized_role
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if !request
            .required_roles
            .iter()
            .any(|required| required == &normalized_role)
        {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release approve_role rejected: role not required ({normalized_role})"
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if let Some(existing_approver) = request.role_approvals.get(&normalized_role)
            && existing_approver != approver_agent_id
        {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release approve_role rejected: role {} already approved by {}",
                            normalized_role, existing_approver
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        self.append_event(
            WorldEventBody::Domain(DomainEvent::ModuleReleaseRoleApproved {
                request_id,
                approver_agent_id: approver_agent_id.to_string(),
                role: normalized_role,
            }),
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
        if !self.state.agents.contains_key(operator_agent_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: operator_agent_id.to_string(),
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if !self.state.agents.contains_key(target_agent_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: target_agent_id.to_string(),
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let normalized_roles = Self::normalize_module_release_role_set(roles);
        self.append_event(
            WorldEventBody::Domain(DomainEvent::ModuleReleaseRolesBound {
                operator_agent_id: operator_agent_id.to_string(),
                target_agent_id: target_agent_id.to_string(),
                roles: normalized_roles,
            }),
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
        if !self.state.agents.contains_key(rejector_agent_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: rejector_agent_id.to_string(),
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let Some(request) = self.state.module_release_requests.get(&request_id).cloned() else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release reject rejected: request not found ({request_id})"
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        if matches!(
            request.status,
            ModuleReleaseRequestStatus::Rejected | ModuleReleaseRequestStatus::Applied
        ) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "module release reject rejected: invalid status {:?} for request {}",
                            request.status, request_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if reason.trim().is_empty() {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["module release reject rejected: reason is empty".to_string()],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        self.append_event(
            WorldEventBody::Domain(DomainEvent::ModuleReleaseRejected {
                request_id,
                rejector_agent_id: rejector_agent_id.to_string(),
                reason: reason.trim().to_string(),
            }),
            Some(CausedBy::Action(action_id)),
        )?;
        Ok(true)
    }
}
