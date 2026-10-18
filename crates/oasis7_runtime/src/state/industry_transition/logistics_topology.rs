use super::*;

impl PreparedLogisticsTopology {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let (route, actor) = match event {
            DomainEvent::LogisticsRouteRegistered {
                requester_agent_id,
                route_id,
                from_ledger,
                to_ledger,
                kind,
                distance_km,
                priority,
                owner_agent_id,
                available,
                capacity_units,
                tariff_electricity_per_unit,
            } => {
                let route = route_id.as_ref().map(|id| {
                    (
                        id.clone(),
                        LogisticsRouteV1 {
                            route_id: id.clone(),
                            from_ledger: from_ledger.clone(),
                            to_ledger: to_ledger.clone(),
                            kind: kind.clone(),
                            distance_km: *distance_km,
                            priority: *priority,
                            owner_agent_id: if owner_agent_id.is_empty() {
                                requester_agent_id.clone()
                            } else {
                                owner_agent_id.clone()
                            },
                            available: *available,
                            capacity_units: *capacity_units,
                            reserved_capacity_units: 0,
                            tariff_electricity_per_unit: *tariff_electricity_per_unit,
                        },
                    )
                });
                (route, requester_agent_id)
            }
            DomainEvent::LogisticsRouteAvailabilityChanged {
                requester_agent_id,
                route_id,
                available,
                owner_agent_id,
            } => {
                let mut route = state
                    .logistics_routes
                    .get(route_id)
                    .cloned()
                    .ok_or_else(|| {
                        invalid(format!("logistics route not found: route_id={route_id}"))
                    })?;
                let owner = if owner_agent_id.is_empty() {
                    route.owner_agent_id.as_str()
                } else {
                    owner_agent_id.as_str()
                };
                if !owner.is_empty() && owner != requester_agent_id {
                    return Err(invalid(format!(
                        "logistics route availability owner mismatch: route_id={route_id} owner={owner} requester={requester_agent_id}"
                    )));
                }
                route.available = *available;
                (Some((route_id.clone(), route)), requester_agent_id)
            }
            _ => {
                return Err(invalid(
                    "logistics topology preparation requires a supported event",
                ));
            }
        };
        let agent = touched_agent(state, actor, now);
        Ok(Self {
            event: event.clone(),
            route,
            agent,
        })
    }
    pub(crate) fn matches(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }
    pub(crate) fn install(self, state: &mut WorldState) {
        if let Some((id, route)) = self.route {
            state.logistics_routes.insert(id, route);
        }
        if let Some((id, cell)) = self.agent {
            state.agents.insert(id, cell);
        }
    }
    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        serialize_agents(&self.event, self.agent.as_ref(), state, out)
    }
    pub(crate) fn serialize_routes<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updates = self.route.clone().into_iter().collect();
        out.serialize_field(
            "logistics_routes",
            &SparseOverlay {
                base: &state.logistics_routes,
                updates: &updates,
            },
        )
    }
}
