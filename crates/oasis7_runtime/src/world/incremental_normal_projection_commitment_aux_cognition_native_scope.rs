//! Original matching-SafeAlt pre-dispatch checks for a real readonly quote.
//! The permanent records and every joined identity/owner/position are taken
//! from the same actual Current projection sources, not supplied verdicts.
use super::*;
use crate::world::{
    industrial_control::readonly_runtime_owner::{
        current_agent_identity_readonly_v1, current_agent_owner_readonly_v1,
        owner_public_key_readonly_v1,
    },
    safe_alternative::readonly as scope,
};
struct Source<'v, 'f, 'a, 'c, 'b> {
    view: &'v values::View<'f, 'a, 'c>,
    binding: Option<&'b ReservedValue<oasis7_agent_api::RuntimeBindingV1>>,
}
impl scope::sealed::Sources for Source<'_, '_, '_, '_, '_> {}
impl scope::Sources for Source<'_, '_, '_, '_, '_> {
    type Error = BodyMutationError;
    fn work(&self, s: u64, b: usize, h: usize) -> Result<(), Self::Error> {
        self.view.facts.work(s, b)?;
        if h > 0 {
            self.view.facts.before_owned(h)?
        }
        Ok(())
    }
    fn before_value<T: serde::Serialize + ?Sized>(&self, v: &T) -> Result<(), Self::Error> {
        self.view.before_clone(v)
    }
    fn identity(&self, a: &str) -> Result<Option<crate::CapabilityAgentIdentity>, Self::Error> {
        current_agent_identity_readonly_v1(&self.view.owner_source()?, a)
    }
    fn binding(&self) -> Result<Option<oasis7_agent_api::RuntimeBindingV1>, Self::Error> {
        match self.binding {
            None => Ok(None),
            Some(value) => {
                if !value.uses_budget(self.view.facts.common.borrow().memory()) {
                    return Err(invalid("native quote Runtime binding pool"));
                }
                self.view.before_clone(&**value)?;
                Ok(Some((**value).clone()))
            }
        }
    }
    fn owner(&self, a: &str) -> Result<Option<crate::ResolvedAgentOwnerV1>, Self::Error> {
        current_agent_owner_readonly_v1(&self.view.owner_source()?, a)
    }
    fn instances_len(&self) -> Result<usize, Self::Error> {
        Ok(self.view.facts.rows("module_instances")?.len())
    }
    fn active_modules_len(&self) -> Result<usize, Self::Error> {
        let row = self.view.required(&["module_registry", "active"])?;
        let memory = self.view.facts.common.borrow().memory().clone();
        row.share_data(&memory, &mut |s, b| {
            self.view
                .facts
                .common
                .borrow_mut()
                .charge(s, b)
                .map_err(paid)
        })
        .map_err(graph)?
        .object_len_data_v1(&memory, &mut |s, b| {
            self.view
                .facts
                .common
                .borrow_mut()
                .charge(s, b)
                .map_err(paid)
        })
        .map_err(graph)
    }
    fn owner_key(&self, a: &str) -> Result<Option<String>, Self::Error> {
        Ok(owner_public_key_readonly_v1(&self.view.owner_source()?, a)?.ok())
    }
    fn position(
        &self,
        a: &str,
    ) -> Result<Option<oasis7_agent_api::world_action::GeoPos>, Self::Error> {
        match self.view.facts.point("agents", a)? {
            None => Ok(None),
            Some(row) => super::super::super::fact_decode::with_agent_position_data_v1(
                self.view.facts.raw(row)?,
                &self.view.facts.common,
                |pos| Ok(Some(pos.clone())),
            ),
        }
    }
    fn with_forbidden<T>(
        &self,
        visit: impl FnOnce(&[String]) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        oasis7_world_state::gameplay_capability::GameplayCapabilityReadonlyFactsV1::with_gameplay_policy(self.view.facts,|policy|visit(&policy.forbidden_location_ids))
    }
}
fn graph(error: ProjectionColumnErrorV1<BodyMutationError>) -> BodyMutationError {
    match error {
        ProjectionColumnErrorV1::Work(error) => error,
        ProjectionColumnErrorV1::Memory(error) => ValueTreeError::Memory(error).into(),
        ProjectionColumnErrorV1::Invalid(error) => invalid(error),
    }
}

enum Stop {
    Body(BodyMutationError),
    Refused(String),
}
fn resource(error: BodyMutationError) -> ProjectionColumnErrorV1<Stop> {
    ProjectionColumnErrorV1::Work(Stop::Body(error))
}
pub(super) fn guard(
    view: &values::View<'_, '_, '_>,
    binding: Option<&ReservedValue<oasis7_agent_api::RuntimeBindingV1>>,
    envelope: &crate::ActionEnvelope,
) -> Result<Result<(), String>, BodyMutationError> {
    view.required(&["capability_revocation_state"])?;
    let Some(rows) = view.optional(&["capability_revocation_state", "safe_alternatives"])? else {
        return Ok(Ok(()));
    };
    let memory = view.facts.common.borrow().memory().clone();
    let result = rows.object_members_data(
        &memory,
        &mut |s, b| {
            view.facts
                .common
                .borrow_mut()
                .charge(s, b)
                .map_err(paid)
                .map_err(Stop::Body)
        },
        &mut |_, row, _| {
            let Some(authorization) = view
                .member_like_json_get(&row, "authorization")
                .map_err(resource)?
            else {
                return Ok(());
            };
            let Some(id) = view
                .member_like_json_get(&authorization, "action_id")
                .map_err(resource)?
            else {
                return Ok(());
            };
            if id.unsigned_data() != Some(envelope.id) {
                return Ok(());
            }
            let record: serde_json::Value = view
                .decode(&row)
                .map_err(resource)?
                .map_err(|_| resource(invalid("native quote actual SafeAlt JSON")))?;
            view.before_clone(&record).map_err(resource)?;
            let candidate =
                crate::world::incremental_safe_alternative_scope::matching_candidate_v1(
                    &record,
                    view.facts.clock,
                )
                .map_err(|note| ProjectionColumnErrorV1::Work(Stop::Refused(note)))?;
            let source = Source { view, binding };
            scope::validate_scope_from_sources_v1(&source, &record, candidate, view.facts.clock)
                .map_err(resource)?
                .map_err(|note| ProjectionColumnErrorV1::Work(Stop::Refused(note)))?;
            view.before_clone(&(record.get("agent_id"), candidate, envelope))
                .map_err(resource)?;
            crate::world::incremental_safe_alternative_scope::validate_matching_action_v1(
                &record, candidate, envelope,
            )
            .map_err(|note| ProjectionColumnErrorV1::Work(Stop::Refused(note)))?;
            Ok(())
        },
    );
    match result {
        Ok(()) => {
            view.facts.check()?;
            Ok(Ok(()))
        }
        Err(ProjectionColumnErrorV1::Work(Stop::Refused(note))) => Ok(Err(note)),
        Err(ProjectionColumnErrorV1::Work(Stop::Body(error))) => Err(error),
        Err(ProjectionColumnErrorV1::Memory(error)) => Err(ValueTreeError::Memory(error).into()),
        Err(ProjectionColumnErrorV1::Invalid(error)) => Err(invalid(error)),
    }
}
