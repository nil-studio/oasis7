use super::*;

/// Small deterministic script used by the direct Viewer observer/debug lane.
/// It stays separate from the transport/session helpers so the support module
/// remains focused on shared server plumbing.
#[derive(Debug, Clone, Default)]
pub(super) struct RuntimeLiveScript {
    phase: u8,
    move_direction: i64,
}

impl RuntimeLiveScript {
    pub(super) fn enqueue(&mut self, world: &mut RuntimeWorld) {
        let mut agent_ids: Vec<String> = world.state().agents.keys().cloned().collect();
        agent_ids.sort();

        if agent_ids.is_empty() {
            world.submit_action(RuntimeAction::RegisterAgent {
                agent_id: "runtime-agent-0".to_string(),
                pos: GeoPos::new(0, 0, 0),
            });
            world.submit_action(RuntimeAction::RegisterAgent {
                agent_id: "runtime-agent-1".to_string(),
                pos: GeoPos::new(0, 0, 0),
            });
            return;
        }

        let phase = self.phase;
        self.phase = self.phase.wrapping_add(1) % 4;

        match phase {
            0 => {
                let first = &agent_ids[0];
                let Some(from_pos) = world.state().agents.get(first).map(|cell| cell.state.pos)
                else {
                    return;
                };
                if self.move_direction == 0 {
                    self.move_direction = 1;
                } else {
                    self.move_direction = -self.move_direction;
                }
                let delta_cm = self.move_direction * 1_000;
                world.submit_action(RuntimeAction::MoveAgent {
                    agent_id: first.clone(),
                    to: GeoPos::new(from_pos.x_cm + delta_cm, from_pos.y_cm, from_pos.z_cm),
                });
            }
            1 => {
                if agent_ids.len() < 2 {
                    world.submit_action(RuntimeAction::MoveAgent {
                        agent_id: "missing-agent".to_string(),
                        to: GeoPos::new(0, 0, 0),
                    });
                    return;
                }
                let first = &agent_ids[0];
                let second = &agent_ids[1];
                let Some(target) = world.state().agents.get(first).map(|cell| cell.state.pos)
                else {
                    return;
                };
                world.submit_action(RuntimeAction::MoveAgent {
                    agent_id: second.clone(),
                    to: target,
                });
            }
            2 => {
                if agent_ids.len() < 2 {
                    world.submit_action(RuntimeAction::MoveAgent {
                        agent_id: "missing-agent".to_string(),
                        to: GeoPos::new(0, 0, 0),
                    });
                    return;
                }
                let from = &agent_ids[0];
                let to = &agent_ids[1];
                let _ = world.set_agent_resource_balance(from, ResourceKind::Electricity, 64);
                let _ = world.set_agent_resource_balance(to, ResourceKind::Electricity, 64);
                world.submit_action(RuntimeAction::EmitResourceTransfer {
                    from_agent_id: from.clone(),
                    to_agent_id: to.clone(),
                    kind: ResourceKind::Electricity,
                    amount: 1,
                });
            }
            _ => {
                world.submit_action(RuntimeAction::MoveAgent {
                    agent_id: "missing-agent".to_string(),
                    to: GeoPos::new(0, 0, 0),
                });
            }
        }
    }
}
