//! Unique original event-to-quote projection. The input event is supplied by
//! the fixed real ScheduleRecipe evaluator, never an authorization verdict.
use super::*;
use crate::RejectReason;
use crate::world::agent_delegation::readonly::Work;
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn from_event_v1<W: Work>(
    action: Action,
    authorization: crate::AgentDelegationAuthorizationV1,
    body: WorldEventBody,
    electricity_before: i64,
    quoted_at_tick: u64,
    agent: &str,
    factory: &str,
    recipe: &str,
    batches: u32,
    work: &mut W,
) -> Result<NativeBuiltinRecipeQuote, W::Error> {
    let invalid = |reason: &str| WorldError::DistributedValidationFailed {
        reason: reason.into(),
    };
    work.ownership(std::mem::size_of::<NativeBuiltinRecipeQuote>() + 256)?;
    let (rejection, preview, electricity_after) = match body {
        WorldEventBody::Domain(DomainEvent::RecipeStarted {
            consume_ledger,
            output_ledger,
            consume,
            produce,
            byproducts,
            power_required,
            duration_ticks,
            ..
        }) => (
            None,
            Some(BuiltinRecipeStartPreview {
                consume_ledger,
                output_ledger,
                consume,
                produce,
                byproducts,
                power_required,
                duration_ticks,
            }),
            electricity_before.checked_sub(power_required),
        ),
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            (Some(reason), None, None)
        }
        _ => return Err(invalid("native recipe preview unavailable").into()),
    };
    let output = NativeBuiltinRecipeQuote {
        action,
        quoted_at_tick,
        authorization,
        electricity_before,
        electricity_after,
        rejection,
        preview,
    };
    let checked = output.validate_selection_with_work_v1(
        agent,
        factory,
        recipe,
        batches,
        &mut |s, b, h| {
            work.work(s, b)?;
            if h > 0 {
                work.ownership(h)?
            }
            Ok::<_, W::Error>(())
        },
    )?;
    checked.map_err(|_| invalid("native recipe quote inconsistent"))?;
    Ok(output)
}

impl World {
    pub(in crate::world) fn action_to_event(
        &self,
        envelope: &ActionEnvelope,
    ) -> Result<WorldEventBody, WorldError> {
        self.action_to_event_with_native_recipe_preview_v1(envelope, None)
    }
}

impl World {
    /// One original pre-dispatch gate for both ordinary action conversion and
    /// the private borrowed quote preview. No predicate is supplied by a caller.
    pub(in crate::world) fn action_to_event_readonly_prefix_v1(
        &self,
        envelope: &ActionEnvelope,
        native_preview: Option<
            &super::super::agent_delegation::readonly::NativeRecipePreviewBindingV1<'_>,
        >,
    ) -> Result<Option<WorldEventBody>, WorldError> {
        self.validate_governance_action_authorization(envelope)?;
        let action_id = envelope.id;
        if let Err(reason) = self.safe_alternative_action_guard(envelope) {
            return Ok(Some(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![reason],
                },
            })));
        }
        let delegation = match native_preview {
            Some(binding) => binding.validate(self, &envelope.action),
            None => self.delegation_action_admission(&envelope.action),
        };
        if let Err(error) = delegation {
            return Ok(Some(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("{error:?}")],
                },
            })));
        }
        Ok(None)
    }
}
