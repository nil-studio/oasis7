//! Fixed real default-context recipe binding over the actual native ledger.
//! This retained DATA does not install a decision or permit native execution.
use super::*;
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamError, CapabilityCanonicalWorkV1,
};
struct Canonical<'v, 'f, 'a, 'c, 'k> {
    view: &'v values::View<'f, 'a, 'c>,
    work: RefCell<&'k mut oasis7_wasm_abi::CanonicalCborWorkV1>,
    first: RefCell<Option<BodyMutationError>>,
}
impl CanonicalCborAllocator for Canonical<'_, '_, '_, '_, '_> {
    type Charge = MemoryReservation;
    fn charge_work(&self, s: u64, b: u64) -> Result<(), CanonicalCborStreamError> {
        if self.first.borrow().is_some() {
            return Err(CanonicalCborStreamError::Poisoned);
        }
        self.view
            .facts
            .work(
                s,
                usize::try_from(b).map_err(|_| {
                    CanonicalCborStreamError::ResourceLimited("native quote work bytes")
                })?,
            )
            .map_err(|error| {
                *self.first.borrow_mut() = Some(error);
                CanonicalCborStreamError::Poisoned
            })
    }
    fn reserve(&self, n: usize) -> Result<Self::Charge, CanonicalCborStreamError> {
        if self.first.borrow().is_some() {
            return Err(CanonicalCborStreamError::Poisoned);
        }
        self.view
            .facts
            .common
            .borrow()
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(|error| {
                *self.first.borrow_mut() = Some(ValueTreeError::Memory(error).into());
                CanonicalCborStreamError::Poisoned
            })
    }
}
impl CapabilityCanonicalWorkV1 for Canonical<'_, '_, '_, '_, '_> {
    type Loan<'x>
        = std::cell::RefMut<'x, oasis7_wasm_abi::CanonicalCborWorkV1>
    where
        Self: 'x;
    fn is_metered(&self) -> bool {
        true
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        std::cell::RefMut::map(self.work.borrow_mut(), |work| &mut **work)
    }
    fn record_stream_error(&self, error: CanonicalCborStreamError) -> CanonicalCborStreamError {
        if self.first.borrow().is_none() {
            *self.first.borrow_mut() = Some(paid(error))
        }
        CanonicalCborStreamError::Poisoned
    }
}
pub(in crate::world) struct PaidCurrentBuiltinRecipeQuoteDataV1 {
    result: Result<crate::world::builtin_recipe_quote::NativeBuiltinRecipeQuote, crate::WorldError>,
    fees: CurrentProjectionFactsFeesV1,
}
impl PaidCurrentBuiltinRecipeQuoteDataV1 {
    pub(in crate::world) fn result(
        &self,
    ) -> Result<&crate::world::builtin_recipe_quote::NativeBuiltinRecipeQuote, &crate::WorldError>
    {
        self.result.as_ref()
    }
    pub(in crate::world) fn into_parts_v1(
        self,
    ) -> (
        Result<crate::world::builtin_recipe_quote::NativeBuiltinRecipeQuote, crate::WorldError>,
        CurrentProjectionFactsFeesV1,
    ) {
        (self.result, self.fees)
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_current_builtin_recipe_quote_data_v1(
    state: &OriginalStateColumnsDataV1,
    electricity: &OriginalProjectionRowsDataV1,
    aux: &ProjectionColumnDataV1,
    metadata: &OriginalProjectionReadMetadataV1<'_>,
    raw_state: &raw_hash::PaidOriginalStateHashDataV1,
    binding: Option<&ReservedValue<oasis7_agent_api::RuntimeBindingV1>>,
    agent: &str,
    factory: &str,
    recipe: &str,
    batches: u32,
    origin: &dyn crate::world::incremental_normal_projection_commitment::facts::ProjectionReadSourceV1,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
) -> Result<PaidCurrentBuiltinRecipeQuoteDataV1, BodyMutationError> {
    raw_state.validate_context(common)?;
    let facts = CurrentProjectionReadFactsV1::for_projection_data_v1(
        state,
        electricity,
        metadata,
        origin,
        common,
    )?;
    facts.before_owned(size_of::<PaidCurrentBuiltinRecipeQuoteDataV1>() + 1024)?;
    let view = values::View::new(&facts, aux)?;
    let digest = Canonical {
        view: &view,
        work: RefCell::new(canonical),
        first: RefCell::new(None),
    };
    let mut stream_error = |error| {
        NativeError::Body(
            digest
                .first
                .borrow_mut()
                .take()
                .unwrap_or_else(|| paid(error)),
        )
    };
    let result = (|| -> Result<crate::world::builtin_recipe_quote::NativeBuiltinRecipeQuote, NativeError> {
        let plan = oasis7_agent_api::world_quote::builtin_recipe_plan_with_work_v1(
            recipe,
            batches,
            &mut |s, b, h| {
                facts.work(s, b)?;
                if h > 0 {
                    facts.before_owned(h)?
                }
                Ok::<_, NativeError>(())
            },
        )?
        .ok_or_else(|| crate::WorldError::DistributedValidationFailed {
            reason: "unsupported builtin recipe selection".into(),
        })?;
        let action = crate::Action::ScheduleRecipe {
            requester_agent_id: view.copy(agent)?,
            factory_id: view.copy(factory)?,
            recipe_id: view.copy(recipe)?,
            plan,
            logistics_route_ids: vec![],
            logistics_path_ids: vec![],
        };
        view.required(&["cognition"])?;
        let ledger: Ledger = match view.optional(&["cognition", "agent_delegation"])? {
            None => {
                facts.before_owned(size_of::<Ledger>())?;
                Ledger::default()
            }
            Some(row) if view.is_null(&row)? => {
                facts.before_owned(size_of::<Ledger>())?;
                Ledger::default()
            }
            Some(row) => view.decode(&row)?.map_err(crate::WorldError::from)?,
        };
        let mut ops = Ops(&view);
        let decision = readonly::build_default_recipe_decision_v1(
            &ledger,
            "builtin-recipe-read-only-preview",
            agent,
            &action,
            facts.clock,
            &mut ops,
            &digest,
            &mut stream_error,
        )?;
        // Original bind identity, binding validation, exact replay check, prepare
        // recheck and bound-authorization recheck occur in that same order.
        readonly::validate_decision_identity_v1(
            &decision,
            &action,
            &mut ops,
            &digest,
            &mut stream_error,
        )?;
        readonly::validate_default_recipe_decision_v1(
            &mut ops,
            &ledger,
            &decision,
            &action,
            facts.clock,
        )?;
        readonly::validate_decision_replay_v1(&ledger, &decision, &mut ops)?;
        let payload = {
            view.before_clone(&action)?;
            serde_json::to_value(&action).map_err(crate::WorldError::from)?
        };
        let check = oasis7_agent_api::world_cognition::cognition_digest_with_work_v1(
            "oasis7.cognition.action.v1",
            &payload,
            &digest,
        )
        .map_err(&mut stream_error)?;
        facts.work(1, decision.action_digest.len() + check.as_str().len())?;
        if decision.action_digest != check.as_str() {
            return Err(crate::WorldError::ResourceBalanceInvalid {
                reason: "agent_delegation:decision_action_changed".into(),
            }
            .into());
        }
        readonly::validate_default_recipe_decision_v1(
            &mut ops,
            &ledger,
            &decision,
            &action,
            facts.clock,
        )?;
        let mut authorization = readonly::validate_default_recipe_decision_v1(
            &mut ops,
            &ledger,
            &decision,
            &action,
            facts.clock,
        )?
        .ok_or_else(|| crate::WorldError::DistributedValidationFailed {
            reason: "native recipe grant required".into(),
        })?;
        // The fixed producer creates the same id-zero, unsigned governance
        // envelope as the original quote; the shared classifier is not skipped.
        view.before_clone(&action)?;
        facts.before_owned(std::mem::size_of::<crate::ActionEnvelope>()+4096)?;
        let envelope=crate::ActionEnvelope{id:0,action:action.clone(),governance_authorization:None,committed_recipe_origin:None};
        if !crate::world::governance_authorization::is_unprotected_governance_envelope(&envelope)? {
            return Err(crate::WorldError::DistributedValidationFailed{reason:"native recipe governance authorization required".into()}.into());
        }
        let reject=|note:String|crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected{action_id:0,reason:crate::RejectReason::RuleDenied{notes:vec![note]}});
        let body=match super::safe_alt::guard(&view,binding,&envelope)? {
            Err(note)=>reject(note),
            Ok(())=>{
                let admission=(||->Result<(),NativeError>{
                    readonly::validate_decision_identity_v1(&decision,&action,&mut ops,&digest,&mut stream_error)?;
                    readonly::validate_default_recipe_decision_v1(&mut ops,&ledger,&decision,&action,facts.clock)?;
                    Ok(())
                })();
                match admission{
                    Err(NativeError::Body(error))=>return Err(NativeError::Body(error)),
                    Err(NativeError::World(error))=>reject(view.format(format_args!("{error:?}"))?),
                    Ok(())=>super::recipe_sources::RecipeFacts(&view).eval(0,&action)?,
                }
            }
        };
        let crate::Action::ScheduleRecipe{plan,..}=&action else{return Err(invalid("fixed native recipe action changed").into())};
        authorization.cost_units=u64::try_from(plan.power_required).map_err(|_|crate::WorldError::DistributedValidationFailed{reason:"invalid canonical power".into()})?;
        let electricity=facts.quote_agent_electricity_data(agent)?.ok_or_else(||crate::WorldError::DistributedValidationFailed{reason:"recipe agent unavailable".into()})?;
        crate::world::builtin_recipe_quote::readonly::from_event_v1(action,authorization,body,electricity,facts.clock,agent,factory,recipe,batches,&mut ops)
    })();
    if let Some(error) = digest.first.borrow_mut().take() {
        return Err(error);
    }
    drop(digest);
    let result = match result {
        Ok(value) => Ok(value),
        Err(NativeError::World(error)) => Err(error),
        Err(NativeError::Body(error)) => return Err(error),
    };
    facts.check()?;
    drop(view);
    Ok(PaidCurrentBuiltinRecipeQuoteDataV1 {
        result,
        fees: facts.into_fees(),
    })
}
