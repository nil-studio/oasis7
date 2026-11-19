//! Complete ordered grant/context data from the actual Current projection.
//! The snapshot is not a capability installation or an admission decision.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::{
    self as codec, BoundedCborDataErrorV1, CborDataLayoutV1, CborDataLimitsV1, PaidCborDataV1,
    PaidCborRefusalV1,
};
use std::collections::BTreeMap;
type Grants = BTreeMap<String, serde_json::Value>;
type Contexts = BTreeMap<String, crate::capability_authorization::CapabilityInvocationContext>;

/// Complete decoded DATA, inseparable from the strict Current snapshot it
/// came from. No input-only map or caller-selected grant can construct it.
pub(in crate::world) struct PaidCurrentBootstrapAuthInventoryV1<'a, 's, 'p> {
    snapshot: &'a PaidCurrentBootstrapAuthSnapshotV1<'s, 'p>,
    grants: PaidCborDataV1<Grants>,
    contexts: PaidCborDataV1<Contexts>,
    _fee: ReservedValue<()>,
}
impl PaidCurrentBootstrapAuthInventoryV1<'_, '_, '_> {
    /// Read-only catalogue joins may add real read dependencies. Keep the old
    /// strict cut intact and prove its authorization data has not changed.
    pub(in crate::world) fn validate_read_stability_v1<M:MerkleNodeReader>(
        &self,tx:&oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_,M>,
        remaining:&mut usize,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence,
    )->Result<(),BootstrapInventoryErrorV1>{
        self.grants.validate_context(common).map_err(BootstrapInventoryErrorV1::Decode)?;
        self.contexts.validate_context(common).map_err(BootstrapInventoryErrorV1::Decode)?;
        if !self._fee.uses_budget(common.memory()){return Err(BootstrapInventoryErrorV1::Source(invalid("bootstrap inventory stable pool").into()));}
        self.snapshot.validate_read_stability_v1(tx,remaining,common,live).map_err(BootstrapInventoryErrorV1::Source)
    }
    pub(in crate::world) fn grants(&self) -> &Grants {
        std::borrow::Borrow::<Grants>::borrow(&self.grants)
    }
    pub(in crate::world) fn invocation_contexts(&self) -> &Contexts {
        std::borrow::Borrow::<Contexts>::borrow(&self.contexts)
    }
    pub(in crate::world) fn validate_current<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        remaining: &mut usize,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BootstrapInventoryErrorV1> {
        self.grants
            .validate_context(common)
            .map_err(BootstrapInventoryErrorV1::Decode)?;
        self.contexts
            .validate_context(common)
            .map_err(BootstrapInventoryErrorV1::Decode)?;
        if !self._fee.uses_budget(common.memory()) {
            return Err(BootstrapInventoryErrorV1::Source(
                invalid("bootstrap decoded Current inventory pool").into(),
            ));
        }
        self.snapshot
            .validate_current(tx, remaining, common, live)
            .map_err(BootstrapInventoryErrorV1::Source)
    }
}
#[derive(Debug)]
pub(in crate::world) enum BootstrapInventoryErrorV1 {
    Source(SourceError),
    Decode(BoundedCborDataErrorV1),
}
impl std::fmt::Display for BootstrapInventoryErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BootstrapInventoryErrorV1 {}
fn inventory_layout<T>() -> CborDataLayoutV1 {
    use std::mem::size_of;
    // The only recursive graph is serde_json::Value. Context's subject,
    // presenter and audience are inline closed structs/enums containing Strings.
    let value = size_of::<crate::capability_authorization::CapabilityInvocationContext>()
        .max(size_of::<serde_json::Value>());
    CborDataLayoutV1 {
        root: size_of::<T>(),
        sequence_element: value,
        map_pair: size_of::<String>() + value,
        set_element: size_of::<String>(),
        box_pointee: value,
        default_heap_per_value: 64,
        symbols_bytes: 2048,
        visits: 256,
        missing: None,
    }
}

pub(in crate::world) struct PaidCurrentBootstrapAuthSnapshotV1<'s, 'p> {
    stage: &'s CurrentAuthorizationProjectionV1<'p>,
    grants: ReservedValue<Vec<u8>>,
    contexts: ReservedValue<Vec<u8>>,
    guard: PreparedGuard,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    fee: ReservedValue<()>,
}
impl PaidCurrentBootstrapAuthSnapshotV1<'_, '_> {
    pub(in crate::world) fn validate_read_stability_v1<M:MerkleNodeReader>(
        &self,tx:&oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_,M>,
        remaining:&mut usize,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence,
    )->Result<(),SourceError>{
        self.stage.validate(tx,common)?;
        tx.validate_transaction_identity(&self.guard).map_err(SourceError::Execution)?;
        tx.validate_guard(&tx.guard(),live).map_err(SourceError::Execution)?;
        if self.core!=common.work().identity() || !self.grants.uses_budget(common.memory())
            || !self.contexts.uses_budget(common.memory()) || !self.fee.uses_budget(common.memory()) {
            return Err(invalid("bootstrap Current auth read stability").into());
        }
        // self holds an immutable borrow of the exact stage, so its graph and
        // stage identity cannot refresh. Check all real net authorization writes
        // against that stage; do not replace the retained guard with tx.guard().
        self.stage.validate_registered_writes(tx,remaining,common)
    }
    pub(in crate::world) fn grants_cbor(&self) -> &[u8] {
        &self.grants
    }
    pub(in crate::world) fn invocation_contexts_cbor(&self) -> &[u8] {
        &self.contexts
    }
    pub(in crate::world) fn validate_current<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        remaining: &mut usize,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), SourceError> {
        self.stage.validate(tx, common)?;
        tx.validate_guard(&self.guard, live)
            .map_err(SourceError::Execution)?;
        if self.core != common.work().identity()
            || !self.grants.uses_budget(common.memory())
            || !self.contexts.uses_budget(common.memory())
            || !self.fee.uses_budget(common.memory())
        {
            return Err(invalid("bootstrap Current auth snapshot context").into());
        }
        self.stage.validate_registered_writes(tx, remaining, common)
    }
}
impl<'s, 'p> PaidCurrentBootstrapAuthSnapshotV1<'s, 'p> {
    pub(in crate::world) fn decode_inventory<'a>(
        &'a self,
        limits: &CborDataLimitsV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        Result<PaidCurrentBootstrapAuthInventoryV1<'a, 's, 'p>, PaidCborRefusalV1>,
        BoundedCborDataErrorV1,
    > {
        if self.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(BoundedCborDataErrorV1::Invalid(
                "bootstrap inventory source context",
            ));
        }
        let grants = match codec::decode_closed_cbor_data_v1::<Grants>(
            &self.grants,
            inventory_layout::<Grants>(),
            limits,
            common,
        )? {
            Ok(value) => value,
            Err(note) => return Ok(Err(note)),
        };
        let contexts = match codec::decode_closed_cbor_data_v1::<Contexts>(
            &self.contexts,
            inventory_layout::<Contexts>(),
            limits,
            common,
        )? {
            Ok(value) => value,
            Err(note) => return Ok(Err(note)),
        };
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<PaidCurrentBootstrapAuthInventoryV1<'a, 's, 'p>>(),
                MemoryClass::Ordinary,
            )?
            .retain(());
        Ok(Ok(PaidCurrentBootstrapAuthInventoryV1 {
            snapshot: self,
            grants,
            contexts,
            _fee: fee,
        }))
    }
}
fn field(
    stage: &CurrentAuthorizationProjectionV1<'_>,
    name: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>, SourceError> {
    let path = [AuthorizationProjectionPathV1::Field(name)];
    // Count and emit through the exact resident canonical writer. No guessed
    // entry count, point lookup or input-grant list substitutes for this map.
    let size = stage
        .data
        .write_subtree(&path, &mut std::io::sink(), common)?;
    let size = usize::try_from(size).map_err(|_| limited())?;
    let bytes = size
        .checked_add(std::mem::size_of::<Vec<u8>>())
        .ok_or_else(limited)?;
    let fee = common.memory().reserve(bytes, MemoryClass::Ordinary)?;
    let mut out = Vec::new();
    out.try_reserve_exact(size).map_err(|_| limited())?;
    if out.capacity() > size {
        return Err(limited().into());
    }
    let written = stage.data.write_subtree(&path, &mut out, common)?;
    if written != size as u64 || out.len() != size || out.capacity() > size {
        return Err(invalid("bootstrap Current auth canonical snapshot size").into());
    }
    Ok(fee.retain(out))
}
impl<'p> CurrentAuthorizationProjectionV1<'p> {
    pub(in crate::world) fn read_bootstrap_auth_snapshot_v1<'s, M: MerkleNodeReader>(
        &'s self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        remaining: &mut usize,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<PaidCurrentBootstrapAuthSnapshotV1<'s, 'p>, SourceError> {
        self.validate(tx, common)?;
        tx.validate_guard(&tx.guard(), live)
            .map_err(SourceError::Execution)?;
        // Unregistered after-writes must not turn this resident projection
        // into an alleged complete Current authority view.
        self.validate_registered_writes(tx, remaining, common)?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<PaidCurrentBootstrapAuthSnapshotV1<'s, 'p>>(),
                MemoryClass::Ordinary,
            )?
            .retain(());
        let grants = field(self, "grants", common)?;
        let contexts = field(self, "invocation_contexts", common)?;
        let guard = tx.guard();
        tx.validate_guard(&guard, live)
            .map_err(SourceError::Execution)?;
        common.consume_steps(0)?;
        Ok(PaidCurrentBootstrapAuthSnapshotV1 {
            stage: self,
            grants,
            contexts,
            guard,
            core: common.work().identity(),
            fee,
        })
    }
}
