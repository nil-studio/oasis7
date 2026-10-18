use super::*;

impl PreparedMaterialTransfer {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let DomainEvent::MaterialTransferred {
            transfer_id,
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            amount,
            distance_km,
            priority,
            route_id,
        } = event
        else {
            return Err(invalid(
                "material transfer preparation requires a supported event",
            ));
        };
        let receipt = transfer_id.map(|id| MaterialTransferReceiptV1 {
            transfer_id: id,
            requester_agent_id: requester_agent_id.clone(),
            from_ledger: from_ledger.clone(),
            to_ledger: to_ledger.clone(),
            kind: kind.clone(),
            amount: *amount,
            distance_km: *distance_km,
            priority: *priority,
            route_id: route_id.clone(),
        });
        if let Some(receipt) = receipt.as_ref()
            && let Some(existing) = state
                .direct_material_transfer_receipts
                .get(&receipt.transfer_id)
        {
            if existing == receipt {
                let (materials, world) = normalized_materials(state);
                return Ok(Self {
                    event: event.clone(),
                    ledgers: BTreeMap::from([(MaterialLedgerId::world(), world)]),
                    materials,
                    completed_route: None,
                    receipt: None,
                    agent: state
                        .agents
                        .get(requester_agent_id)
                        .cloned()
                        .map(|cell| (requester_agent_id.clone(), cell)),
                });
            }
            return Err(invalid(format!(
                "direct material transfer receipt mismatch: transfer_id={}",
                receipt.transfer_id
            )));
        }
        let world_id = MaterialLedgerId::world();
        let (_, world) = normalized_materials(state);
        let mut all_ledgers = BTreeMap::from([(world_id.clone(), world)]);
        for ledger_id in [from_ledger, to_ledger] {
            if ledger_id != &world_id {
                all_ledgers.insert(
                    ledger_id.clone(),
                    state
                        .material_ledgers
                        .get(ledger_id)
                        .cloned()
                        .unwrap_or_default(),
                );
            }
        }
        preflight_material_balance_additions(
            &all_ledgers,
            to_ledger,
            &[MaterialStack::new(kind.clone(), *amount)],
        )
        .map_err(|r| invalid(format!("material transfer add preflight failed: {r}")))?;
        remove_material_balance_for_ledger(&mut all_ledgers, from_ledger, kind, *amount)
            .map_err(|r| invalid(format!("material transfer remove failed: {r}")))?;
        add_material_balance_for_ledger(&mut all_ledgers, to_ledger, kind, *amount)
            .map_err(|r| invalid(format!("material transfer add failed: {r}")))?;
        let materials = all_ledgers
            .get(&MaterialLedgerId::world())
            .cloned()
            .unwrap_or_default();
        Ok(Self {
            event: event.clone(),
            ledgers: all_ledgers,
            materials,
            completed_route: route_id.clone(),
            receipt: receipt.map(|r| (r.transfer_id, r)),
            agent: touched_agent(state, requester_agent_id, now),
        })
    }
    pub(crate) fn matches(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }
    pub(crate) fn install(self, state: &mut WorldState) {
        state.material_ledgers.extend(self.ledgers);
        state.materials = self.materials;
        if let Some(id) = self.completed_route {
            state.completed_logistics_route_ids.insert(id);
        }
        if let Some((id, r)) = self.receipt {
            state.direct_material_transfer_receipts.insert(id, r);
        }
        if let Some((id, c)) = self.agent {
            state.agents.insert(id, c);
        }
    }
    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        serialize_agents(&self.event, self.agent.as_ref(), state, out)
    }
    pub(crate) fn serialize_materials<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("materials", &self.materials)?;
        out.serialize_field(
            "material_ledgers",
            &SparseOverlay {
                base: &state.material_ledgers,
                updates: &self.ledgers,
            },
        )
    }
    pub(crate) fn serialize_completed_routes<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let mut routes = state.completed_logistics_route_ids.clone();
        if let Some(id) = &self.completed_route {
            routes.insert(id.clone());
        }
        out.serialize_field("completed_logistics_route_ids", &routes)
    }
    pub(crate) fn serialize_receipt<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updates = self.receipt.clone().into_iter().collect();
        out.serialize_field(
            "direct_material_transfer_receipts",
            &SparseOverlay {
                base: &state.direct_material_transfer_receipts,
                updates: &updates,
            },
        )
    }
}
