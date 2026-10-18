//! Event-bound sparse projection for raw resource and data events.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde::ser::{SerializeMap, SerializeStruct};

use oasis7_agent_api::world_action::ResourceKind;

use super::super::agent_cell::AgentCell;
use super::super::{DomainEvent, MaterialLedgerId, WorldError, WorldState, WorldTime};

#[derive(Debug)]
pub(crate) struct PreparedEconomyDataEvent {
    event: DomainEvent,
    agents: BTreeMap<String, AgentCell>,
    permission_update: Option<(String, Option<BTreeSet<String>>)>,
    nonce_update: Option<(String, BTreeMap<String, u64>)>,
    world_materials: BTreeMap<String, i64>,
}

impl PreparedEconomyDataEvent {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let mut prepared = Self {
            event: event.clone(),
            agents: BTreeMap::new(),
            permission_update: None,
            nonce_update: None,
            world_materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|ledger| !ledger.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        };
        match event {
            DomainEvent::ResourceTransferred {
                from_agent_id,
                to_agent_id,
                kind,
                amount,
            } => {
                if from_agent_id == to_agent_id {
                    let mut cell = required_agent(state, from_agent_id)?;
                    cell.last_active = now;
                    prepared.agents.insert(from_agent_id.clone(), cell);
                } else {
                    let mut from = required_agent(state, from_agent_id)?;
                    let mut to = required_agent(state, to_agent_id)?;
                    from.state.resources.remove(*kind, *amount).map_err(|err| {
                        WorldError::ResourceBalanceInvalid {
                            reason: format!("transfer remove failed: {err:?}"),
                        }
                    })?;
                    to.state.resources.add(*kind, *amount).map_err(|err| {
                        WorldError::ResourceBalanceInvalid {
                            reason: format!("transfer add failed: {err:?}"),
                        }
                    })?;
                    from.last_active = now;
                    to.last_active = now;
                    prepared.agents.insert(from_agent_id.clone(), from);
                    prepared.agents.insert(to_agent_id.clone(), to);
                }
            }
            DomainEvent::DataCollected {
                collector_agent_id,
                electricity_cost,
                data_amount,
            } => {
                validate_collection_amounts(*electricity_cost, *data_amount)?;
                let mut collector = required_agent(state, collector_agent_id)?;
                apply_collection_resources(&mut collector, *electricity_cost, *data_amount)?;
                collector.last_active = now;
                prepared
                    .agents
                    .insert(collector_agent_id.clone(), collector);
            }
            DomainEvent::DataCollectedAuthenticated {
                collector_agent_id,
                electricity_cost,
                data_amount,
                player_id,
                public_key,
                nonce,
                signature,
            } => {
                if *electricity_cost <= 0 || *data_amount <= 0 || *nonce == 0 {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason:
                            "authenticated data collection event contains invalid amount or nonce"
                                .to_string(),
                    });
                }
                let mut player_nonces = state
                    .authenticated_collect_data_last_nonces
                    .get(player_id)
                    .cloned()
                    .unwrap_or_default();
                let last_nonce = player_nonces.get(public_key).copied().unwrap_or(0);
                if *nonce <= last_nonce {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "authenticated data collection nonce must advance: last={last_nonce} next={nonce}"
                        ),
                    });
                }
                oasis7_proto::collect_data_auth::verify_authorization(
                    oasis7_proto::collect_data_auth::COLLECT_DATA_SUBMIT_OPERATION,
                    *electricity_cost,
                    *data_amount,
                    player_id,
                    public_key,
                    *nonce,
                    signature,
                )
                .map_err(|error| WorldError::ResourceBalanceInvalid {
                    reason: format!("authenticated data collection signature invalid: {error}"),
                })?;
                let matching_claims = state
                    .starter_oc_claims
                    .values()
                    .filter(|claim| {
                        claim.player_id == *player_id
                            && claim.public_key.as_deref() == Some(public_key.as_str())
                    })
                    .collect::<Vec<_>>();
                if matching_claims.len() != 1 || matching_claims[0].agent_id != *collector_agent_id
                {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "authenticated data collection requires exactly one starter OC player/key binding for collector {collector_agent_id}; found {}",
                            matching_claims.len()
                        ),
                    });
                }
                let mut collector = required_agent(state, collector_agent_id)?;
                apply_collection_resources(&mut collector, *electricity_cost, *data_amount)?;
                collector.last_active = now;
                player_nonces.insert(public_key.clone(), *nonce);
                prepared
                    .agents
                    .insert(collector_agent_id.clone(), collector);
                prepared.nonce_update = Some((player_id.clone(), player_nonces));
            }
            DomainEvent::DataAccessGranted {
                owner_agent_id,
                grantee_agent_id,
            } => prepared.prepare_access(state, owner_agent_id, grantee_agent_id, now, true)?,
            DomainEvent::DataAccessRevoked {
                owner_agent_id,
                grantee_agent_id,
            } => prepared.prepare_access(state, owner_agent_id, grantee_agent_id, now, false)?,
            _ => {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: "economy/data preparation requires a supported event".to_string(),
                });
            }
        }
        Ok(prepared)
    }

    fn prepare_access(
        &mut self,
        state: &WorldState,
        owner_agent_id: &str,
        grantee_agent_id: &str,
        now: WorldTime,
        grant: bool,
    ) -> Result<(), WorldError> {
        let mut owner = required_agent(state, owner_agent_id)?;
        let mut grantee = if owner_agent_id == grantee_agent_id {
            None
        } else {
            Some(required_agent(state, grantee_agent_id)?)
        };
        if owner_agent_id != grantee_agent_id {
            let mut permissions = state
                .data_access_permissions
                .get(owner_agent_id)
                .cloned()
                .unwrap_or_default();
            if grant {
                permissions.insert(grantee_agent_id.to_string());
            } else {
                permissions.remove(grantee_agent_id);
            }
            self.permission_update = Some((
                owner_agent_id.to_string(),
                (!permissions.is_empty()).then_some(permissions),
            ));
        }
        owner.last_active = now;
        self.agents.insert(owner_agent_id.to_string(), owner);
        if let Some(grantee) = grantee.as_mut() {
            grantee.last_active = now;
            self.agents
                .insert(grantee_agent_id.to_string(), grantee.clone());
        }
        Ok(())
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub(crate) fn routed_agents(&self) -> BTreeMap<String, AgentCell> {
        let mut agents = self.agents.clone();
        match &self.event {
            DomainEvent::ResourceTransferred {
                from_agent_id,
                to_agent_id,
                ..
            } => route_two(&mut agents, from_agent_id, to_agent_id, &self.event),
            DomainEvent::DataAccessGranted {
                owner_agent_id,
                grantee_agent_id,
            }
            | DomainEvent::DataAccessRevoked {
                owner_agent_id,
                grantee_agent_id,
            } => route_two(&mut agents, owner_agent_id, grantee_agent_id, &self.event),
            DomainEvent::DataCollected {
                collector_agent_id, ..
            }
            | DomainEvent::DataCollectedAuthenticated {
                collector_agent_id, ..
            } => {
                if let Some(cell) = agents.get_mut(collector_agent_id) {
                    cell.mailbox.push_back(self.event.clone());
                }
            }
            _ => unreachable!("prepared event was validated at construction"),
        }
        agents
    }

    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state.agents.extend(self.agents);
        if let Some((owner, permissions)) = self.permission_update {
            if let Some(permissions) = permissions {
                state.data_access_permissions.insert(owner, permissions);
            } else {
                state.data_access_permissions.remove(&owner);
            }
        }
        if let Some((player, nonces)) = self.nonce_update {
            state
                .authenticated_collect_data_last_nonces
                .insert(player, nonces);
        }
        state.materials = self.world_materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.world_materials);
    }

    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        output.serialize_field(
            "agents",
            &SparseMapProjection {
                base: &state.agents,
                updates: &self.routed_agents(),
                deletion: None,
            },
        )
    }

    pub(crate) fn serialize_material_fields<S: SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        output.serialize_field("materials", &self.world_materials)?;
        let updates = BTreeMap::from([(MaterialLedgerId::world(), self.world_materials.clone())]);
        output.serialize_field(
            "material_ledgers",
            &SparseMapProjection {
                base: &state.material_ledgers,
                updates: &updates,
                deletion: None,
            },
        )
    }

    pub(crate) fn serialize_permissions<S: SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        let mut updates = BTreeMap::new();
        let mut deletion = None;
        if let Some((owner, permissions)) = &self.permission_update {
            if let Some(permissions) = permissions {
                updates.insert(owner.clone(), permissions.clone());
            } else {
                deletion = Some(owner);
            }
        }
        output.serialize_field(
            "data_access_permissions",
            &SparseMapProjection {
                base: &state.data_access_permissions,
                updates: &updates,
                deletion,
            },
        )
    }

    pub(crate) fn serialize_nonces<S: SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        let updates = self
            .nonce_update
            .as_ref()
            .map(|(player, nonces)| BTreeMap::from([(player.clone(), nonces.clone())]))
            .unwrap_or_default();
        output.serialize_field(
            "authenticated_collect_data_last_nonces",
            &SparseMapProjection {
                base: &state.authenticated_collect_data_last_nonces,
                updates: &updates,
                deletion: None,
            },
        )
    }

    pub(crate) fn has_projected_nonces(&self, state: &WorldState) -> bool {
        !state.authenticated_collect_data_last_nonces.is_empty() || self.nonce_update.is_some()
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

fn validate_collection_amounts(electricity_cost: i64, data_amount: i64) -> Result<(), WorldError> {
    if electricity_cost <= 0 {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!("data collection electricity_cost must be > 0, got {electricity_cost}"),
        });
    }
    if data_amount <= 0 {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!("data collection data_amount must be > 0, got {data_amount}"),
        });
    }
    Ok(())
}

fn apply_collection_resources(
    collector: &mut AgentCell,
    electricity_cost: i64,
    data_amount: i64,
) -> Result<(), WorldError> {
    collector
        .state
        .resources
        .remove(ResourceKind::Electricity, electricity_cost)
        .map_err(|err| WorldError::ResourceBalanceInvalid {
            reason: format!("data collection electricity debit failed: {err:?}"),
        })?;
    collector
        .state
        .resources
        .add(ResourceKind::Data, data_amount)
        .map_err(|err| WorldError::ResourceBalanceInvalid {
            reason: format!("data collection data credit failed: {err:?}"),
        })
}

fn route_two(
    agents: &mut BTreeMap<String, AgentCell>,
    first: &str,
    second: &str,
    event: &DomainEvent,
) {
    if let Some(cell) = agents.get_mut(first) {
        cell.mailbox.push_back(event.clone());
    }
    if first != second
        && let Some(cell) = agents.get_mut(second)
    {
        cell.mailbox.push_back(event.clone());
    }
}

pub(crate) struct SparseMapProjection<'a, K, V> {
    pub(crate) base: &'a BTreeMap<K, V>,
    pub(crate) updates: &'a BTreeMap<K, V>,
    pub(crate) deletion: Option<&'a K>,
}

impl<K, V> Serialize for SparseMapProjection<'_, K, V>
where
    K: Serialize + Ord,
    V: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let replaced = self
            .base
            .keys()
            .filter(|key| self.updates.contains_key(*key))
            .count();
        let deleted = usize::from(self.deletion.is_some_and(|key| self.base.contains_key(key)));
        let mut map = serializer.serialize_map(Some(
            self.base.len() + self.updates.len() - replaced - deleted,
        ))?;
        let keys = self
            .base
            .keys()
            .chain(self.updates.keys())
            .collect::<BTreeSet<_>>();
        for key in keys {
            if self.deletion == Some(key) {
                continue;
            }
            if let Some(value) = self.updates.get(key) {
                map.serialize_entry(key, value)?;
            } else if let Some(value) = self.base.get(key) {
                map.serialize_entry(key, value)?;
            }
        }
        map.end()
    }
}
