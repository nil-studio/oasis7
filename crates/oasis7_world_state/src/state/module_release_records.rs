//! Original selected release-row transition data. This module has no storage,
//! authority, event publisher or ability to construct Current qualification.
use super::*;
pub struct ModuleReleaseRecordProjectionV1 {
    pub request: ModuleReleaseRequestState,
    pub mapping: Option<ModuleReleaseManifestMappingState>,
    pub next_request_id: Option<u64>,
}
pub fn prepare_module_release_records_v1(
    event: &DomainEvent,
    before_request: Option<&ModuleReleaseRequestState>,
    before_mapping: Option<&ModuleReleaseManifestMappingState>,
    now: WorldTime,
    next_current: u64,
) -> Result<ModuleReleaseRecordProjectionV1, WorldError> {
    // Every accepted event produces exactly one request row; rejected events return early.
    let projected_request;
    let mut projected_mapping = None;
    let mut next_request_id = None;
    match event {
        DomainEvent::ModuleReleaseRequested {
            request_id,
            requester_agent_id,
            manifest,
            activate,
            install_target,
            required_roles,
            profile_changes,
        } => {
            validate_module_release_request_id_v1(*request_id)?;
            if before_request.is_some() {
                return Err(invalid(format!(
                    "module release request already exists: request_id={request_id}"
                )));
            }
            let mut normalized_roles: Vec<String> = required_roles
                .iter()
                .map(|role| role.trim().to_ascii_lowercase())
                .filter(|role| !role.is_empty())
                .collect();
            normalized_roles.sort();
            normalized_roles.dedup();
            if normalized_roles.is_empty() {
                normalized_roles = default_module_release_required_roles();
            }
            projected_request = Some(ModuleReleaseRequestState {
                request_id: *request_id,
                requester_agent_id: requester_agent_id.clone(),
                manifest: manifest.clone(),
                activate: *activate,
                install_target: install_target.clone(),
                profile_changes: profile_changes.clone(),
                required_roles: normalized_roles,
                role_approvals: BTreeMap::new(),
                attestations: BTreeMap::new(),
                status: ModuleReleaseRequestStatus::Requested,
                shadow_manifest_hash: None,
                applied_manifest_hash: None,
                applied_proposal_id: None,
                rejected_reason: None,
                created_at: now,
                updated_at: now,
            });
            projected_mapping = Some(ModuleReleaseManifestMappingState {
                request_id: *request_id,
                release_id: format!("release-{request_id}"),
                module_id: manifest.module_id.clone(),
                attestation_count: 0,
                release_wasm_hash: None,
                release_source_hash: None,
                release_build_manifest_hash: None,
                release_builder_image_digest: None,
                release_container_platform: None,
                release_canonicalizer_version: None,
                attestation_platforms: Vec::new(),
                attestation_proof_cids: Vec::new(),
                receipt_evidence_conflict: false,
                shadow_manifest_hash: None,
                applied_manifest_hash: None,
                applied_proposal_id: None,
                status: ModuleReleaseRequestStatus::Requested,
                created_at: now,
                updated_at: now,
            });
            next_request_id = Some(next_current.max(request_id.saturating_add(1)));
        }
        DomainEvent::ModuleReleaseAttested {
            request_id,
            operator_agent_id,
            signer_node_id,
            platform,
            build_manifest_hash,
            source_hash,
            wasm_hash,
            proof_cid,
            builder_image_digest,
            container_platform,
            canonicalizer_version,
        } => {
            validate_module_release_attestation_signer_v1(*request_id, signer_node_id)?;
            let normalized_platform = platform.trim().to_ascii_lowercase();
            if normalized_platform.is_empty() {
                return Err(invalid(format!(
                    "module release attestation platform cannot be empty (request_id={request_id})"
                )));
            }
            let mut request = before_request
                .ok_or_else(|| {
                    invalid(format!(
                        "module release attestation rejected: request not found ({request_id})"
                    ))
                })?
                .clone();
            if matches!(
                request.status,
                ModuleReleaseRequestStatus::Rejected | ModuleReleaseRequestStatus::Applied
            ) {
                return Err(invalid(format!(
                    "module release attestation invalid status for request {}: {:?}",
                    request_id, request.status
                )));
            }
            if request.manifest.wasm_hash != *wasm_hash {
                return Err(invalid(format!(
                    "module release attestation wasm hash mismatch: request_id={} expected={} found={}",
                    request_id, request.manifest.wasm_hash, wasm_hash
                )));
            }
            let attestation_key = format!("{}|{}", signer_node_id.trim(), normalized_platform);
            let next_attestation = ModuleReleaseAttestationState {
                request_id: *request_id,
                signer_node_id: signer_node_id.trim().to_string(),
                platform: normalized_platform,
                submitted_by_agent_id: operator_agent_id.clone(),
                build_manifest_hash: build_manifest_hash.clone(),
                source_hash: source_hash.clone(),
                wasm_hash: wasm_hash.clone(),
                proof_cid: proof_cid.clone(),
                builder_image_digest: builder_image_digest.clone(),
                container_platform: container_platform.clone(),
                canonicalizer_version: canonicalizer_version.clone(),
                submitted_at: now,
            };
            if let Some(existing) = request.attestations.get(attestation_key.as_str()) {
                let same_payload = existing.request_id == *request_id
                    && existing.signer_node_id == next_attestation.signer_node_id
                    && existing.platform == next_attestation.platform
                    && existing.build_manifest_hash == next_attestation.build_manifest_hash
                    && existing.source_hash == next_attestation.source_hash
                    && existing.wasm_hash == next_attestation.wasm_hash
                    && existing.proof_cid == next_attestation.proof_cid
                    && existing.builder_image_digest == next_attestation.builder_image_digest
                    && existing.container_platform == next_attestation.container_platform
                    && existing.canonicalizer_version == next_attestation.canonicalizer_version;
                if !same_payload {
                    return Err(invalid(format!(
                        "module release attestation conflict: request_id={} signer={} platform={}",
                        request_id, signer_node_id, platform
                    )));
                }
            } else {
                request
                    .attestations
                    .insert(attestation_key, next_attestation.clone());
            }
            request.updated_at = now;
            let mut mapping = before_mapping
                .ok_or_else(|| {
                    invalid(format!(
                        "module release mapping missing for attestation request_id={request_id}"
                    ))
                })?
                .clone();
            mapping.attestation_count = request.attestations.len() as u32;
            if !mapping
                .attestation_platforms
                .contains(&next_attestation.platform)
            {
                mapping
                    .attestation_platforms
                    .push(next_attestation.platform.clone());
                mapping.attestation_platforms.sort();
            }
            if !mapping
                .attestation_proof_cids
                .contains(&next_attestation.proof_cid)
            {
                mapping
                    .attestation_proof_cids
                    .push(next_attestation.proof_cid.clone());
                mapping.attestation_proof_cids.sort();
            }
            match mapping.release_wasm_hash.as_ref() {
                None => {
                    mapping.release_wasm_hash = Some(next_attestation.wasm_hash.clone());
                    mapping.release_source_hash = Some(next_attestation.source_hash.clone());
                    mapping.release_build_manifest_hash =
                        Some(next_attestation.build_manifest_hash.clone());
                    mapping.release_builder_image_digest =
                        Some(next_attestation.builder_image_digest.clone());
                    mapping.release_container_platform =
                        Some(next_attestation.container_platform.clone());
                    mapping.release_canonicalizer_version =
                        Some(next_attestation.canonicalizer_version.clone());
                }
                Some(existing_wasm_hash) => {
                    let same_release_evidence = existing_wasm_hash == &next_attestation.wasm_hash
                        && mapping.release_source_hash.as_ref()
                            == Some(&next_attestation.source_hash)
                        && mapping.release_build_manifest_hash.as_ref()
                            == Some(&next_attestation.build_manifest_hash)
                        && mapping.release_builder_image_digest.as_ref()
                            == Some(&next_attestation.builder_image_digest)
                        && mapping.release_container_platform.as_ref()
                            == Some(&next_attestation.container_platform)
                        && mapping.release_canonicalizer_version.as_ref()
                            == Some(&next_attestation.canonicalizer_version);
                    if !same_release_evidence {
                        mapping.receipt_evidence_conflict = true;
                    }
                }
            }
            mapping.updated_at = now;
            projected_request = Some(request);
            projected_mapping = Some(mapping);
        }

        DomainEvent::ModuleReleaseShadowed {
            request_id,
            manifest_hash,
            ..
        } => {
            let mut request = before_request
                .ok_or_else(|| {
                    invalid(format!(
                        "module release shadow rejected: request not found ({request_id})"
                    ))
                })?
                .clone();
            if !matches!(request.status, ModuleReleaseRequestStatus::Requested) {
                return Err(invalid(format!(
                    "module release shadow invalid status for request {}: {:?}",
                    request_id, request.status
                )));
            }
            let mut mapping = before_mapping
                .ok_or_else(|| {
                    invalid(format!(
                        "module release mapping missing for shadow request_id={request_id}"
                    ))
                })?
                .clone();
            request.status = ModuleReleaseRequestStatus::Shadowed;
            request.shadow_manifest_hash = Some(manifest_hash.clone());
            request.updated_at = now;
            mapping.status = ModuleReleaseRequestStatus::Shadowed;
            mapping.shadow_manifest_hash = Some(manifest_hash.clone());
            mapping.updated_at = now;
            projected_request = Some(request);
            projected_mapping = Some(mapping);
        }
        DomainEvent::ModuleReleaseRoleApproved {
            request_id,
            approver_agent_id,
            role,
        } => {
            let mut request = before_request
                .ok_or_else(|| {
                    invalid(format!(
                        "module release approve_role rejected: request not found ({request_id})"
                    ))
                })?
                .clone();
            if !matches!(
                request.status,
                ModuleReleaseRequestStatus::Shadowed
                    | ModuleReleaseRequestStatus::PartiallyApproved
                    | ModuleReleaseRequestStatus::Approved
            ) {
                return Err(invalid(format!(
                    "module release approve_role invalid status for request {}: {:?}",
                    request_id, request.status
                )));
            }
            let normalized_role = role.trim().to_ascii_lowercase();
            if normalized_role.is_empty() {
                return Err(invalid(format!(
                    "module release approve_role role cannot be empty (request_id={request_id})"
                )));
            }
            if !request
                .required_roles
                .iter()
                .any(|item| item == &normalized_role)
            {
                return Err(invalid(format!(
                    "module release approve_role role not required: request_id={} role={}",
                    request_id, normalized_role
                )));
            }
            if let Some(existing) = request.role_approvals.get(&normalized_role) {
                if existing != approver_agent_id {
                    return Err(invalid(format!(
                        "module release approve_role approver mismatch: request_id={} role={} existing={} incoming={}",
                        request_id, normalized_role, existing, approver_agent_id
                    )));
                }
            } else {
                request
                    .role_approvals
                    .insert(normalized_role, approver_agent_id.clone());
            }
            request.status = if request
                .required_roles
                .iter()
                .all(|required| request.role_approvals.contains_key(required))
            {
                ModuleReleaseRequestStatus::Approved
            } else {
                ModuleReleaseRequestStatus::PartiallyApproved
            };
            request.updated_at = now;
            if let Some(mut mapping) = before_mapping.cloned() {
                mapping.status = request.status;
                mapping.updated_at = now;
                projected_mapping = Some(mapping);
            }
            projected_request = Some(request);
        }
        DomainEvent::ModuleReleaseRejected {
            request_id, reason, ..
        } => {
            let mut request = before_request
                .ok_or_else(|| {
                    invalid(format!(
                        "module release reject rejected: request not found ({request_id})"
                    ))
                })?
                .clone();
            if matches!(
                request.status,
                ModuleReleaseRequestStatus::Applied | ModuleReleaseRequestStatus::Rejected
            ) {
                return Err(invalid(format!(
                    "module release reject invalid status for request {}: {:?}",
                    request_id, request.status
                )));
            }
            if reason.trim().is_empty() {
                return Err(invalid(format!(
                    "module release reject reason cannot be empty (request_id={request_id})"
                )));
            }
            request.status = ModuleReleaseRequestStatus::Rejected;
            request.rejected_reason = Some(reason.clone());
            request.updated_at = now;
            if let Some(mut mapping) = before_mapping.cloned() {
                mapping.status = ModuleReleaseRequestStatus::Rejected;
                mapping.updated_at = now;
                projected_mapping = Some(mapping);
            }
            projected_request = Some(request);
        }
        DomainEvent::ModuleReleaseApplied {
            request_id,
            manifest_hash,
            proposal_id,
            ..
        } => {
            let mut request = before_request
                .ok_or_else(|| {
                    invalid(format!(
                        "module release apply rejected: request not found ({request_id})"
                    ))
                })?
                .clone();
            if !matches!(request.status, ModuleReleaseRequestStatus::Approved) {
                return Err(invalid(format!(
                    "module release apply invalid status for request {}: {:?}",
                    request_id, request.status
                )));
            }
            request.status = ModuleReleaseRequestStatus::Applied;
            request.applied_manifest_hash = Some(manifest_hash.clone());
            request.applied_proposal_id = (*proposal_id != 0).then_some(*proposal_id);
            request.updated_at = now;
            let mut mapping = before_mapping
                .ok_or_else(|| {
                    invalid(format!(
                        "module release mapping missing for apply request_id={request_id}"
                    ))
                })?
                .clone();
            mapping.status = ModuleReleaseRequestStatus::Applied;
            mapping.applied_manifest_hash = Some(manifest_hash.clone());
            mapping.applied_proposal_id = (*proposal_id != 0).then_some(*proposal_id);
            mapping.updated_at = now;
            projected_request = Some(request);
            projected_mapping = Some(mapping);
        }
        _ => {
            return Err(invalid(
                "release row projection requires a request-record event",
            ));
        }
    }
    Ok(ModuleReleaseRecordProjectionV1 {
        request: projected_request
            .ok_or_else(|| invalid("release row projection missing request"))?,
        mapping: projected_mapping,
        next_request_id,
    })
}
fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}

pub fn validate_module_release_request_id_v1(request_id: u64) -> Result<(), WorldError> {
    if request_id == 0 {
        Err(invalid("module release request_id must be > 0"))
    } else {
        Ok(())
    }
}
pub fn validate_module_release_attestation_signer_v1(
    request_id: u64,
    signer: &str,
) -> Result<(), WorldError> {
    if signer.trim().is_empty() {
        Err(invalid(format!(
            "module release attestation signer_node_id cannot be empty (request_id={request_id})"
        )))
    } else {
        Ok(())
    }
}
