//! Staged transition adapters for industrial construction, production completion
//! control, and logistics supply-chain writes (#6140 PR 5/6).

use super::World;
use crate::world::event_processing::publication::staged_industrial::{
    IndustrialBatchPublicationDelta, IndustrialPublicationDelta, StagedIndustrialPublication,
};
use crate::world::industrial_completion::IndustrialCompletionGuard;
use crate::world::logistics_sla_metrics::LogisticsSlaMetricsDelta;
use crate::world::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedTransition,
    SubsystemDelta, SynchronousEventTransition, TransitionContext, TransitionReject,
};
use crate::{ActionId, DomainEvent, RecipeJobState, WorldError, WorldEventBody, WorldTime};
use oasis7_wasm_abi::FactoryModuleSpec;

/// Staged factory construction transition wrapping `World::build_factory_to_event`
/// and optional module-backed admission validation.
#[cfg_attr(not(test), expect(dead_code, reason = "pending #6140 PR 6/6 industrial write cutover"))]
pub(in crate::world) struct StagedFactoryConstruction<'a> {
    pub action_id: ActionId,
    pub builder_agent_id: &'a String,
    pub site_id: &'a String,
    pub spec: &'a FactoryModuleSpec,
    pub validate_materials: bool,
    pub module_id: Option<&'a str>,
}

#[cfg_attr(not(test), expect(dead_code, reason = "pending #6140 PR 6/6 industrial write cutover"))]
pub(in crate::world) struct FactoryConstructionDelta {
    publication: IndustrialPublicationDelta,
}

impl SubsystemDelta for FactoryConstructionDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        self.publication.apply_infallible(ctx);
    }
}

impl StagedTransition for StagedFactoryConstruction<'_> {
    type Delta = FactoryConstructionDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let world = view.world();
        let body = if let Some(module_id) = self.module_id
            && let Some(reason) = world.validate_module_backed_factory_admission(
                self.action_id, self.builder_agent_id, self.site_id, module_id, self.spec,
            )?
        {
            WorldEventBody::Domain(DomainEvent::ActionRejected { action_id: self.action_id, reason })
        } else {
            world.build_factory_to_event(
                self.action_id, self.builder_agent_id, self.site_id, self.spec, self.validate_materials,
            )?
        };
        let (consensus_record, publication) = match &body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { .. }) => {
                world.prepare_industrial_action_rejected_publication(body, ctx.caused_by.clone())?
            }
            WorldEventBody::Domain(event @ DomainEvent::FactoryBuildStarted { .. }) => {
                world.validate_manufactured_identity_acquisition(event)?;
                let transition = StagedIndustrialPublication::from_body(&body).ok_or_else(|| {
                    WorldError::ResourceBalanceInvalid { reason: "unclassified factory construction event".into() }
                })?;
                let (_, consensus, publication) = transition.prepare_event(world, ctx)?;
                (consensus, publication)
            }
            _ => return Err(WorldError::ResourceBalanceInvalid { reason: "unexpected event body".into() }.into()),
        };
        Ok(PreparedStagedCommit::new(view.base_head().clone(), Some(consensus_record), FactoryConstructionDelta { publication }))
    }
}

/// Staged production completion control for a due [`RecipeJobState`].
#[cfg_attr(not(test), expect(dead_code, reason = "pending #6140 PR 6/6 industrial write cutover"))]
pub(in crate::world) struct StagedIndustrialCompletion<'a> {
    pub job: &'a RecipeJobState,
}

#[cfg_attr(not(test), expect(dead_code, reason = "pending #6140 PR 6/6 industrial write cutover"))]
pub(in crate::world) enum IndustrialCompletionDelta {
    NoOp,
    Publish(Box<IndustrialPublicationDelta>),
}

impl SubsystemDelta for IndustrialCompletionDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        if let Self::Publish(delta) = self { (*delta).apply_infallible(ctx); }
    }
}

impl StagedTransition for StagedIndustrialCompletion<'_> {
    type Delta = IndustrialCompletionDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let world = view.world();
        let body = match world.industrial_completion_guard(self.job) {
            IndustrialCompletionGuard::Ready => {
                if let Some(binding) = self.job.emergency_power_world_binding.as_ref()
                    && world.world_rule_binding().as_ref() != Some(binding)
                {
                    return Err(crate::world_rules::invalid("essential_recipe_world_binding_mismatch").into());
                }
                World::recipe_completion_body(self.job.clone())
            }
            IndustrialCompletionGuard::Blocked(Some(body)) => *body,
            IndustrialCompletionGuard::Blocked(None) => {
                return Ok(PreparedStagedCommit::new(view.base_head().clone(), None, IndustrialCompletionDelta::NoOp));
            }
        };
        let transition = StagedIndustrialPublication::from_body(&body).ok_or_else(|| {
            WorldError::ResourceBalanceInvalid { reason: "unclassified industrial completion event".into() }
        })?;
        let (_, consensus_record, delta) = transition.prepare_event(world, ctx)?;
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            Some(consensus_record),
            IndustrialCompletionDelta::Publish(Box::new(delta)),
        ))
    }
}

/// Staged due material transit completions and SLA metrics update.
#[cfg_attr(not(test), expect(dead_code, reason = "pending #6140 PR 6/6 industrial write cutover"))]
pub(in crate::world) struct StagedDueMaterialTransits {
    pub now: Option<WorldTime>,
}

#[cfg_attr(not(test), expect(dead_code, reason = "pending #6140 PR 6/6 industrial write cutover"))]
pub(in crate::world) struct LogisticsDueTransitsDelta {
    publication: IndustrialBatchPublicationDelta,
    sla_metrics: Option<LogisticsSlaMetricsDelta>,
}

impl SubsystemDelta for LogisticsDueTransitsDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        self.publication.apply_infallible(ctx);
        if let Some(sla_metrics) = self.sla_metrics {
            sla_metrics.apply_infallible(ctx);
        }
    }
}

impl StagedTransition for StagedDueMaterialTransits {
    type Delta = LogisticsDueTransitsDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let world = view.world();
        let now = self.now.unwrap_or_else(|| view.time());
        let prepared = world.prepare_due_material_transits_at(now);
        if prepared.bodies.is_empty() {
            return Ok(PreparedStagedCommit::new(
                view.base_head().clone(),
                None,
                LogisticsDueTransitsDelta {
                    publication: IndustrialBatchPublicationDelta::default(),
                    sla_metrics: None,
                },
            ));
        }
        let (consensus_record, publication) =
            world.prepare_industrial_batch_publication(&prepared.bodies, ctx.caused_by.clone())?;
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            consensus_record,
            LogisticsDueTransitsDelta {
                publication,
                sla_metrics: Some(LogisticsSlaMetricsDelta::new(prepared.logistics_sla_metrics)),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::transition::{
        DurableCommitConfirmation, StagedRootTransaction, TransitionReject,
    };
    use crate::{
        Action, AgentLocationAuthorityV1, FactoryConstructionPowerMode, FactoryConstructionPowerProfileV1,
        FactoryProfileV1, FactorySiteAuthorityV1, LocationAnchorV1, MaterialLedgerId,
    };
    use oasis7_agent_api::world_action::{GeoPos, ResourceKind};
    use oasis7_wasm_abi::MaterialStack;

    fn setup_factory_world(builder: &str, site_id: &str, factory_id: &str, build_ticks: u32) -> (World, FactoryModuleSpec) {
        let mut world = World::new();
        let spec = FactoryModuleSpec {
            factory_id: factory_id.into(), display_name: "Test".into(), tier: 1, tags: vec!["assembly".into()],
            build_cost: vec![MaterialStack::new("steel_plate", 10), MaterialStack::new("circuit_board", 2)],
            build_time_ticks: build_ticks, base_power_draw: 5, recipe_slots: 1, throughput_bps: 10_000, maintenance_per_tick: 1,
        };
        world.submit_action(Action::RegisterAgent { agent_id: builder.into(), pos: GeoPos::new(0, 0, 0) });
        world.step().unwrap();
        let loc = format!("location-{site_id}");
        world.set_location_anchor(LocationAnchorV1 { location_id: loc.clone(), active: true, authority_revision: 1, effective_at: 0 }).unwrap();
        world.set_agent_location_authority(AgentLocationAuthorityV1 { agent_id: builder.into(), location_id: loc.clone(), active: true, authority_revision: 1, effective_at: 0 }).unwrap();
        world.set_factory_site_authority(FactorySiteAuthorityV1 { site_id: site_id.into(), location_id: loc, owner_agent_id: builder.into(), authorized_agent_ids: Vec::new(), chunk_ready: true, active: true, authority_revision: 1, registered_at: 0 }).unwrap();
        world.set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 { factory_id: factory_id.into(), factory_kind: "test".into(), source_module_id: None, electricity_amount: 10, mode: FactoryConstructionPowerMode::StartOnlySink, authority_revision: 1, active: true }).unwrap();
        world.upsert_factory_profile(FactoryProfileV1 { factory_id: factory_id.into(), tier: 1, recipe_slots: 1, tags: vec!["assembly".into()] }).unwrap();
        world.set_agent_resource_balance(builder, ResourceKind::Electricity, 10).unwrap();
        world.set_ledger_material_balance(MaterialLedgerId::agent(builder), "steel_plate", 10).unwrap();
        world.set_ledger_material_balance(MaterialLedgerId::agent(builder), "circuit_board", 2).unwrap();
        (world, spec)
    }

    #[test]
    fn staged_factory_construction_fences_unknown_rejects_stale_and_matches_original() {
        let (builder, site_id) = ("builder-staged-1".to_string(), "site-staged-1".to_string());
        let (mut staged, spec) = setup_factory_world(&builder, &site_id, "factory-staged-1", 3);
        let mut ordinary = staged.clone();
        let tx = StagedFactoryConstruction { action_id: 42, builder_agent_id: &builder, site_id: &site_id, spec: &spec, validate_materials: true, module_id: None };
        let prep = |w: &World| { let v = BorrowedWorldView::new(w).unwrap(); tx.prepare(&v, &TransitionContext::new(v.time(), None)).unwrap() };
        let before = staged.snapshot();
        assert!(matches!(prep(&staged).install_confirmed(&mut staged, DurableCommitConfirmation::StatusUnknown { reason: "lost".into() }), Err(TransitionReject::StatusUnknownFenced { .. })));
        assert_eq!(staged.snapshot(), before);

        let stale_commit = prep(&staged);
        let mut stale_world = staged.clone();
        stale_world.set_ledger_material_balance(MaterialLedgerId::agent(builder.clone()), "steel_plate", 99).unwrap();
        assert!(matches!(stale_commit.install_confirmed(&mut stale_world, DurableCommitConfirmation::Confirmed), Err(TransitionReject::BaseHeadStale)));

        prep(&staged).install_confirmed(&mut staged, DurableCommitConfirmation::Confirmed).unwrap();
        let body = ordinary.build_factory_to_event(42, &builder, &site_id, &spec, true).unwrap();
        ordinary.append_event(body, None).unwrap();
        assert_eq!((staged.snapshot(), staged.journal()), (ordinary.snapshot(), ordinary.journal()));
    }

    #[test]
    fn staged_industrial_completion_handles_blocked_and_fault_fencing() {
        let (builder, site_id) = ("builder-comp-1".to_string(), "site-comp-1".to_string());
        let (mut world, spec) = setup_factory_world(&builder, &site_id, "factory-comp-1", 1);
        let build_body = world.build_factory_to_event(100, &builder, &site_id, &spec, true).unwrap();
        world.append_event(build_body, None).unwrap();
        world.step().unwrap();

        let mut job = RecipeJobState {
            industrial_root_ref: None, emergency_power_world_binding: Some("stale".into()), committed_recipe_origin: None,
            job_id: 200, requester_agent_id: builder.clone(), factory_id: spec.factory_id.clone(),
            recipe_id: "recipe.comp.1".into(), accepted_batches: 1, consume: Vec::new(),
            produce: Vec::new(), byproducts: Vec::new(), power_required: 0,
            power_owner_agent_id: Some(builder.clone()), duration_ticks: 1,
            consume_ledger: MaterialLedgerId::agent(builder.clone()), output_ledger: MaterialLedgerId::agent(builder.clone()),
            bottleneck_tags: Vec::new(), logistics_route_ids: Vec::new(), logistics_path_ids: Vec::new(), ready_at: world.state().time,
        };
        let view = BorrowedWorldView::new(&world).unwrap();
        assert!(matches!(StagedIndustrialCompletion { job: &job }.prepare(&view, &TransitionContext::new(view.time(), None)), Err(TransitionReject::World(_))));

        job.emergency_power_world_binding = None;
        job.industrial_root_ref = Some(oasis7_agent_api::world_action::IndustrialRefV1 {
            world_binding: "b".into(), domain: "d".into(), schema: "s".into(), id: "i".into(), revision: 1, digest: "h".into(),
        });
        world.state.pending_recipe_jobs.insert(200, job.clone());
        let before = world.snapshot();
        let blocked = StagedIndustrialCompletion { job: &job };
        let prep = |w: &World| {
            let v = BorrowedWorldView::new(w).unwrap();
            blocked.prepare(&v, &TransitionContext::new(v.time(), None)).unwrap()
        };
        assert!(matches!(
            prep(&world).install_confirmed(&mut world, DurableCommitConfirmation::StatusUnknown { reason: "lost".into() }),
            Err(TransitionReject::StatusUnknownFenced { .. })
        ));
        assert_eq!(world.snapshot(), before);

        prep(&world).install_confirmed(&mut world, DurableCommitConfirmation::Confirmed).unwrap();
        assert!(matches!(
            world.journal().events.last().map(|e| &e.body),
            Some(WorldEventBody::Domain(DomainEvent::FactoryProductionBlocked { .. }))
        ));
        let len = world.journal().len();
        prep(&world).install_confirmed(&mut world, DurableCommitConfirmation::Confirmed).unwrap();
        assert_eq!(world.journal().len(), len);
    }

    #[test]
    fn staged_due_material_transits_installs_batch_and_metrics_atomically() {
        let mut staged = World::new();
        staged.submit_action(Action::RegisterAgent {
            agent_id: "operator-staged".into(), pos: GeoPos::new(0, 0, 0),
        });
        staged.step().unwrap();
        staged.set_ledger_material_balance(MaterialLedgerId::site("site-a"), "copper_wire", 50).unwrap();
        staged.set_ledger_material_balance(MaterialLedgerId::site("site-c"), "oxygen_pack", 50).unwrap();
        for (from, kind) in [("site-a", "copper_wire"), ("site-c", "oxygen_pack")] {
            staged.submit_action(Action::TransferMaterial {
                requester_agent_id: "operator-staged".into(), from_ledger: MaterialLedgerId::site(from),
                to_ledger: MaterialLedgerId::site("site-b"), kind: kind.into(), amount: 20,
                distance_km: 100, priority: None, route_id: None, route_ids: Vec::new(), auto_reroute: false,
            });
        }
        staged.step().unwrap();

        let due_time = staged.state().time.saturating_add(1);
        let mut ordinary = staged.clone();
        ordinary.state.time = due_time;
        let _ = ordinary.process_due_material_transits().unwrap();

        staged.state.time = due_time;
        let (before_snapshot, before_metrics) = (staged.snapshot(), staged.logistics_sla_metrics().clone());
        let tx = StagedDueMaterialTransits { now: Some(due_time) };
        let prep = |w: &World| {
            let v = BorrowedWorldView::new(w).unwrap();
            tx.prepare(&v, &TransitionContext::new(v.time(), None)).unwrap()
        };
        assert!(matches!(
            prep(&staged).install_confirmed(&mut staged, DurableCommitConfirmation::StatusUnknown { reason: "lost".into() }),
            Err(TransitionReject::StatusUnknownFenced { .. })
        ));
        assert_eq!((staged.snapshot(), staged.logistics_sla_metrics()), (before_snapshot, &before_metrics));

        prep(&staged).install_confirmed(&mut staged, DurableCommitConfirmation::Confirmed).unwrap();
        assert_eq!(staged.snapshot(), ordinary.snapshot());
        assert_eq!(staged.journal(), ordinary.journal());
        assert_eq!(staged.logistics_sla_metrics(), ordinary.logistics_sla_metrics());
        assert_eq!((staged.logistics_sla_metrics().completed_transits, staged.logistics_sla_metrics().urgent_completed_transits), (2, 1));
    }

    #[test]
    fn staged_root_transaction_rolls_back_nested_industrial_write_on_fault() {
        let (builder, site_id) = ("builder-nested-1".to_string(), "site-nested-1".to_string());
        let (mut world, spec) = setup_factory_world(&builder, &site_id, "factory-nested-1", 2);
        enum TxDelta { Construction(Box<FactoryConstructionDelta>), Logistics(LogisticsDueTransitsDelta) }
        impl SubsystemDelta for TxDelta {
            fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
                match self { Self::Construction(d) => (*d).apply_infallible(ctx), Self::Logistics(d) => d.apply_infallible(ctx) }
            }
        }
        let mut root_tx = StagedRootTransaction::<TxDelta>::begin(&world, TransitionContext::default()).unwrap();
        let body = root_tx.view().world().build_factory_to_event(700, &builder, &site_id, &spec, true).unwrap();
        let (_, consensus, publication) = StagedIndustrialPublication::from_body(&body).unwrap()
            .prepare_event(root_tx.view().world(), root_tx.context()).unwrap();
        root_tx.set_consensus_record(consensus);
        root_tx.stage_delta(TxDelta::Construction(Box::new(FactoryConstructionDelta { publication })));

        let nested_res = root_tx.stage_nested(|tx| {
            let prepared = tx.view().world().prepare_due_material_transits_at(tx.view().time());
            let (_, publication) = tx.view().world().prepare_industrial_batch_publication(&prepared.bodies, tx.context().caused_by.clone())?;
            tx.stage_delta(TxDelta::Logistics(LogisticsDueTransitsDelta {
                publication, sla_metrics: Some(LogisticsSlaMetricsDelta::new(prepared.logistics_sla_metrics)),
            }));
            Err::<(), TransitionReject>(TransitionReject::World(WorldError::ResourceBalanceInvalid { reason: "nested fault".into() }))
        });
        assert!(nested_res.is_err());
        assert_eq!(root_tx.staged_count(), 1);
        root_tx.finish_prepare().install_confirmed(&mut world, DurableCommitConfirmation::Confirmed).unwrap();
        assert!(world.state().pending_factory_builds.contains_key(&700));
    }
}
