use oasis7_agent_api::world_action::GeoPos;
use oasis7_agent_api::world_resource::ResourceStock;
use serde::{Deserialize, Serialize};

pub const DEFAULT_BODY_FRAME_KIND: &str = "standard_frame";
pub const DEFAULT_BODY_SLOT_CAPACITY: u16 = 7;

pub use oasis7_agent_api::world_action::BodyKernelView;

pub use oasis7_agent_api::world_entity::{DEFAULT_AGENT_HEIGHT_CM, RobotBodySpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodySlotType {
    Power,
    Sensor,
    Mobility,
    Cognitive,
    Cargo,
    Universal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyModuleSlot {
    pub slot_id: String,
    pub slot_type: BodySlotType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_module: Option<String>,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CargoEntityKind {
    Mineral,
    ModuleItem,
    InterfaceModuleItem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CargoEntityEntry {
    pub entity_id: String,
    pub entity_kind: CargoEntityKind,
    pub quantity: i64,
    pub size_per_unit: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentBodyState {
    pub frame_kind: String,
    pub slot_capacity: u16,
    pub slots: Vec<BodyModuleSlot>,
    #[serde(default)]
    pub expansion_level: u16,
    #[serde(default)]
    pub cargo_entries: Vec<CargoEntityEntry>,
}

impl Default for AgentBodyState {
    fn default() -> Self {
        Self {
            frame_kind: DEFAULT_BODY_FRAME_KIND.to_string(),
            slot_capacity: DEFAULT_BODY_SLOT_CAPACITY,
            slots: vec![
                BodyModuleSlot {
                    slot_id: "slot-1".to_string(),
                    slot_type: BodySlotType::Power,
                    installed_module: Some("m1.power.radiation_harvest".to_string()),
                    locked: false,
                },
                BodyModuleSlot {
                    slot_id: "slot-2".to_string(),
                    slot_type: BodySlotType::Power,
                    installed_module: Some("m1.power.storage".to_string()),
                    locked: false,
                },
                BodyModuleSlot {
                    slot_id: "slot-3".to_string(),
                    slot_type: BodySlotType::Sensor,
                    installed_module: Some("m1.sensor.basic".to_string()),
                    locked: false,
                },
                BodyModuleSlot {
                    slot_id: "slot-4".to_string(),
                    slot_type: BodySlotType::Mobility,
                    installed_module: Some("m1.mobility.basic".to_string()),
                    locked: false,
                },
                BodyModuleSlot {
                    slot_id: "slot-5".to_string(),
                    slot_type: BodySlotType::Cognitive,
                    installed_module: Some("m1.memory.core".to_string()),
                    locked: false,
                },
                BodyModuleSlot {
                    slot_id: "slot-6".to_string(),
                    slot_type: BodySlotType::Cargo,
                    installed_module: Some("m1.storage.cargo".to_string()),
                    locked: false,
                },
                BodyModuleSlot {
                    slot_id: "slot-7".to_string(),
                    slot_type: BodySlotType::Universal,
                    installed_module: None,
                    locked: false,
                },
            ],
            expansion_level: 0,
            cargo_entries: Vec::new(),
        }
    }
}

impl AgentBodyState {
    pub fn has_interface_module_item(&self, item_id: &str) -> bool {
        self.cargo_entries.iter().any(|entry| {
            entry.entity_id == item_id
                && entry.entity_kind == CargoEntityKind::InterfaceModuleItem
                && entry.quantity > 0
        })
    }

    pub fn consume_interface_module_item(&mut self, item_id: &str) -> Result<(), String> {
        let Some(entry) = self.cargo_entries.iter_mut().find(|entry| {
            entry.entity_id == item_id && entry.entity_kind == CargoEntityKind::InterfaceModuleItem
        }) else {
            return Err(format!("interface module item not found: {item_id}"));
        };

        if entry.quantity <= 0 {
            return Err(format!(
                "interface module item depleted: {item_id} quantity={}",
                entry.quantity
            ));
        }

        entry.quantity -= 1;
        self.cargo_entries
            .retain(|entry| !(entry.entity_id == item_id && entry.quantity <= 0));
        Ok(())
    }

    pub fn next_slot_id(&self) -> String {
        let mut next = self.slots.len() + 1;
        loop {
            let candidate = format!("slot-{next}");
            if !self.slots.iter().any(|slot| slot.slot_id == candidate) {
                return candidate;
            }
            next += 1;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentState {
    pub agent_id: String,
    pub pos: GeoPos,
    pub body: RobotBodySpec,
    #[serde(default)]
    pub resources: ResourceStock,
    #[serde(default)]
    pub body_view: BodyKernelView,
    #[serde(default)]
    pub body_state: AgentBodyState,
}

impl AgentState {
    pub fn new(agent_id: impl Into<String>, pos: GeoPos) -> Self {
        Self {
            agent_id: agent_id.into(),
            pos,
            body: RobotBodySpec::default(),
            resources: ResourceStock::default(),
            body_view: BodyKernelView::default(),
            body_state: AgentBodyState::default(),
        }
    }
}
