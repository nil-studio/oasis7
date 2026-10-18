use super::*;
use crate::AgentActivityV1;
use crate::agent_state::BodyModuleSlot;
use serde::Serialize;
use serde::ser::{SerializeMap, SerializeStruct};

#[derive(Debug)]
pub(crate) struct PreparedCorePolicyEvent {
    event: DomainEvent,
    agent: Option<(String, AgentCell)>,
    materials: BTreeMap<String, i64>,
    world_materials: BTreeMap<String, i64>,
    gameplay_policy: Option<GameplayPolicyState>,
    industry_progress: Option<IndustryProgressState>,
    material_profile: Option<(String, MaterialProfileV1)>,
}

impl PreparedCorePolicyEvent {
    pub(crate) fn supports(event: &DomainEvent) -> bool {
        matches!(
            event,
            DomainEvent::AgentRegistered { .. }
                | DomainEvent::AgentMoved { .. }
                | DomainEvent::ActionAccepted { .. }
                | DomainEvent::ActionRejected { .. }
                | DomainEvent::Observation { .. }
                | DomainEvent::BodyAttributesUpdated { .. }
                | DomainEvent::BodyAttributesRejected { .. }
                | DomainEvent::BodyInterfaceExpanded { .. }
                | DomainEvent::BodyInterfaceExpandRejected { .. }
                | DomainEvent::GameplayPolicyUpdated { .. }
                | DomainEvent::MaterialProfileGoverned { .. }
        )
    }

    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let (materials, world_materials) = normalized_materials(state);
        let mut prepared = Self {
            event: event.clone(),
            agent: None,
            materials,
            world_materials,
            gameplay_policy: None,
            industry_progress: None,
            material_profile: None,
        };
        match event {
            DomainEvent::AgentRegistered { agent_id, pos } => {
                let mut cell = AgentCell::new(AgentState::new(agent_id, *pos), now);
                cell.activity = Some(AgentActivityV1::idle(now));
                prepared.agent = Some((agent_id.clone(), cell));
            }
            DomainEvent::AgentMoved { agent_id, to, .. } => {
                prepared.agent = state.agents.get(agent_id).cloned().map(|mut cell| {
                    cell.state.pos = *to;
                    cell.last_active = now;
                    (agent_id.clone(), cell)
                });
            }
            DomainEvent::ActionAccepted { actor_id, .. } => {
                prepared.agent = unchanged_agent(state, actor_id);
            }
            DomainEvent::ActionRejected { .. } => {}
            DomainEvent::Observation { observation } => {
                prepared.agent = unchanged_agent(state, &observation.agent_id);
            }
            DomainEvent::BodyAttributesUpdated { agent_id, view, .. } => {
                let mut cell = required_agent(state, agent_id)?;
                cell.state.body_view = view.clone();
                cell.last_active = now;
                prepared.agent = Some((agent_id.clone(), cell));
            }
            DomainEvent::BodyAttributesRejected { agent_id, .. }
            | DomainEvent::BodyInterfaceExpandRejected { agent_id, .. } => {
                let mut cell = required_agent(state, agent_id)?;
                cell.last_active = now;
                prepared.agent = Some((agent_id.clone(), cell));
            }
            DomainEvent::BodyInterfaceExpanded {
                agent_id,
                slot_capacity,
                expansion_level,
                consumed_item_id,
                new_slot_id,
                slot_type,
            } => {
                let mut cell = required_agent(state, agent_id)?;
                cell.state
                    .body_state
                    .consume_interface_module_item(consumed_item_id)
                    .map_err(|reason| WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "consume interface module item failed for {agent_id}: {reason}"
                        ),
                    })?;
                cell.state.body_state.slot_capacity = *slot_capacity;
                cell.state.body_state.expansion_level = *expansion_level;
                if !cell
                    .state
                    .body_state
                    .slots
                    .iter()
                    .any(|slot| slot.slot_id == *new_slot_id)
                {
                    cell.state.body_state.slots.push(BodyModuleSlot {
                        slot_id: new_slot_id.clone(),
                        slot_type: *slot_type,
                        installed_module: None,
                        locked: false,
                    });
                }
                cell.last_active = now;
                prepared.agent = Some((agent_id.clone(), cell));
            }
            DomainEvent::GameplayPolicyUpdated {
                operator_agent_id,
                electricity_tax_bps,
                data_tax_bps,
                power_trade_fee_bps,
                max_open_contracts_per_agent,
                blocked_agents,
                forbidden_location_ids,
            } => {
                let mut cell = required_agent(state, operator_agent_id)?;
                let policy = GameplayPolicyState {
                    electricity_tax_bps: *electricity_tax_bps,
                    data_tax_bps: *data_tax_bps,
                    power_trade_fee_bps: *power_trade_fee_bps,
                    max_open_contracts_per_agent: *max_open_contracts_per_agent,
                    blocked_agents: normalized_values(blocked_agents),
                    forbidden_location_ids: normalized_values(forbidden_location_ids),
                    updated_at: now,
                };
                cell.last_active = now;
                prepared.industry_progress = Some(refreshed_industry_progress(state, &policy, now));
                prepared.gameplay_policy = Some(policy);
                prepared.agent = Some((operator_agent_id.clone(), cell));
            }
            DomainEvent::MaterialProfileGoverned {
                operator_agent_id,
                proposal_id,
                profile,
            } => {
                let mut cell = required_agent(state, operator_agent_id)?;
                validate_material_profile(*proposal_id, profile)?;
                cell.last_active = now;
                prepared.material_profile = Some((profile.kind.clone(), profile.clone()));
                prepared.agent = Some((operator_agent_id.clone(), cell));
            }
            _ => unreachable!("core/policy preparation called with unsupported event"),
        }
        Ok(prepared)
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state.materials = self.materials;
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.world_materials);
        if let Some((id, cell)) = self.agent {
            state.agents.insert(id, cell);
        }
        if let Some(policy) = self.gameplay_policy {
            state.gameplay_policy = policy;
        }
        if let Some(progress) = self.industry_progress {
            state.industry_progress = progress;
        }
        if let Some((kind, profile)) = self.material_profile {
            state.material_profiles.insert(kind, profile);
        }
    }

    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let mut updates = BTreeMap::new();
        if let Some((id, cell)) = &self.agent {
            let mut cell = cell.clone();
            if self.event.agent_id() == Some(id.as_str()) {
                cell.mailbox.push_back(self.event.clone());
            }
            updates.insert(id.clone(), cell);
        }
        out.serialize_field(
            "agents",
            &SparseMap {
                base: &state.agents,
                updates: &updates,
            },
        )
    }

    pub(crate) fn serialize_materials<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("materials", &self.materials)?;
        let updates = BTreeMap::from([(MaterialLedgerId::world(), self.world_materials.clone())]);
        out.serialize_field(
            "material_ledgers",
            &SparseMap {
                base: &state.material_ledgers,
                updates: &updates,
            },
        )
    }

    pub(crate) fn serialize_profile<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updates = self.material_profile.clone().into_iter().collect();
        out.serialize_field(
            "material_profiles",
            &SparseMap {
                base: &state.material_profiles,
                updates: &updates,
            },
        )
    }

    pub(crate) fn projected_progress<'a>(
        &'a self,
        state: &'a WorldState,
    ) -> &'a IndustryProgressState {
        self.industry_progress
            .as_ref()
            .unwrap_or(&state.industry_progress)
    }

    pub(crate) fn projected_policy<'a>(&'a self, state: &'a WorldState) -> &'a GameplayPolicyState {
        self.gameplay_policy
            .as_ref()
            .unwrap_or(&state.gameplay_policy)
    }
}

fn required_agent(state: &WorldState, agent_id: &str) -> Result<AgentCell, WorldError> {
    state
        .agents
        .get(agent_id)
        .cloned()
        .ok_or_else(|| WorldError::AgentNotFound {
            agent_id: agent_id.to_string(),
        })
}

fn unchanged_agent(state: &WorldState, agent_id: &str) -> Option<(String, AgentCell)> {
    state
        .agents
        .get(agent_id)
        .cloned()
        .map(|cell| (agent_id.to_string(), cell))
}

fn normalized_values(values: &[String]) -> Vec<String> {
    let mut normalized = values
        .iter()
        .filter_map(|value| match value.trim() {
            "" => None,
            value => Some(value.to_string()),
        })
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn validate_material_profile(
    proposal_id: ProposalId,
    profile: &MaterialProfileV1,
) -> Result<(), WorldError> {
    if proposal_id == 0 {
        return Err(invalid("material profile governed proposal_id must be > 0"));
    }
    if profile.kind.trim().is_empty() {
        return Err(invalid("material profile kind cannot be empty"));
    }
    if profile.tier == 0 {
        return Err(invalid(format!(
            "material profile tier must be >= 1: {}",
            profile.kind
        )));
    }
    if profile.category.trim().is_empty() {
        return Err(invalid(format!(
            "material profile category cannot be empty: {}",
            profile.kind
        )));
    }
    if profile.stack_limit <= 0 {
        return Err(invalid(format!(
            "material profile stack_limit must be > 0: {}",
            profile.kind
        )));
    }
    Ok(())
}

fn refreshed_industry_progress(
    state: &WorldState,
    policy: &GameplayPolicyState,
    now: WorldTime,
) -> IndustryProgressState {
    let mut progress = state.industry_progress.clone();
    let jobs = state
        .factories
        .values()
        .map(|factory| factory.production.completed_jobs)
        .sum::<u64>();
    let mut next = if state
        .factories
        .values()
        .any(super::industry_transition::factory_has_canonical_stable_line)
    {
        IndustryStage::ScaleOut
    } else {
        IndustryStage::Bootstrap
    };
    let governed = policy.electricity_tax_bps > 0 || policy.data_tax_bps > 0;
    if next == IndustryStage::ScaleOut
        && governed
        && (jobs >= 6 || progress.completed_material_transits >= 3)
    {
        next = IndustryStage::Governance;
    }
    if next != progress.stage {
        progress.stage = next;
        progress.stage_updated_at = now;
    }
    progress
}

fn normalized_materials(state: &WorldState) -> (BTreeMap<String, i64>, BTreeMap<String, i64>) {
    let world = state
        .material_ledgers
        .get(&MaterialLedgerId::world())
        .filter(|ledger| !ledger.is_empty())
        .cloned()
        .unwrap_or_else(|| state.materials.clone());
    (world.clone(), world)
}

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}

struct SparseMap<'a, K, V> {
    base: &'a BTreeMap<K, V>,
    updates: &'a BTreeMap<K, V>,
}

impl<K: Ord + Serialize, V: Serialize> Serialize for SparseMap<'_, K, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.base.len() + self.updates.len()))?;
        let mut base = self.base.iter().peekable();
        let mut updates = self.updates.iter().peekable();
        while base.peek().is_some() || updates.peek().is_some() {
            let ordering = match (base.peek(), updates.peek()) {
                (Some((base_key, _)), Some((update_key, _))) => Some(update_key.cmp(base_key)),
                (None, Some(_)) => Some(std::cmp::Ordering::Less),
                _ => None,
            };
            match ordering {
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal) => {
                    let (update_key, update_value) = updates.next().expect("update is present");
                    map.serialize_entry(update_key, update_value)?;
                    if ordering == Some(std::cmp::Ordering::Equal) {
                        base.next();
                    }
                }
                None | Some(std::cmp::Ordering::Greater) => {
                    let (key, value) = base.next().expect("base is present");
                    map.serialize_entry(key, value)?;
                }
            }
        }
        map.end()
    }
}
