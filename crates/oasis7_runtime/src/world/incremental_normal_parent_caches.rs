//! Explicit GEN/cold bootstrap of the five original accepted normal caches.
//! Ordinary tick execution only borrows these owners and never invokes scans.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_factory_directory_orders::{
        AcceptedFactoryOrderDirectoryV1, FactoryOrderDirectoryWorkV1,
        build_accepted_factory_order_directory_v1,
    },
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
        build_accepted_invocation_directory_v1,
    },
    incremental_normal_source_action_views::{
        NormalActionParentCachesV1, bind_normal_action_source_parents_v1,
    },
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionError, ExecutionFence},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_factory_records::FactoryWorkBudgetV1,
    incremental_factory_schedule_index::{
        FactoryScheduleIndexV1, FactoryScheduleIndexWorkV1, build_factory_schedule_index_v1,
    },
    incremental_factory_schedule_receipts::{
        FactoryScheduleReceiptIndexV1, FactoryScheduleReceiptWorkV1,
        build_factory_schedule_receipt_index_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::IndustryWorkBudgetV1,
    incremental_inventory::{
        InventoryFamilyWorkV1, InventoryIdentityDirectoryV1, InventoryIdentityDirectoryWorkV1,
        build_inventory_identity_directory_v1,
    },
    incremental_world_rule_composite::WorldRuleCompositeReadWorkV1,
    incremental_world_rule_composite_family::bind_current_agent_orders_v1,
};
/// Explicit caller-owned continuous work. This view has no refill/default path.
pub struct NormalParentCacheBootstrapWorkV1<'a> {
    pub scan: &'a mut StateScanBudget,
    pub factory: &'a mut FactoryWorkBudgetV1,
    pub industry: &'a mut IndustryWorkBudgetV1,
    pub inventory: &'a mut InventoryFamilyWorkV1,
    pub builds: &'a mut FactoryScheduleIndexWorkV1,
    pub receipts: &'a mut FactoryScheduleReceiptWorkV1,
    pub identity: &'a mut InventoryIdentityDirectoryWorkV1,
    pub invocations: &'a mut CurrentInvocationEnumerationWorkV1,
    pub order_read: &'a mut WorldRuleCompositeReadWorkV1,
    pub orders: &'a mut FactoryOrderDirectoryWorkV1,
    pub pristine_entries: &'a mut usize,
    pub max_payload_bytes: u64,
}
/// Owning opaque producer outputs, never a caller-ID cache or acceptance token.
pub struct NormalParentCachesV1 {
    backing: NormalParentCacheBackingV1,
}
enum NormalParentCacheBackingV1 {
    Built {
        builds: ReservedValue<FactoryScheduleIndexV1>,
        receipts: ReservedValue<FactoryScheduleReceiptIndexV1>,
        inventory: ReservedValue<InventoryIdentityDirectoryV1>,
        invocations: ReservedValue<AcceptedInvocationDirectoryV1>,
        orders: ReservedValue<AcceptedFactoryOrderDirectoryV1>,
    },
    Warm(
        std::sync::Arc<
            ReservedValue<
                crate::world::incremental_normal_cache_promotion::NormalWarmCacheBackingV1,
            >,
        >,
    ),
}
impl NormalParentCachesV1 {
    pub(in crate::world) fn from_warm_backing_v1(
        owner: std::sync::Arc<
            ReservedValue<
                crate::world::incremental_normal_cache_promotion::NormalWarmCacheBackingV1,
            >,
        >,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<ReservedValue<Self>, ValueTreeError> {
        if !owner.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("warm normal parent cache pool"));
        }
        let fee = memory
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(Self {
            backing: NormalParentCacheBackingV1::Warm(owner),
        }))
    }
    pub(in crate::world) fn borrow(&self) -> NormalActionParentCachesV1<'_> {
        match &self.backing {
            NormalParentCacheBackingV1::Built {
                builds,
                receipts,
                inventory,
                invocations,
                orders,
            } => NormalActionParentCachesV1 {
                builds,
                receipts,
                inventory_identity: inventory,
                invocations,
                orders,
            },
            NormalParentCacheBackingV1::Warm(owner) => owner.borrow(),
        }
    }
    pub(in crate::world) fn validate_parent_v1(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let _bound = bind_normal_action_source_parents_v1(parent, self.borrow(), common)?;
        Ok(())
    }
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("GEN/cold normal cache bootstrap")
}
fn overlay(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
#[derive(Debug)]
struct Original<T: std::fmt::Debug>(T);
impl<T: std::fmt::Debug> std::fmt::Display for Original<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl<T: std::fmt::Debug> std::error::Error for Original<T> {}
fn pristine<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    entries: &mut usize,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError> {
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    for namespace in [
        StateNamespaceV1::Business,
        StateNamespaceV1::Allocator,
        StateNamespaceV1::Scheduler,
        StateNamespaceV1::Authorization,
        StateNamespaceV1::Effects,
        StateNamespaceV1::Artifacts,
    ] {
        ctx.transaction.visit_pending_writes::<BodyMutationError>(
            namespace,
            entries,
            || {
                if (ctx.cancelled)() {
                    Err(ValueTreeError::Cancelled.into())
                } else {
                    Ok(())
                }
            },
            |_, _, _| Err(bad()),
        )?;
    }
    Ok(())
}
/// This is deliberately a private, explicitly named bootstrap entry. The fixed
/// Host GEN/cold path must call it before actions; no normal cache miss calls it.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn bootstrap_genesis_or_cold_normal_parent_caches_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &BoundNormalTickParentV1<'_>,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut NormalParentCacheBootstrapWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<NormalParentCachesV1>, BodyMutationError> {
    ctx.transaction.contribute(|tx| {
        let mut local = WorldFieldOverlayContextV1 {
            transaction: tx,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        parent
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if local.transaction.parent_fence() != parent.fence() {
            return Err(bad());
        }
        let layout = common
            .memory()
            .reserve(
                std::mem::size_of::<NormalParentCachesV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let caches = paid(&mut local, work, common, |ctx, work| {
            pristine(ctx, work.pristine_entries, live)?;
            let refs = parent.refs();
            let current = refs.storage.current();
            let native = current.native();
            let (builds, receipts, inventory) = {
                let mut read = WorldFieldReadContextV1 {
                    root: refs.roots.state_root(StateNamespaceV1::Business),
                    state,
                    values: ValueReadContext {
                        source: ctx.values.source,
                        memory: ctx.values.memory,
                        class: ctx.values.class,
                        io: &mut *ctx.values.io,
                    },
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    take_state_error: &mut ctx.take_state_error,
                    cancelled: &mut ctx.cancelled,
                };
                let builds = build_factory_schedule_index_v1(
                    native.factory(),
                    refs.roots,
                    &mut read,
                    work.scan,
                    work.factory,
                    work.builds,
                    work.max_payload_bytes,
                )?;
                let receipts = build_factory_schedule_receipt_index_v1(
                    native.industry(),
                    refs.roots,
                    &mut read,
                    work.scan,
                    work.industry,
                    work.receipts,
                    work.max_payload_bytes,
                )
                .map_err(overlay)?;
                let inventory = build_inventory_identity_directory_v1(
                    native.inventory(),
                    refs.roots,
                    &mut read,
                    work.scan,
                    work.inventory,
                    work.identity,
                )?;
                (builds, receipts, inventory)
            };
            let invocations = build_accepted_invocation_directory_v1(
                refs.runtime,
                refs.head,
                current.raw().instances(),
                current.runtime().config(),
                state,
                ctx,
                work.invocations,
                live,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(Original(e))))?;
            let guard = ctx.transaction.guard();
            let order_source = bind_current_agent_orders_v1(
                native.rules().orders(),
                refs.roots,
                state,
                ctx,
                work.scan,
                work.order_read,
                &guard,
                live,
            )
            .map_err(overlay)?;
            let guard = ctx.transaction.guard();
            let orders = build_accepted_factory_order_directory_v1(
                refs.runtime,
                refs.head,
                &order_source,
                state,
                ctx,
                work.scan,
                work.order_read,
                work.orders,
                &guard,
                live,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(Original(e))))?;
            pristine(ctx, work.pristine_entries, live)?;
            Ok(NormalParentCachesV1 {
                backing: NormalParentCacheBackingV1::Built {
                    builds,
                    receipts,
                    inventory,
                    invocations,
                    orders,
                },
            })
        })?;
        caches.validate_parent_v1(parent, common)?;
        Ok(layout.retain(caches))
    })
}
#[allow(clippy::too_many_arguments)]
fn paid<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    T,
>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut NormalParentCacheBootstrapWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    run: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut dyn FnMut() -> bool>,
        &mut NormalParentCacheBootstrapWorkV1<'_>,
    ) -> Result<T, BodyMutationError>,
) -> Result<T, BodyMutationError> {
    if !ctx.values.memory.same_pool(common.memory())
        || !ctx.transaction.uses_budget(common.memory())
        || ctx.values.class != MemoryClass::Ordinary
    {
        return Err(bad());
    }
    common
        .charge(1, 4096)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let _phase = common
        .memory()
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    macro_rules! apply {
        (clamp_bytes; $($args:tt)*) => { clamp_bytes!($($args)*) };
        (clamp_steps; $($args:tt)*) => { clamp_steps!($($args)*) };
        (restore_bytes; $($args:tt)*) => { restore_bytes!($($args)*) };
        (restore_steps; $($args:tt)*) => { restore_steps!($($args)*) };
        (snapshot; $($args:tt)*) => { snapshot!($($args)*) };
    }
    macro_rules! steps {
        ($m:ident) => {
            apply!($m;
                ctx.values.io.nodes_remaining,
                work.scan.nodes_remaining,
                work.scan.leaves_remaining,
                work.factory.cells_remaining,
                work.factory.scan.nodes_remaining,
                work.factory.scan.leaves_remaining,
                work.industry.cells_remaining,
                work.industry.scan.nodes_remaining,
                work.industry.scan.leaves_remaining,
                work.inventory.records_remaining,
                work.inventory.index_steps_remaining,
                work.builds.cells_remaining,
                work.receipts.cells_remaining,
                work.identity.visits_remaining,
                work.invocations.instance_scan.nodes_remaining,
                work.invocations.instance_scan.leaves_remaining,
                work.invocations.registry_scan.nodes_remaining,
                work.invocations.registry_scan.leaves_remaining,
                work.order_read.json.cells_remaining,
                work.order_read.json.json_nodes_remaining
            )
        };
    }
    macro_rules! bytes {
        ($m:ident) => {
            apply!($m;
                ctx.values.io.read_bytes_remaining,
                ctx.values.io.write_bytes_remaining,
                ctx.key_work.bytes_remaining,
                work.scan.read_bytes_remaining,
                work.factory.scan.read_bytes_remaining,
                work.industry.scan.read_bytes_remaining,
                work.builds.comparison_bytes_remaining,
                work.receipts.bytes_remaining,
                work.identity.comparison_bytes_remaining,
                work.invocations.instance_scan.read_bytes_remaining,
                work.invocations.registry_scan.read_bytes_remaining,
                work.invocations.comparison_bytes_remaining,
                work.orders.ids.comparison_bytes_remaining,
                work.orders.merge_comparison_bytes_remaining
            )
        };
    }
    macro_rules! snapshot{($($v:expr),*)=>{[$($v),*]}}
    let original_steps = steps!(snapshot);
    let original_bytes = bytes!(snapshot);
    let original_pending = [
        work.identity.pending_entries_remaining,
        work.invocations.pending_entries,
        work.orders.pending_entries_remaining,
        *work.pristine_entries,
    ];
    let (_, meter, stop) = common.parts();
    let child = meter
        .begin_child()
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let step_cap = child.remaining_steps() / 24;
    let byte_cap = child.remaining_read_bytes() / 14;
    macro_rules! clamp_steps{($($v:expr),*)=>{$(($v)=($v).min(step_cap);)*}}
    macro_rules! clamp_bytes{($($v:expr),*)=>{$(($v)=($v).min(byte_cap);)*}}
    steps!(clamp_steps);
    bytes!(clamp_bytes);
    let pending_cap = usize::try_from(step_cap).unwrap_or(usize::MAX);
    work.identity.pending_entries_remaining =
        work.identity.pending_entries_remaining.min(pending_cap);
    work.invocations.pending_entries = work.invocations.pending_entries.min(pending_cap);
    work.orders.pending_entries_remaining = work.orders.pending_entries_remaining.min(pending_cap);
    *work.pristine_entries = (*work.pristine_entries).min(pending_cap);
    let before_steps = steps!(snapshot);
    let before_bytes = bytes!(snapshot);
    let before_pending = [
        work.identity.pending_entries_remaining,
        work.invocations.pending_entries,
        work.orders.pending_entries_remaining,
        *work.pristine_entries,
    ];
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut cancel = || stop() || (ctx.cancelled)();
        let mut local = WorldFieldOverlayContextV1 {
            transaction: &mut *ctx.transaction,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut cancel as &mut dyn FnMut() -> bool,
        };
        run(&mut local, work)
    }));
    let after_steps = steps!(snapshot);
    let after_bytes = bytes!(snapshot);
    let after_pending = [
        work.identity.pending_entries_remaining,
        work.invocations.pending_entries,
        work.orders.pending_entries_remaining,
        *work.pristine_entries,
    ];
    let accounted = (|| -> Result<(), BodyMutationError> {
        let mut steps = 0u64;
        let mut bytes = 0u64;
        for (a, b) in before_steps.into_iter().zip(after_steps) {
            steps = steps
                .checked_add(a.checked_sub(b).ok_or_else(bad)?)
                .ok_or_else(bad)?;
        }
        for (a, b) in before_pending.into_iter().zip(after_pending) {
            steps = steps
                .checked_add(u64::try_from(a.checked_sub(b).ok_or_else(bad)?).map_err(|_| bad())?)
                .ok_or_else(bad)?;
        }
        for (a, b) in before_bytes.into_iter().zip(after_bytes) {
            bytes = bytes
                .checked_add(a.checked_sub(b).ok_or_else(bad)?)
                .ok_or_else(bad)?;
        }
        child
            .consume(steps, bytes)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        Ok(())
    })();
    if accounted.is_ok() {
        child.finish();
        let mut at = 0usize;
        macro_rules! restore_steps{($($v:expr),*)=>{$(($v)=original_steps[at].saturating_sub(before_steps[at].saturating_sub(after_steps[at]));at+=1;)*}}
        steps!(restore_steps);
        let _ = at;
        let mut at = 0usize;
        macro_rules! restore_bytes{($($v:expr),*)=>{$(($v)=original_bytes[at].saturating_sub(before_bytes[at].saturating_sub(after_bytes[at]));at+=1;)*}}
        bytes!(restore_bytes);
        let _ = at;
        work.identity.pending_entries_remaining =
            original_pending[0].saturating_sub(before_pending[0].saturating_sub(after_pending[0]));
        work.invocations.pending_entries =
            original_pending[1].saturating_sub(before_pending[1].saturating_sub(after_pending[1]));
        work.orders.pending_entries_remaining =
            original_pending[2].saturating_sub(before_pending[2].saturating_sub(after_pending[2]));
        *work.pristine_entries =
            original_pending[3].saturating_sub(before_pending[3].saturating_sub(after_pending[3]));
    } else {
        drop(child)
    }
    let value = match result {
        Err(payload) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(payload)
        }
        Ok(Err(source)) => Err(source),
        Ok(Ok(value)) => accounted
            .and_then(|_| {
                common
                    .consume_steps(0)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))
            })
            .map(|_| value),
    };
    if value.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    value
}
