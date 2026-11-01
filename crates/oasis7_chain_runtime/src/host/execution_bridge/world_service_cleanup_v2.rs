//! Registered typed V2 lane. V1-only input preserves the old execution path.
use oasis7::{
    runtime::World,
    world_service::{self as wire, WorldIdentity, WorldServicePayloadV1},
};
use oasis7_node::NodeExecutionCommitContext;
use wire::task_cleanup_v2::TaskCleanupSubmitV2;
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub(super) enum RegisteredServiceIntent {
    // Untagged serialization deliberately preserves legacy capture bytes.
    LegacyV1(Box<wire::SubmitIntentRequest<WorldServicePayloadV1>>),
    CleanupV2(Box<TaskCleanupSubmitV2>),
    GuardedStopV2(Box<wire::task_stop_v2::TaskStopSubmitV2>),
}
impl RegisteredServiceIntent {
    pub(super) fn is_gameplay(&self) -> bool {
        matches!(self, Self::LegacyV1(request) if matches!(&request.signed_payload, WorldServicePayloadV1::GameplayJson(_)))
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CleanupBatchErrorV2 {
    UnsupportedMixedV2,
    MultipleCleanupV2,
}
pub(super) fn validate_batch(
    inputs: &[(u64, RegisteredServiceIntent)],
) -> Result<(), CleanupBatchErrorV2> {
    let cleanup = inputs
        .iter()
        .filter(|(_, r)| {
            matches!(
                r,
                RegisteredServiceIntent::CleanupV2(_) | RegisteredServiceIntent::GuardedStopV2(_)
            )
        })
        .count();
    if cleanup > 0 && cleanup != inputs.len() {
        return Err(CleanupBatchErrorV2::UnsupportedMixedV2);
    }
    if cleanup > 1 {
        return Err(CleanupBatchErrorV2::MultipleCleanupV2);
    }
    Ok(())
}
pub(super) fn apply_registered_intents(
    world: &mut World,
    context: &NodeExecutionCommitContext,
    expected_world: Option<&WorldIdentity>,
    inputs: Vec<(u64, RegisteredServiceIntent)>,
) -> Result<Vec<super::world_service_execution::StagedResult>, String> {
    validate_batch(&inputs).map_err(|e| format!("{e:?}"))?;
    if inputs
        .iter()
        .all(|(_, r)| matches!(r, RegisteredServiceIntent::LegacyV1(_)))
    {
        let legacy = inputs
            .into_iter()
            .map(|(id, r)| match r {
                RegisteredServiceIntent::LegacyV1(r) => (id, *r),
                RegisteredServiceIntent::CleanupV2(_)
                | RegisteredServiceIntent::GuardedStopV2(_) => unreachable!(),
            })
            .collect();
        return super::world_service_execution::apply_intents(
            world,
            context,
            expected_world,
            legacy,
        );
    }
    let mut staged = world.clone();
    for (_, input) in inputs {
        match input {
            RegisteredServiceIntent::CleanupV2(request) => {
                request.verify_owner_signature()?;
                if expected_world != Some(&request.expected_world)
                    || request.expected_world.world_id != context.world_id
                {
                    return Err("cleanup V2 committed World mismatch".into());
                }
                staged.apply_task_revoke_and_close_v2(&request.original)?;
            }
            RegisteredServiceIntent::GuardedStopV2(request) => {
                request.verify_owner_signature()?;
                if expected_world != Some(&request.expected_world)
                    || request.expected_world.world_id != context.world_id
                {
                    return Err("guarded Stop V2 committed World mismatch".into());
                }
                staged.apply_guarded_task_stop_v2(&request.original)?;
            }
            RegisteredServiceIntent::LegacyV1(_) => unreachable!(),
        }
    }
    *world = staged;
    Ok(Vec::new())
}

#[cfg(test)]
#[path = "world_service_cleanup_v2_tests.rs"]
mod tests;
