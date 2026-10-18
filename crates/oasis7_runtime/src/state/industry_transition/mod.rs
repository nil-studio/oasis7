use super::*;
use serde::Serialize;
use serde::ser::{SerializeMap, SerializeStruct};

mod factory_lifecycle;
mod logistics_topology;
mod material_transfer;
mod material_transit;
mod recipe_lifecycle;

pub(crate) fn factory_has_canonical_stable_line(factory: &FactoryState) -> bool {
    factory.production.same_recipe_repeat_count >= 3
        && factory
            .production
            .last_completed_canonical_snapshot
            .as_ref()
            .zip(factory.production.last_completed_recipe_id.as_ref())
            .is_some_and(|(snapshot, recipe_id)| {
                !snapshot.recipe_id.trim().is_empty()
                    && !recipe_id.trim().is_empty()
                    && snapshot.recipe_id == *recipe_id
            })
}

impl<'a> WorldStateProjection<'a> {
    pub(crate) fn with_industry_overlay(mut self, overlay: &'a PreparedIndustryEvent) -> Self {
        self.industry_overlay = Some(overlay);
        self
    }
}

#[derive(Debug)]
pub(crate) enum PreparedIndustryEvent {
    LogisticsTopology(Box<PreparedLogisticsTopology>),
    MaterialTransfer(Box<PreparedMaterialTransfer>),
    MaterialTransit(Box<PreparedMaterialTransit>),
    FactoryLifecycle(Box<PreparedFactoryLifecycle>),
    RecipeLifecycle(Box<PreparedRecipeLifecycle>),
}

#[derive(Debug)]
pub(crate) struct PreparedLogisticsTopology {
    event: DomainEvent,
    route: Option<(String, LogisticsRouteV1)>,
    agent: Option<(String, AgentCell)>,
}

#[derive(Debug)]
pub(crate) struct PreparedMaterialTransfer {
    event: DomainEvent,
    ledgers: BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    materials: BTreeMap<String, i64>,
    completed_route: Option<String>,
    receipt: Option<(ActionId, MaterialTransferReceiptV1)>,
    agent: Option<(String, AgentCell)>,
}

#[derive(Debug)]
pub(crate) struct PreparedMaterialTransit {
    event: DomainEvent,
    ledgers: BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    materials: BTreeMap<String, i64>,
    pending: Option<(ActionId, Option<MaterialTransitJobState>)>,
    route_updates: BTreeMap<String, LogisticsRouteV1>,
    completed_route_ids: BTreeSet<String>,
    path: Option<(String, LogisticsPathAuthorityV1)>,
    settled_id: Option<ActionId>,
    receipt: Option<(ActionId, LogisticsSettlementReceiptV1)>,
    agents: BTreeMap<String, AgentCell>,
    progress: Option<IndustryProgressState>,
}

#[derive(Debug)]
pub(crate) struct PreparedFactoryLifecycle {
    event: DomainEvent,
    pending: Option<(ActionId, Option<FactoryBuildJobState>)>,
    factory: Option<(String, Option<FactoryState>)>,
    settled_id: Option<ActionId>,
    retired_insert: Option<String>,
    pending_recipe_deletions: BTreeSet<ActionId>,
    ledgers: BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    materials: BTreeMap<String, i64>,
    agent: Option<(String, AgentCell)>,
    progress: Option<IndustryProgressState>,
    construction_receipt: Option<(String, FactoryBuildPowerObligationV1)>,
    recycle_receipt: Option<(String, FactoryRecycleReceiptV1)>,
}

#[derive(Debug)]
pub(crate) struct PreparedRecipeLifecycle {
    event: DomainEvent,
    pending: Option<(ActionId, Option<RecipeJobState>)>,
    settled_id: Option<ActionId>,
    factory: Option<(String, FactoryState)>,
    ledgers: BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    materials: BTreeMap<String, i64>,
    paths: BTreeMap<String, LogisticsPathAuthorityV1>,
    agent: Option<(String, AgentCell)>,
    progress: Option<IndustryProgressState>,
    failure_disposition: Option<(ActionId, FactoryProductionFailureDispositionV1)>,
    settlement_order: Option<(ActionId, u64)>,
    next_settlement_order: Option<u64>,
    completion_receipt: Option<(ActionId, RecipeCompletionReceiptV1)>,
}

impl PreparedIndustryEvent {
    pub(crate) fn has_construction_receipt(&self) -> bool {
        matches!(self, Self::FactoryLifecycle(value) if value.construction_receipt.is_some())
    }
    pub(crate) fn has_completion_receipt(&self) -> bool {
        matches!(self, Self::RecipeLifecycle(value) if value.completion_receipt.is_some())
    }
    pub(crate) fn has_recycle_receipt(&self) -> bool {
        matches!(self, Self::FactoryLifecycle(value) if value.recycle_receipt.is_some())
    }
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        match event {
            DomainEvent::LogisticsRouteRegistered { .. }
            | DomainEvent::LogisticsRouteAvailabilityChanged { .. } => Ok(Self::LogisticsTopology(
                Box::new(PreparedLogisticsTopology::prepare(state, event, now)?),
            )),
            DomainEvent::MaterialTransferred { .. } => Ok(Self::MaterialTransfer(Box::new(
                PreparedMaterialTransfer::prepare(state, event, now)?,
            ))),
            DomainEvent::MaterialTransitStarted { .. }
            | DomainEvent::MaterialTransitCompleted { .. } => Ok(Self::MaterialTransit(Box::new(
                PreparedMaterialTransit::prepare(state, event, now)?,
            ))),
            DomainEvent::FactoryBuildStarted { .. }
            | DomainEvent::FactoryBuilt { .. }
            | DomainEvent::FactoryDurabilityChanged { .. }
            | DomainEvent::FactoryMaintained { .. }
            | DomainEvent::FactoryRecycled { .. } => Ok(Self::FactoryLifecycle(Box::new(
                PreparedFactoryLifecycle::prepare(state, event, now)?,
            ))),
            DomainEvent::RecipeStarted { .. }
            | DomainEvent::RecipeCompleted { .. }
            | DomainEvent::FactoryProductionBlocked { .. }
            | DomainEvent::FactoryProductionResumed { .. }
            | DomainEvent::FactoryProductionPaused { .. } => Ok(Self::RecipeLifecycle(Box::new(
                PreparedRecipeLifecycle::prepare(state, event, now)?,
            ))),
            _ => Err(invalid("industry preparation requires a supported event")),
        }
    }
    pub(crate) fn matches(&self, event: &DomainEvent) -> bool {
        match self {
            Self::LogisticsTopology(value) => value.matches(event),
            Self::MaterialTransfer(value) => value.matches(event),
            Self::MaterialTransit(value) => value.matches(event),
            Self::FactoryLifecycle(value) => value.matches(event),
            Self::RecipeLifecycle(value) => value.matches(event),
        }
    }

    pub(crate) fn install(self, state: &mut WorldState) {
        match self {
            Self::LogisticsTopology(value) => (*value).install(state),
            Self::MaterialTransfer(value) => (*value).install(state),
            Self::MaterialTransit(value) => (*value).install(state),
            Self::FactoryLifecycle(value) => (*value).install(state),
            Self::RecipeLifecycle(value) => (*value).install(state),
        }
    }

    pub(super) fn serialize_terminal_receipts<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        match self {
            Self::RecipeLifecycle(value) => value.serialize_terminal_receipts(state, out),
            Self::FactoryLifecycle(value) => value.serialize_terminal_receipts(state, out),
            _ => {
                if !state.recipe_completion_receipts.is_empty() {
                    out.serialize_field(
                        "recipe_completion_receipts",
                        &state.recipe_completion_receipts,
                    )?;
                }
                if !state.factory_recycle_receipts.is_empty() {
                    out.serialize_field(
                        "factory_recycle_receipts",
                        &state.factory_recycle_receipts,
                    )?;
                }
                Ok(())
            }
        }
    }

    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        match self {
            Self::LogisticsTopology(value) => value.serialize_agents(state, out),
            Self::MaterialTransfer(value) => value.serialize_agents(state, out),
            Self::MaterialTransit(value) => value.serialize_agents(state, out),
            Self::FactoryLifecycle(value) => value.serialize_agents(state, out),
            Self::RecipeLifecycle(value) => value.serialize_agents(state, out),
        }
    }

    pub(crate) fn serialize_materials<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        match self {
            Self::MaterialTransfer(value) => value.serialize_materials(state, out),
            Self::MaterialTransit(value) => value.serialize_materials(state, out),
            Self::FactoryLifecycle(value) => value.serialize_materials(state, out),
            Self::RecipeLifecycle(value) => value.serialize_materials(state, out),
            Self::LogisticsTopology(_) => {
                out.serialize_field("materials", &state.materials)?;
                out.serialize_field("material_ledgers", &state.material_ledgers)
            }
        }
    }

    pub(crate) fn serialize_failure_dispositions<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        match self {
            Self::RecipeLifecycle(value) => value.serialize_failure_dispositions(state, out),
            _ => out.serialize_field(
                "factory_production_failure_dispositions",
                &state.factory_production_failure_dispositions,
            ),
        }
    }

    pub(crate) fn serialize_settlement_history<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        match self {
            Self::RecipeLifecycle(value) => value.serialize_settlement_history(state, out),
            _ => {
                out.serialize_field(
                    "next_industry_settlement_order",
                    &state.next_industry_settlement_order,
                )?;
                out.serialize_field(
                    "industry_settlement_orders",
                    &state.industry_settlement_orders,
                )
            }
        }
    }

    pub(crate) fn serialize_logistics<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        match self {
            Self::LogisticsTopology(value) => {
                value.serialize_routes(state, out)?;
                serialize_unchanged_logistics_tail(state, out)
            }
            Self::MaterialTransfer(value) => {
                out.serialize_field("logistics_routes", &state.logistics_routes)?;
                value.serialize_completed_routes(state, out)?;
                out.serialize_field(
                    "completed_logistics_paths",
                    &state.completed_logistics_paths,
                )?;
                out.serialize_field(
                    "settled_logistics_transit_ids",
                    &state.settled_logistics_transit_ids,
                )?;
                out.serialize_field(
                    "logistics_settlement_receipts",
                    &state.logistics_settlement_receipts,
                )?;
                value.serialize_receipt(state, out)
            }
            Self::MaterialTransit(value) => value.serialize_logistics(state, out),
            Self::FactoryLifecycle(_) => serialize_unchanged_logistics_tail_with_routes(state, out),
            Self::RecipeLifecycle(value) => value.serialize_logistics(state, out),
        }
    }

    pub(crate) fn serialize_pending_and_progress<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        match self {
            Self::MaterialTransit(value) => value.serialize_pending_and_progress(state, out),
            Self::FactoryLifecycle(value) => value.serialize_transit_and_progress(state, out),
            Self::RecipeLifecycle(value) => value.serialize_pending_and_progress(state, out),
            _ => {
                out.serialize_field(
                    "pending_material_transits",
                    &state.pending_material_transits,
                )?;
                out.serialize_field("industry_progress", &state.industry_progress)
            }
        }
    }

    pub(crate) fn serialize_factory_fields<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        match self {
            Self::FactoryLifecycle(value) => value.serialize_factory_fields(state, out),
            Self::RecipeLifecycle(value) => value.serialize_factory_fields(state, out),
            _ => serialize_unchanged_factory_fields(state, out),
        }
    }

    pub(crate) fn serialize_construction_receipts<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        match self {
            Self::FactoryLifecycle(value) => value.serialize_construction_receipts(state, out),
            _ if !state.factory_construction_receipts.is_empty() => out.serialize_field(
                "factory_construction_receipts",
                &state.factory_construction_receipts,
            ),
            _ => Ok(()),
        }
    }
}

fn serialize_unchanged_logistics_tail_with_routes<S: SerializeStruct>(
    state: &WorldState,
    out: &mut S,
) -> Result<(), S::Error> {
    out.serialize_field("logistics_routes", &state.logistics_routes)?;
    serialize_unchanged_logistics_tail(state, out)
}

fn serialize_unchanged_factory_fields<S: SerializeStruct>(
    state: &WorldState,
    out: &mut S,
) -> Result<(), S::Error> {
    out.serialize_field("factories", &state.factories)?;
    out.serialize_field("retired_factory_ids", &state.retired_factory_ids)?;
    out.serialize_field(
        "settled_factory_build_ids",
        &state.settled_factory_build_ids,
    )?;
    out.serialize_field("pending_factory_builds", &state.pending_factory_builds)?;
    out.serialize_field("pending_recipe_jobs", &state.pending_recipe_jobs)?;
    out.serialize_field("settled_recipe_job_ids", &state.settled_recipe_job_ids)
}

fn serialize_unchanged_logistics_tail<S: SerializeStruct>(
    state: &WorldState,
    out: &mut S,
) -> Result<(), S::Error> {
    out.serialize_field(
        "completed_logistics_route_ids",
        &state.completed_logistics_route_ids,
    )?;
    out.serialize_field(
        "completed_logistics_paths",
        &state.completed_logistics_paths,
    )?;
    out.serialize_field(
        "settled_logistics_transit_ids",
        &state.settled_logistics_transit_ids,
    )?;
    out.serialize_field(
        "logistics_settlement_receipts",
        &state.logistics_settlement_receipts,
    )?;
    out.serialize_field(
        "direct_material_transfer_receipts",
        &state.direct_material_transfer_receipts,
    )
}

struct SparseOverlay<'a, K, V> {
    base: &'a BTreeMap<K, V>,
    updates: &'a BTreeMap<K, V>,
}
impl<K: Ord + Serialize, V: Serialize> Serialize for SparseOverlay<'_, K, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(
            self.base.len()
                + self
                    .updates
                    .keys()
                    .filter(|k| !self.base.contains_key(*k))
                    .count(),
        ))?;
        for (k, v) in self.base {
            map.serialize_entry(k, self.updates.get(k).unwrap_or(v))?;
        }
        for (k, v) in self.updates {
            if !self.base.contains_key(k) {
                map.serialize_entry(k, v)?;
            }
        }
        map.end()
    }
}

struct SparseOptionalOverlay<'a, K, V> {
    base: &'a BTreeMap<K, V>,
    changes: &'a BTreeMap<K, Option<V>>,
}

impl<K: Ord + Serialize, V: Serialize> Serialize for SparseOptionalOverlay<'_, K, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let len = self.base.len()
            + self
                .changes
                .iter()
                .filter(|(key, value)| value.is_some() && !self.base.contains_key(*key))
                .count()
            - self
                .changes
                .iter()
                .filter(|(key, value)| value.is_none() && self.base.contains_key(*key))
                .count();
        let mut map = serializer.serialize_map(Some(len))?;
        for (key, value) in self.base {
            match self.changes.get(key) {
                Some(Some(replacement)) => map.serialize_entry(key, replacement)?,
                Some(None) => {}
                None => map.serialize_entry(key, value)?,
            }
        }
        for (key, value) in self.changes {
            if !self.base.contains_key(key)
                && let Some(value) = value
            {
                map.serialize_entry(key, value)?;
            }
        }
        map.end()
    }
}

struct SetInsertOverlay<'a, T> {
    base: &'a BTreeSet<T>,
    inserts: &'a BTreeSet<T>,
}

impl<T: Ord + Serialize> Serialize for SetInsertOverlay<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let len = self.base.len() + self.inserts.difference(self.base).count();
        let mut sequence = serializer.serialize_seq(Some(len))?;
        for value in self.base.union(self.inserts) {
            sequence.serialize_element(value)?;
        }
        sequence.end()
    }
}

struct SetInsertOneOverlay<'a, T> {
    base: &'a BTreeSet<T>,
    insert: Option<&'a T>,
}

impl<T: Ord + Serialize> Serialize for SetInsertOneOverlay<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let extra = usize::from(self.insert.is_some_and(|value| !self.base.contains(value)));
        let mut sequence = serializer.serialize_seq(Some(self.base.len() + extra))?;
        let mut emitted = false;
        for value in self.base {
            if let Some(insert) = self.insert
                && !emitted
                && insert < value
            {
                sequence.serialize_element(insert)?;
                emitted = true;
            }
            sequence.serialize_element(value)?;
            if self.insert == Some(value) {
                emitted = true;
            }
        }
        if let Some(insert) = self.insert
            && !emitted
        {
            sequence.serialize_element(insert)?;
        }
        sequence.end()
    }
}
fn serialize_agents<S: SerializeStruct>(
    event: &DomainEvent,
    agent: Option<&(String, AgentCell)>,
    state: &WorldState,
    out: &mut S,
) -> Result<(), S::Error> {
    let mut updates = BTreeMap::new();
    if let Some((id, cell)) = agent {
        let mut cell = cell.clone();
        if event.agent_id() == Some(id.as_str()) {
            cell.mailbox.push_back(event.clone());
        }
        updates.insert(id.clone(), cell);
    }
    out.serialize_field(
        "agents",
        &SparseOverlay {
            base: &state.agents,
            updates: &updates,
        },
    )
}

fn touched_agent(state: &WorldState, id: &str, now: WorldTime) -> Option<(String, AgentCell)> {
    state.agents.get(id).cloned().map(|mut c| {
        c.last_active = now;
        (id.into(), c)
    })
}
fn normalized_materials(state: &WorldState) -> (BTreeMap<String, i64>, BTreeMap<String, i64>) {
    let world = state
        .material_ledgers
        .get(&MaterialLedgerId::world())
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| state.materials.clone());
    (world.clone(), world)
}
fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
