//! Borrowed state serialization for trusted module-command preparation.
//!
//! A command stage owns only changed map entries and agent cells.  These
//! projections overlay those values while hashing without constructing a
//! second `WorldState`.

use super::super::agent_cell::AgentCell;
use oasis7_agent_api::world_action::ResourceKind;
use serde::Serialize;
use serde::ser::SerializeMap;
use std::collections::BTreeMap;

/// Changed state projections owned by a trusted command stage.
#[derive(Debug, Clone, Copy)]
pub struct CommandStateOverlay<'a> {
    pub module_states: &'a BTreeMap<String, Vec<u8>>,
    pub resources: &'a BTreeMap<ResourceKind, i64>,
    pub agents: &'a BTreeMap<String, AgentCell>,
}

pub struct CommandAgentMapProjection<'a> {
    pub agents: &'a BTreeMap<String, AgentCell>,
    pub updates: &'a BTreeMap<String, AgentCell>,
}

impl Serialize for CommandAgentMapProjection<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.agents.len()))?;
        for (agent_id, cell) in self.agents {
            map.serialize_entry(agent_id, self.updates.get(agent_id).unwrap_or(cell))?;
        }
        map.end()
    }
}

pub struct CommandResourceMapProjection<'a> {
    pub resources: &'a BTreeMap<ResourceKind, i64>,
    pub updates: &'a BTreeMap<ResourceKind, i64>,
}

impl Serialize for CommandResourceMapProjection<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let added = self
            .updates
            .keys()
            .filter(|key| !self.resources.contains_key(*key))
            .count();
        let mut map = serializer.serialize_map(Some(self.resources.len() + added))?;
        for (key, value) in self.resources {
            map.serialize_entry(key, self.updates.get(key).unwrap_or(value))?;
        }
        for (key, value) in self.updates {
            if !self.resources.contains_key(key) {
                map.serialize_entry(key, value)?;
            }
        }
        map.end()
    }
}

pub struct CommandModuleStateMapProjection<'a> {
    pub module_states: &'a BTreeMap<String, Vec<u8>>,
    pub updates: &'a BTreeMap<String, Vec<u8>>,
}

impl Serialize for CommandModuleStateMapProjection<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let added = self
            .updates
            .keys()
            .filter(|key| !self.module_states.contains_key(*key))
            .count();
        let mut map = serializer.serialize_map(Some(self.module_states.len() + added))?;
        for (key, value) in self.module_states {
            map.serialize_entry(key, self.updates.get(key).unwrap_or(value))?;
        }
        for (key, value) in self.updates {
            if !self.module_states.contains_key(key) {
                map.serialize_entry(key, value)?;
            }
        }
        map.end()
    }
}
