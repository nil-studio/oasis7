//! Fixed Current adapter to the original complete historical task reducer.
//! All returned controls are observation DATA; this adapter grants no reader,
//! provider execution, terminal or accepted-source permission.
use super::*;
use crate::service_admission::game_task::reduce_rules::TaskReductionOpsV1;
use crate::service_admission::{
    CanonicalIntentResultV1, SubmitIntentRequest, WorldServicePayloadV1,
};
use oasis7_agent_api::game::{GameExecutorBindingV1, GameGoalReferenceV1, GameTaskControlV1};
use oasis7_client_api::legacy_task_results::{ExecutorReceipt, PublishReceipt};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use std::mem::size_of;
pub(super) struct CurrentTaskReductionOpsV1<'f, 'a, 'c, 'w> {
    pub(super) facts: &'f CurrentProjectionReadFactsV1<'a, 'c>,
    pub(super) canonical: RefCell<&'w mut CanonicalCborWorkV1>,
}
impl CurrentTaskReductionOpsV1<'_, '_, '_, '_> {
    fn meter<T>(
        &self,
        run: impl FnOnce(
            &crate::world::normal_bootstrap_current::hash::BootstrapCanonicalMeterV1<'_, '_, '_, '_>,
        ) -> T,
    ) -> Result<T, BodyMutationError> {
        let mut common = self.facts.common.borrow_mut();
        let mut canonical = self.canonical.borrow_mut();
        crate::world::normal_bootstrap_current::hash::with_current_meter_v1(
            &mut common,
            &mut canonical,
            run,
        )
    }
    fn goal(&self, v: &GameGoalReferenceV1) -> Result<GameGoalReferenceV1, BodyMutationError> {
        self.before_owned(size_of::<GameGoalReferenceV1>())?;
        Ok(GameGoalReferenceV1 {
            goal_revision: v.goal_revision,
            goal_ref: self.copy(&v.goal_ref)?,
            prepare_operation_id: self.copy(&v.prepare_operation_id)?,
            publish_operation_id: self.copy(&v.publish_operation_id)?,
            activation_committed_height: v.activation_committed_height,
        })
    }
    fn binding(
        &self,
        v: &GameExecutorBindingV1,
    ) -> Result<GameExecutorBindingV1, BodyMutationError> {
        self.before_owned(size_of::<GameExecutorBindingV1>())?;
        Ok(GameExecutorBindingV1 {
            executor_id: self.copy(&v.executor_id)?,
            executor_public_key: self.copy(&v.executor_public_key)?,
            delegation_generation: v.delegation_generation,
            executor_epoch: v.executor_epoch,
        })
    }
    pub(super) fn control(
        &self,
        v: &GameTaskControlV1,
    ) -> Result<GameTaskControlV1, BodyMutationError> {
        self.before_owned(size_of::<GameTaskControlV1>())?;
        Ok(GameTaskControlV1 {
            schema_revision: v.schema_revision,
            task_id: self.copy(&v.task_id)?,
            world_id: self.copy(&v.world_id)?,
            genesis_digest: self.copy(&v.genesis_digest)?,
            branch_id: self.copy(&v.branch_id)?,
            reorg_epoch: v.reorg_epoch,
            agent_id: self.copy(&v.agent_id)?,
            owner_binding: self.copy(&v.owner_binding)?,
            owner_generation: v.owner_generation,
            owner_public_key: self.copy(&v.owner_public_key)?,
            task_revision: v.task_revision,
            task_status: v.task_status.clone(),
            latest_goal_revision: v.latest_goal_revision,
            active_goal_revision: v.active_goal_revision,
            active_goal_reference: v
                .active_goal_reference
                .as_ref()
                .map(|g| self.goal(g))
                .transpose()?,
            executor_epoch: v.executor_epoch,
            executor_id: v.executor_id.as_ref().map(|v| self.copy(v)).transpose()?,
            executor_binding: v
                .executor_binding
                .as_ref()
                .map(|v| self.binding(v))
                .transpose()?,
            created_operation_id: self.copy(&v.created_operation_id)?,
        })
    }
}
impl TaskReductionOpsV1 for CurrentTaskReductionOpsV1<'_, '_, '_, '_> {
    type Error = BodyMutationError;
    fn is_metered(&self) -> bool {
        true
    }
    fn work(&self, s: u64, b: usize) -> Result<(), Self::Error> {
        self.facts.work(s, b)
    }
    fn before_owned(&self, n: usize) -> Result<(), Self::Error> {
        self.facts.before_owned(n)
    }
    fn business(&self, note: &'static str) -> Self::Error {
        let mut common = self.facts.common.borrow_mut();
        if let Err(error) = common.charge(1, size_of::<TaskReductionFailure>() as u64) {
            return paid(error);
        }
        match common
            .memory()
            .reserve(size_of::<TaskReductionFailure>(), MemoryClass::Ordinary)
        {
            Ok(fee) => paid(TaskReductionFailure { note, _fee: fee }),
            Err(error) => ValueTreeError::Memory(error).into(),
        }
    }
    fn copy(&self, text: &str) -> Result<String, Self::Error> {
        self.before_owned(
            size_of::<String>()
                .checked_add(text.len())
                .ok_or_else(|| invalid("task reduction string layout"))?,
        )?;
        let mut bytes = Vec::with_capacity(text.len());
        if bytes.capacity() != text.len() {
            return Err(invalid("task reduction string capacity"));
        }
        for part in text.as_bytes().chunks(8192) {
            self.work(1, part.len())?;
            bytes.extend_from_slice(part)
        }
        String::from_utf8(bytes).map_err(paid)
    }
    fn digest<T: serde::Serialize>(&self, domain: &str, value: &T) -> Result<String, Self::Error> {
        let value = self
            .meter(|meter| {
                oasis7_client_api::world_service::request_digest_with_work_v1(domain, value, meter)
            })?
            .map_err(paid)?;
        self.copy(value.as_str())
    }
    fn create_control(
        &self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<GameTaskControlV1, Self::Error> {
        let value = self
            .meter(|meter| {
                oasis7_client_api::legacy_task_results::create_control_with_work_v1(request, meter)
            })?
            .map_err(paid)?;
        self.control(value.control())
    }
    fn executor_receipt(
        &self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<ExecutorReceipt, Self::Error> {
        let value = self
            .meter(|meter| {
                oasis7_client_api::legacy_task_results::executor_receipt_with_work_v1(
                    request, meter,
                )
            })?
            .map_err(paid)?;
        let v = value.receipt();
        self.before_owned(size_of::<ExecutorReceipt>())?;
        let binding = v
            .executor_binding
            .as_ref()
            .map(|v| self.binding(v))
            .transpose()?;
        // Original apply_executor_receipt copies this ID once before moving binding.
        if let Some(binding) = &binding {
            self.before_owned(size_of::<String>() + binding.executor_id.len())?;
            for part in binding.executor_id.as_bytes().chunks(8192) {
                self.work(1, part.len())?;
            }
        }
        Ok(ExecutorReceipt {
            task_id: self.copy(&v.task_id)?,
            task_revision: v.task_revision,
            executor_epoch: v.executor_epoch,
            executor_binding: binding,
            task_status: v.task_status.clone(),
        })
    }
    fn publish_receipt(
        &self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
        height: u64,
    ) -> Result<PublishReceipt, Self::Error> {
        let value = self
            .meter(|meter| {
                oasis7_client_api::legacy_task_results::publish_receipt_with_work_v1(
                    request, height, meter,
                )
            })?
            .map_err(paid)?;
        let v = value.receipt();
        self.before_owned(size_of::<PublishReceipt>())?;
        Ok(PublishReceipt {
            task_id: self.copy(&v.task_id)?,
            task_revision: v.task_revision,
            goal_reference: self.goal(&v.goal_reference)?,
        })
    }
    fn control_comparison(&self, v: &GameTaskControlV1) -> Result<(), Self::Error> {
        self.before_owned(size_of::<oasis7_agent_api::game::GameTaskFenceV1>())?;
        for text in [
            &v.task_id,
            &v.world_id,
            &v.genesis_digest,
            &v.branch_id,
            &v.agent_id,
            &v.owner_binding,
            &v.owner_public_key,
            &v.created_operation_id,
        ] {
            self.before_owned(size_of::<String>() + text.len())?;
            for part in text.as_bytes().chunks(8192) {
                self.work(1, part.len())?;
            }
        }
        if let Some(v) = &v.executor_id {
            self.before_owned(size_of::<String>() + v.len())?;
            self.work(1, v.len())?;
        }
        if let Some(v) = &v.executor_binding {
            self.before_owned(
                size_of::<GameExecutorBindingV1>()
                    + v.executor_id.len()
                    + v.executor_public_key.len(),
            )?;
            self.work(1, v.executor_id.len() + v.executor_public_key.len())?;
        }
        if let Some(v) = &v.active_goal_reference {
            self.before_owned(
                size_of::<GameGoalReferenceV1>()
                    + v.goal_ref.len()
                    + v.prepare_operation_id.len()
                    + v.publish_operation_id.len(),
            )?;
            self.work(
                1,
                v.goal_ref.len() + v.prepare_operation_id.len() + v.publish_operation_id.len(),
            )?;
        }
        Ok(())
    }
}
#[derive(Debug)]
struct TaskReductionFailure {
    note: &'static str,
    _fee: MemoryReservation,
}
impl std::fmt::Display for TaskReductionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.note)
    }
}
impl std::error::Error for TaskReductionFailure {}

enum OriginalResultOwner {
    Hot(oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::PaidCborDataV1<CanonicalIntentResultV1>),Cold(crate::world::incremental_normal_projection_retained_reader::PaidDecodedRetainedServiceDataV1)}
pub(super) struct CurrentTaskResultDataV1(std::sync::Arc<OriginalResultOwner>);
impl std::borrow::Borrow<CanonicalIntentResultV1> for CurrentTaskResultDataV1 {
    fn borrow(&self) -> &CanonicalIntentResultV1 {
        match self.0.as_ref() {
            OriginalResultOwner::Hot(v) => std::borrow::Borrow::borrow(v),
            OriginalResultOwner::Cold(v) => v.result(),
        }
    }
}
impl CurrentTaskResultDataV1 {
    fn clone_data(
        &self,
        ops: &impl TaskReductionOpsV1<Error = BodyMutationError>,
    ) -> Result<Self, BodyMutationError> {
        ops.work(1, 0)?;
        Ok(Self(std::sync::Arc::clone(&self.0)))
    }
}
fn admit_result(
    value: OriginalResultOwner,
    ops: &impl TaskReductionOpsV1<Error = BodyMutationError>,
) -> Result<CurrentTaskResultDataV1, BodyMutationError> {
    ops.before_owned(size_of::<OriginalResultOwner>() + 2 * size_of::<usize>())?;
    Ok(CurrentTaskResultDataV1(std::sync::Arc::new(value)))
}
fn map_work<V>(
    map: &std::collections::BTreeMap<String, V>,
    key: &str,
    ops: &impl TaskReductionOpsV1<Error = BodyMutationError>,
) -> Result<(), BodyMutationError> {
    for old in map.keys() {
        ops.work(1, old.len() + key.len())?;
    }
    Ok(())
}
fn map_insert<V>(
    map: &mut std::collections::BTreeMap<String, V>,
    key: String,
    value: V,
    ops: &impl TaskReductionOpsV1<Error = BodyMutationError>,
) -> Result<(), BodyMutationError> {
    map_work(map, &key, ops)?;
    let levels = (usize::BITS - map.len().leading_zeros()) as usize + 2;
    let n = size_of::<(String, V)>()
        .checked_mul(32)
        .and_then(|n| n.checked_add(1024))
        .and_then(|n| n.checked_mul(levels))
        .ok_or_else(|| invalid("task collected tree layout"))?;
    ops.before_owned(n)?;
    map.insert(key, value);
    Ok(())
}
fn task_payload(value: &CanonicalIntentResultV1) -> bool {
    matches!(
        value.request.signed_payload,
        WorldServicePayloadV1::GameTaskCreate(_)
            | WorldServicePayloadV1::GameGoalPublish(_)
            | WorldServicePayloadV1::GameExecutorControl(_)
    )
}
pub(super) struct PaidCurrentReducedTaskHistoryDataV1<'f, 'a, 'c> {
    tasks: std::collections::BTreeMap<
        String,
        crate::service_admission::game_task::ReducedTask<CurrentTaskResultDataV1>,
    >,
    _source: &'f CurrentProjectionReadFactsV1<'a, 'c>,
}
impl PaidCurrentReducedTaskHistoryDataV1<'_, '_, '_> {
    pub(super) fn tasks(
        &self,
    ) -> &std::collections::BTreeMap<
        String,
        crate::service_admission::game_task::ReducedTask<CurrentTaskResultDataV1>,
    > {
        &self.tasks
    }
}
/// Complete original no-staged-input collect/reduce used by readonly output.
/// It validates every hot row first; cold rows are selected through every
/// actual identity-bearing Agent and the exact retained root. No permission or
/// terminal status is inferred by this data reduction.
pub(super) fn reduce_current_task_history_data_v1<
    'f,
    'a,
    'c,
    R: crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
>(
    facts: &'f CurrentProjectionReadFactsV1<'a, 'c>,
    aux: &super::aux_facts::CurrentProjectionAuxFactsV1<'_, '_, '_>,
    binding: Option<&oasis7_agent_api::RuntimeBindingV1>,
    reader: &mut R,
    canonical: &mut CanonicalCborWorkV1,
) -> Result<PaidCurrentReducedTaskHistoryDataV1<'f, 'a, 'c>, BodyMutationError> {
    use crate::world::incremental_normal_projection_retained_reader as retained;
    use std::{borrow::Borrow, collections::BTreeMap};
    facts.check()?;
    let ops = CurrentTaskReductionOpsV1 {
        facts,
        canonical: RefCell::new(canonical),
    };
    let mut hot = BTreeMap::new();
    {
        let mut canonical = ops.canonical.borrow_mut();
        aux.visit_hot_service_results_data_v1(&mut canonical, |key, value| {
            let value = admit_result(OriginalResultOwner::Hot(value), &ops)?;
            map_insert(&mut hot, ops.copy(key)?, value, &ops)?;
            Ok(ControlFlow::Continue(()))
        })?;
    }
    let mut collected = BTreeMap::new();
    for (key, value) in &hot {
        ops.work(1, 0)?;
        map_insert(
            &mut collected,
            ops.copy(key)?,
            value.clone_data(&ops)?,
            &ops,
        )?;
    }
    let root = facts.retained_root_data()?;
    let need_cold = if let Some(root) = root.as_ref() {
        let mut common = facts.common.borrow_mut();
        let mut canonical = ops.canonical.borrow_mut();
        !retained::is_canonical_empty_retained_root_data_v1(root, &mut common, &mut canonical)?
    } else {
        false
    };
    if need_cold {
        let root = root
            .as_ref()
            .ok_or_else(|| invalid("actual retained root disappeared"))?;
        facts.visit_agent_ids_data_v1(|agent| {
            let cold = {
                let mut canonical = ops.canonical.borrow_mut();
                aux.with_task_lookup_data_v1(
                    agent,
                    binding,
                    &mut canonical,
                    |lookup, canonical| match lookup {
                        None => Ok(None),
                        Some(lookup) => {
                            let mut common = facts.common.borrow_mut();
                            retained::read_task_results_data_v1(
                                root,
                                lookup,
                                reader,
                                &mut common,
                                canonical,
                            )
                            .map(Some)
                        }
                    },
                )?
            };
            if let Some(cold) = cold {
                for result in cold.into_values_data_v1() {
                    ops.work(1, 0)?;
                    let value = admit_result(OriginalResultOwner::Cold(result), &ops)?;
                    let record: &CanonicalIntentResultV1 = value.borrow();
                    let key = ops.digest("request-key", &record.request.correlation.key)?;
                    map_work(&hot, &key, &ops)?;
                    if let Some(previous) = hot.get(&key) {
                        let equal = {
                            let mut common = facts.common.borrow_mut();
                            let mut canonical = ops.canonical.borrow_mut();
                            retained::equal_canonical_service_result_data_v1(
                                previous.borrow(),
                                record,
                                &mut common,
                                &mut canonical,
                            )?
                        };
                        if !equal {
                            return Err(ops.business("hot/cold canonical result conflict"));
                        }
                    }
                    map_work(&collected, &key, &ops)?;
                    if let Some(previous) = collected.get(&key) {
                        let equal = {
                            let mut common = facts.common.borrow_mut();
                            let mut canonical = ops.canonical.borrow_mut();
                            retained::equal_canonical_service_result_data_v1(
                                previous.borrow(),
                                record,
                                &mut common,
                                &mut canonical,
                            )?
                        };
                        if !equal {
                            return Err(ops.business("ambiguous retained canonical task result"));
                        }
                    } else {
                        map_insert(&mut collected, key, value, &ops)?;
                    }
                }
            }
            Ok(ControlFlow::Continue(()))
        })?;
    }
    let n = collected
        .len()
        .checked_mul(size_of::<CurrentTaskResultDataV1>())
        .and_then(|n| n.checked_add(size_of::<Vec<CurrentTaskResultDataV1>>()))
        .ok_or_else(|| invalid("collected task result vector layout"))?;
    ops.before_owned(n)?;
    let mut results = Vec::with_capacity(collected.len());
    if results.capacity() != collected.len() {
        return Err(invalid("collected task result vector capacity"));
    }
    for (_, value) in collected {
        ops.work(1, 0)?;
        if task_payload(value.borrow()) {
            results.push(value)
        }
    }
    let tasks =
        crate::service_admission::game_task::reduce_rules::reduce_with_work_v1(results, &ops)?;
    facts.check()?;
    Ok(PaidCurrentReducedTaskHistoryDataV1 {
        tasks,
        _source: facts,
    })
}

pub(super) fn select_current_task_data_v1<'t>(
    history: &'t PaidCurrentReducedTaskHistoryDataV1<'_, '_, '_>,
    aux: &super::aux_facts::CurrentProjectionAuxFactsV1<'_, '_, '_>,
    binding: Option<&oasis7_agent_api::RuntimeBindingV1>,
    agent: &str,
    canonical: &mut CanonicalCborWorkV1,
) -> Result<
    Option<&'t crate::service_admission::game_task::ReducedTask<CurrentTaskResultDataV1>>,
    BodyMutationError,
> {
    use crate::service_admission::game_task::task_scope::{
        CurrentTaskScopeBorrowedDataV1, matches_current_task_scope_data_v1,
    };
    use crate::world::industrial_control::readonly_runtime_owner::{
        RuntimeOwnerReadonlyFactsV1, current_agent_identity_readonly_v1,
        owner_public_key_readonly_v1,
    };
    let facts = history._source;
    facts.check()?;
    let ops = CurrentTaskReductionOpsV1 {
        facts,
        canonical: RefCell::new(canonical),
    };
    let n = history
        .tasks
        .len()
        .checked_mul(size_of::<
            &crate::service_admission::game_task::ReducedTask<CurrentTaskResultDataV1>,
        >())
        .and_then(|n| n.checked_add(size_of::<Vec<usize>>()))
        .ok_or_else(|| invalid("current task candidates layout"))?;
    ops.before_owned(n)?;
    let mut relevant = Vec::with_capacity(history.tasks.len());
    if relevant.capacity() != history.tasks.len() {
        return Err(invalid("current task candidates capacity"));
    }
    for task in history.tasks.values() {
        let control = task.control_data_v1();
        ops.work(1, control.agent_id.len() + agent.len())?;
        if control.agent_id == agent {
            relevant.push(task)
        }
    }
    if relevant.is_empty() {
        facts.check()?;
        return Ok(None);
    }
    let identity = current_agent_identity_readonly_v1(aux, agent)?
        .ok_or_else(|| ops.business("task current owner identity unavailable"))?;
    let binding = binding
        .ok_or_else(|| invalid("actual selected task runtime binding source unavailable"))?;
    let owner = owner_public_key_readonly_v1(aux, agent)?.map_err(|note| ops.business(note))?;
    let selected = aux.runtime_manifest(|_, _, genesis| {
        let mut selected = None;
        for task in relevant {
            ops.work(1, 0)?;
            let control = task.control_data_v1();
            ops.control_comparison(control)?;
            let fence = oasis7_agent_api::game::GameTaskFenceV1::from_control(control);
            if !matches_current_task_scope_data_v1(
                CurrentTaskScopeBorrowedDataV1 {
                    agent,
                    world: &binding.world_id,
                    genesis,
                    branch: &binding.branch_id,
                    reorg: binding.reorg_epoch,
                    owner: &owner,
                    binding: &identity.owner_binding,
                    generation: identity.generation,
                },
                &fence,
                &mut |s, b| ops.work(s, b),
            )? {
                continue;
            }
            if selected.replace(task).is_some() {
                return Err(ops.business("ambiguous canonical task control"));
            }
        }
        Ok(selected)
    })?;
    facts.check()?;
    Ok(selected)
}

impl CurrentTaskReductionOpsV1<'_, '_, '_, '_> {
    fn validation(
        &self,
        run: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), BodyMutationError> {
        let fee = {
            let mut common = self.facts.common.borrow_mut();
            let n = size_of::<TaskShapeFailure>() + 4096;
            common.charge(1, n as u64).map_err(paid)?;
            common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?
        };
        run().map_err(|note| paid(TaskShapeFailure { note, _fee: fee }))
    }
    fn fence(
        &self,
        v: &oasis7_agent_api::game::GameTaskFenceV1,
    ) -> Result<oasis7_agent_api::game::GameTaskFenceV1, BodyMutationError> {
        self.before_owned(size_of::<oasis7_agent_api::game::GameTaskFenceV1>())?;
        Ok(oasis7_agent_api::game::GameTaskFenceV1 {
            world_id: self.copy(&v.world_id)?,
            genesis_digest: self.copy(&v.genesis_digest)?,
            branch_id: self.copy(&v.branch_id)?,
            reorg_epoch: v.reorg_epoch,
            agent_id: self.copy(&v.agent_id)?,
            owner_binding: self.copy(&v.owner_binding)?,
            owner_generation: v.owner_generation,
            owner_public_key: self.copy(&v.owner_public_key)?,
            task_id: self.copy(&v.task_id)?,
            task_revision: v.task_revision,
            task_status: v.task_status.clone(),
            latest_goal_revision: v.latest_goal_revision,
            active_goal_revision: v.active_goal_revision,
            active_goal_reference: v
                .active_goal_reference
                .as_ref()
                .map(|v| self.goal(v))
                .transpose()?,
            executor_epoch: v.executor_epoch,
            executor_binding: v
                .executor_binding
                .as_ref()
                .map(|v| self.binding(v))
                .transpose()?,
        })
    }
}
#[derive(Debug)]
struct TaskShapeFailure {
    note: String,
    _fee: MemoryReservation,
}
impl std::fmt::Display for TaskShapeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.note)
    }
}
impl std::error::Error for TaskShapeFailure {}
impl crate::world::task_terminal_v2::readonly_rules::TerminalSignatureOpsV2
    for CurrentTaskReductionOpsV1<'_, '_, '_, '_>
{
    type Error = BodyMutationError;
    type Witness =
        oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::PaidCborDataV1<
            CanonicalIntentResultV1,
        >;
    fn record_work(
        &self,
        record: &oasis7_world_state::TaskTerminalRecordV2,
    ) -> Result<(), Self::Error> {
        let mut common = self.facts.common.borrow_mut();
        let mut canonical = self.canonical.borrow_mut();
        crate::world::incremental_normal_projection_retained_reader::charge_terminal_record_data_v2(
            record,
            &mut common,
            &mut canonical,
        )
    }
    fn verify_original(
        &self,
        original: &crate::service_admission::SignedReadRequest<
            oasis7_agent_api::game::GameTaskRevokeAndCloseV2,
        >,
    ) -> Result<(), Self::Error> {
        self.meter(|meter|oasis7_client_api::world_service::private_goal::verify_owner_signature_strict_with_work_v1(oasis7_agent_api::game::GAME_TASK_CLEANUP_DOMAIN_V2,&original.request,&original.subject_public_key,&original.signature_hex,meter))?.map_err(paid)
    }
    fn validate_cleanup(
        &self,
        value: &oasis7_agent_api::game::GameTaskRevokeAndCloseV2,
    ) -> Result<(), Self::Error> {
        self.validation(|| value.validate())
    }
    fn validate_fence(
        &self,
        value: &oasis7_agent_api::game::GameTaskFenceV1,
    ) -> Result<(), Self::Error> {
        self.validation(|| value.validate())
    }
    fn hash<T: serde::Serialize>(&self, value: &T) -> Result<String, Self::Error> {
        let output = self
            .meter(|meter| oasis7_wasm_abi::canonical_hash_with_work_v1(value, meter))?
            .map_err(paid)?;
        self.copy(output.as_str())
    }
    fn raw_digest(&self, bytes: &[u8]) -> Result<String, Self::Error> {
        let output = {
            let mut common = self.facts.common.borrow_mut();
            crate::world::incremental_normal_projection_retained_reader::blake3_raw_data_v1(
                bytes,
                &mut common,
            )?
        };
        self.copy(output.as_str())
    }
    fn decode_witness(&self, bytes: &[u8]) -> Result<Self::Witness, Self::Error> {
        let mut common = self.facts.common.borrow_mut();
        crate::world::incremental_normal_projection_retained_reader::decode_terminal_stop_witness_data_v2(bytes,&mut common)
    }
    fn validate_result(&self, value: &CanonicalIntentResultV1) -> Result<(), Self::Error> {
        let key = self.digest("request-key", &value.request.correlation.key)?;
        self.meter(|meter| {
            oasis7_client_api::legacy_world_service::validate_canonical_result_fields_with_work_v1(
                &key, value, meter,
            )
        })?
        .map_err(paid)
    }
    fn executor_receipt(
        &self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<ExecutorReceipt, Self::Error> {
        TaskReductionOpsV1::executor_receipt(self, request)
    }
    fn clone_fence(
        &self,
        fence: &oasis7_agent_api::game::GameTaskFenceV1,
    ) -> Result<oasis7_agent_api::game::GameTaskFenceV1, Self::Error> {
        self.fence(fence)
    }
    fn business(&self, note: &'static str) -> Self::Error {
        TaskReductionOpsV1::business(self, note)
    }
}

impl CurrentTaskReductionOpsV1<'_, '_, '_, '_> {
    fn closed(
        &self,
        v: &oasis7_agent_api::game::GameTaskClosedV2,
    ) -> Result<oasis7_agent_api::game::GameTaskClosedV2, BodyMutationError> {
        use oasis7_agent_api::game::{
            GameOwnStopReferenceV2, GameTaskClosePredicateV2, GameTaskClosedV2,
        };
        self.before_owned(size_of::<GameTaskClosedV2>())?;
        let predicate = match &v.selected_predicate {
            GameTaskClosePredicateV2::InitialActive => GameTaskClosePredicateV2::InitialActive,
            GameTaskClosePredicateV2::OwnStopSuspended(reference) => {
                self.before_owned(size_of::<GameOwnStopReferenceV2>())?;
                GameTaskClosePredicateV2::OwnStopSuspended(Box::new(GameOwnStopReferenceV2 {
                    operation_id: self.copy(&reference.operation_id)?,
                    original_digest: self.copy(&reference.original_digest)?,
                    canonical_result_digest: self.copy(&reference.canonical_result_digest)?,
                    post_fence: self.fence(&reference.post_fence)?,
                }))
            }
        };
        Ok(GameTaskClosedV2 {
            schema_revision: v.schema_revision,
            close_operation_id: self.copy(&v.close_operation_id)?,
            close_original_digest: self.copy(&v.close_original_digest)?,
            termination_plan_digest: self.copy(&v.termination_plan_digest)?,
            initial_fence: self.fence(&v.initial_fence)?,
            actual_before_fence: self.fence(&v.actual_before_fence)?,
            selected_predicate: predicate,
            revoked_signer_public_key: self.copy(&v.revoked_signer_public_key)?,
            revoked_generation: v.revoked_generation,
            permanent_nonce_high_water: v.permanent_nonce_high_water,
        })
    }
    fn validate_terminal_index(
        &self,
        index: &oasis7_world_state::TaskTerminalIndexV2,
        aux: &super::aux_facts::CurrentProjectionAuxFactsV1<'_, '_, '_>,
    ) -> Result<(), BodyMutationError> {
        use crate::world::task_terminal_v2::readonly_rules::{
            TerminalSignatureOpsV2, signature_with_work_v2,
        };
        // Original index.validate completes all record shapes before the first
        // signature/floor check. Its bounded work hook does not alter a predicate.
        let error_fee = {
            let mut common = self.facts.common.borrow_mut();
            let n = size_of::<TaskShapeFailure>() + 4096;
            common.charge(1, n as u64).map_err(paid)?;
            common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?
        };
        match oasis7_world_state::validate_task_terminal_index_with_work_v2(
            index,
            &mut |key, record| {
                self.work(1, key.len())?;
                TerminalSignatureOpsV2::record_work(self, record)
            },
        ) {
            Ok(()) => {}
            Err(oasis7_world_state::TaskTerminalIndexValidationErrorV2::Work(error)) => {
                return Err(error);
            }
            Err(oasis7_world_state::TaskTerminalIndexValidationErrorV2::Invalid(note)) => {
                return Err(paid(TaskShapeFailure {
                    note,
                    _fee: error_fee,
                }));
            }
        }
        for record in index.records.values() {
            self.work(1, 0)?;
            signature_with_work_v2(record, self)?;
            let f = &record.closed.initial_fence;
            aux.with_signer_floor_data_v1(&f.agent_id, |current| {
                let current = current.ok_or_else(|| {
                    TaskReductionOpsV1::business(self, "terminal permanent signer floor missing")
                })?;
                self.work(1, 0)?;
                if current.generation < record.closed.revoked_generation.0 {
                    return Err(TaskReductionOpsV1::business(
                        self,
                        "terminal permanent signer floor regressed",
                    ));
                }
                self.work(1, 0)?;
                if current.nonce < record.closed.permanent_nonce_high_water.0 {
                    return Err(TaskReductionOpsV1::business(
                        self,
                        "terminal permanent signer floor regressed",
                    ));
                }
                self.work(1, 0)?;
                if current.generation == record.closed.revoked_generation.0 {
                    self.work(1, 0)?;
                    if !current.revoked {
                        return Err(TaskReductionOpsV1::business(
                            self,
                            "terminal permanent signer floor regressed",
                        ));
                    }
                    self.work(
                        1,
                        current.delegate_public_key.len()
                            + record.closed.revoked_signer_public_key.len(),
                    )?;
                    if current.delegate_public_key != record.closed.revoked_signer_public_key {
                        return Err(TaskReductionOpsV1::business(
                            self,
                            "terminal permanent signer floor regressed",
                        ));
                    }
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}
pub(in crate::world) struct PaidCurrentClosedProjectionDataV1 {
    projection: oasis7_client_api::formal_projection::ClosedTaskFormalProjectionV2,
    _fees: CurrentProjectionFactsFeesV1,
}
impl PaidCurrentClosedProjectionDataV1 {
    pub(in crate::world) fn projection(
        &self,
    ) -> &oasis7_client_api::formal_projection::ClosedTaskFormalProjectionV2 {
        &self.projection
    }
}
/// Fixed actual-source Closed consumer. Absence is returned solely for the
/// original non-Closed branches; a present terminal record takes the complete
/// original index/strict signature/Stop witness/permanent floor path.
pub(in crate::world) fn prepare_current_closed_projection_data_v1<
    R: crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
>(
    state: &OriginalStateColumnsDataV1,
    electricity: &OriginalProjectionRowsDataV1,
    auxiliary:&oasis7_world_state::observation_projection::current_commitment::ProjectionColumnDataV1,
    metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,
    raw_state: &super::super::raw_hash::PaidOriginalStateHashDataV1,
    binding: Option<&ReservedValue<oasis7_agent_api::RuntimeBindingV1>>,
    agent: &str,
    retained: &mut R,
    canonical: &mut CanonicalCborWorkV1,
    origin: &dyn crate::world::incremental_normal_projection_commitment::facts::ProjectionReadSourceV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<PaidCurrentClosedProjectionDataV1>, BodyMutationError> {
    origin.validate_metadata(metadata, common)?;
    raw_state.validate_context(common)?;
    let expected = metadata.runtime_binding_for_state_hash_bounded_v1(
        raw_state.as_str(),
        common,
        canonical,
    )?;
    if let Some(value) = binding {
        if !value.uses_budget(common.memory()) {
            return Err(invalid("closed formatter runtime binding pool"));
        }
    }
    common.consume_steps(1).map_err(paid)?;
    match (expected.as_ref(), binding) {
        (Some(expected), Some(actual)) => {
            for value in [&**expected, &**actual] {
                for text in [
                    &value.world_id,
                    &value.branch_id,
                    &value.finality_status,
                    &value.base_world_hash.0,
                    &value.runtime_manifest_hash.0,
                ] {
                    for part in text.as_bytes().chunks(8192) {
                        common.charge(1, part.len() as u64).map_err(paid)?;
                    }
                }
                if let Some(hash) = &value.finality_block_hash {
                    for part in hash.0.as_bytes().chunks(8192) {
                        common.charge(1, part.len() as u64).map_err(paid)?;
                    }
                }
                common.consume_steps(9).map_err(paid)?;
            }
            if **expected != **actual {
                return Err(invalid("closed formatter runtime binding source mismatch"));
            }
        }
        (None, None) => {}
        _ => return Err(invalid("closed formatter runtime binding source mismatch")),
    }
    let facts = CurrentProjectionReadFactsV1::for_projection_data_v1(
        state,
        electricity,
        metadata,
        origin,
        common,
    )?;
    let projection = {
        let aux = super::aux_facts::CurrentProjectionAuxFactsV1::new(&facts, auxiliary)?;
        let history = reduce_current_task_history_data_v1(
            &facts,
            &aux,
            binding.map(|v| &**v),
            retained,
            canonical,
        )?;
        let selected =
            select_current_task_data_v1(&history, &aux, binding.map(|v| &**v), agent, canonical)?;
        match selected {
            None => None,
            Some(task) => {
                let ops = CurrentTaskReductionOpsV1 {
                    facts: &facts,
                    canonical: RefCell::new(canonical),
                };
                let task_id = &task.control_data_v1().task_id;
                aux.with_task_terminal_index_data_v2(|index|{let Some(index)=index else{return Ok(None)};map_work(&index.records,task_id,&ops)?;let Some(record)=index.records.get(task_id)else{return Ok(None)};let closed=&record.closed;
    ops.validate_terminal_index(index,&aux)?;
    map_work(&index.records,&closed.initial_fence.task_id,&ops)?;let verified=index.records.get(&closed.initial_fence.task_id).ok_or_else(||ops.business("terminal projection record unavailable"))?;
    {let mut common=facts.common.borrow_mut();let mut canonical=ops.canonical.borrow_mut();crate::world::incremental_normal_projection_retained_reader::charge_terminal_record_data_v2(verified,&mut common,&mut canonical)?;}
    if verified.closed!=*closed{return Err(ops.business("terminal projection current/index mismatch"))}
    ops.before_owned(size_of::<oasis7_client_api::formal_projection::ClosedTaskFormalProjectionV2>())?;let closed=Box::new(ops.closed(&verified.closed)?);Ok(Some(oasis7_client_api::formal_projection::ClosedTaskFormalProjectionV2{api_revision:2,closed}))
   })?
            }
        }
    };
    facts.check()?;
    let fees = facts.into_fees();
    Ok(
        projection.map(|projection| PaidCurrentClosedProjectionDataV1 {
            projection,
            _fees: fees,
        }),
    )
}

/// Observation-only selected task. A reader rejection suppresses the control,
/// while IO/cancellation/work failures retain their typed first cause.
pub(in crate::world) struct PaidCurrentTaskControlDataV1 {
    control: Option<GameTaskControlV1>,
    _fees: CurrentProjectionFactsFeesV1,
}
impl PaidCurrentTaskControlDataV1 {
    pub(in crate::world) fn into_parts_v1(
        self,
    ) -> (Option<GameTaskControlV1>, CurrentProjectionFactsFeesV1) {
        (self.control, self._fees)
    }
}
pub(in crate::world) fn prepare_current_task_control_data_v1<
    R: crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
>(
    state: &OriginalStateColumnsDataV1,
    electricity: &OriginalProjectionRowsDataV1,
    auxiliary:&oasis7_world_state::observation_projection::current_commitment::ProjectionColumnDataV1,
    metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,
    binding: Option<&oasis7_agent_api::RuntimeBindingV1>,
    agent: &str,
    subject: &str,
    retained: &mut R,
    canonical: &mut CanonicalCborWorkV1,
    origin: &dyn crate::world::incremental_normal_projection_commitment::facts::ProjectionReadSourceV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidCurrentTaskControlDataV1, BodyMutationError> {
    let facts = CurrentProjectionReadFactsV1::for_projection_data_v1(
        state,
        electricity,
        metadata,
        origin,
        common,
    )?;
    let control = {
        let aux = super::aux_facts::CurrentProjectionAuxFactsV1::new(&facts, auxiliary)?;
        let history =
            reduce_current_task_history_data_v1(&facts, &aux, binding, retained, canonical)?;
        let selected = select_current_task_data_v1(&history, &aux, binding, agent, canonical)?;
        match selected {
            None => {
                let ops = CurrentTaskReductionOpsV1 {
                    facts: &facts,
                    canonical: RefCell::new(canonical),
                };
                let closed = aux.with_task_terminal_index_data_v2(|index| {
                    let Some(index) = index else { return Ok(false) };
                    let mut relevant = false;
                    for record in index.records.values() {
                        ops.work(1, record.closed.initial_fence.agent_id.len() + agent.len())?;
                        if record.closed.initial_fence.agent_id == agent {
                            relevant = true;
                            break;
                        }
                    }
                    if !relevant {
                        return Ok(false);
                    }
                    use crate::service_admission::game_task::task_scope::{
                        CurrentTaskScopeBorrowedDataV1, matches_current_task_scope_data_v1,
                    };
                    use crate::world::industrial_control::readonly_runtime_owner::{
                        RuntimeOwnerReadonlyFactsV1, current_agent_identity_readonly_v1,
                        owner_public_key_readonly_v1,
                    };
                    let identity = current_agent_identity_readonly_v1(&aux, agent)?
                        .ok_or_else(|| ops.business("task current owner identity unavailable"))?;
                    let binding = binding.ok_or_else(|| {
                        invalid("actual terminal fallback runtime binding source unavailable")
                    })?;
                    let owner = owner_public_key_readonly_v1(&aux, agent)?
                        .map_err(|note| ops.business(note))?;
                    aux.runtime_manifest(|_, _, genesis| {
                        let Some(genesis) = genesis else {
                            return Err(
                                ops.business("terminal fallback requires exact current genesis")
                            );
                        };
                        for record in index.records.values() {
                            ops.work(1, 0)?;
                            if matches_current_task_scope_data_v1(
                                CurrentTaskScopeBorrowedDataV1 {
                                    agent,
                                    world: &binding.world_id,
                                    genesis: Some(genesis),
                                    branch: &binding.branch_id,
                                    reorg: binding.reorg_epoch,
                                    owner: &owner,
                                    binding: &identity.owner_binding,
                                    generation: identity.generation,
                                },
                                &record.closed.initial_fence,
                                &mut |s, b| ops.work(s, b),
                            )? {
                                return Ok(true);
                            }
                        }
                        Ok(false)
                    })
                })?;
                if closed {
                    return Err(ops.business("closed task requires explicit V2 task identity"));
                }
                None
            }
            Some(task) => {
                let ops = CurrentTaskReductionOpsV1 {
                    facts: &facts,
                    canonical: RefCell::new(canonical),
                };
                // current_control validates all permanent terminal records when the selected
                // task has an index record, exactly like the original Closed path.
                let closed = aux.with_task_terminal_index_data_v2(|index| {
                    let Some(index) = index else { return Ok(false) };
                    map_work(&index.records, &task.control_data_v1().task_id, &ops)?;
                    if !index.records.contains_key(&task.control_data_v1().task_id) {
                        return Ok(false);
                    }
                    Ok(true)
                })?;
                if closed {
                    return Err(
                        ops.business("task Closed V2 is unsupported by legacy V1 lifecycle")
                    );
                } else {
                    let binding = binding.ok_or_else(|| {
                        invalid("actual selected task runtime binding source unavailable")
                    })?;
                    let value = task.control_data_v1();
                    ops.control_comparison(value)?;
                    let fence = oasis7_agent_api::game::GameTaskFenceV1::from_control(value);
                    // Fixed original fence validator; the actual graph census covers its
                    // string/option traversal before it runs, and refusal strings stay paid.
                    ops.before_owned(1024)?;
                    ops.work(64, 0)?;
                    let valid = fence.validate().is_ok();
                    if !valid {
                        None
                    } else {
                        ops.work(1, subject.len() + fence.owner_public_key.len())?;
                        let allowed = if subject == fence.owner_public_key {
                            true
                        } else {
                            let result=crate::service_admission::game_task::executor::validate_executor_binding_readonly_v1(&fence,subject,&mut|s,b|ops.work(s,b),&mut|key|{
      match ops.meter(|meter|oasis7_client_api::legacy_task_results::strong_key_with_work_v1(key,meter))?{Ok(())=>Ok(Ok(())),Err(oasis7_client_api::world_service::ServiceAuthorityWorkErrorV1::Work(error))=>Err(paid(error)),Err(oasis7_client_api::world_service::ServiceAuthorityWorkErrorV1::Protocol(error))=>Ok(Err(ops.copy(error.message())?))}
     },&mut|agent,key,generation|{
      use crate::world::industrial_control::readonly_runtime_owner::{owner_public_key_readonly_v1,current_agent_identity_readonly_v1};
      let owner=match owner_public_key_readonly_v1(&aux,agent)?{Ok(value)=>value,Err(note)=>return Ok(Err(ops.copy(note)?))};ops.work(1,owner.len()+key.len())?;
      match crate::service_admission::agent_authority::original_agent_signer_is_owner_v1(&owner,key,generation){Ok(true)=>return Ok(Ok(())),Err(note)=>return Ok(Err(ops.copy(note)?)),Ok(false)=>{}}
      aux.with_signer_floor_data_v1(agent,|delegation|{let Some(delegation)=delegation else{return Ok(Err(ops.copy("provider signer has no canonical delegation")?))};let Some(identity)=current_agent_identity_readonly_v1(&aux,agent)?else{return Ok(Err(ops.copy("Agent identity unavailable")?))};
       for text in [&delegation.delegate_public_key,&delegation.owner_binding,&delegation.world_id,&delegation.branch_id,&identity.owner_binding,&binding.world_id,&binding.branch_id]{for chunk in text.as_bytes().chunks(8192){ops.work(1,chunk.len())?;}}
       match crate::service_admission::agent_authority::validate_original_delegation_binding_v1(delegation,key,generation,&identity.owner_binding,identity.generation,&binding.world_id,&binding.branch_id){Ok(())=>Ok(Ok(())),Err(note)=>Ok(Err(ops.copy(note)?))}
      })
     })?;
                            result.is_ok()
                        };
                        if allowed {
                            Some(ops.control(value)?)
                        } else {
                            None
                        }
                    }
                }
            }
        }
    };
    facts.check()?;
    let fees = facts.into_fees();
    Ok(PaidCurrentTaskControlDataV1 {
        control,
        _fees: fees,
    })
}
