//! Complete native delegation read-model DATA from the actual Cognition row.
//! It never installs a grant, decision, permission, or spending result.
use super::*;
use crate::world::agent_delegation::{Ledger, readonly};
struct Ops<'v, 'f, 'a, 'c>(&'v values::View<'f, 'a, 'c>);
#[derive(Debug)]
enum NativeError {
    World(crate::WorldError),
    Body(BodyMutationError),
}
impl From<crate::WorldError> for NativeError {
    fn from(e: crate::WorldError) -> Self {
        Self::World(e)
    }
}
impl From<BodyMutationError> for NativeError {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl readonly::sealed::Work for Ops<'_, '_, '_, '_> {}
impl readonly::Work for Ops<'_, '_, '_, '_> {
    type Error = NativeError;
    fn work(&mut self, s: u64, b: usize) -> Result<(), NativeError> {
        self.0.facts.work(s, b)?;
        Ok(())
    }
    fn ownership(&mut self, n: usize) -> Result<(), NativeError> {
        self.0.facts.before_owned(n)?;
        Ok(())
    }
    fn before_value<T: serde::Serialize + ?Sized>(&mut self, value: &T) -> Result<(), NativeError> {
        self.0.before_clone(value)?;
        Ok(())
    }
}
pub(in crate::world) struct PaidCurrentNativeAuthorizationsDataV1 {
    values: Vec<crate::AgentDelegationAuthorizationV1>,
    fees: CurrentProjectionFactsFeesV1,
}
impl PaidCurrentNativeAuthorizationsDataV1 {
    pub(in crate::world) fn into_parts_v1(
        self,
    ) -> (
        Vec<crate::AgentDelegationAuthorizationV1>,
        CurrentProjectionFactsFeesV1,
    ) {
        (self.values, self.fees)
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_current_native_authorizations_data_v1(
    state: &OriginalStateColumnsDataV1,
    electricity: &OriginalProjectionRowsDataV1,
    aux: &ProjectionColumnDataV1,
    metadata: &OriginalProjectionReadMetadataV1<'_>,
    agent: &str,
    origin: &dyn crate::world::incremental_normal_projection_commitment::facts::ProjectionReadSourceV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidCurrentNativeAuthorizationsDataV1, BodyMutationError> {
    let facts = CurrentProjectionReadFactsV1::for_projection_data_v1(
        state,
        electricity,
        metadata,
        origin,
        common,
    )?;
    facts.before_owned(size_of::<PaidCurrentNativeAuthorizationsDataV1>())?;
    let view = values::View::new(&facts, aux)?;
    // Missing delegation is the original empty legacy ledger, but missing the
    // actual Cognition source itself is an infrastructure failure.
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
        Some(row) => match view.decode(&row)? {
            Ok(value) => value,
            Err(_) => return Err(view.business(format_args!("native delegation read unavailable"))),
        },
    };
    let result = readonly::authorizations_v1(&ledger, agent, &mut Ops(&view));
    let values = match result {
        Ok(value) => value,
        Err(NativeError::Body(error)) => return Err(error),
        Err(NativeError::World(_)) => {
            return Err(view.business(format_args!("native delegation read unavailable")));
        }
    };
    facts.check()?;
    drop(view);
    Ok(PaidCurrentNativeAuthorizationsDataV1 {
        values,
        fees: facts.into_fees(),
    })
}

impl readonly::GrantSource for Ops<'_, '_, '_, '_> {
    fn owner(&mut self, agent: &str) -> Result<Option<crate::ResolvedAgentOwnerV1>, NativeError> {
        Ok(crate::world::industrial_control::readonly_runtime_owner::current_agent_owner_readonly_v1(&self.0.owner_source()?,agent)?)
    }
    fn agent_present(&mut self, agent: &str) -> Result<bool, NativeError> {
        Ok(self.0.facts.owner_agent_present_data(agent)?)
    }
}

#[path = "incremental_normal_projection_commitment_aux_cognition_native_binding.rs"]
mod recipe_binding;

#[path = "incremental_normal_projection_commitment_aux_cognition_native_scope.rs"]
mod safe_alt;

#[path = "incremental_normal_projection_commitment_aux_cognition_native_recipe_sources.rs"]
mod recipe_sources;
pub(in crate::world) use recipe_binding::{
    PaidCurrentBuiltinRecipeQuoteDataV1, prepare_current_builtin_recipe_quote_data_v1,
};
