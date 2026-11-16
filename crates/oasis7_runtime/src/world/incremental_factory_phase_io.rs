//! One actual Common child for Root and every external factory-prologue lane.
//! Only decrementing fields enter the inventory; maxima remain caller limits.
use super::*;
use crate::world::incremental_module_invocation::RootModuleExecutionErrorV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::{ValueIoBudget, ValueReadContext},
};
use std::cell::RefCell;
use std::sync::atomic::Ordering;
pub(in crate::world) enum FactoryPhaseMeterLaneV1<'a> {
    Steps(&'a mut u64),
    Bytes(&'a mut u64),
    Entries(&'a mut usize),
}
pub(in crate::world) trait FactoryPhaseMeterLanesV1 {
    fn visit_lanes(&mut self, visit: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>));
}
fn scan(scan: &mut StateScanBudget, visit: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
    visit(FactoryPhaseMeterLaneV1::Steps(&mut scan.nodes_remaining));
    visit(FactoryPhaseMeterLaneV1::Bytes(
        &mut scan.read_bytes_remaining,
    ));
    visit(FactoryPhaseMeterLaneV1::Steps(&mut scan.leaves_remaining));
}
fn io(io: &mut ValueIoBudget, visit: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
    visit(FactoryPhaseMeterLaneV1::Steps(&mut io.nodes_remaining));
    visit(FactoryPhaseMeterLaneV1::Bytes(&mut io.read_bytes_remaining));
    visit(FactoryPhaseMeterLaneV1::Bytes(
        &mut io.write_bytes_remaining,
    ));
}
struct External<'a, 'read, 'factory, 'recipe, P, V, E, C, FR, FC, RR, RC> {
    read: &'a mut WorldFieldReadContextV1<'read, P, V, E, C>,
    factory: &'a mut FactoryTransitionReadContextV1<'factory, FR, FC>,
    recipe: &'a mut IndustryTransitionReadContextV1<'recipe, RR, RC>,
    work: &'a mut FactoryPhaseWorkV1,
}
impl<P, V, E, C, FR, FC, RR, RC> FactoryPhaseMeterLanesV1
    for External<'_, '_, '_, '_, P, V, E, C, FR, FC, RR, RC>
{
    fn visit_lanes(&mut self, visit: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
        use FactoryPhaseMeterLaneV1::Bytes as B;
        io(self.read.values.io, visit);
        visit(B(&mut self.read.key_work.bytes_remaining));
        io(self.factory.values.io, visit);
        visit(B(&mut self.factory.key_work.bytes_remaining));
        io(self.recipe.values.io, visit);
        visit(B(&mut self.recipe.key_work.bytes_remaining));
        visit_factory_phase_work_lanes_v1(self.work, visit);
    }
}
/// The single inventory of every decrementing FactoryPhaseWork lane. Both
/// prologue and terminal directory capture borrow this exact inventory.
pub(in crate::world) fn visit_factory_phase_work_lanes_v1(
    w: &mut FactoryPhaseWorkV1,
    visit: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>),
) {
    use FactoryPhaseMeterLaneV1::{Bytes as B, Entries as N, Steps as S};
    visit(B(&mut w.orders.ids.comparison_bytes_remaining));
    visit(N(&mut w.orders.pending_entries_remaining));
    visit(B(&mut w.orders.merge_comparison_bytes_remaining));
    visit(B(&mut w.records.comparisons_remaining));
    visit(N(&mut w.records.pending_entries_remaining));
    visit(S(&mut w.material_family.records_remaining));
    visit(S(&mut w.material_family.index_steps_remaining));
    visit(N(&mut w.material_scope.pending_entries_remaining));
    visit(B(&mut w.material_scope.comparison_bytes_remaining));
    visit(S(&mut w.factory.cells_remaining));
    scan(&mut w.factory.scan, visit);
    visit(S(&mut w.recipe.cells_remaining));
    scan(&mut w.recipe.scan, visit);
    visit(S(&mut w.industrial.read.cells_remaining));
    visit(B(&mut w.industrial.read.comparison_bytes_remaining));
    visit(S(&mut w.industrial.typed.json.cells_remaining));
    visit(S(&mut w.industrial.typed.json.json_nodes_remaining));
    scan(&mut w.industrial.scan, visit);
    visit(S(&mut w.industrial.revocation.json.cells_remaining));
    visit(S(&mut w.industrial.revocation.json.json_nodes_remaining));
    visit(N(&mut w.industrial.pending_entries_remaining));
    visit(S(&mut w.prepare.cells_remaining));
    visit(B(&mut w.prepare.comparison_bytes_remaining));
    visit(S(&mut w.reduce.cells_remaining));
    visit(B(&mut w.reduce.comparison_bytes_remaining));
    visit(S(&mut w.group.steps_remaining));
    scan(&mut w.enumeration.instance_scan, visit);
    scan(&mut w.enumeration.registry_scan, visit);
    visit(N(&mut w.enumeration.pending_entries));
    visit(B(&mut w.enumeration.comparison_bytes_remaining));
    scan(&mut w.scan, visit);
    visit(S(&mut w.typed.json.cells_remaining));
    visit(S(&mut w.typed.json.json_nodes_remaining));
}
#[derive(Debug)]
struct MeterError<OE>(RootModuleExecutionErrorV1<OE>);
impl<OE: std::fmt::Debug> std::fmt::Display for MeterError<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "factory prologue Common: {:?}", self.0)
    }
}
impl<OE: std::fmt::Debug + Send + Sync + 'static> std::error::Error for MeterError<OE> {}
fn map_error<OE: std::error::Error + Send + Sync + 'static>(
    error: RootModuleExecutionErrorV1<OE>,
) -> FactoryPhaseErrorV1<OE> {
    match error {
        RootModuleExecutionErrorV1::Value(e) => FactoryPhaseErrorV1::Value(e),
        RootModuleExecutionErrorV1::Body(e) => FactoryPhaseErrorV1::Body(e),
        other => {
            FactoryPhaseErrorV1::Body(BodyMutationError::Original(Box::new(MeterError(other))))
        }
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn begin_current_factory_prologue_bounded_v1<
        P,
        V,
        ER,
        CR,
        FR,
        FC,
        RR,
        RC,
    >(
        &mut self,
        caches: &FactoryPhaseAcceptedCachesV1<'_>,
        families: &FactoryPhaseFamiliesV1<'_>,
        expected_time: u64,
        read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
        factory: &mut FactoryTransitionReadContextV1<'_, FR, FC>,
        recipe: &mut IndustryTransitionReadContextV1<'_, RR, RC>,
        work: &mut FactoryPhaseWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<FactoryPhaseOutcomeV1, FactoryPhaseErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        V: ValueNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        FR: ValueNodeReader,
        FC: FnMut() -> bool,
        RR: ValueNodeReader,
        RC: FnMut() -> bool,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if read.values.class != MemoryClass::Ordinary
                || factory.values.class != MemoryClass::Ordinary
                || recipe.values.class != MemoryClass::Ordinary
                || !read.values.memory.same_pool(common.memory())
                || !factory.values.memory.same_pool(common.memory())
                || !recipe.values.memory.same_pool(common.memory())
            {
                return Err(FactoryPhaseErrorV1::Value(ValueTreeError::Invalid(
                    "factory prologue external Common pool/class",
                )));
            }
            let mut external = External {
                read,
                factory,
                recipe,
                work,
            };
            let mut first = None;
            let mapped = self.with_factory_prologue_external_meter_v1(
                common,
                live_owner,
                &mut external,
                |root, live, external| {
                    // The existing mapper's live callback polls and latches Common cancellation.
                    // Reborrow external readers so their long scans observe that same latch.
                    let live = RefCell::new(live);
                    let worker_stop = root.worker.cancelled;
                    let mut read_cancel = || {
                        let _ = (live.borrow_mut())();
                        (external.read.cancelled)() | worker_stop.load(Ordering::Relaxed)
                    };
                    let mut factory_cancel = || {
                        let _ = (live.borrow_mut())();
                        (external.factory.cancelled)() | worker_stop.load(Ordering::Relaxed)
                    };
                    let mut recipe_cancel = || {
                        let _ = (live.borrow_mut())();
                        (external.recipe.cancelled)() | worker_stop.load(Ordering::Relaxed)
                    };
                    let mut read = WorldFieldReadContextV1 {
                        root: external.read.root,
                        state: external.read.state,
                        values: ValueReadContext {
                            source: external.read.values.source,
                            memory: external.read.values.memory,
                            class: external.read.values.class,
                            io: &mut *external.read.values.io,
                        },
                        key_work: &mut *external.read.key_work,
                        semantic_bytes: external.read.semantic_bytes,
                        record_bytes: external.read.record_bytes,
                        take_state_error: &mut external.read.take_state_error,
                        cancelled: &mut read_cancel,
                    };
                    let mut factory = FactoryTransitionReadContextV1 {
                        values: ValueReadContext {
                            source: external.factory.values.source,
                            memory: external.factory.values.memory,
                            class: external.factory.values.class,
                            io: &mut *external.factory.values.io,
                        },
                        key_work: &mut *external.factory.key_work,
                        semantic_bytes: external.factory.semantic_bytes,
                        record_bytes: external.factory.record_bytes,
                        max_payload_bytes: external.factory.max_payload_bytes,
                        cancelled: &mut factory_cancel,
                    };
                    let mut recipe = IndustryTransitionReadContextV1 {
                        values: ValueReadContext {
                            source: external.recipe.values.source,
                            memory: external.recipe.values.memory,
                            class: external.recipe.values.class,
                            io: &mut *external.recipe.values.io,
                        },
                        key_work: &mut *external.recipe.key_work,
                        semantic_bytes: external.recipe.semantic_bytes,
                        record_bytes: external.recipe.record_bytes,
                        max_payload_bytes: external.recipe.max_payload_bytes,
                        cancelled: &mut recipe_cancel,
                    };
                    let mut live_owner = || (live.borrow_mut())();
                    match root.begin_current_factory_prologue_v1(
                        caches,
                        families,
                        expected_time,
                        &mut read,
                        &mut factory,
                        &mut recipe,
                        external.work,
                        &mut live_owner,
                    ) {
                        Ok(value) => Ok(value),
                        Err(error) => {
                            first = Some(error);
                            Err(RootModuleExecutionErrorV1::Body(
                                BodyMutationError::Invalid("factory prologue typed error retained"),
                            ))
                        }
                    }
                },
            );
            if let Some(error) = first {
                Err(error)
            } else {
                mapped.map_err(map_error)
            }
        }));
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(panic)
            }
        }
    }
}
