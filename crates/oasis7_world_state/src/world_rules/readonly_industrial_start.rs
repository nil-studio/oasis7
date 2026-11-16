//! Original StartRoot raw-fact admission and sequential escrow consumption.
//! The result is a local rehearsal DATA graph, never a PreparedWorldRule.
use super::{
    invalid, readonly_industrial_consume::*, readonly_industrial_data as data,
    readonly_industrial_scratch::*, readonly_manufacturing::ManufacturingQuoteReadonlyFactsV1,
    readonly_owner::*, readonly_parent_consumption as parents,
};
use crate::{IndustryProgressState, MaterialLedgerId, WorldError, WorldState};
use oasis7_agent_api::world_action::*;
use oasis7_wasm_abi::{RecipeExecutionPlan, RecipeProfileV1};
use std::{collections::BTreeMap, mem::size_of, ops::ControlFlow};
pub trait IndustrialStartReadonlyFactsV1:
    ManufacturingQuoteReadonlyFactsV1
    + IndustrialProductReadonlyFactsV1
    + IndustrialScratchReadonlyFactsV1
{
    fn start_with_root<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&RootOperationV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
    fn start_visit_jobs(
        &self,
        visit: impl FnMut(&crate::RecipeJobState) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error>;
    fn start_with_recipe<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&RecipeProfileV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
    fn start_with_progress<T>(
        &self,
        visit: impl FnOnce(&IndustryProgressState) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
}
pub(super) struct StartInputDataV1 {
    pub root: RootOperationV1,
    pub plan: RecipeExecutionPlan,
    pub consume: Vec<crate::MaterialStack>,
    pub parents: Vec<IndustrialParentV1>,
}
macro_rules! domain {
    ($v:expr) => {
        match $v {
            Ok(value) => value,
            Err(error) => return Ok(Err(error)),
        }
    };
}
fn refusal<S: IndustrialOwnerReadonlyFactsV1, T>(
    s: &S,
    reason: &str,
) -> Result<Result<T, WorldError>, S::Error> {
    s.owner_work(1, reason.len())?;
    s.owner_before_owned(reason.len() + size_of::<WorldError>())?;
    Ok(Err(invalid(reason)))
}
pub(super) fn hash<S: IndustrialOwnerReadonlyFactsV1, T: serde::Serialize + ?Sized>(
    s: &S,
    value: &T,
) -> Result<Result<String, WorldError>, S::Error> {
    s.owner_before_owned(64 + size_of::<String>())?;
    match crate::util::hash_json_with_work_v1(value, &mut |steps, bytes| {
        s.owner_work(steps, bytes as usize)
    }) {
        Ok(v) => Ok(Ok(v)),
        Err(crate::util::JsonHashWorkErrorV1::Work(e)) => Err(e),
        Err(crate::util::JsonHashWorkErrorV1::Codec(e)) => {
            let text = super::readonly_format::format(
                format_args!("{}", e),
                &mut |steps, bytes, heap| {
                    s.owner_work(steps, bytes)?;
                    s.owner_before_owned(heap)
                },
            )?;
            Ok(Err(WorldError::Serde(text)))
        }
    }
}
fn ledger<S: IndustrialStartReadonlyFactsV1>(
    s: &S,
    actor: &str,
    id: &MaterialLedgerId,
    now: u64,
) -> Result<Result<(), WorldError>, S::Error> {
    let check = |factory: Option<&crate::FactoryState>,
                 retired,
                 site: Option<&crate::FactorySiteAuthorityV1>| {
        s.owner_work(
            1,
            actor.len()
                + factory.map_or(0, |f| f.builder_agent_id.len())
                + site.map_or(0, |v| v.owner_agent_id.len()),
        )?;
        if let Some(site) = site {
            for id in &site.authorized_agent_ids {
                s.owner_work(1, id.len() + actor.len())?;
            }
        }
        s.owner_before_owned(size_of::<WorldError>() + "industrial_ledger_access_denied".len())?;
        Ok(
            super::industrial_work_admission::industrial_ledger_access_from_records_v1(
                actor, id, now, factory, retired, site,
            ),
        )
    };
    match id {
        MaterialLedgerId::Factory(id) => s.quote_with_factory(id, |factory| match factory {
            Some(f) => s.quote_with_site(&f.site_id, |site| {
                let retired = s.quote_factory_retired(id)?;
                check(Some(f), retired, site)
            }),
            None => {
                let retired = s.quote_factory_retired(id)?;
                check(None, retired, None)
            }
        }),
        MaterialLedgerId::Site(id) => s.quote_with_site(id, |site| check(None, false, site)),
        _ => check(None, false, None),
    }
}
fn capacity<S: IndustrialStartReadonlyFactsV1>(
    s: &S,
    root: &RootOperationV1,
    plan: &RecipeExecutionPlan,
) -> Result<Result<(), WorldError>, S::Error> {
    s.quote_with_ledger(&root.output_ledger, |ledger| {
        super::readonly_output_capacity::capacity(
            root,
            plan,
            None,
            ledger,
            &mut |visit| {
                s.start_visit_jobs(|job| {
                    visit(super::industrial_work_start::IndustrialOutputReservationRefV1::from(job))
                })
            },
            &mut |steps, bytes, heap| {
                s.owner_work(steps, bytes)?;
                if heap == 0 {
                    Ok(())
                } else {
                    s.owner_before_owned(heap)
                }
            },
        )
    })
}
pub(super) trait StartRowLoaderV1<S: IndustrialStartReadonlyFactsV1> {
    fn lot(&self, s: &S, scratch: &mut IndustrialWorkStateV1, id: &str) -> Result<(), S::Error>;
    fn authority(
        &self,
        s: &S,
        scratch: &mut IndustrialWorkStateV1,
        id: &str,
    ) -> Result<(), S::Error>;
}
pub(super) struct ResidentStartRowsV1;
impl<S: IndustrialStartReadonlyFactsV1> StartRowLoaderV1<S> for ResidentStartRowsV1 {
    fn lot(&self, _: &S, _: &mut IndustrialWorkStateV1, _: &str) -> Result<(), S::Error> {
        Ok(())
    }
    fn authority(&self, _: &S, _: &mut IndustrialWorkStateV1, _: &str) -> Result<(), S::Error> {
        Ok(())
    }
}
pub(super) struct SelectedStartRowsV1;
impl<S: IndustrialStartReadonlyFactsV1> StartRowLoaderV1<S> for SelectedStartRowsV1 {
    fn lot(&self, s: &S, scratch: &mut IndustrialWorkStateV1, id: &str) -> Result<(), S::Error> {
        s.owner_work(
            1,
            (id.len() + 1).saturating_mul((scratch.lots.len() + 1).ilog2() as usize * 2 + 4),
        )?;
        if !scratch.lots.contains_key(id) {
            s.industrial_with_lot(id, |value| {
                if let Some(value) = value {
                    data::tree_slot::<_, IndustrialLotV1>(s, id, scratch.lots.len())?;
                    scratch
                        .lots
                        .insert(data::text(s, id)?, data::lot(s, value)?);
                }
                Ok(())
            })?;
        }
        Ok(())
    }
    fn authority(
        &self,
        s: &S,
        scratch: &mut IndustrialWorkStateV1,
        id: &str,
    ) -> Result<(), S::Error> {
        s.owner_work(
            1,
            (id.len() + 1).saturating_mul(
                (scratch.imported_lot_authorities.len() + 1).ilog2() as usize * 2 + 4,
            ),
        )?;
        if !scratch.imported_lot_authorities.contains_key(id) {
            s.industrial_with_lot_authority(id, |value| {
                if let Some(value) = value {
                    data::tree_slot::<_, IndustrialLotAuthorityV1>(
                        s,
                        id,
                        scratch.imported_lot_authorities.len(),
                    )?;
                    scratch
                        .imported_lot_authorities
                        .insert(data::text(s, id)?, data::authority(s, value)?);
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}
/// Source loading occurs at the original per-input read point. Legacy passes
/// its existing staged maps; Current loads only these selected parent records.
pub(super) fn prepare_inputs<
    S: IndustrialStartReadonlyFactsV1,
    W: parents::ParentWorkV1<Error = S::Error>,
>(
    s: &S,
    loader: &impl StartRowLoaderV1<S>,
    parent_work: &W,
    scratch: &mut IndustrialWorkStateV1,
    actor: &str,
    root_ref: &IndustrialRefV1,
    expected_profile_digest: &str,
    now: u64,
    world: &str,
) -> Result<Result<StartInputDataV1, WorldError>, S::Error> {
    let mut charge = |steps, bytes, heap| {
        s.owner_work(steps, bytes)?;
        if heap == 0 {
            Ok(())
        } else {
            s.owner_before_owned(heap)
        }
    };
    s.owner_work(
        1,
        (root_ref.id.len() + 1).saturating_mul((scratch.roots.len() + 1).ilog2() as usize * 2 + 4),
    )?;
    let root = domain!(super::readonly_start_gates::root(
        scratch.roots.get(&root_ref.id),
        root_ref,
        actor,
        world,
        &mut charge
    )?);
    let root = data::root(s, root)?;
    let recipe_id = data::text(s, &root.pinned_recipe_id)?;
    s.start_with_recipe(&recipe_id, |profile| {
        let Some(profile) = profile else {
            return refusal(s, "industrial_recipe_profile_missing");
        };
        s.owner_work(
            1,
            root.pinned_profile_digest.len() + expected_profile_digest.len(),
        )?;
        if root.pinned_profile_digest != expected_profile_digest {
            return refusal(s, "industrial_start_profile_stale");
        }
        let profile_digest = domain!(hash(s, profile)?);
        s.owner_work(1, root.pinned_profile_digest.len() + profile_digest.len())?;
        if root.pinned_profile_digest != profile_digest {
            return refusal(s, "industrial_start_profile_stale");
        }
        let factory_id = data::text(s, &root.factory_id)?;
        s.quote_with_factory(&factory_id, |factory| {
            let Some(factory) = factory else {
                return refusal(s, "industrial_start_factory_unavailable");
            };
            s.owner_work(1, factory.builder_agent_id.len() + actor.len())?;
            // Preserve && short-circuit: retired source is not read for wrong owner or
            // destroyed factory, and paused is tested only after the retired lookup.
            if factory.builder_agent_id != actor || factory.durability_ppm == 0 {
                return refusal(s, "industrial_start_factory_unavailable");
            }
            if s.quote_factory_retired(&root.factory_id)?
                || factory.production.status == crate::FactoryProductionStatus::Paused
            {
                return refusal(s, "industrial_start_factory_unavailable");
            }
            s.owner_work(1, size_of::<MaterialLedgerId>())?;
            if root.output_ledger != factory.output_ledger {
                return refusal(s, "industrial_start_output_destination_drifted");
            }
            domain!(ledger(s, actor, &factory.input_ledger, now)?);
            domain!(ledger(s, actor, &factory.output_ledger, now)?);
            let stage = s.start_with_progress(|progress| Ok(progress.stage))?;
            s.owner_work(1, profile.stage_gate.len())?;
            if !crate::world::recipe_stage_gate_allowed(stage, &profile.stage_gate) {
                return refusal(s, "industrial_start_recipe_ineligible");
            }
            if !super::readonly_recipe_tags::compatible(
                &profile.preferred_factory_tags,
                &factory.spec.tags,
                &mut charge,
            )? {
                return refusal(s, "industrial_start_recipe_ineligible");
            }
            let Some(plan) = oasis7_agent_api::world_quote::builtin_recipe_plan_with_work_v1(
                &root.pinned_recipe_id,
                root.accepted_batches,
                &mut charge,
            )?
            else {
                return refusal(s, "industrial_start_recipe_unsupported");
            };
            for output in plan.produce.iter().chain(&plan.byproducts) {
                s.owner_work(1, 0)?;
                let locked = s.industrial_with_product(&output.kind, |product| {
                    if let Some(product) = product {
                        s.owner_work(1, product.unlock_stage.len())?;
                        Ok(!crate::world::recipe_stage_gate_allowed(
                            stage,
                            &product.unlock_stage,
                        ))
                    } else {
                        Ok(false)
                    }
                })?;
                if locked {
                    return refusal(s, "industrial_start_product_locked");
                }
            }
            domain!(capacity(s, &root, &plan)?);
            let consume = domain!(industrial_effective_consume_readonly_v1(s, &plan)?);
            if consume.len() != root.slots.len() {
                return refusal(s, "industrial_start_inputs_drifted");
            }
            let owner = resolve_industrial_owner_readonly_v1(s, actor)?;
            let binding = domain!(
                super::industrial_work_admission::industrial_owner_binding_with_work_v1(
                    owner.as_ref(),
                    &mut charge
                )?
            );
            s.owner_work(1, root.owner_binding_digest.len() + binding.len())?;
            if root.owner_binding_digest != binding {
                return refusal(s, "industrial_start_root_owner_stale");
            }
            let mut collected = Vec::new();
            for input in &consume {
                s.owner_work(
                    1,
                    (input.kind.len() + 1)
                        .saturating_mul((root.slots.len() + 1).ilog2() as usize * 2 + 4),
                )?;
                let slot = root.slots.get(&input.kind);
                if let Some(slot) = slot {
                    s.owner_work(1, slot.material.len() + input.kind.len())?
                }
                let Some(slot) = slot.filter(|slot| {
                    slot.material == input.kind
                        && i64::try_from(slot.quantity).ok() == Some(input.amount)
                        && !slot.consumed
                        && !slot.parents.is_empty()
                }) else {
                    return refusal(s, "industrial_start_inputs_incomplete");
                };
                for parent in &slot.parents {
                    loader.lot(s, scratch, &parent.lot_ref.id)?;
                    let lot = domain!(super::readonly_start_gates::lot(
                        scratch.lots.get(&parent.lot_ref.id),
                        parent,
                        &factory.input_ledger,
                        &mut charge
                    )?);
                    // The selected lot's authority key is its actual verified original ID.
                    let id = data::text(s, &lot.lot_ref.id)?;
                    loader.authority(s, scratch, &id)?;
                    domain!(super::readonly_start_gates::authority(
                        scratch.imported_lot_authorities.get(&id),
                        slot,
                        &binding,
                        now,
                        &mut charge
                    )?);
                }
                let selected = domain!(parents::consume_bound_slot(
                    parent_work,
                    scratch,
                    root_ref,
                    &input.kind,
                    world
                )?);
                let n = collected
                    .len()
                    .checked_add(selected.len())
                    .expect("bounded industrial parents");
                s.owner_before_owned(n.saturating_mul(size_of::<IndustrialParentV1>()))?;
                s.owner_work(
                    1,
                    collected
                        .len()
                        .saturating_mul(size_of::<IndustrialParentV1>()),
                )?;
                let mut next = Vec::with_capacity(n);
                assert_eq!(next.capacity(), n, "industrial parents exact capacity");
                next.append(&mut collected);
                for parent in selected {
                    s.owner_work(1, size_of::<IndustrialParentV1>())?;
                    next.push(parent)
                }
                collected = next;
            }
            Ok(Ok(StartInputDataV1 {
                root,
                plan,
                consume,
                parents: collected,
            }))
        })
    })
}
impl IndustrialStartReadonlyFactsV1 for WorldState {
    fn start_with_root<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&RootOperationV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        visit(
            self.world_rules
                .as_ref()
                .and_then(|rules| rules.industrial_work.roots.get(id)),
        )
    }
    fn start_visit_jobs(
        &self,
        mut visit: impl FnMut(&crate::RecipeJobState) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        for job in self.pending_recipe_jobs.values() {
            if visit(job)?.is_break() {
                break;
            }
        }
        Ok(())
    }
    fn start_with_recipe<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&RecipeProfileV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        visit(self.recipe_profiles.get(id))
    }
    fn start_with_progress<T>(
        &self,
        visit: impl FnOnce(&IndustryProgressState) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        visit(&self.industry_progress)
    }
}

/// An owned, selected rehearsal; no public fields or conversion to a staged
/// mutation/permission. Its allocations remain charged to the source owner.
pub struct IndustrialStartInputPreviewDataV1<'source> {
    selected: StartInputDataV1,
    scratch: IndustrialWorkStateV1,
    _source: std::marker::PhantomData<&'source ()>,
}
impl IndustrialStartInputPreviewDataV1<'_> {
    pub fn root(&self) -> &RootOperationV1 {
        &self.selected.root
    }
    pub fn plan(&self) -> &RecipeExecutionPlan {
        &self.selected.plan
    }
    pub fn consume(&self) -> &[crate::MaterialStack] {
        &self.selected.consume
    }
    pub fn parents(&self) -> &[IndustrialParentV1] {
        &self.selected.parents
    }
    pub fn selected_work(&self) -> &IndustrialWorkStateV1 {
        &self.scratch
    }
}
pub fn preview_industrial_start_inputs_readonly_v1<'source, S: IndustrialStartReadonlyFactsV1>(
    s: &'source S,
    actor: &str,
    root_ref: &IndustrialRefV1,
    expected_profile_digest: &str,
    now: u64,
    world: &str,
) -> Result<Result<IndustrialStartInputPreviewDataV1<'source>, WorldError>, S::Error> {
    s.start_with_root(&root_ref.id, |value| {
        let mut charge = |steps, bytes, heap| {
            s.owner_work(steps, bytes)?;
            if heap == 0 {
                Ok(())
            } else {
                s.owner_before_owned(heap)
            }
        };
        let root = domain!(super::readonly_start_gates::root(
            value,
            root_ref,
            actor,
            world,
            &mut charge
        )?);
        s.owner_before_owned(
            size_of::<IndustrialStartInputPreviewDataV1<'_>>() + size_of::<IndustrialWorkStateV1>(),
        )?;
        let mut scratch = IndustrialWorkStateV1::default();
        data::tree_slot::<_, RootOperationV1>(s, &root_ref.id, 0)?;
        scratch
            .roots
            .insert(data::text(s, &root_ref.id)?, data::root(s, root)?);
        let selected = domain!(prepare_inputs(
            s,
            &SelectedStartRowsV1,
            &parents::PaidParentWorkV1(s),
            &mut scratch,
            actor,
            root_ref,
            expected_profile_digest,
            now,
            world
        )?);
        Ok(Ok(IndustrialStartInputPreviewDataV1 {
            selected,
            scratch,
            _source: std::marker::PhantomData,
        }))
    })
}
fn stacks<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    values: &[crate::MaterialStack],
) -> Result<Vec<crate::MaterialStack>, S::Error> {
    s.owner_before_owned(
        values
            .len()
            .saturating_mul(size_of::<crate::MaterialStack>()),
    )?;
    let mut out = Vec::with_capacity(values.len());
    assert_eq!(
        out.capacity(),
        values.len(),
        "preview material exact capacity"
    );
    for value in values {
        s.owner_work(1, 0)?;
        out.push(crate::MaterialStack::new(
            data::text(s, &value.kind)?,
            value.amount,
        ));
    }
    Ok(out)
}
pub struct IndustrialStartRecipePreviewDataV1<'source> {
    event: crate::DomainEvent,
    _source: std::marker::PhantomData<&'source ()>,
}
impl IndustrialStartRecipePreviewDataV1<'_> {
    pub fn event(&self) -> &crate::DomainEvent {
        &self.event
    }
}
/// Continue the actual selected StartRoot input result through escrow credit,
/// ready-time checks and the shared ordinary RecipeStarted admission. It does
/// not cover the outer request replay/receipt gates, nor mint a prepared event.
pub fn preview_industrial_start_recipe_readonly_v1<
    'source,
    S: IndustrialStartReadonlyFactsV1 + super::RecipeStartedReadonlyFactsV1,
>(
    s: &'source S,
    selected: &IndustrialStartInputPreviewDataV1<'source>,
    actor: &str,
    job_id: u64,
    now: u64,
) -> Result<Result<IndustrialStartRecipePreviewDataV1<'source>, WorldError>, S::Error> {
    let value = &selected.selected;
    s.quote_with_factory(&value.root.factory_id, |factory| {
        // The fixed source is immutable across the selected input and this callback;
        // recheck the raw presence rather than fabricating a factory projection.
        let Some(factory) = factory else {
            return refusal(s, "industrial_start_factory_unavailable");
        };
        let mut credits = BTreeMap::new();
        for input in &value.consume {
            s.owner_work(1, 0)?;
            let old = s.started_material(&factory.input_ledger, &input.kind)?;
            let Some(credited) = old.checked_add(input.amount) else {
                return refusal(s, "industrial_start_quantity_overflow");
            };
            data::tree_slot::<_, i64>(s, &input.kind, credits.len())?;
            credits.insert(data::text(s, &input.kind)?, credited);
        }
        let Some(ready_at) = now.checked_add(u64::from(value.plan.duration_ticks)) else {
            return refusal(s, "industrial_start_time_overflow");
        };
        let recipe = data::text(s, &value.root.pinned_recipe_id)?;
        s.start_with_recipe(&recipe, |profile| {
            let Some(profile) = profile else {
                return refusal(s, "industrial_recipe_profile_missing");
            };
            s.owner_before_owned(size_of::<IndustrialStartRecipePreviewDataV1<'source>>())?;
            s.owner_before_owned(
                profile
                    .bottleneck_tags
                    .len()
                    .saturating_mul(size_of::<String>()),
            )?;
            let mut tags = Vec::with_capacity(profile.bottleneck_tags.len());
            assert_eq!(
                tags.capacity(),
                profile.bottleneck_tags.len(),
                "preview tags exact capacity"
            );
            for tag in &profile.bottleneck_tags {
                s.owner_work(1, 0)?;
                tags.push(data::text(s, tag)?);
            }
            let event = crate::DomainEvent::RecipeStarted {
                job_id,
                requester_agent_id: data::text(s, actor)?,
                factory_id: data::text(s, &value.root.factory_id)?,
                recipe_id: data::text(s, &value.root.pinned_recipe_id)?,
                accepted_batches: value.plan.accepted_batches,
                consume: stacks(s, &value.consume)?,
                produce: stacks(s, &value.plan.produce)?,
                byproducts: stacks(s, &value.plan.byproducts)?,
                power_required: value.plan.power_required,
                power_owner_agent_id: Some(data::text(s, actor)?),
                duration_ticks: value.plan.duration_ticks,
                consume_ledger: data::ledger(s, &factory.input_ledger)?,
                output_ledger: data::ledger(s, &factory.output_ledger)?,
                bottleneck_tags: tags,
                market_quotes: Vec::new(),
                logistics_route_ids: Vec::new(),
                logistics_path_ids: Vec::new(),
                ready_at,
                emergency_power_obligation: None,
                committed_recipe_origin: None,
            };
            let credited = super::readonly_start_recipe_overlay::EscrowCreditFactsV1 {
                source: s,
                ledger: &factory.input_ledger,
                credits: &credits,
            };
            domain!(
                super::readonly_recipe_started::factory_capacity_reservation_gate_v1(
                    &credited, &event
                )?
            );
            let prefix = domain!(super::readonly_recipe_started::admission_prefix(
                &credited, &event, now
            )?);
            domain!(super::readonly_recipe_started::ordinary_power(
                s,
                prefix.electricity,
                value.plan.power_required
            )?);
            s.owner_work(
                1,
                value.root.root_ref.world_binding.len()
                    + value.root.root_ref.domain.len()
                    + value.root.root_ref.schema.len()
                    + value.root.root_ref.id.len()
                    + value.root.root_ref.digest.len(),
            )?;
            if !value.root.root_ref.is_qualified() {
                return refusal(s, "industrial_job_root_invalid");
            }
            Ok(Ok(IndustrialStartRecipePreviewDataV1 {
                event,
                _source: std::marker::PhantomData,
            }))
        })
    })
}
