//! Durable owner task bootstrap derived exclusively from canonical intent results.
use super::*;
use crate::World;
use oasis7_agent_api::game::{GAME_TASK_CREATE_DOMAIN, GameTaskStatusV1};
pub use oasis7_agent_api::game::{
    GameGoalPublishV1, GameGoalReferenceV1, GameTaskControlV1, GameTaskCreateV1,
};
use std::collections::BTreeMap;
mod executor;
mod task_scope;
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
    let identity = world
        .current_agent_identity(&request.agent_id)
        .ok_or("Agent identity unavailable")?;
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

pub use oasis7_client_api::legacy_task_results::{create_control, publish_receipt, verify_publish};

pub fn validate_publish_owner(
    world: &World,
    signed: &SignedReadRequest<GameGoalPublishV1>,
) -> Result<(), String> {
    verify_publish(signed)?;
    let r = &signed.request;
    let owner = agent_authority::owner_public_key(world, &r.agent_id)?;
    let identity = world
        .current_agent_identity(&r.agent_id)
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
        oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexRootV1::empty()?,
    )?;
    // Exact canonical empty commitment proves there are no cold task results.
    // Nonempty or malformed descriptors still require authenticated CAS access.
    if world
        .state()
        .retained_history_root
        .as_ref()
        .is_some_and(|root| root != &canonical_empty)
    {
        for agent in world
            .state()
            .agents
            .keys()
            .filter(|id| world.current_agent_identity(id).is_some())
        {
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
enum ReducedTaskEvent {
    Legacy(Box<CanonicalIntentResultV1>),
    Guarded(Box<oasis7_world_state::TaskStopRecordV2>),
}
fn reduce(
    world: &World,
    results: Vec<CanonicalIntentResultV1>,
) -> Result<BTreeMap<String, ReducedTask>, String> {
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
                ReducedTaskEvent::Legacy(Box::new(result)),
            )),
            WorldServicePayloadV1::GameGoalPublish(signed) => publishes.push((
                signed.request.task_id.clone(),
                signed.request.expected_task_revision.0,
                ReducedTaskEvent::Legacy(Box::new(result)),
            )),
            _ => (),
        }
    }
    if let Some(index) = &world.capability_revocation_state().task_stops_v2 {
        index.validate()?;
        for record in index.records.values() {
            crate::world::task_stop_v2::validate_record(record)?;
            publishes.push((
                record.original.request.initial_fence.task_id.clone(),
                record.original.request.initial_fence.task_revision.0,
                ReducedTaskEvent::Guarded(Box::new(record.clone())),
            ));
        }
    }
    publishes.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    for (_, _, event) in publishes {
        let result = match event {
            ReducedTaskEvent::Guarded(record) => {
                let task = tasks
                    .get_mut(&record.original.request.initial_fence.task_id)
                    .ok_or("guarded Stop has no canonical task predecessor")?;
                if task.control != record.before_control
                    || task.last_height > record.committed_height
                {
                    return Err("guarded Stop canonical predecessor mismatch".into());
                }
                task.control = record.after_control;
                task.last_height = record.committed_height;
                continue;
            }
            ReducedTaskEvent::Legacy(result) => *result,
        };
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
    let scope = task_scope::current_task_scope(world, agent)?;
    let mut current = None;
    for task in relevant {
        let c = &task.control;
        if !scope.matches(&oasis7_agent_api::game::GameTaskFenceV1::from_control(c)) {
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
    crate::world::task_terminal_v2::validate_index(world)?;
    crate::world::task_stop_v2::validate_index(world)?;
    reduce(world, collect(world, [])?)?;
    Ok(())
}
pub(crate) fn current_control_before_terminal(
    world: &World,
    agent: &str,
) -> Result<Option<GameTaskControlV1>, String> {
    let tasks = reduce(world, collect(world, [])?)?;
    Ok(select_current(world, agent, &tasks)?.map(|task| task.control.clone()))
}
pub fn current_control(world: &World, agent: &str) -> Result<Option<GameTaskControlV1>, String> {
    let control = current_control_before_terminal(world, agent)?;
    if let Some(c) = &control {
        ensure_task_not_closed(world, &c.task_id)?;
    } else if task_scope::has_closed_in_current_scope(world, agent)? {
        return Err("closed task requires explicit V2 task identity".into());
    }
    Ok(control)
}
pub fn ensure_task_not_closed(world: &World, task_id: &str) -> Result<(), String> {
    if world
        .capability_revocation_state()
        .task_terminals_v2
        .as_ref()
        .is_some_and(|index| index.records.contains_key(task_id))
    {
        return Err("task Closed V2 is unsupported by legacy V1 lifecycle".into());
    }
    Ok(())
}
/// Exact authoritative publication used to prove the historical activation cut.
pub fn active_publish(
    world: &World,
    agent: &str,
) -> Result<Option<CanonicalIntentResultV1>, String> {
    let tasks = reduce(world, collect(world, [])?)?;
    let task = select_current(world, agent, &tasks)?;
    if let Some(task) = task {
        ensure_task_not_closed(world, &task.control.task_id)?;
    }
    Ok(task.and_then(|task| task.publish.clone()))
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
    // Preserve hot scheduler/prefix records, but rebuild task predecessors
    // from the same authenticated hot+cold source used by validate_history.
    // Permanent GuardedStop records are not a substitute for those predecessors.
    // This checkpoint/restore builder may read its attached cold CAS reader;
    // a local-only HTTP witness must instead require already-hydrated provenance.
    let mut results = world
        .capability_revocation_state()
        .world_service_results
        .clone();
    for result in collect(world, [])? {
        let key = correlation::key_digest(&result.request.correlation.key)?;
        if let std::collections::btree_map::Entry::Vacant(entry) = results.entry(key) {
            entry.insert(serde_json::to_value(result).map_err(|error| error.to_string())?);
        }
    }
    validate_retention_history_results(world, &results)
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
        reduce(world, history)?
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
        let task = select_current(self.world, agent, &self.tasks)?;
        if let Some(task) = task {
            ensure_task_not_closed(self.world, &task.control.task_id)?;
        }
        Ok(task.and_then(|task| task.publish.clone()))
    }

    /// The current control selected from this view's complete task history.
    /// The view is bound to the immutable World it validated and cannot be
    /// supplied with a different World by the caller.
    #[doc(hidden)]
    pub fn current_control(&self, agent: &str) -> Result<Option<GameTaskControlV1>, String> {
        let task = select_current(self.world, agent, &self.tasks)?;
        if let Some(task) = task {
            ensure_task_not_closed(self.world, &task.control.task_id)?;
        } else if task_scope::has_closed_in_current_scope(self.world, agent)? {
            return Err("closed task requires explicit V2 task identity".into());
        }
        Ok(task.map(|task| task.control.clone()))
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
    ensure_task_not_closed(world, &create_control(request)?.task_id)?;
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
    let tasks = reduce(world, collect(world, staged)?)?;
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
    ensure_task_not_closed(world, &signed.request.task_id)?;
    validate_publish_owner(world, signed)?;
    if derive_correlation(
        request.correlation.key.world.clone(),
        &request.signed_payload,
    )? != request.correlation
    {
        return Err("goal publish correlation mismatch".into());
    }
    let tasks = reduce(world, collect(world, staged)?)?;
    let current = select_current(world, &signed.request.agent_id, &tasks)?
        .ok_or("goal publish canonical task unavailable")?;
    if !matches_control(&signed.request, &current.control) {
        return Err("goal publish expected control mismatch".into());
    }
    serde_json::to_value(publish_receipt(request, committed_height)?).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests;
