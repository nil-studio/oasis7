//! Exact ProductValidation recovery sources. Resident sparse indexes are DATA;
//! only actual accepted Journal originals or typed counted successes add rows.
use crate::world::World;
use crate::world::incremental_body::BodyMutationError;
use crate::world::incremental_world_event_counter::{
    CountedWorldEventSourceV1, WorldEventCounterV1, WorldJournalRetentionV1,
};
use crate::{
    DomainEvent, ProductValidationAttemptV1, ProductValidationReceiptV1, WorldEvent, WorldEventBody,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError, VerifiedFormalCommitV1,
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    },
    incremental_execution::{
        ExecutionError, ExecutionFactKindV1, ExecutionFence, ExecutionTransaction, PreparedGuard,
        RootSetV1, SealedExecutionBindingV1,
    },
    incremental_material::OriginalObjectStreamReader,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{
    MemoryBudget, MemoryClass, MemoryLimitExceeded, MemoryReservation, ReservedValue,
};
use oasis7_world_state::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_index::RuntimeHistoryIndexWorkV1,
    incremental_runtime_history_manifest::RuntimeHistoryLaneV1,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_world_event_decode::{DecodedWorldEventV1, WorldEventDecodeWorkV1},
};
use std::{
    mem::{align_of, size_of},
    sync::Arc,
};
#[path = "incremental_normal_industry_due_delivery_freeze.rs"]
mod freeze;
#[path = "incremental_normal_industry_due_delivery_original.rs"]
mod original;
#[path = "incremental_normal_industry_due_delivery_plan.rs"]
mod plan;
#[path = "incremental_normal_industry_due_delivery_source.rs"]
mod source;
#[path = "incremental_normal_industry_due_delivery_tree.rs"]
mod tree;
pub(in crate::world) use freeze::FrozenProductValidationDeliveryIndexV1;
pub(in crate::world) use plan::{
    PreparedProductValidationReplayV1, ProductValidationDeliveryPlanV1,
    ProductValidationReplayItemV1,
};
pub(in crate::world) use source::build_product_validation_delivery_index_v1;
#[derive(Debug)]
pub(in crate::world) enum DeliveryDataErrorV1 {
    Body(BodyMutationError),
    Value(ValueTreeError),
    Formal(FormalError),
}
impl From<BodyMutationError> for DeliveryDataErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<ValueTreeError> for DeliveryDataErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<MemoryLimitExceeded> for DeliveryDataErrorV1 {
    fn from(e: MemoryLimitExceeded) -> Self {
        Self::Value(ValueTreeError::Memory(e))
    }
}
impl From<FormalError> for DeliveryDataErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Formal(e)
    }
}
impl From<ExecutionError> for DeliveryDataErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Body(e.into())
    }
}
impl From<WorldFieldOverlayErrorV1> for DeliveryDataErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        match e {
            WorldFieldOverlayErrorV1::Value(e) => Self::Value(e),
            WorldFieldOverlayErrorV1::Execution(e) => Self::Body(e.into()),
            e => Self::Body(BodyMutationError::Original(Box::new(e))),
        }
    }
}
#[derive(Debug)]
pub(in crate::world) enum DeliverySourceErrorV1<E> {
    Data(DeliveryDataErrorV1),
    Original(E),
}
impl<E> From<DeliveryDataErrorV1> for DeliverySourceErrorV1<E> {
    fn from(e: DeliveryDataErrorV1) -> Self {
        Self::Data(e)
    }
}
impl<E> From<BodyMutationError> for DeliverySourceErrorV1<E> {
    fn from(e: BodyMutationError) -> Self {
        Self::Data(e.into())
    }
}
impl<E> From<ValueTreeError> for DeliverySourceErrorV1<E> {
    fn from(e: ValueTreeError) -> Self {
        Self::Data(e.into())
    }
}
impl<E> From<FormalError> for DeliverySourceErrorV1<E> {
    fn from(e: FormalError) -> Self {
        Self::Data(e.into())
    }
}
impl<E> From<MemoryLimitExceeded> for DeliverySourceErrorV1<E> {
    fn from(e: MemoryLimitExceeded) -> Self {
        Self::Data(e.into())
    }
}
impl<E> From<ExecutionError> for DeliverySourceErrorV1<E> {
    fn from(e: ExecutionError) -> Self {
        Self::Data(e.into())
    }
}
impl<E> From<WorldFieldOverlayErrorV1> for DeliverySourceErrorV1<E> {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Data(e.into())
    }
}
fn invalid(s: &'static str) -> DeliveryDataErrorV1 {
    ValueTreeError::Invalid(s).into()
}
fn limited() -> DeliveryDataErrorV1 {
    ValueTreeError::ResourceLimited("ProductValidation delivery index").into()
}
pub(in crate::world) struct ProductValidationDeliveryWorkV1 {
    pub index: RuntimeHistoryIndexWorkV1,
    pub decode: WorldEventDecodeWorkV1,
    pub originals_remaining: u64,
    pub original_bytes_remaining: u64,
}
type EventRef = Arc<ReservedValue<EventSlot>>;
struct EventSlot {
    event: ReservedValue<DecodedWorldEventV1>,
    original: [u8; 32],
    encoded: u64,
    position: u64,
    wraps: u64,
    lookup: [u8; 32],
    current: Option<CountedWorldEventSourceV1>,
}
impl EventSlot {
    fn value(&self) -> &WorldEvent {
        self.event.event()
    }
}
#[derive(Clone, Default)]
struct Trees {
    positions: Option<tree::NodeRef>,
    recorded: Option<tree::NodeRef>,
    attempts: Option<tree::NodeRef>,
    deliveries: Option<tree::NodeRef>,
    charges: Option<tree::NodeRef>,
}
#[derive(Clone)]
struct Data {
    trees: Trees,
    start: u64,
    end: u64,
    last_id: Option<u64>,
    wraps: u64,
}
pub(in crate::world) struct ProductValidationDeliveryIndexV1 {
    data: Data,
    head: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    parent_original: [u8; 32],
}
pub(in crate::world) struct CurrentProductValidationDeliveryIndexV1<'a> {
    parent: &'a ReservedValue<ProductValidationDeliveryIndexV1>,
    retention: &'a ReservedValue<WorldJournalRetentionV1>,
    data: Data,
    identity: PreparedGuard,
    core: WorkIdentityV1,
    counter: WorldEventCounterV1,
    next_fact: usize,
    appended: u64,
    references: Option<Arc<JournalReference>>,
    fee: ReservedValue<()>,
}
fn arc_heap<T>() -> Result<usize, DeliveryDataErrorV1> {
    size_of::<ReservedValue<T>>()
        .checked_add(2 * size_of::<usize>())
        .and_then(|n| n.checked_add(4 * align_of::<ReservedValue<T>>()))
        .ok_or_else(limited)
}
impl CurrentProductValidationDeliveryIndexV1<'_> {
    fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), DeliveryDataErrorV1> {
        common.consume_steps(1)?;
        tx.validate_transaction_identity(&self.identity)?;
        self.counter.validate(tx)?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || !self.parent.uses_budget(common.memory())
            || !self.retention.uses_budget(common.memory())
            || self.retention.allocation_class() != MemoryClass::Ordinary
            || !tx.uses_budget(common.memory())
            || self.parent.body != tx.parent_fence().parent_commit
            || self.parent.roots != tx.parent_fence().parent_roots
        {
            return Err(invalid("ProductValidation delivery Current binding"));
        }
        Ok(())
    }
}

struct JournalReference {
    original: [u8; 32],
    encoded: u64,
    position: u64,
    ordinal: usize,
    previous: Option<Arc<JournalReference>>,
    _fee: MemoryReservation,
}
impl Drop for JournalReference {
    fn drop(&mut self) {
        let mut previous = self.previous.take();
        while let Some(note) = previous {
            match Arc::try_unwrap(note) {
                Ok(mut note) => previous = note.previous.take(),
                Err(_) => break,
            }
        }
    }
}

#[path = "incremental_normal_industry_due_delivery_driver.rs"]
mod driver;
pub(in crate::world) use driver::{DueProductReplayOutcomeV1, RoutedProductReplayV1};
