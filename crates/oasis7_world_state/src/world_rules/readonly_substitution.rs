//! Original substitution selection over fixed typed readonly facts. No World
//! snapshot, status/verdict callback, authority mint or mutation is involved.
use super::{
    IndustrialOwnerReadonlyFactsV1, IndustrialReceiptReadonlyFactsV1,
    IndustrialStartEnvelopeReadonlyFactsV1, invalid, readonly_industrial_data as data,
    readonly_industrial_start::hash, readonly_substitution_data as copy,
};
use crate::{MaterialLedgerId, WorldError};
use oasis7_agent_api::world_action::*;
use oasis7_wasm_abi::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
    ops::ControlFlow,
};
type Error<S> = <S as IndustrialOwnerReadonlyFactsV1>::Error;
pub trait SubstitutionReadonlyFactsV1:
    Sized
    + IndustrialStartEnvelopeReadonlyFactsV1
    + IndustrialReceiptReadonlyFactsV1<Error = <Self as IndustrialOwnerReadonlyFactsV1>::Error>
{
    fn substitution_metered(&self) -> bool {
        true
    }
    fn substitution_rules_present(&self) -> Result<bool, Error<Self>>;
    fn substitution_with_authority<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&SubstitutionExecutionAuthorityV1>) -> Result<T, Error<Self>>,
    ) -> Result<T, Error<Self>>;
    fn substitution_visit_authorities(
        &self,
        visit: impl FnMut(&SubstitutionExecutionAuthorityV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>>;
    fn substitution_visit_roots(
        &self,
        visit: impl FnMut(&RootOperationV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>>;
    fn substitution_visit_lots(
        &self,
        visit: impl FnMut(&IndustrialLotV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>>;
}
macro_rules! domain {
    ($value:expr) => {
        match $value {
            Ok(value) => value,
            Err(error) => return Ok(Err(error)),
        }
    };
}
fn fail<S: IndustrialOwnerReadonlyFactsV1, T>(
    s: &S,
    note: &str,
) -> Result<Result<T, WorldError>, Error<S>> {
    s.owner_work(1, note.len())?;
    s.owner_before_owned(size_of::<WorldError>() + note.len())?;
    Ok(Err(invalid(note)))
}
fn formatted<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    args: std::fmt::Arguments<'_>,
) -> Result<String, Error<S>> {
    super::format_readonly_projection_data_v1(args, &mut |steps, bytes, heap| {
        s.owner_work(steps, bytes)?;
        if heap == 0 {
            Ok(())
        } else {
            s.owner_before_owned(heap)
        }
    })
}
fn qualified<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &IndustrialRefV1,
    world: &str,
) -> Result<Result<(), WorldError>, Error<S>> {
    for text in [&v.world_binding, &v.domain, &v.schema, &v.id, &v.digest] {
        for part in text.as_bytes().chunks(8192) {
            s.owner_work(1, part.len())?;
        }
    }
    s.owner_work(1, world.len())?;
    s.owner_before_owned(size_of::<WorldError>() + "substitution_reference_invalid".len())?;
    Ok(super::recipe_substitution::qualified(v, world))
}
fn comparison<S: SubstitutionReadonlyFactsV1, T: serde::Serialize + ?Sized>(
    s: &S,
    value: &T,
) -> Result<Result<(), WorldError>, Error<S>> {
    if !s.substitution_metered() {
        return Ok(Ok(()));
    }
    match crate::util::measure_json_with_work_v1(value, &mut |steps, bytes| {
        s.owner_work(steps, bytes as usize)
    }) {
        Ok(n) => {
            s.owner_work(
                (n as u64).saturating_mul(4).saturating_add(2048),
                n.saturating_mul(4),
            )?;
            Ok(Ok(()))
        }
        Err(crate::util::JsonHashWorkErrorV1::Work(error)) => Err(error),
        Err(crate::util::JsonHashWorkErrorV1::Codec(error)) => Ok(Err(WorldError::Serde(
            formatted(s, format_args!("{}", error))?,
        ))),
    }
}
fn binding<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    actor: &str,
) -> Result<Result<String, WorldError>, Error<S>> {
    let owner = super::resolve_industrial_owner_readonly_v1(s, actor)?;
    super::industrial_work_admission::industrial_owner_binding_with_work_v1(
        owner.as_ref(),
        &mut |steps, bytes, heap| {
            s.owner_work(steps, bytes)?;
            if heap == 0 {
                Ok(())
            } else {
                s.owner_before_owned(heap)
            }
        },
    )
}
/// Every selection arm is the original predicate sequence. References are
/// recovered through their real fixed field rows before this function runs.
pub(super) fn candidate<S: SubstitutionReadonlyFactsV1>(
    s: &S,
    request: &SubstitutionPreviewRequestV1,
    reference: &IndustrialRefV1,
    actor: &str,
    world: &str,
    now: u64,
) -> Result<Result<SubstitutionCandidateV1, WorldError>, Error<S>> {
    domain!(qualified(s, &request.root_ref, world)?);
    domain!(qualified(s, reference, world)?);
    s.start_with_root(&request.root_ref.id,|root|{
  let Some(root)=root else{return fail(s,"substitution_root_missing")};domain!(comparison(s,&(&root.root_ref,&request.root_ref))?);if root.root_ref!=request.root_ref{return fail(s,"substitution_root_missing")};let binding=domain!(binding(s,actor)?);domain!(comparison(s,&(root,actor,&binding))?);if root.owner_id!=actor||root.owner_binding_digest!=binding{return fail(s,"owner_mismatch")}
  s.owner_work(1,(request.slot_id.len()+1).saturating_mul(((root.slots.len()+1).ilog2()as usize+2)*11))?;let Some(slot)=root.slots.get(&request.slot_id)else{return fail(s,"substitution_slot_missing")};s.owner_work(1,0)?;if slot.consumed{return fail(s,"substitution_slot_unavailable")}if s.industrial_with_disposition(&root.root_ref.id,|v|Ok(v.is_some()))?{return fail(s,"substitution_slot_unavailable")}
  s.substitution_with_authority(&reference.id,|authority|{
   let Some(authority)=authority else{return fail(s,"missing_authority")};domain!(comparison(s,&(&authority.authority_ref,reference))?);if authority.authority_ref!=*reference{return fail(s,"missing_authority")};domain!(comparison(s,&(authority,root,&request.slot_id,&binding))?);
   if authority.owner_id!=actor||authority.root_ref!=root.root_ref||authority.slot_id!=request.slot_id||authority.expected_binding_revision!=slot.binding_revision||authority.valid_until<now||authority.owner_binding_digest!=binding{return fail(s,"stale_candidate")}
   s.quote_with_factory(&root.factory_id,|factory|{
    let Some(factory)=factory else{return fail(s,"substitution_factory_missing")};s.owner_work(1,factory.builder_agent_id.len()+actor.len())?;
    if factory.builder_agent_id!=actor||factory.durability_ppm<=0{return fail(s,"substitution_factory_stale")}if s.quote_factory_retired(&root.factory_id)?||factory.production.status==crate::FactoryProductionStatus::Paused{return fail(s,"substitution_factory_stale")}
    let factory_digest=domain!(hash(s,factory)?);s.owner_work(1,factory_digest.len()+authority.factory_digest.len())?;if factory_digest!=authority.factory_digest{return fail(s,"substitution_factory_stale")}
    s.start_with_recipe(&root.pinned_recipe_id,|profile|{
     let Some(profile)=profile else{return fail(s,"substitution_profile_unknown")};let digest=domain!(hash(s,profile)?);s.owner_work(1,digest.len()*2+root.pinned_profile_digest.len()+authority.profile_digest.len())?;if digest!=root.pinned_profile_digest||digest!=authority.profile_digest{return fail(s,"substitution_profile_stale")}
     domain!(comparison(s,&profile.substitution_rules)?);if let Err(error)=profile.validate_substitution_rules(){let note=formatted(s,format_args!("{}",error))?;return fail(s,&note)}
     let baseline=domain!(hash(s,&authority.baseline_impacts)?);s.owner_work(1,baseline.len()+root.causal_digest.len())?;if baseline!=root.causal_digest{return fail(s,"substitution_causal_baseline_stale")}
     s.owner_work(1,authority.valuation.unit.len())?;let mut risk_invalid=false;if authority.valuation.unit.is_empty()||authority.valuation.unit.len()>256||authority.valuation.unit.trim()!=authority.valuation.unit||authority.valuation.consumed!=0||authority.risks.is_empty()||authority.risks.len()>32{risk_invalid=true}else{for risk in &authority.risks{s.owner_work(1,risk.len())?;if risk.is_empty()||risk.len()>256||risk.trim()!=risk{risk_invalid=true;break}}}
     if risk_invalid||authority.next_recheck<now||authority.next_recheck>authority.valid_until{return fail(s,"substitution_risk_evidence_invalid")}if authority.duration_ticks!=0{return fail(s,"unsupported_conversion_duration")}
     s.owner_before_owned(size_of::<SubstitutionCandidateV1>())?;let mut result=SubstitutionCandidateV1{candidate_digest:String::new(),authority:copy::authority(s,authority)?,rule:None,kind:SubstitutionCandidateKindV1::SameMaterial,plan:None,native_source_quantity:None,source_material_kind:None,candidate_root:None,cutover_plan:None,cutover_plan_digest:None,recipe_plan:None,causal_equivalent:false,conversion_required:false,source_remaining:0,binding_revision:slot.binding_revision,resource_balances:BTreeMap::new()};
     let selected=match &authority.selection{
      SubstitutionSelectionV1::NewRecipe{candidate_root}=>{
       domain!(qualified(s,candidate_root,world)?);s.start_with_root(&candidate_root.id,|target|{
        let Some(target)=target else{return fail(s,"substitution_candidate_root_missing")};domain!(comparison(s,&(&target.root_ref,candidate_root))?);if target.root_ref!=*candidate_root{return fail(s,"substitution_candidate_root_missing")};s.start_with_recipe(&target.pinned_recipe_id,|target_profile|{
         let Some(target_profile)=target_profile else{return fail(s,"substitution_target_profile_missing")};domain!(comparison(s,&(target,root,&binding,authority))?);
         if target.root_ref==root.root_ref||target.owner_id!=actor||target.owner_binding_digest!=binding||target.factory_id!=root.factory_id||target.pinned_recipe_id==root.pinned_recipe_id||target.pinned_recipe_id!=authority.target_recipe||target.work!=0||target.first_sink||target.started_job_id.is_some(){return fail(s,"substitution_target_recipe_not_applicable")}
         if s.industrial_with_disposition(&target.root_ref.id,|v|Ok(v.is_some()))?{return fail(s,"substitution_target_recipe_not_applicable")}
         let profile_hash=domain!(hash(s,target_profile)?);if target.pinned_profile_digest!=profile_hash{return fail(s,"substitution_target_recipe_not_applicable")}let impacts=domain!(hash(s,&authority.candidate_impacts)?);if target.causal_digest!=impacts||!authority.costs.is_empty()||authority.power_cost!=0{return fail(s,"substitution_target_recipe_not_applicable")}
         let plan=match oasis7_agent_api::world_quote::builtin_recipe_plan_with_work_v1(&target.pinned_recipe_id,target.accepted_batches,&mut|steps,bytes,heap|{s.owner_work(steps,bytes)?;if heap==0{Ok(())}else{s.owner_before_owned(heap)}})?{Some(plan)=>plan,None=>return fail(s,"substitution_target_recipe_unsupported")};
         let mut job=u64::MAX;while s.started_pending(job)?||s.started_settled(job)?{s.owner_work(1,0)?;let Some(next)=job.checked_sub(1)else{return fail(s,"substitution_preview_identity_unavailable")};job=next;}
         let request_id=domain!(hash(s,&(reference,"preview-start"))?);s.owner_before_owned(size_of::<crate::DomainEvent>()+size_of::<IndustrialWorkCommandV1>())?;
         let event=crate::DomainEvent::WorldRuleCommitted{action_id:job,actor_id:data::text(s,actor)?,request_id,command:WorldRuleCommandV1::IndustrialWork{command:Box::new(IndustrialWorkCommandV1::StartRoot{root_ref:data::reference(s,&target.root_ref)?,expected_profile_digest:data::text(s,&target.pinned_profile_digest)?,logistics_route_ids:Vec::new(),logistics_path_ids:Vec::new()})},governance_epoch:0,world_binding:Some(data::text(s,world)?)};
         domain!(super::preview_new_recipe_start_root_readonly_v1(s,&event,now)?);s.owner_work(1,root.allowed_dispositions.len()*size_of::<IndustrialDispositionV1>())?;if !root.allowed_dispositions.contains(&IndustrialDispositionV1::Terminate){return fail(s,"substitution_cutover_disposition_unsupported")}
         let plan_id=domain!(hash(s,&(world,&root.root_ref,&target.root_ref,"substitution-cutover"))?);s.owner_before_owned(size_of::<DispositionPlanV1>())?;
         let plan_ref=domain!(super::industrial_work::industrial_ref_with_work_v1(world,"industrial","disposition/v1",&plan_id,1,&(&root.root_ref,&target.root_ref,&root.causal_digest,&target.causal_digest,"terminate"),&mut|steps,bytes,heap|{s.owner_work(steps,bytes)?;if heap==0{Ok(())}else{s.owner_before_owned(heap)}})?);
         let disposition=DispositionPlanV1{root_ref:data::reference(s,&root.root_ref)?,plan_ref,expected_causal_digest:data::text(s,&root.causal_digest)?,disposition:IndustrialDispositionV1::Terminate};result.cutover_plan_digest=Some(domain!(hash(s,&(&disposition,target))?));result.cutover_plan=Some(disposition);result.kind=SubstitutionCandidateKindV1::NewRecipe;result.candidate_root=Some(data::root(s,target)?);result.recipe_plan=Some(plan);Ok(Ok(()))
        })
       })
      },
      selection=>{
       let Some((source_ref,material))=selection.source()else{return fail(s,"substitution_source_missing")};domain!(qualified(s,source_ref,world)?);
       s.industrial_with_lot(&source_ref.id,|lot|{let Some(lot)=lot else{return fail(s,"substitution_source_missing")};domain!(comparison(s,&(&lot.lot_ref,source_ref))?);if lot.lot_ref!=*source_ref{return fail(s,"substitution_source_missing")};s.industrial_with_lot_authority(&source_ref.id,|metadata|{
        let Some(metadata)=metadata else{return fail(s,"substitution_source_authority_unknown")};domain!(comparison(s,&(metadata,lot,material,factory,authority,slot))?);
        if metadata.expires_at<now||metadata.owner_binding_digest!=binding||metadata.material_profile!=*material||lot.owner_id!=actor||lot.source_ledger!=factory.input_ledger{return fail(s,"substitution_source_not_applicable")}
        if authority.output_material!=slot.material||authority.output_unit!=slot.unit||authority.output_spec!=slot.spec||authority.output_quality!=slot.quality||authority.output_custody!=slot.custody||authority.terminal_capacity<slot.quantity||authority.target_recipe!=root.pinned_recipe_id{return fail(s,"substitution_target_not_applicable")}
        result.source_material_kind=Some(data::text(s,&lot.material)?);
        match selection{
         SubstitutionSelectionV1::SameMaterialSource{..}=>{if *material!=slot.material_profile||lot.material!=slot.material||lot.unit!=slot.unit||lot.spec!=slot.spec||lot.quality!=slot.quality||lot.custody!=slot.custody||lot.remaining<slot.quantity||authority.baseline_impacts!=authority.candidate_impacts||!authority.costs.is_empty()||authority.power_cost!=0{return fail(s,"substitution_source_swap_not_applicable")}result.native_source_quantity=Some(slot.quantity);result.causal_equivalent=true;},
         SubstitutionSelectionV1::ProfileRule{rule,..}=>{
          let mut selected=None;for candidate in &profile.substitution_rules{s.owner_work(1,candidate.rule.id.len()+rule.id.len()+candidate.input_edge.len()+request.slot_id.len())?;domain!(comparison(s,&candidate.rule)?);if candidate.rule==*rule&&candidate.input_edge==request.slot_id{selected=Some(candidate);break}}
          let Some(rule)=selected else{return fail(s,"unsupported_substitution")};domain!(comparison(s,rule)?);
          if rule.required_material!=slot.material_profile||lot.material==slot.material||rule.target_unit!=slot.unit||!rule.allowed_materials.contains(material)||lot.unit!=rule.source_unit||!rule.accepted_specs.contains(&lot.spec)||!rule.accepted_quality.contains(&lot.quality)||!rule.accepted_custody.contains(&lot.custody)||authority.owner_ledger_path_policy!=rule.owner_ledger_path_policy||authority.candidate_impacts!=rule.impacts||!rule.applicable_input_states.contains(&authority.input_state){return fail(s,"not_applicable")}
          if !matches!(rule.conversion,SubstitutionConversionV1::Required{..}){return fail(s,"unsupported_direct_substitution")}
          let key=domain!(hash(s,&(&root.root_ref,&request.slot_id,"substitution_conversion"))?);let domain=data::profile(s,&rule.aggregation_domain)?;
          let n=size_of::<SubstitutionQuantityPlanV1>()+rule.rule.id.len()+rule.rule.version.len()+rule.rule.digest.len()+rule.source_unit.len()+rule.target_unit.len();s.owner_before_owned(n)?;domain!(comparison(s,rule)?);
          let plan=match rule.quantity_plan(domain,key,slot.quantity,&lot.unit,&slot.unit){Ok(plan)=>plan,Err(error)=>{let note=formatted(s,format_args!("{}",error))?;return fail(s,&note)}};
          if lot.remaining<plan.source_required{return fail(s,"resource_unavailable")}result.kind=SubstitutionCandidateKindV1::ProfileSubstitute;result.plan=Some(plan);result.rule=Some(copy::rule(s,rule)?);result.conversion_required=true;result.causal_equivalent=authority.baseline_impacts==authority.candidate_impacts;
         },_=>unreachable!()
        }result.source_remaining=lot.remaining;Ok(Ok(()))
       })})
      }
     };domain!(selected?);
     let ledger=MaterialLedgerId::Agent(data::text(s,actor)?);s.quote_with_ledger(&ledger,|ledger|{
      for(material,amount)in &authority.costs{s.owner_work(1,0)?;let required=match i64::try_from(*amount){Ok(value)=>value,Err(_)=>return fail(s,"substitution_cost_overflow")};s.owner_work(1,(material.len()+1).saturating_mul((ledger.map_or(0,BTreeMap::len)+1).ilog2()as usize*2+4))?;let balance=ledger.and_then(|l|l.get(material)).copied().unwrap_or(0);if required<=0||balance<required{return fail(s,"resource_unavailable")}data::tree_slot::<_,i64>(s,material,result.resource_balances.len())?;result.resource_balances.insert(data::text(s,material)?,balance);}
      let Some(power)=s.quote_agent_electricity(actor)?else{return fail(s,"actor_not_found")};let cost=match i64::try_from(authority.power_cost){Ok(value)=>value,Err(_)=>return fail(s,"substitution_power_overflow")};if cost>power{return fail(s,"resource_unavailable")}data::tree_slot::<_,i64>(s,"@electricity",result.resource_balances.len())?;result.resource_balances.insert(data::text(s,"@electricity")?,power);result.candidate_digest=domain!(hash(s,&result)?);Ok(Ok(result))
     })
    })
   })
  })
 })
}

fn insert_ref<S: SubstitutionReadonlyFactsV1>(
    s: &S,
    set: &mut BTreeSet<IndustrialRefV1>,
    value: &IndustrialRefV1,
) -> Result<Result<(), WorldError>, Error<S>> {
    domain!(comparison(s, value)?);
    let levels = (set.len() + 1).ilog2() as usize + 2;
    let node = 11 * size_of::<IndustrialRefV1>() + 13 * size_of::<usize>() + 128;
    s.owner_work((levels * 11) as u64, node.saturating_mul(levels))?;
    s.owner_before_owned(node.saturating_mul(levels))?;
    set.insert(data::reference(s, value)?);
    Ok(Ok(()))
}
/// Complete original preview, including all three selections, ordered authority
/// set equality and the original post-codec 48KiB business limit.
pub fn preview_substitution_readonly_v1<'source, S: SubstitutionReadonlyFactsV1>(
    s: &'source S,
    actor: &str,
    request: &SubstitutionPreviewRequestV1,
    world: &str,
    now: u64,
) -> Result<Result<SubstitutionPreviewV1, WorldError>, Error<S>> {
    if !s.substitution_rules_present()? {
        return fail(s, "missing_authority");
    }
    domain!(qualified(s, &request.root_ref, world)?);
    s.start_with_root(&request.root_ref.id, |root| {
        let Some(root) = root else {
            return fail(s, "substitution_root_missing_or_unauthorized");
        };
        domain!(comparison(
            s,
            &(&root.root_ref, &request.root_ref, &root.owner_id, actor)
        )?);
        if root.root_ref != request.root_ref || root.owner_id != actor {
            return fail(s, "substitution_root_missing_or_unauthorized");
        }
        let current_binding = domain!(binding(s, actor)?);
        s.owner_work(1, root.owner_binding_digest.len() + current_binding.len())?;
        if root.owner_binding_digest != current_binding {
            return fail(s, "substitution_root_owner_stale");
        }
        s.owner_work(
            1,
            (request.slot_id.len() + 1)
                .saturating_mul(((root.slots.len() + 1).ilog2() as usize + 2) * 11),
        )?;
        if !root.slots.contains_key(&request.slot_id) {
            return fail(s, "substitution_slot_missing");
        }
        let mut expected = BTreeSet::new();
        let mut source_domain_error = None;
        s.substitution_visit_authorities(|authority| {
            if let Err(error) = comparison(
                s,
                &(&authority.root_ref, &authority.slot_id, &authority.owner_id),
            )? {
                source_domain_error = Some(error);
                return Ok(ControlFlow::Break(()));
            }
            if authority.root_ref == request.root_ref
                && authority.slot_id == request.slot_id
                && authority.owner_id == actor
            {
                match insert_ref(s, &mut expected, &authority.authority_ref)? {
                    Ok(()) => {}
                    Err(error) => {
                        source_domain_error = Some(error);
                        return Ok(ControlFlow::Break(()));
                    }
                }
            }
            Ok(ControlFlow::Continue(()))
        })?;
        if let Some(error) = source_domain_error {
            return Ok(Err(error));
        }
        let mut supplied = BTreeSet::new();
        for reference in &request.authority_refs {
            s.owner_work(1, 0)?;
            domain!(insert_ref(s, &mut supplied, reference)?);
        }
        domain!(comparison(s, &(&expected, &supplied))?);
        if expected.len() > 32
            || (!supplied.is_empty()
                && (supplied != expected || supplied.len() != request.authority_refs.len()))
        {
            return fail(s, "substitution_incomplete_authority_set");
        }
        s.owner_before_owned(
            size_of::<Vec<SubstitutionCandidateV1>>()
                + expected
                    .len()
                    .checked_mul(size_of::<SubstitutionCandidateV1>())
                    .expect("substitution candidates layout"),
        )?;
        let mut candidates = Vec::with_capacity(expected.len());
        assert_eq!(
            candidates.capacity(),
            expected.len(),
            "substitution candidates exact capacity"
        );
        let mut blocker = None;
        for reference in &expected {
            s.owner_work(1, 0)?;
            match candidate(s, request, reference, actor, world, now)? {
                Ok(value) => candidates.push(value),
                Err(error) => blocker = Some(formatted(s, format_args!("{error:?}"))?),
            }
        }
        if expected.is_empty() {
            blocker = Some(data::text(s, "missing_authority")?)
        }
        let no_rules = s.start_with_recipe(&root.pinned_recipe_id, |profile| {
            let Some(profile) = profile else {
                return Ok(Ok(false));
            };
            s.owner_work(1, 0)?;
            if !profile.substitution_rules.is_empty() {
                return Ok(Ok(false));
            }
            match hash(s, profile)? {
                Ok(digest) => {
                    s.owner_work(1, digest.len() + root.pinned_profile_digest.len())?;
                    Ok(Ok(digest == root.pinned_profile_digest))
                }
                Err(_) => Ok(Ok(false)),
            }
        })?;
        let no_rules = domain!(no_rules);
        let status = if !candidates.is_empty() {
            SubstitutionPreviewStatusV1::Available
        } else {
            let mut possible_source = false;
            s.substitution_visit_lots(|lot| {
                s.owner_work(1, lot.owner_id.len() + actor.len())?;
                if lot.owner_id == actor && lot.remaining > 0 {
                    s.owner_work(
                        1,
                        (request.slot_id.len() + 1)
                            .saturating_mul(((root.slots.len() + 1).ilog2() as usize + 2) * 11),
                    )?;
                    if let Some(slot) = root.slots.get(&request.slot_id) {
                        s.owner_work(1, slot.material.len() + lot.material.len())?;
                        if slot.material == lot.material {
                            possible_source = true;
                            return Ok(ControlFlow::Break(()));
                        }
                    }
                }
                Ok(ControlFlow::Continue(()))
            })?;
            let mut possible_recipe = false;
            let mut root_error = None;
            s.substitution_visit_roots(|other| {
                if let Err(error) = comparison(
                    s,
                    &(
                        &other.root_ref,
                        &root.root_ref,
                        &other.owner_id,
                        &other.factory_id,
                    ),
                )? {
                    root_error = Some(error);
                    return Ok(ControlFlow::Break(()));
                }
                if other.root_ref != root.root_ref
                    && other.owner_id == actor
                    && other.factory_id == root.factory_id
                    && other.work == 0
                    && !other.first_sink
                {
                    possible_recipe = true;
                    return Ok(ControlFlow::Break(()));
                }
                Ok(ControlFlow::Continue(()))
            })?;
            if let Some(error) = root_error {
                return Ok(Err(error));
            }
            if no_rules && expected.is_empty() && !possible_source && !possible_recipe {
                blocker = None;
                SubstitutionPreviewStatusV1::NoLegalSubstitute
            } else {
                SubstitutionPreviewStatusV1::Unknown
            }
        };
        s.owner_before_owned(size_of::<SubstitutionPreviewV1>())?;
        let mut recheck = now;
        if let Some(first) = candidates.first() {
            recheck = first.authority.next_recheck;
            for candidate in candidates.iter().skip(1) {
                s.owner_work(1, 0)?;
                recheck = recheck.min(candidate.authority.next_recheck)
            }
        }
        let mut preview = SubstitutionPreviewV1 {
            preview_digest: String::new(),
            status,
            blocker,
            next_action: data::text(
                s,
                if candidates.is_empty() {
                    "resolve_blocker"
                } else {
                    "explicit_confirmation"
                },
            )?,
            next_recheck: recheck,
            request: copy::request(s, request)?,
            candidates,
            evaluated_at: now,
        };
        preview.preview_digest = domain!(hash(s, &preview)?);
        let len = match crate::util::measure_json_with_work_v1(&preview, &mut |steps, bytes| {
            s.owner_work(steps, bytes as usize)
        }) {
            Ok(n) => n,
            Err(crate::util::JsonHashWorkErrorV1::Work(error)) => return Err(error),
            Err(crate::util::JsonHashWorkErrorV1::Codec(error)) => {
                return Ok(Err(WorldError::Serde(formatted(
                    s,
                    format_args!("{}", error),
                )?)));
            }
        };
        if len > 48 * 1024 {
            return fail(s, "substitution_preview_capacity_exceeded");
        }
        Ok(Ok(preview))
    })
}
