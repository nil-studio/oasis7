use super::super::{
    ActionId, BodyOverlay, CausedBy, DomainEvent, RejectReason, WorldError, WorldEventBody,
    WorldEventId, WorldTime,
};
use super::World;
use crate::agent_state::{BodyKernelView, BodySlotType, CargoEntityEntry};

/// Typed reducer delta for a validated body attribute update.
///
/// The delta is derived entirely from the current canonical view and the
/// candidate update.  Derivation does not touch canonical state; installation
/// is kept at the append seam so future transition-buffer work can stage the
/// same fields without changing the public operation contract.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PreparedBodyAttributesUpdate {
    agent_id: String,
    body_view: BodyKernelView,
    last_active: WorldTime,
}

impl PreparedBodyAttributesUpdate {
    fn new(agent_id: String, body_view: BodyKernelView, last_active: WorldTime) -> Self {
        Self {
            agent_id,
            body_view,
            last_active,
        }
    }

    pub(super) fn install(self, world: &mut World) -> Result<(), WorldError> {
        let cell = world.state.agents.get_mut(&self.agent_id).ok_or_else(|| {
            WorldError::AgentNotFound {
                agent_id: self.agent_id.clone(),
            }
        })?;
        cell.state.body_view = self.body_view;
        cell.last_active = self.last_active;
        Ok(())
    }

    pub(super) fn install_infallible(self, world: &mut World) {
        let cell = world
            .state
            .agents
            .get_mut(&self.agent_id)
            .expect("prepared body target was validated before publication");
        cell.state.body_view = self.body_view;
        cell.last_active = self.last_active;
    }

    pub(super) fn body_overlay(&self) -> BodyOverlay {
        BodyOverlay::new(
            self.agent_id.clone(),
            self.body_view.clone(),
            self.last_active,
        )
    }

    pub(super) fn matches_event(&self, event: &DomainEvent) -> bool {
        matches!(
            event,
            DomainEvent::BodyAttributesUpdated {
                agent_id,
                view,
                ..
            } if agent_id == &self.agent_id && view == &self.body_view
        )
    }
}

const BODY_MASS_KG_MIN: u64 = 1;
const BODY_MASS_KG_MAX: u64 = 1_000_000_000;
const BODY_RADIUS_CM_MIN: u64 = 1;
const BODY_RADIUS_CM_MAX: u64 = 1_000_000;
const BODY_THRUST_LIMIT_MIN: u64 = 0;
const BODY_THRUST_LIMIT_MAX: u64 = 10_000_000_000;
const BODY_CROSS_SECTION_CM2_MIN: u64 = 1;
const BODY_CHANGE_FACTOR_MAX: u64 = 10;
const BODY_SLOT_CAPACITY_MAX: u16 = 128;

impl World {
    // ---------------------------------------------------------------------
    // Body module helpers
    // ---------------------------------------------------------------------

    pub fn record_body_attributes_update(
        &mut self,
        agent_id: impl Into<String>,
        view: BodyKernelView,
        reason: impl Into<String>,
        caused_by: Option<CausedBy>,
    ) -> Result<WorldEventId, WorldError> {
        let agent_id = agent_id.into();
        let current_view = self
            .state
            .agents
            .get(&agent_id)
            .ok_or_else(|| WorldError::AgentNotFound {
                agent_id: agent_id.clone(),
            })?
            .state
            .body_view
            .clone();
        if let Err(reason) = validate_body_kernel_view(&current_view, &view) {
            return self.record_body_attributes_reject(agent_id, reason, caused_by);
        }
        let reason = reason.into();
        let body = WorldEventBody::Domain(DomainEvent::BodyAttributesUpdated {
            agent_id: agent_id.clone(),
            view: view.clone(),
            reason,
        });
        let prepared = PreparedBodyAttributesUpdate::new(agent_id, view, self.state.time);

        // This test seam is deliberately after typed delta preparation and
        // before the append flow installs any canonical reducer state.
        if self.take_fail_next_append_after_reducer_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after reducer delta preparation".to_string(),
            });
        }

        self.append_event_with_prepared_body(body, caused_by, prepared)
    }

    pub fn record_body_attributes_reject(
        &mut self,
        agent_id: impl Into<String>,
        reason: impl Into<String>,
        caused_by: Option<CausedBy>,
    ) -> Result<WorldEventId, WorldError> {
        let agent_id = agent_id.into();
        let reason = reason.into();
        self.append_body_attributes_rejected(
            WorldEventBody::Domain(DomainEvent::BodyAttributesRejected {
                agent_id: agent_id.clone(),
                reason,
            }),
            caused_by,
            agent_id,
        )
    }

    pub fn add_agent_cargo_entity(
        &mut self,
        agent_id: impl Into<String>,
        entry: CargoEntityEntry,
    ) -> Result<(), WorldError> {
        if entry.entity_id.trim().is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "cargo entry requires non-empty entity_id".to_string(),
            });
        }
        if entry.quantity <= 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!("cargo entry quantity must be > 0, got {}", entry.quantity),
            });
        }
        if entry.size_per_unit <= 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "cargo entry size_per_unit must be > 0, got {}",
                    entry.size_per_unit
                ),
            });
        }

        let agent_id = agent_id.into();
        let cell =
            self.state
                .agents
                .get_mut(&agent_id)
                .ok_or_else(|| WorldError::AgentNotFound {
                    agent_id: agent_id.clone(),
                })?;

        if let Some(existing) = cell
            .state
            .body_state
            .cargo_entries
            .iter_mut()
            .find(|existing| existing.entity_id == entry.entity_id)
        {
            if existing.entity_kind != entry.entity_kind
                || existing.size_per_unit != entry.size_per_unit
            {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: format!(
                        "cargo entry {} kind/size mismatch for merge",
                        entry.entity_id
                    ),
                });
            }
            existing.quantity = existing.quantity.saturating_add(entry.quantity);
        } else {
            cell.state.body_state.cargo_entries.push(entry);
        }
        Ok(())
    }
}

pub(super) fn evaluate_expand_body_interface(
    world: &World,
    action_id: ActionId,
    agent_id: &str,
    interface_module_item_id: &str,
) -> WorldEventBody {
    let Some(cell) = world.state.agents.get(agent_id) else {
        return WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::AgentNotFound {
                agent_id: agent_id.to_string(),
            },
        });
    };

    let item_id = interface_module_item_id.trim();
    if item_id.is_empty() {
        return WorldEventBody::Domain(DomainEvent::BodyInterfaceExpandRejected {
            agent_id: agent_id.to_string(),
            consumed_item_id: interface_module_item_id.to_string(),
            reason: "interface module item id is empty".to_string(),
        });
    }

    if cell.state.body_state.slot_capacity >= BODY_SLOT_CAPACITY_MAX {
        return WorldEventBody::Domain(DomainEvent::BodyInterfaceExpandRejected {
            agent_id: agent_id.to_string(),
            consumed_item_id: item_id.to_string(),
            reason: format!("body slot capacity reached max {}", BODY_SLOT_CAPACITY_MAX),
        });
    }

    if !cell.state.body_state.has_interface_module_item(item_id) {
        return WorldEventBody::Domain(DomainEvent::BodyInterfaceExpandRejected {
            agent_id: agent_id.to_string(),
            consumed_item_id: item_id.to_string(),
            reason: "interface module item unavailable or depleted".to_string(),
        });
    }

    WorldEventBody::Domain(DomainEvent::BodyInterfaceExpanded {
        agent_id: agent_id.to_string(),
        slot_capacity: cell.state.body_state.slot_capacity.saturating_add(1),
        expansion_level: cell.state.body_state.expansion_level.saturating_add(1),
        consumed_item_id: item_id.to_string(),
        new_slot_id: cell.state.body_state.next_slot_id(),
        slot_type: BodySlotType::Universal,
    })
}

pub(super) fn validate_body_kernel_view(
    current: &BodyKernelView,
    candidate: &BodyKernelView,
) -> Result<(), String> {
    ensure_range(
        "mass_kg",
        candidate.mass_kg,
        BODY_MASS_KG_MIN,
        BODY_MASS_KG_MAX,
    )?;
    ensure_range(
        "radius_cm",
        candidate.radius_cm,
        BODY_RADIUS_CM_MIN,
        BODY_RADIUS_CM_MAX,
    )?;
    ensure_range(
        "thrust_limit",
        candidate.thrust_limit,
        BODY_THRUST_LIMIT_MIN,
        BODY_THRUST_LIMIT_MAX,
    )?;
    let cross_section_max = cross_section_max_for_radius(candidate.radius_cm);
    ensure_range(
        "cross_section_cm2",
        candidate.cross_section_cm2,
        BODY_CROSS_SECTION_CM2_MIN,
        cross_section_max,
    )?;
    ensure_rate("mass_kg", current.mass_kg, candidate.mass_kg)?;
    ensure_rate("radius_cm", current.radius_cm, candidate.radius_cm)?;
    ensure_rate("thrust_limit", current.thrust_limit, candidate.thrust_limit)?;
    ensure_rate(
        "cross_section_cm2",
        current.cross_section_cm2,
        candidate.cross_section_cm2,
    )?;
    Ok(())
}

fn ensure_range(field: &str, value: u64, min: u64, max: u64) -> Result<(), String> {
    if value < min || value > max {
        return Err(format!(
            "body guard {field} out of range: {value} not in {min}..={max}"
        ));
    }
    Ok(())
}

fn ensure_rate(field: &str, previous: u64, next: u64) -> Result<(), String> {
    if previous == 0 || BODY_CHANGE_FACTOR_MAX <= 1 {
        return Ok(());
    }
    let max = previous.saturating_mul(BODY_CHANGE_FACTOR_MAX);
    let min = previous / BODY_CHANGE_FACTOR_MAX;
    if next > max || next < min {
        return Err(format!(
            "body guard {field} rate violation: prev={previous} next={next} factor={BODY_CHANGE_FACTOR_MAX}"
        ));
    }
    Ok(())
}

fn cross_section_max_for_radius(radius_cm: u64) -> u64 {
    radius_cm
        .saturating_mul(radius_cm)
        .saturating_mul(4)
        .max(BODY_CROSS_SECTION_CM2_MIN)
}
