//! Owned sparse profile/release mutations; validation never writes canonical state.
use super::*;
use serde::ser::SerializeMap;

#[derive(Debug)]
pub(crate) struct PreparedModuleRelease {
    events: Vec<DomainEvent>,
    agents: BTreeMap<String, AgentCell>,
    pub(crate) products: BTreeMap<String, ProductProfileV1>,
    pub(crate) recipes: BTreeMap<String, RecipeProfileV1>,
    pub(crate) factories: BTreeMap<String, FactoryProfileV1>,
    pub(crate) requests: BTreeMap<u64, ModuleReleaseRequestState>,
    pub(crate) mappings: BTreeMap<u64, ModuleReleaseManifestMappingState>,
    pub(crate) next_request_id: Option<u64>,
    pub(crate) role_bindings: BTreeMap<String, Option<BTreeSet<String>>>,
    world_materials: BTreeMap<String, i64>,
}

impl PreparedModuleRelease {
    pub(crate) fn new(state: &WorldState, agents: BTreeMap<String, AgentCell>) -> Self {
        Self {
            events: Vec::new(),
            agents,
            products: BTreeMap::new(),
            recipes: BTreeMap::new(),
            factories: BTreeMap::new(),
            requests: BTreeMap::new(),
            mappings: BTreeMap::new(),
            next_request_id: None,
            role_bindings: BTreeMap::new(),
            world_materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|ledger| !ledger.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        }
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        self.events.as_slice() == std::slice::from_ref(event)
    }

    pub(crate) fn routed_agents(&self) -> BTreeMap<String, AgentCell> {
        let mut agents = self.agents.clone();
        for event in &self.events {
            if let Some(agent_id) = event.agent_id()
                && let Some(cell) = agents.get_mut(agent_id)
            {
                cell.mailbox.push_back(event.clone());
            }
        }
        agents
    }

    pub(crate) fn install_routed(mut self, state: &mut WorldState) {
        self.agents = self.routed_agents();
        self.install_infallible(state);
    }

    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state.agents.extend(self.agents);
        state.product_profiles.extend(self.products);
        state.recipe_profiles.extend(self.recipes);
        state.factory_profiles.extend(self.factories);
        state.module_release_requests.extend(self.requests);
        state.module_release_manifest_mappings.extend(self.mappings);
        if let Some(next_request_id) = self.next_request_id {
            state.next_module_release_request_id = next_request_id;
        }
        for (agent_id, roles) in self.role_bindings {
            match roles {
                Some(roles) => {
                    state.module_release_role_bindings.insert(agent_id, roles);
                }
                None => {
                    state.module_release_role_bindings.remove(&agent_id);
                }
            }
        }
        state.materials = self.world_materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.world_materials);
    }

    pub(crate) fn serialize_material_fields<S: serde::ser::SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        output.serialize_field("materials", &self.world_materials)?;
        let updates = BTreeMap::from([(MaterialLedgerId::world(), &self.world_materials)]);
        let base = state
            .material_ledgers
            .iter()
            .map(|(key, value)| (key.clone(), value))
            .collect();
        output.serialize_field(
            "material_ledgers",
            &ReleaseMapProjection {
                base: &base,
                updates: &updates,
            },
        )
    }

    pub(crate) fn apply_event(
        &mut self,
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        let (operator, profile_kind, proposal_id) = match event {
            DomainEvent::ProductProfileGoverned {
                operator_agent_id,
                proposal_id,
                ..
            } => (operator_agent_id, "product", Some(*proposal_id)),
            DomainEvent::RecipeProfileGoverned {
                operator_agent_id,
                proposal_id,
                ..
            } => (operator_agent_id, "recipe", Some(*proposal_id)),
            DomainEvent::FactoryProfileGoverned {
                operator_agent_id,
                proposal_id,
                ..
            } => (operator_agent_id, "factory", Some(*proposal_id)),
            DomainEvent::ModuleReleaseApplied {
                operator_agent_id, ..
            } => (operator_agent_id, "", None),
            DomainEvent::ModuleReleaseRequested {
                requester_agent_id, ..
            } => (requester_agent_id, "", None),
            DomainEvent::ModuleReleaseAttested {
                operator_agent_id, ..
            }
            | DomainEvent::ModuleReleaseRolesBound {
                operator_agent_id, ..
            } => (operator_agent_id, "", None),
            DomainEvent::ModuleReleaseShadowed {
                operator_agent_id, ..
            } => (operator_agent_id, "", None),
            DomainEvent::ModuleReleaseRoleApproved {
                approver_agent_id, ..
            } => (approver_agent_id, "", None),
            DomainEvent::ModuleReleaseRejected {
                rejector_agent_id, ..
            } => (rejector_agent_id, "", None),
            _ => unreachable!(
                "release preparation requires a precursor, review, profile or completion event"
            ),
        };
        if let Some(proposal_id) = proposal_id {
            if !self.agents.contains_key(operator) && !state.agents.contains_key(operator) {
                return Err(WorldError::AgentNotFound {
                    agent_id: operator.clone(),
                });
            }
            if proposal_id == 0 {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: format!("{profile_kind} profile governed proposal_id must be > 0"),
                });
            }
        }
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
                if *request_id == 0 {
                    return Err(invalid("module release request_id must be > 0"));
                }
                if !state.agents.contains_key(requester_agent_id) {
                    return Err(WorldError::AgentNotFound {
                        agent_id: requester_agent_id.clone(),
                    });
                }
                if state.module_release_requests.contains_key(request_id) {
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
                self.requests.insert(
                    *request_id,
                    ModuleReleaseRequestState {
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
                    },
                );
                self.mappings.insert(
                    *request_id,
                    ModuleReleaseManifestMappingState {
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
                    },
                );
                self.next_request_id = Some(
                    state
                        .next_module_release_request_id
                        .max(request_id.saturating_add(1)),
                );
            }
            DomainEvent::ModuleReleaseShadowed {
                request_id,
                manifest_hash,
                ..
            } => {
                let mut request = self
                    .requests
                    .get(request_id)
                    .or_else(|| state.module_release_requests.get(request_id))
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
                let mut mapping = self
                    .mappings
                    .get(request_id)
                    .or_else(|| state.module_release_manifest_mappings.get(request_id))
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
                self.requests.insert(*request_id, request);
                self.mappings.insert(*request_id, mapping);
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
                if !state.agents.contains_key(operator_agent_id) {
                    return Err(WorldError::AgentNotFound {
                        agent_id: operator_agent_id.clone(),
                    });
                }
                if signer_node_id.trim().is_empty() {
                    return Err(invalid(format!(
                        "module release attestation signer_node_id cannot be empty (request_id={request_id})"
                    )));
                }
                if !state
                    .node_identity_bindings
                    .contains_key(signer_node_id.trim())
                {
                    return Err(invalid(format!(
                        "module release attestation signer_node_id is untrusted: {signer_node_id}"
                    )));
                }
                let normalized_platform = platform.trim().to_ascii_lowercase();
                if normalized_platform.is_empty() {
                    return Err(invalid(format!(
                        "module release attestation platform cannot be empty (request_id={request_id})"
                    )));
                }
                let mut request = state
                    .module_release_requests
                    .get(request_id)
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
                let mut mapping = state
                    .module_release_manifest_mappings
                    .get(request_id)
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
                        let same_release_evidence = existing_wasm_hash
                            == &next_attestation.wasm_hash
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
                self.requests.insert(*request_id, request);
                self.mappings.insert(*request_id, mapping);
            }
            DomainEvent::ModuleReleaseRoleApproved {
                request_id,
                approver_agent_id,
                role,
            } => {
                let mut request = self
                    .requests
                    .get(request_id)
                    .or_else(|| state.module_release_requests.get(request_id))
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
                if let Some(mut mapping) = self
                    .mappings
                    .get(request_id)
                    .or_else(|| state.module_release_manifest_mappings.get(request_id))
                    .cloned()
                {
                    mapping.status = request.status;
                    mapping.updated_at = now;
                    self.mappings.insert(*request_id, mapping);
                }
                self.requests.insert(*request_id, request);
            }
            DomainEvent::ModuleReleaseRolesBound {
                operator_agent_id,
                target_agent_id,
                roles,
            } => {
                if !state.agents.contains_key(operator_agent_id) {
                    return Err(WorldError::AgentNotFound {
                        agent_id: operator_agent_id.clone(),
                    });
                }
                if !state.agents.contains_key(target_agent_id) {
                    return Err(WorldError::AgentNotFound {
                        agent_id: target_agent_id.clone(),
                    });
                }
                let normalized_roles: BTreeSet<String> = roles
                    .iter()
                    .map(|role| role.trim().to_ascii_lowercase())
                    .filter(|role| !role.is_empty())
                    .collect();
                self.role_bindings.insert(
                    target_agent_id.clone(),
                    (!normalized_roles.is_empty()).then_some(normalized_roles),
                );
                if operator_agent_id != target_agent_id
                    && let Some(cell) = state.agents.get(target_agent_id)
                {
                    let mut cell = cell.clone();
                    cell.last_active = now;
                    self.agents.insert(target_agent_id.clone(), cell);
                }
            }
            DomainEvent::ModuleReleaseRejected {
                request_id, reason, ..
            } => {
                let mut request = self
                    .requests
                    .get(request_id)
                    .or_else(|| state.module_release_requests.get(request_id))
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
                if let Some(mut mapping) = self
                    .mappings
                    .get(request_id)
                    .or_else(|| state.module_release_manifest_mappings.get(request_id))
                    .cloned()
                {
                    mapping.status = ModuleReleaseRequestStatus::Rejected;
                    mapping.updated_at = now;
                    self.mappings.insert(*request_id, mapping);
                }
                self.requests.insert(*request_id, request);
            }
            DomainEvent::ProductProfileGoverned { profile, .. } => {
                if profile.product_id.trim().is_empty() {
                    return Err(invalid("product profile product_id cannot be empty"));
                }
                if profile.role_tag.trim().is_empty() {
                    return Err(invalid(format!(
                        "product profile role_tag cannot be empty: {}",
                        profile.product_id
                    )));
                }
                self.products
                    .insert(profile.product_id.clone(), profile.clone());
            }
            DomainEvent::RecipeProfileGoverned { profile, .. } => {
                if profile.recipe_id.trim().is_empty() {
                    return Err(invalid("recipe profile recipe_id cannot be empty"));
                }
                self.recipes
                    .insert(profile.recipe_id.clone(), profile.clone());
            }
            DomainEvent::FactoryProfileGoverned { profile, .. } => {
                if profile.factory_id.trim().is_empty() {
                    return Err(invalid("factory profile factory_id cannot be empty"));
                }
                if profile.tier == 0 {
                    return Err(invalid(format!(
                        "factory profile tier must be >= 1: {}",
                        profile.factory_id
                    )));
                }
                if profile.recipe_slots == 0 {
                    return Err(invalid(format!(
                        "factory profile recipe_slots must be > 0: {}",
                        profile.factory_id
                    )));
                }
                if let Some(existing) = self
                    .factories
                    .get(&profile.factory_id)
                    .or_else(|| state.factory_profiles.get(&profile.factory_id))
                {
                    if existing != profile {
                        return Err(invalid(format!(
                            "factory profile id is immutable and conflicts with persisted profile: factory_id={}",
                            profile.factory_id
                        )));
                    }
                } else {
                    self.factories
                        .insert(profile.factory_id.clone(), profile.clone());
                }
            }
            DomainEvent::ModuleReleaseApplied {
                request_id,
                manifest_hash,
                proposal_id,
                ..
            } => {
                let mut request = self
                    .requests
                    .get(request_id)
                    .or_else(|| state.module_release_requests.get(request_id))
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
                let mut mapping = self
                    .mappings
                    .get(request_id)
                    .or_else(|| state.module_release_manifest_mappings.get(request_id))
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
                self.requests.insert(*request_id, request);
                self.mappings.insert(*request_id, mapping);
            }
            _ => unreachable!(),
        }
        // Resolve against the staged fee-debited installer first when roles coincide.
        if !self.agents.contains_key(operator)
            && let Some(cell) = state.agents.get(operator)
        {
            self.agents.insert(operator.clone(), cell.clone());
        }
        if let Some(cell) = self.agents.get_mut(operator) {
            cell.last_active = now;
        }
        self.events.push(event.clone());
        Ok(())
    }
}

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}

impl WorldState {
    pub(crate) fn prepare_module_release_event(
        &self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<PreparedModuleRelease, WorldError> {
        let mut prepared = PreparedModuleRelease::new(self, BTreeMap::new());
        prepared.apply_event(self, event, now)?;
        Ok(prepared)
    }
}

pub(crate) struct ReleaseMapProjection<'a, K, V> {
    pub(crate) base: &'a BTreeMap<K, V>,
    pub(crate) updates: &'a BTreeMap<K, V>,
}

pub(crate) struct ReleaseOptionalMapProjection<'a, K, V> {
    pub(crate) base: &'a BTreeMap<K, V>,
    pub(crate) updates: &'a BTreeMap<K, Option<V>>,
}

impl<K: Ord + Serialize, V: Serialize> Serialize for ReleaseOptionalMapProjection<'_, K, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let retained = self
            .base
            .keys()
            .filter(|key| !matches!(self.updates.get(*key), Some(None)))
            .count();
        let added = self
            .updates
            .iter()
            .filter(|(key, value)| value.is_some() && !self.base.contains_key(*key))
            .count();
        let mut map = serializer.serialize_map(Some(retained + added))?;
        let mut updates = self.updates.iter().peekable();
        for (key, value) in self.base {
            while updates.peek().is_some_and(|(next, _)| *next < key) {
                let (next, replacement) = updates.next().unwrap();
                if let Some(replacement) = replacement {
                    map.serialize_entry(next, replacement)?;
                }
            }
            if updates.peek().is_some_and(|(next, _)| *next == key) {
                let (_, replacement) = updates.next().unwrap();
                if let Some(replacement) = replacement {
                    map.serialize_entry(key, replacement)?;
                }
            } else {
                map.serialize_entry(key, value)?;
            }
        }
        for (key, replacement) in updates {
            if let Some(replacement) = replacement {
                map.serialize_entry(key, replacement)?;
            }
        }
        map.end()
    }
}

impl<K: Ord + Serialize, V: Serialize> Serialize for ReleaseMapProjection<'_, K, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let added = self
            .updates
            .keys()
            .filter(|key| !self.base.contains_key(*key))
            .count();
        let mut map = serializer.serialize_map(Some(self.base.len() + added))?;
        let mut updates = self.updates.iter().peekable();
        for (key, value) in self.base {
            while updates.peek().is_some_and(|(next, _)| *next < key) {
                let (next, replacement) = updates.next().unwrap();
                map.serialize_entry(next, replacement)?;
            }
            if updates.peek().is_some_and(|(next, _)| *next == key) {
                let (_, replacement) = updates.next().unwrap();
                map.serialize_entry(key, replacement)?;
            } else {
                map.serialize_entry(key, value)?;
            }
        }
        for (key, value) in updates {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}
