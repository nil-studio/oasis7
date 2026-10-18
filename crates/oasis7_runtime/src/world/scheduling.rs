use super::super::AgentSchedule;
use super::World;

impl World {
    // ---------------------------------------------------------------------
    // Scheduling
    // ---------------------------------------------------------------------

    pub fn schedule_next(&mut self) -> Option<AgentSchedule> {
        let ready: Vec<String> = self
            .state
            .agents
            .iter()
            .filter(|(_, cell)| !cell.mailbox.is_empty())
            .map(|(id, _)| id.clone())
            .collect();

        if ready.is_empty() {
            return None;
        }

        let next_id = match &self.scheduler_cursor {
            None => ready[0].clone(),
            Some(cursor) => ready
                .iter()
                .find(|id| id.as_str() > cursor.as_str())
                .cloned()
                .unwrap_or_else(|| ready[0].clone()),
        };

        let cell = self.state.agents.get_mut(&next_id)?;
        let event = cell.mailbox.pop_front()?;
        cell.last_active = self.state.time;
        self.scheduler_cursor = Some(next_id.clone());

        Some(AgentSchedule {
            agent_id: next_id,
            event,
        })
    }

    pub fn agent_mailbox_len(&self, agent_id: &str) -> Option<usize> {
        self.state
            .agents
            .get(agent_id)
            .map(|cell| cell.mailbox.len())
    }
}
