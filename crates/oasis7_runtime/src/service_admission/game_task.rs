//! Durable owner task bootstrap derived exclusively from canonical intent results.
use super::*;
use crate::World;
use oasis7_agent_api::game::GAME_GOAL_PUBLISH_DOMAIN;
use oasis7_agent_api::game::{DecimalU64, GAME_TASK_CREATE_DOMAIN, GameTaskStatusV1};
pub use oasis7_agent_api::game::{
    GameGoalPublishV1, GameGoalReferenceV1, GameTaskControlV1, GameTaskCreateV1,
};
use std::collections::BTreeMap;
mod executor;
pub use executor::*;
mod task_context;
pub use self::task_context::*;
mod task_retirement;
pub use task_retirement::*;

pub fn validate_owner(
    world: &World,
    signed: &SignedReadRequest<GameTaskCreateV1>,
) -> Result<(), String> {
    verify_read_request(GAME_TASK_CREATE_DOMAIN, signed)?;
    signed.request.validate()?;
    let request = &signed.request;
    if agent_authority::owner_public_key(world, &request.agent_id)? != signed.subject_public_key {
        return Err("Game task requires canonical owner signature".into());
    }
    let identity = &world.capability_revocation_state().agent_identities[&request.agent_id];
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    if request.owner_binding != identity.owner_binding
        || request.owner_generation.0 != identity.generation
        || request.world_id != binding.world_id
        || request.branch_id != binding.branch_id
        || request.reorg_epoch.0 != binding.reorg_epoch
        || world
            .chain_resource_manifest()
            .genesis_ref
            .as_ref()
            .is_some_and(|genesis| genesis != &request.genesis_digest)
    {
        return Err("Game task owner or history fence mismatch".into());
    }
    Ok(())
}

fn create_control(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<GameTaskControlV1, String> {
    let WorldServicePayloadV1::GameTaskCreate(signed) = &request.signed_payload else {
        return Err("not a Game task create".into());
    };
    let value = &signed.request;
    Ok(GameTaskControlV1 {
        schema_revision: 1,
        task_id: format!(
            "game-task-{}",
            request_digest("game-task-id", &request.correlation.key)?
        ),
        world_id: value.world_id.clone(),
        genesis_digest: value.genesis_digest.clone(),
        branch_id: value.branch_id.clone(),
        reorg_epoch: value.reorg_epoch,
        agent_id: value.agent_id.clone(),
        owner_binding: value.owner_binding.clone(),
        owner_generation: value.owner_generation,
        owner_public_key: signed.subject_public_key.clone(),
        task_revision: DecimalU64(1),
        task_status: GameTaskStatusV1::Suspended,
        latest_goal_revision: DecimalU64(0),
        active_goal_revision: DecimalU64(0),
        active_goal_reference: None,
        executor_epoch: DecimalU64(0),
        executor_id: None,
        executor_binding: None,
        created_operation_id: value.client_operation_id.clone(),
    })
}

/// Public metadata only. No local private-body availability predicate.
pub fn verify_publish(signed: &SignedReadRequest<GameGoalPublishV1>) -> Result<(), String> {
    signed.request.validate()?;
    if signed.subject_public_key != signed.request.owner_public_key {
        return Err("goal publish signer differs from owner".into());
    }
    private_goal_prepare::verify_owner_signature_strict(
        GAME_GOAL_PUBLISH_DOMAIN,
        &signed.request,
        &signed.subject_public_key,
        &signed.signature_hex,
    )
}
pub fn validate_publish_owner(
    world: &World,
    signed: &SignedReadRequest<GameGoalPublishV1>,
) -> Result<(), String> {
    verify_publish(signed)?;
    let r = &signed.request;
    let owner = agent_authority::owner_public_key(world, &r.agent_id)?;
    let identity = world
        .capability_revocation_state()
        .agent_identities
        .get(&r.agent_id)
        .ok_or("goal owner identity unavailable")?;
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    if owner != signed.subject_public_key
        || r.owner_binding != identity.owner_binding
        || r.owner_generation.0 != identity.generation
        || r.world_id != binding.world_id
        || r.branch_id != binding.branch_id
        || r.reorg_epoch.0 != binding.reorg_epoch
        || world
            .chain_resource_manifest()
            .genesis_ref
            .as_ref()
            .is_some_and(|genesis| genesis != &r.genesis_digest)
    {
        return Err("goal publish current owner or history mismatch".into());
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishReceipt {
    task_id: String,
    task_revision: DecimalU64,
    goal_reference: GameGoalReferenceV1,
}
fn publish_receipt(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    height: u64,
) -> Result<PublishReceipt, String> {
    let WorldServicePayloadV1::GameGoalPublish(signed) = &request.signed_payload else {
        return Err("not a goal publication".into());
    };
    verify_publish(signed)?;
    if height == 0 {
        return Err("goal activation height must be committed".into());
    }
    let r = &signed.request;
    Ok(PublishReceipt {
        task_id: r.task_id.clone(),
        task_revision: DecimalU64(
            r.expected_task_revision
                .0
                .checked_add(1)
                .ok_or("task revision overflow")?,
        ),
        goal_reference: GameGoalReferenceV1 {
            goal_revision: r.goal_revision,
            goal_ref: r.goal_ref.clone(),
            prepare_operation_id: r.prepare_operation_id.clone(),
            publish_operation_id: r.client_operation_id.clone(),
            activation_committed_height: DecimalU64(height),
        },
    })
}
pub(super) fn validate_result(result: &CanonicalIntentResultV1) -> Result<(), String> {
    let expected = match &result.request.signed_payload {
        WorldServicePayloadV1::GameTaskCreate(_) => Some(
            serde_json::to_value(create_control(&result.request)?).map_err(|e| e.to_string())?,
        ),
        WorldServicePayloadV1::GameExecutorControl(_) => Some(
            serde_json::to_value(executor::executor_receipt(&result.request)?)
                .map_err(|e| e.to_string())?,
        ),
        WorldServicePayloadV1::GameGoalPublish(_) => Some(
            serde_json::to_value(publish_receipt(&result.request, result.committed_height)?)
                .map_err(|e| e.to_string())?,
        ),
        _ => None,
    };
    if let Some(expected) = expected {
        if result.rejected.is_some() {
            if !result.receipt.is_null() {
                return Err("rejected task operation contains success receipt".into());
            }
        } else if result.receipt != expected {
            return Err("invalid canonical task operation receipt".into());
        }
    }
    Ok(())
}
#[derive(Clone)]
struct ReducedTask {
    control: GameTaskControlV1,
    publish: Option<CanonicalIntentResultV1>,
    last_height: u64,
}
fn collect<'a>(
    world: &World,
    staged: impl IntoIterator<Item = &'a CanonicalIntentResultV1>,
) -> Result<Vec<CanonicalIntentResultV1>, String> {
    let mut retained = super::retained_queries::hot_results(world)?;
    let canonical_empty = crate::FormalRetainedHistoryRootV1::from_index_root(
        oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexRootV1::empty()?,
    )?;
    // Exact canonical empty commitment proves there are no cold task results.
    // Nonempty or malformed descriptors still require authenticated CAS access.
    if world
        .state()
        .retained_history_root
        .as_ref()
        .is_some_and(|root| root != &canonical_empty)
    {
        for agent in world.capability_revocation_state().agent_identities.keys() {
            for (key, result) in super::retained_queries::game_task_results(world, agent)? {
                if let Some(previous) = retained.get(&key) {
                    if previous != &result {
                        return Err("ambiguous retained canonical task result".into());
                    }
                } else {
                    retained.insert(key, result);
                }
            }
        }
    }
    let mut results = BTreeMap::new();
    for (key, result) in retained {
        if matches!(
            result.request.signed_payload,
            WorldServicePayloadV1::GameTaskCreate(_)
                | WorldServicePayloadV1::GameGoalPublish(_)
                | WorldServicePayloadV1::GameExecutorControl(_)
        ) {
            results.insert(key, result);
        }
    }
    for result in staged {
        let key = correlation::key_digest(&result.request.correlation.key)?;
        let value = serde_json::to_value(result).map_err(|e| e.to_string())?;
        correlation::validate_result(&key, &value)?;
        if !matches!(
            result.request.signed_payload,
            WorldServicePayloadV1::GameTaskCreate(_)
                | WorldServicePayloadV1::GameGoalPublish(_)
                | WorldServicePayloadV1::GameExecutorControl(_)
        ) {
            continue;
        }
        if let Some(previous) = results.get(&key) {
            if serde_json::to_value(previous).map_err(|e| e.to_string())? != value {
                return Err("ambiguous staged canonical task result".into());
            }
        } else {
            results.insert(key, result.clone());
        }
    }
    Ok(results.into_values().collect())
}
fn matches_control(r: &GameGoalPublishV1, c: &GameTaskControlV1) -> bool {
    r.world_id == c.world_id
        && r.genesis_digest == c.genesis_digest
        && r.task_id == c.task_id
        && r.agent_id == c.agent_id
        && r.owner_public_key == c.owner_public_key
        && r.owner_binding == c.owner_binding
        && r.owner_generation == c.owner_generation
        && r.branch_id == c.branch_id
        && r.reorg_epoch == c.reorg_epoch
        && r.expected_task_revision == c.task_revision
        && r.expected_latest_goal_revision == c.latest_goal_revision
}
fn reduce(results: Vec<CanonicalIntentResultV1>) -> Result<BTreeMap<String, ReducedTask>, String> {
    let mut tasks = BTreeMap::new();
    let mut scopes = BTreeMap::new();
    let mut publishes = Vec::new();
    for result in results {
        if result.rejected.is_some() {
            continue;
        }
        match &result.request.signed_payload {
            WorldServicePayloadV1::GameTaskCreate(_) => {
                let control = create_control(&result.request)?;
                let scope = request_digest(
                    "game-task-history-scope",
                    &(
                        &control.world_id,
                        &control.genesis_digest,
                        &control.agent_id,
                        &control.owner_public_key,
                        &control.owner_binding,
                        control.owner_generation,
                        &control.branch_id,
                        control.reorg_epoch,
                    ),
                )?;
                if scopes.insert(scope, control.task_id.clone()).is_some() {
                    return Err("duplicate canonical task owner/history scope".into());
                }

                if tasks
                    .insert(
                        control.task_id.clone(),
                        ReducedTask {
                            control,
                            publish: None,
                            last_height: result.committed_height,
                        },
                    )
                    .is_some()
                {
                    return Err("duplicate canonical task identity".into());
                }
            }
            WorldServicePayloadV1::GameExecutorControl(signed) => publishes.push((
                signed.request.fence.task_id.clone(),
                signed.request.fence.task_revision.0,
                result,
            )),
            WorldServicePayloadV1::GameGoalPublish(signed) => publishes.push((
                signed.request.task_id.clone(),
                signed.request.expected_task_revision.0,
                result,
            )),
            _ => (),
        }
    }
    publishes.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    for (_, _, result) in publishes {
        if let WorldServicePayloadV1::GameExecutorControl(signed) = &result.request.signed_payload {
            let task = tasks
                .get_mut(&signed.request.fence.task_id)
                .ok_or("executor has no canonical task predecessor")?;
            if oasis7_agent_api::game::GameTaskFenceV1::from_control(&task.control)
                != signed.request.fence
            {
                return Err("executor predecessor or owner fence mismatch".into());
            }
            if task.last_height > result.committed_height {
                return Err("executor control heights regress".into());
            }
            executor::apply_executor_receipt(
                &mut task.control,
                executor::executor_receipt(&result.request)?,
            );
            task.last_height = result.committed_height;
            continue;
        }
        let WorldServicePayloadV1::GameGoalPublish(signed) = &result.request.signed_payload else {
            unreachable!()
        };
        let task = tasks
            .get_mut(&signed.request.task_id)
            .ok_or("goal publication has no canonical task predecessor")?;
        if !matches_control(&signed.request, &task.control) {
            return Err("goal publication predecessor or owner fence mismatch".into());
        }
        let receipt = publish_receipt(&result.request, result.committed_height)?;
        if task.last_height > result.committed_height {
            return Err("goal publication heights regress".into());
        }
        task.control.task_revision = receipt.task_revision;
        task.control.latest_goal_revision = receipt.goal_reference.goal_revision;
        task.control.active_goal_revision = receipt.goal_reference.goal_revision;
        task.control.active_goal_reference = Some(receipt.goal_reference);
        task.last_height = result.committed_height;
        task.publish = Some(result);
    }
    Ok(tasks)
}
fn select_current<'a>(
    world: &World,
    agent: &str,
    tasks: &'a BTreeMap<String, ReducedTask>,
) -> Result<Option<&'a ReducedTask>, String> {
    let relevant = tasks
        .values()
        .filter(|task| task.control.agent_id == agent)
        .collect::<Vec<_>>();
    if relevant.is_empty() {
        return Ok(None);
    }
    let identity = world
        .capability_revocation_state()
        .agent_identities
        .get(agent)
        .ok_or("task current owner identity unavailable")?;
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    let owner = agent_authority::owner_public_key(world, agent)?;
    let mut current = None;
    for task in relevant {
        let c = &task.control;
        if c.owner_public_key != owner
            || c.owner_binding != identity.owner_binding
            || c.owner_generation.0 != identity.generation
            || c.world_id != binding.world_id
            || c.branch_id != binding.branch_id
            || c.reorg_epoch.0 != binding.reorg_epoch
            || world
                .chain_resource_manifest()
                .genesis_ref
                .as_ref()
                .is_some_and(|genesis| genesis != &c.genesis_digest)
        {
            continue;
        }
        if current.replace(task).is_some() {
            return Err("ambiguous canonical task control".into());
        }
    }
    Ok(current)
}
/// Validate every historical task chain, including currently revoked owners.
pub fn validate_history(world: &World) -> Result<(), String> {
    reduce(collect(world, [])?)?;
    Ok(())
}
pub fn current_control(world: &World, agent: &str) -> Result<Option<GameTaskControlV1>, String> {
    let tasks = reduce(collect(world, [])?)?;
    Ok(select_current(world, agent, &tasks)?.map(|task| task.control.clone()))
}
/// Exact authoritative publication used to prove the historical activation cut.
pub fn active_publish(
    world: &World,
    agent: &str,
) -> Result<Option<CanonicalIntentResultV1>, String> {
    let tasks = reduce(collect(world, [])?)?;
    Ok(select_current(world, agent, &tasks)?.and_then(|task| task.publish.clone()))
}

/// One immutable, fully validated view used by checkpoint live-task retention.
/// The hidden public boundary is required because the chain runtime is a separate
/// crate target; private fields and the World borrow prevent caller-supplied or
/// cross-world validation state.
#[doc(hidden)]
pub struct ValidatedRetentionHistory<'a> {
    world: &'a World,
    tasks: BTreeMap<String, ReducedTask>,
    prefixes: Vec<CanonicalIntentResultV1>,
    prefix_keys: std::collections::BTreeSet<(String, Vec<u8>)>,
}

/// Validate canonical world-service results once for one retention refresh.
#[doc(hidden)]
pub fn validate_retention_history(world: &World) -> Result<ValidatedRetentionHistory<'_>, String> {
    validate_retention_history_results(
        world,
        &world.capability_revocation_state().world_service_results,
    )
}

fn validate_retention_history_results<'a>(
    world: &'a World,
    results: &BTreeMap<String, serde_json::Value>,
) -> Result<ValidatedRetentionHistory<'a>, String> {
    let mut history = Vec::new();
    let mut prefixes = Vec::new();
    let mut prefix_keys = std::collections::BTreeSet::new();
    for (key, value) in results {
        let result = correlation::validate_result_record(key, value)?;
        if matches!(
            result.request.signed_payload,
            WorldServicePayloadV1::GameTaskCreate(_)
                | WorldServicePayloadV1::GameGoalPublish(_)
                | WorldServicePayloadV1::GameExecutorControl(_)
        ) {
            history.push(result.clone());
        }
        if result.rejected.is_none()
            && matches!(
                &result.request.signed_payload,
                WorldServicePayloadV1::Scheduler(signed)
                    if matches!(
                        &signed.request.operation,
                        SchedulerOperationV1::TaskProviderPrefix { .. }
                    )
            )
        {
            let WorldServicePayloadV1::Scheduler(signed) = &result.request.signed_payload else {
                unreachable!("prefix predicate requires scheduler payload")
            };
            let SchedulerOperationV1::TaskProviderPrefix { context } = &signed.request.operation
            else {
                unreachable!("prefix predicate requires task prefix")
            };
            prefix_keys.insert((
                signed.subject_public_key.clone(),
                serde_json::to_vec(context).map_err(|e| e.to_string())?,
            ));
            prefixes.push(result);
        }
    }
    let tasks = if world.state().agents.is_empty() && prefixes.is_empty() {
        BTreeMap::new()
    } else {
        reduce(history)?
    };
    Ok(ValidatedRetentionHistory {
        world,
        tasks,
        prefixes,
        prefix_keys,
    })
}

impl ValidatedRetentionHistory<'_> {
    fn contains_prefix(
        &self,
        context: &oasis7_agent_api::game::TaskContextIdentityV1,
        subject: &str,
    ) -> Result<bool, String> {
        let context = serde_json::to_vec(context).map_err(|e| e.to_string())?;
        Ok(self.prefix_keys.contains(&(subject.to_owned(), context)))
    }

    fn validate_cognition_fence(
        &self,
        agent: &str,
        fence: Option<&oasis7_agent_api::game::GameTaskFenceV1>,
        subject: &str,
        generation: u64,
    ) -> Result<(), String> {
        executor::validate_cognition_fence_against_tasks(
            self.world,
            agent,
            fence,
            subject,
            generation,
            &self.tasks,
        )
    }

    /// The current publication after reducing this view's complete task history.
    #[doc(hidden)]
    pub fn active_publish(&self, agent: &str) -> Result<Option<CanonicalIntentResultV1>, String> {
        Ok(select_current(self.world, agent, &self.tasks)?.and_then(|task| task.publish.clone()))
    }

    /// The current control selected from this view's complete task history.
    /// The view is bound to the immutable World it validated and cannot be
    /// supplied with a different World by the caller.
    #[doc(hidden)]
    pub fn current_control(&self, agent: &str) -> Result<Option<GameTaskControlV1>, String> {
        Ok(select_current(self.world, agent, &self.tasks)?.map(|task| task.control.clone()))
    }

    /// Revalidate the caller's live executor fence against this view's World.
    #[doc(hidden)]
    pub fn validate_task_executor(
        &self,
        fence: &oasis7_agent_api::game::GameTaskFenceV1,
        subject: &str,
    ) -> Result<(), String> {
        executor::validate_task_executor_against_tasks(self.world, fence, subject, &self.tasks)
    }

    /// Revalidate the caller's live reader fence against this view's World.
    #[doc(hidden)]
    pub fn validate_task_reader(
        &self,
        fence: &oasis7_agent_api::game::GameTaskFenceV1,
        subject: &str,
    ) -> Result<(), String> {
        executor::validate_task_reader_against_tasks(self.world, fence, subject, &self.tasks)
    }

    /// Find the unique accepted canonical prefix matching this capture.
    /// Duplicate matches remain an error, as they are on the canonical scan path.
    #[doc(hidden)]
    pub fn matching_task_prefix(
        &self,
        context: &oasis7_agent_api::game::TaskContextIdentityV1,
        subject: &str,
    ) -> Result<Option<&CanonicalIntentResultV1>, String> {
        let mut selected = None;
        for result in &self.prefixes {
            let WorldServicePayloadV1::Scheduler(signed) = &result.request.signed_payload else {
                continue;
            };
            let SchedulerOperationV1::TaskProviderPrefix { context: captured } =
                &signed.request.operation
            else {
                continue;
            };
            if captured == context
                && signed.subject_public_key == subject
                && selected.replace(result).is_some()
            {
                return Err("ambiguous canonical task context prefix".into());
            }
        }
        Ok(selected)
    }

    /// Canonical non-rejected prefix candidates; validate each capture against
    /// current authority with `validate_prefix_capture` before retaining it.
    #[doc(hidden)]
    pub fn retained_prefixes(
        &self,
    ) -> impl Iterator<
        Item = (
            &CanonicalIntentResultV1,
            &oasis7_agent_api::game::TaskContextIdentityV1,
            &str,
        ),
    > + '_ {
        self.prefixes.iter().filter_map(|result| {
            let WorldServicePayloadV1::Scheduler(signed) = &result.request.signed_payload else {
                return None;
            };
            let SchedulerOperationV1::TaskProviderPrefix { context } = &signed.request.operation
            else {
                return None;
            };
            Some((result, context, signed.subject_public_key.as_str()))
        })
    }

    /// Repeat live context, fence, expiry, signer, and cognition checks against this view.
    #[doc(hidden)]
    pub fn validate_prefix_capture(
        &self,
        context: &oasis7_agent_api::game::TaskContextIdentityV1,
        subject: &str,
    ) -> Result<(), String> {
        task_context::validate_context_capture_for_retention(self, context, subject)
    }
}

pub fn prepare_create<'a>(
    world: &World,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    staged: impl IntoIterator<Item = &'a CanonicalIntentResultV1>,
) -> Result<serde_json::Value, String> {
    let WorldServicePayloadV1::GameTaskCreate(signed) = &request.signed_payload else {
        return Err("not task create".into());
    };
    validate_owner(world, signed)?;
    if derive_correlation(
        request.correlation.key.world.clone(),
        &request.signed_payload,
    )? != request.correlation
    {
        return Err("task correlation mismatch".into());
    }
    if current_control(world, &signed.request.agent_id)?.is_some() {
        return Err("task already exists".into());
    }
    let tasks = reduce(collect(world, staged)?)?;
    if select_current(world, &signed.request.agent_id, &tasks)?.is_some() {
        return Err("task already exists within block".into());
    }
    serde_json::to_value(create_control(request)?).map_err(|e| e.to_string())
}
pub fn prepare_publish<'a>(
    world: &World,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    staged: impl IntoIterator<Item = &'a CanonicalIntentResultV1>,
    committed_height: u64,
) -> Result<serde_json::Value, String> {
    let WorldServicePayloadV1::GameGoalPublish(signed) = &request.signed_payload else {
        return Err("not goal publish".into());
    };
    validate_publish_owner(world, signed)?;
    if derive_correlation(
        request.correlation.key.world.clone(),
        &request.signed_payload,
    )? != request.correlation
    {
        return Err("goal publish correlation mismatch".into());
    }
    let tasks = reduce(collect(world, staged)?)?;
    let current = select_current(world, &signed.request.agent_id, &tasks)?
        .ok_or("goal publish canonical task unavailable")?;
    if !matches_control(&signed.request, &current.control) {
        return Err("goal publish expected control mismatch".into());
    }
    serde_json::to_value(publish_receipt(request, committed_height)?).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests;
