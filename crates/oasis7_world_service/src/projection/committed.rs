use super::*;
impl WorldServiceProjection {
    /// Bind logical World capability counters inside the formal audience leaf.
    pub fn from_formal_committed_world(
        world: &World,
        agent: Option<&str>,
        reader: &str,
    ) -> Result<Self, String> {
        let mut projection = Self::from_committed_world(world, agent, reader)?;
        let (header, start) = super::super::formal_projection_header::committed_header(world)?;
        projection.events =
            super::super::formal_projection_header::current_era_events(world, start, agent);
        projection.formal_commit_header = Some(header);
        Ok(projection)
    }

    /// Pure formatting after audience authorization. Shared by response reads
    /// and formal commit commitments so they cannot drift to different views.
    pub fn from_committed_world(
        world: &World,
        agent: Option<&str>,
        reader: &str,
    ) -> Result<Self, String> {
        let mut projection = Self::from_authenticated_world(world, agent, reader)?;
        if let Some(agent) = agent {
            projection.agent_context = Some(
                oasis7_runtime::World::canonical_agent_service_context(world, agent)?,
            );
            projection.cognition_leases = world
                .cognition_economy()
                .map_err(|e| format!("{e:?}"))?
                .leases
                .into_values()
                .filter(|lease| lease.agent_id == agent)
                .collect();
            projection.continuations = serde_json::from_value::<
                Vec<oasis7_runtime::AgentContinuation>,
            >(world.cognition_continuations())
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|continuation| continuation.agent_id == agent)
            .collect();
            projection.scheduler_wakes = world
                .cognition_in_flight_wakes()
                .map_err(|e| format!("{e:?}"))?
                .into_iter()
                .filter(|wake| wake.agent_id == agent)
                .collect();
            for continuation in &projection.continuations {
                if let Some(context) =
                    world.service_continuation_context(&continuation.continuation_id)
                {
                    projection.continuation_contexts.insert(
                        continuation.continuation_id.clone(),
                        serde_json::from_value(context).map_err(|e| e.to_string())?,
                    );
                }
            }
        }
        projection.events = world
            .journal()
            .events
            .iter()
            .filter(|event| match &event.body {
                oasis7_runtime::WorldEventBody::Domain(domain) => {
                    agent.is_some_and(|agent| domain.agent_id() == Some(agent))
                }
                _ => false,
            })
            .cloned()
            .collect();
        Ok(projection)
    }
}
