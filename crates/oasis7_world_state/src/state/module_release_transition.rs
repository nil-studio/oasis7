//! Owned sparse profile/release mutations; validation never writes canonical state.
use super::*;
use serde::ser::SerializeMap;

#[derive(Debug)]
pub struct PreparedModuleRelease {
    events: Vec<DomainEvent>,
    agents: BTreeMap<String, AgentCell>,
    pub products: BTreeMap<String, ProductProfileV1>,
    pub recipes: BTreeMap<String, RecipeProfileV1>,
    pub factories: BTreeMap<String, FactoryProfileV1>,
    pub requests: BTreeMap<u64, ModuleReleaseRequestState>,
    pub mappings: BTreeMap<u64, ModuleReleaseManifestMappingState>,
    pub next_request_id: Option<u64>,
    pub role_bindings: BTreeMap<String, Option<BTreeSet<String>>>,
    world_materials: BTreeMap<String, i64>,
}

impl PreparedModuleRelease {
    pub fn new(state: &WorldState, agents: BTreeMap<String, AgentCell>) -> Self {
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

    pub fn matches_event(&self, event: &DomainEvent) -> bool {
        self.events.as_slice() == std::slice::from_ref(event)
    }

    pub fn routed_agents(&self) -> BTreeMap<String, AgentCell> {
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

    pub fn install_routed(mut self, state: &mut WorldState) {
        self.agents = self.routed_agents();
        self.install_infallible(state);
    }

    pub fn install_infallible(self, state: &mut WorldState) {
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

    pub fn serialize_material_fields<S: serde::ser::SerializeStruct>(
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

    pub fn apply_event(
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
            DomainEvent::ModuleReleaseRequested{request_id,..}
            |DomainEvent::ModuleReleaseShadowed{request_id,..}
            |DomainEvent::ModuleReleaseAttested{request_id,..}
            |DomainEvent::ModuleReleaseRoleApproved{request_id,..}
            |DomainEvent::ModuleReleaseRejected{request_id,..}
            |DomainEvent::ModuleReleaseApplied{request_id,..}=>{
                use super::module_release_records::{prepare_module_release_records_v1,validate_module_release_request_id_v1,validate_module_release_attestation_signer_v1};
                // Preserve the original actor/trusted-binding reads at their
                // exact header position. The shared record kernel is DATA only.
                match event{
                    DomainEvent::ModuleReleaseRequested{requester_agent_id,..}=>{
                        validate_module_release_request_id_v1(*request_id)?;
                        if !state.agents.contains_key(requester_agent_id){return Err(WorldError::AgentNotFound{agent_id:requester_agent_id.clone()})}
                    },
                    DomainEvent::ModuleReleaseAttested{operator_agent_id,signer_node_id,..}=>{
                        if !state.agents.contains_key(operator_agent_id){return Err(WorldError::AgentNotFound{agent_id:operator_agent_id.clone()})}
                        validate_module_release_attestation_signer_v1(*request_id,signer_node_id)?;
                        if !state.node_identity_bindings.contains_key(signer_node_id.trim()){return Err(invalid(format!("module release attestation signer_node_id is untrusted: {signer_node_id}")))}
                    },_=>{},
                }
                // Requested/Attested deliberately use the base state, while
                // the other four prefer staged records, exactly as before.
                let base_only=matches!(event,DomainEvent::ModuleReleaseRequested{..}|DomainEvent::ModuleReleaseAttested{..});
                let before_request=if base_only{state.module_release_requests.get(request_id)}else{self.requests.get(request_id).or_else(||state.module_release_requests.get(request_id))};
                let before_mapping=if base_only{state.module_release_manifest_mappings.get(request_id)}else{self.mappings.get(request_id).or_else(||state.module_release_manifest_mappings.get(request_id))};
                let projection=prepare_module_release_records_v1(event,before_request,before_mapping,now,state.next_module_release_request_id)?;
                self.requests.insert(*request_id,projection.request);
                if let Some(mapping)=projection.mapping{self.mappings.insert(*request_id,mapping);}
                if let Some(next)=projection.next_request_id{self.next_request_id=Some(next);}
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
            DomainEvent::ProductProfileGoverned { profile, .. } => {
                validate_governed_product_profile_v1(profile)?;
                self.products.insert(profile.product_id.clone(), profile.clone());
            }
            DomainEvent::RecipeProfileGoverned { profile, .. } => {
                validate_governed_recipe_profile_v1(profile)?;
                self.recipes.insert(profile.recipe_id.clone(), profile.clone());
            }
            DomainEvent::FactoryProfileGoverned { profile, .. } => {
                validate_governed_factory_profile_v1(profile, self.factories.get(&profile.factory_id).or_else(|| state.factory_profiles.get(&profile.factory_id)))?;
                if !self.factories.contains_key(&profile.factory_id) && !state.factory_profiles.contains_key(&profile.factory_id) {
                    self.factories.insert(profile.factory_id.clone(), profile.clone());
                }
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
    pub fn prepare_module_release_event(
        &self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<PreparedModuleRelease, WorldError> {
        let mut prepared = PreparedModuleRelease::new(self, BTreeMap::new());
        prepared.apply_event(self, event, now)?;
        Ok(prepared)
    }
}

pub struct ReleaseMapProjection<'a, K, V> {
    pub base: &'a BTreeMap<K, V>,
    pub updates: &'a BTreeMap<K, V>,
}

pub struct ReleaseOptionalMapProjection<'a, K, V> {
    pub base: &'a BTreeMap<K, V>,
    pub updates: &'a BTreeMap<K, Option<V>>,
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

pub fn validate_governed_product_profile_v1(profile:&oasis7_wasm_abi::ProductProfileV1)->Result<(),WorldError>{
 if profile.product_id.trim().is_empty(){return Err(invalid("product profile product_id cannot be empty"))}
 if profile.role_tag.trim().is_empty(){return Err(invalid(format!("product profile role_tag cannot be empty: {}",profile.product_id)))}Ok(())
}
pub fn validate_governed_recipe_profile_v1(profile:&oasis7_wasm_abi::RecipeProfileV1)->Result<(),WorldError>{
 profile.validate_substitution_rules().map_err(|error|invalid(format!("invalid recipe substitution profile: {error}")))?;
 if profile.recipe_id.trim().is_empty(){return Err(invalid("recipe profile recipe_id cannot be empty"))}Ok(())
}
pub fn validate_governed_factory_profile_v1(profile:&oasis7_wasm_abi::FactoryProfileV1,existing:Option<&oasis7_wasm_abi::FactoryProfileV1>)->Result<(),WorldError>{
 if profile.factory_id.trim().is_empty(){return Err(invalid("factory profile factory_id cannot be empty"))}
 if profile.tier==0{return Err(invalid(format!("factory profile tier must be >= 1: {}",profile.factory_id)))}
 if profile.recipe_slots==0{return Err(invalid(format!("factory profile recipe_slots must be > 0: {}",profile.factory_id)))}
 if existing.is_some_and(|old|old!=profile){return Err(invalid(format!("factory profile id is immutable and conflicts with persisted profile: factory_id={}",profile.factory_id)))}Ok(())
}
