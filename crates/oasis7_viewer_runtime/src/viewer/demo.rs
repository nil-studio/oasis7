use std::path::Path;

use crate::geometry::space_distance_cm;
use crate::simulator::{WorldKernel, initialize_kernel};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_simulator::{
    Action, PersistError, ResourceOwner, WorldConfig, WorldInitConfig, WorldInitError,
    WorldInitReport, WorldScenario,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ViewerDemoSummary {
    pub init: WorldInitReport,
    pub actions: usize,
    pub events: usize,
}

#[derive(Debug)]
pub enum ViewerDemoError {
    Init(WorldInitError),
    Persist(PersistError),
}

impl From<WorldInitError> for ViewerDemoError {
    fn from(err: WorldInitError) -> Self {
        ViewerDemoError::Init(err)
    }
}

impl From<PersistError> for ViewerDemoError {
    fn from(err: PersistError) -> Self {
        ViewerDemoError::Persist(err)
    }
}

pub fn generate_viewer_demo(
    dir: impl AsRef<Path>,
    scenario: WorldScenario,
) -> Result<ViewerDemoSummary, ViewerDemoError> {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(scenario, &config);
    let (mut kernel, report) = initialize_kernel(config, init)?;

    let actions = plan_demo_actions(&kernel);
    for action in &actions {
        kernel.submit_action(action.clone());
    }
    kernel.step_until_empty();

    kernel.save_to_dir(dir)?;

    Ok(ViewerDemoSummary {
        init: report,
        actions: actions.len(),
        events: kernel.journal().len(),
    })
}

fn plan_demo_actions(kernel: &WorldKernel) -> Vec<Action> {
    let model = kernel.model();
    let mut agent_ids: Vec<_> = model.agents.keys().cloned().collect();
    agent_ids.sort();
    let Some(agent_id) = agent_ids.first().cloned() else {
        return Vec::new();
    };
    let Some(agent) = model.agents.get(&agent_id) else {
        return Vec::new();
    };
    let current_location_id = agent.location_id.clone();
    let Some(current_location) = model.locations.get(&current_location_id) else {
        return Vec::new();
    };

    let mut location_ids: Vec<_> = model.locations.keys().cloned().collect();
    location_ids.sort();
    let other_location_id = location_ids
        .into_iter()
        .find(|location_id| location_id != &current_location_id);

    let mut actions = Vec::new();

    if let Some(other_location_id) = other_location_id {
        let other_location = model
            .locations
            .get(&other_location_id)
            .expect("location exists");
        let distance_cm = space_distance_cm(agent.pos, other_location.pos);
        let move_cost = kernel.config().movement_cost(distance_cm);
        let agent_power = agent.resources.get(ResourceKind::Electricity);

        if move_cost > 0 && agent_power < move_cost {
            let needed = move_cost - agent_power;
            let available = current_location.resources.get(ResourceKind::Electricity);
            let transfer_amount = if available > 0 {
                needed.min(available)
            } else {
                1
            };
            actions.push(Action::TransferResource {
                from: ResourceOwner::Location {
                    location_id: current_location_id.clone(),
                },
                to: ResourceOwner::Agent {
                    agent_id: agent_id.clone(),
                },
                kind: ResourceKind::Electricity,
                amount: transfer_amount,
            });
        }

        actions.push(Action::MoveAgent {
            agent_id: agent_id.clone(),
            to: other_location_id,
        });
        return actions;
    }

    let location_power = current_location.resources.get(ResourceKind::Electricity);
    let agent_power = agent.resources.get(ResourceKind::Electricity);
    let (from, to, amount) = if location_power > 0 {
        (
            ResourceOwner::Location {
                location_id: current_location_id.clone(),
            },
            ResourceOwner::Agent {
                agent_id: agent_id.clone(),
            },
            location_power.min(5),
        )
    } else if agent_power > 0 {
        (
            ResourceOwner::Agent {
                agent_id: agent_id.clone(),
            },
            ResourceOwner::Location {
                location_id: current_location_id.clone(),
            },
            agent_power.min(5),
        )
    } else {
        (
            ResourceOwner::Location {
                location_id: current_location_id.clone(),
            },
            ResourceOwner::Agent {
                agent_id: agent_id.clone(),
            },
            1,
        )
    };

    actions.push(Action::TransferResource {
        from,
        to,
        kind: ResourceKind::Electricity,
        amount,
    });

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_demo_actions_includes_move_for_multi_location_scenario() {
        let mut config = WorldConfig::default();
        config.physics.max_move_distance_cm_per_tick = i64::MAX;
        config.physics.max_move_speed_cm_per_s = i64::MAX;
        config.move_cost_per_km_electricity = 0;
        let init = WorldInitConfig::from_scenario(WorldScenario::TwinRegionBootstrap, &config);
        let (mut kernel, _) = initialize_kernel(config, init).expect("init ok");

        let initial_location = kernel
            .model()
            .agents
            .get("agent-0")
            .expect("agent exists")
            .location_id
            .clone();

        let actions = plan_demo_actions(&kernel);
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, Action::MoveAgent { .. }))
        );

        for action in actions {
            kernel.submit_action(action);
        }
        let events = kernel.step_until_empty();
        assert!(!events.is_empty());

        let agent = kernel.model().agents.get("agent-0").expect("agent exists");
        assert_ne!(agent.location_id, initial_location);
    }

    #[test]
    fn plan_demo_actions_falls_back_to_transfer_for_single_location() {
        let config = WorldConfig::default();
        let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
        let (mut kernel, _) = initialize_kernel(config, init).expect("init ok");

        let actions = plan_demo_actions(&kernel);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::TransferResource { .. }));

        for action in actions {
            kernel.submit_action(action);
        }
        let events = kernel.step_until_empty();
        assert_eq!(events.len(), 1);
    }
}
