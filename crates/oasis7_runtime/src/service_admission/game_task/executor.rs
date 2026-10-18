//! Canonical owner-controlled executor fences; no private input material.
use super::*;
use oasis7_agent_api::game::{
    GAME_EXECUTOR_CONTROL_DOMAIN, GameExecutorBindingV1, GameExecutorControlV1,
    GameExecutorOperationV1, GameTaskFenceV1,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExecutorReceipt {
    pub task_id: String,
    pub task_revision: DecimalU64,
    pub executor_epoch: DecimalU64,
    pub executor_binding: Option<GameExecutorBindingV1>,
    pub task_status: GameTaskStatusV1,
}
pub fn verify_executor_control(
    signed: &SignedReadRequest<GameExecutorControlV1>,
) -> Result<(), String> {
    signed.request.validate()?;
    if let GameExecutorOperationV1::Bind {
        executor_public_key,
        ..
    } = &signed.request.operation
    {
        strong_key(executor_public_key)?;
    }
    if signed.subject_public_key != signed.request.fence.owner_public_key {
        return Err("executor control requires owner signature".into());
    }
    private_goal_prepare::verify_owner_signature_strict(
        GAME_EXECUTOR_CONTROL_DOMAIN,
        &signed.request,
        &signed.subject_public_key,
        &signed.signature_hex,
    )
}
fn strong_key(key: &str) -> Result<(), String> {
    let bytes: [u8; 32] = hex::decode(key)
        .map_err(|_| "invalid executor key")?
        .try_into()
        .map_err(|_| "invalid executor key")?;
    let key =
        ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid executor key")?;
    if key.is_weak() {
        return Err("weak executor key".into());
    }
    Ok(())
}
pub fn validate_executor_owner(
    world: &World,
    signed: &SignedReadRequest<GameExecutorControlV1>,
) -> Result<(), String> {
    verify_executor_control(signed)?;
    let f = &signed.request.fence;
    let identity = world
        .capability_revocation_state()
        .agent_identities
        .get(&f.agent_id)
        .ok_or("executor owner identity unavailable")?;
    let runtime = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    if agent_authority::owner_public_key(world, &f.agent_id)? != signed.subject_public_key
        || f.owner_binding != identity.owner_binding
        || f.owner_generation.0 != identity.generation
        || f.world_id != runtime.world_id
        || f.branch_id != runtime.branch_id
        || f.reorg_epoch.0 != runtime.reorg_epoch
        || world
            .chain_resource_manifest()
            .genesis_ref
            .as_ref()
            .is_some_and(|g| g != &f.genesis_digest)
    {
        return Err("executor owner or history fence mismatch".into());
    }
    if let GameExecutorOperationV1::Bind {
        executor_public_key,
        delegation_generation,
        ..
    } = &signed.request.operation
    {
        strong_key(executor_public_key)?;
        if executor_public_key == &signed.subject_public_key {
            return Err("executor binding requires canonical delegate".into());
        }
        agent_authority::validate_agent_signer(
            world,
            &f.agent_id,
            executor_public_key,
            delegation_generation.0,
        )?;
    }
    Ok(())
}
pub(super) fn executor_receipt(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<ExecutorReceipt, String> {
    let WorldServicePayloadV1::GameExecutorControl(signed) = &request.signed_payload else {
        return Err("not executor control".into());
    };
    verify_executor_control(signed)?;
    let r = &signed.request;
    let epoch = DecimalU64(
        r.fence
            .executor_epoch
            .0
            .checked_add(1)
            .ok_or("executor epoch overflow")?,
    );
    let binding = match &r.operation {
        GameExecutorOperationV1::Bind {
            executor_id,
            executor_public_key,
            delegation_generation,
        } => {
            strong_key(executor_public_key)?;
            Some(GameExecutorBindingV1 {
                executor_id: executor_id.clone(),
                executor_public_key: executor_public_key.clone(),
                delegation_generation: *delegation_generation,
                executor_epoch: epoch,
            })
        }
        GameExecutorOperationV1::Stop => None,
    };
    Ok(ExecutorReceipt {
        task_id: r.fence.task_id.clone(),
        task_revision: DecimalU64(
            r.fence
                .task_revision
                .0
                .checked_add(1)
                .ok_or("task revision overflow")?,
        ),
        executor_epoch: epoch,
        task_status: if binding.is_some() {
            GameTaskStatusV1::Active
        } else {
            GameTaskStatusV1::Suspended
        },
        executor_binding: binding,
    })
}
pub(super) fn apply_executor_receipt(control: &mut GameTaskControlV1, receipt: ExecutorReceipt) {
    control.task_revision = receipt.task_revision;
    control.executor_epoch = receipt.executor_epoch;
    control.executor_id = receipt
        .executor_binding
        .as_ref()
        .map(|b| b.executor_id.clone());
    control.executor_binding = receipt.executor_binding;
    control.task_status = receipt.task_status;
}
pub fn prepare_executor_control<'a>(
    world: &World,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    staged: impl IntoIterator<Item = &'a CanonicalIntentResultV1>,
) -> Result<serde_json::Value, String> {
    let WorldServicePayloadV1::GameExecutorControl(signed) = &request.signed_payload else {
        return Err("not executor control".into());
    };
    validate_executor_owner(world, signed)?;
    if derive_correlation(
        request.correlation.key.world.clone(),
        &request.signed_payload,
    )? != request.correlation
    {
        return Err("executor correlation mismatch".into());
    }
    let tasks = reduce(collect(world, staged)?)?;
    let current = select_current(world, &signed.request.fence.agent_id, &tasks)?
        .ok_or("executor task unavailable")?;
    if GameTaskFenceV1::from_control(&current.control) != signed.request.fence {
        return Err("executor expected task fence mismatch".into());
    }
    serde_json::to_value(executor_receipt(request)?).map_err(|e| e.to_string())
}
fn validate_current(world: &World, fence: &GameTaskFenceV1) -> Result<(), String> {
    fence.validate()?;
    let control = current_control(world, &fence.agent_id)?.ok_or("task unavailable")?;
    if GameTaskFenceV1::from_control(&control) != *fence {
        return Err("task control fence changed".into());
    }
    Ok(())
}

fn validate_current_against_tasks(
    world: &World,
    fence: &GameTaskFenceV1,
    tasks: &BTreeMap<String, ReducedTask>,
) -> Result<(), String> {
    fence.validate()?;
    let control = select_current(world, &fence.agent_id, tasks)?
        .map(|task| &task.control)
        .ok_or("task unavailable")?;
    if GameTaskFenceV1::from_control(control) != *fence {
        return Err("task control fence changed".into());
    }
    Ok(())
}

fn validate_executor_binding(
    world: &World,
    fence: &GameTaskFenceV1,
    subject: &str,
) -> Result<(), String> {
    if fence.task_status != GameTaskStatusV1::Active || fence.active_goal_reference.is_none() {
        return Err("task is not executable".into());
    }
    let executor = fence
        .executor_binding
        .as_ref()
        .ok_or("task executor unavailable")?;
    if executor.executor_public_key != subject || subject == fence.owner_public_key {
        return Err("task requires bound executor signature".into());
    }
    strong_key(subject)?;
    agent_authority::validate_agent_signer(
        world,
        &fence.agent_id,
        subject,
        executor.delegation_generation.0,
    )
}

pub(super) fn validate_task_executor_against_tasks(
    world: &World,
    fence: &GameTaskFenceV1,
    subject: &str,
    tasks: &BTreeMap<String, ReducedTask>,
) -> Result<(), String> {
    validate_current_against_tasks(world, fence, tasks)?;
    validate_executor_binding(world, fence, subject)
}

pub(super) fn validate_task_reader_against_tasks(
    world: &World,
    fence: &GameTaskFenceV1,
    subject: &str,
    tasks: &BTreeMap<String, ReducedTask>,
) -> Result<(), String> {
    validate_current_against_tasks(world, fence, tasks)?;
    if subject == fence.owner_public_key {
        return Ok(());
    }
    validate_task_executor_against_tasks(world, fence, subject, tasks)
}

pub fn validate_task_executor(
    world: &World,
    fence: &GameTaskFenceV1,
    subject: &str,
) -> Result<(), String> {
    validate_current(world, fence)?;
    validate_executor_binding(world, fence, subject)
}
pub fn validate_task_fence(
    world: &World,
    fence: &GameTaskFenceV1,
    subject: &str,
    generation: u64,
) -> Result<(), String> {
    validate_task_executor(world, fence, subject)?;
    if fence
        .executor_binding
        .as_ref()
        .is_none_or(|b| b.delegation_generation.0 != generation)
    {
        return Err("task executor delegation generation mismatch".into());
    }
    Ok(())
}
pub fn validate_task_reader(
    world: &World,
    fence: &GameTaskFenceV1,
    subject: &str,
) -> Result<(), String> {
    validate_current(world, fence)?;
    if subject == fence.owner_public_key {
        return Ok(());
    }
    validate_task_executor(world, fence, subject)
}
/// Stage-aware validation before any new lifecycle or action side effect.
pub fn validate_cognition_fence_staged<'a>(
    world: &World,
    agent: &str,
    fence: Option<&GameTaskFenceV1>,
    subject: &str,
    generation: u64,
    staged: impl IntoIterator<Item = &'a CanonicalIntentResultV1>,
) -> Result<(), String> {
    let tasks = reduce(collect(world, staged)?)?;
    validate_cognition_fence_against_tasks(world, agent, fence, subject, generation, &tasks)
}

pub(super) fn validate_cognition_fence_against_tasks(
    world: &World,
    agent: &str,
    fence: Option<&GameTaskFenceV1>,
    subject: &str,
    generation: u64,
    tasks: &BTreeMap<String, ReducedTask>,
) -> Result<(), String> {
    let current = select_current(world, agent, tasks)?.map(|t| &t.control);
    match fence {
        Some(f) => {
            f.validate()?;
            let c = current.ok_or("task unavailable")?;
            if f.agent_id != agent || GameTaskFenceV1::from_control(c) != *f {
                return Err("task control fence changed".into());
            }
            if f.task_status != GameTaskStatusV1::Active || f.active_goal_reference.is_none() {
                return Err("task is not executable".into());
            }
            let b = f
                .executor_binding
                .as_ref()
                .ok_or("task executor unavailable")?;
            if b.executor_public_key != subject
                || subject == f.owner_public_key
                || b.delegation_generation.0 != generation
            {
                return Err("task requires bound executor signature".into());
            }
            strong_key(subject)?;
            agent_authority::validate_agent_signer(world, agent, subject, generation)
        }
        None => {
            if current.is_some_and(|c| c.executor_epoch.0 > 0) {
                return Err("cognition requires task executor fence".into());
            }
            Ok(())
        }
    }
}
pub fn validate_cognition_fence(
    world: &World,
    intent: &CognitionIntentV1,
    subject: &str,
) -> Result<(), String> {
    validate_cognition_fence_staged(
        world,
        &intent.request.agent_id,
        intent.task_fence.as_ref(),
        subject,
        intent.delegation_generation,
        [],
    )
}
