//! Exact Current claim selection and CAS through the original native codecs.
//! DATA observations only: no manufacturing/business admission is minted here.
use super::*;
use crate::incremental_world_native_gameplay::{AgentClaimsFieldV1, StarterOcClaimsFieldV1};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::StatePageWriter,
};
use oasis7_proto::memory_budget::MemoryBudget;
pub struct CurrentIndustrialClaimRecordV1<T> {
    value: Option<WorldNativeRecoveredRecordV1<T>>,
    key: WorldFieldKeyV1,
    root: Option<[u8; 32]>,
    memory: MemoryBudget,
}
impl<T> CurrentIndustrialClaimRecordV1<T> {
    pub fn value(&self) -> Option<&T> {
        self.value.as_ref().map(|r| r.value().value())
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
fn select<
    F: WorldNativeFieldSpecV1<Key = String>,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    parent: &ReservedValue<WorldNativeFieldQualificationV1<F>>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeSelectedReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentIndustrialClaimRecordV1<F::Value>>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        read::check(ctx)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !parent.uses_budget(ctx.values.memory)
            || parent.allocation_class() != MemoryClass::Ordinary
            || roots.digest().map_err(|_| invalid())? != tx.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        parent.validate_binding(roots)?;
        let field = F::descriptor()?;
        let profile = F::profile()?;
        let ns = profile.namespace()?;
        if ctx.root != roots.state_root(ns) {
            return Err(invalid().into());
        }
        let fee = ctx
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentIndustrialClaimRecordV1<F::Value>>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let _phase = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let meta = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let m = tx.read(ns, meta.as_bytes())?.ok_or_else(invalid)?;
        read::check(ctx)?;
        ctx.records().presence(&m, &meta, 0)?;
        let key = ctx.key(field, WorldFieldRecordKindV1::Entry, id)?.0;
        let root = tx.read(ns, key.as_bytes())?;
        read::check(ctx)?;
        let value = if let Some(root) = root {
            let body = read::read_body_at_entry_v1::<F, _, _, _, _>(&key, &root, ctx)?;
            let value = recover_typed_record_body_v1(&body, &profile, ctx, scan, work)?;
            F::validate_record_binding(WorldNativeKeyRefV1::Text(id), &body, ctx, scan, work)?;
            Some(value)
        } else {
            None
        };
        read::check(ctx)?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok(fee.retain(CurrentIndustrialClaimRecordV1 {
            value,
            key,
            root,
            memory: ctx.values.memory.clone(),
        }))
    })
}
macro_rules! selected {
    ($name:ident,$slot:ident,$spec:ty,$value:ty) => {
        #[expect(
            clippy::too_many_arguments,
            reason = "Bounded inputs are independently validated"
        )]
        pub fn $name<
            M: MerkleNodeReader,
            P: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
        >(
            id: &str,
            parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
            roots: &RootSetV1,
            tx: &mut ExecutionTransaction<'_, M>,
            ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
            scan: &mut StateScanBudget,
            work: &mut WorldNativeSelectedReadWorkV1,
            guard: &PreparedGuard,
            live: &ExecutionFence,
        ) -> Result<ReservedValue<CurrentIndustrialClaimRecordV1<$value>>, WorldFieldOverlayErrorV1>
        {
            tx.contribute(|tx| {
                if !parent.uses_budget(ctx.values.memory) || parent.class != MemoryClass::Ordinary {
                    return Err(invalid().into());
                }
                select::<$spec, _, _, _, _, _>(
                    id,
                    &parent.$slot,
                    roots,
                    tx,
                    ctx,
                    scan,
                    work,
                    guard,
                    live,
                )
            })
        }
    };
}
selected!(
    read_current_industrial_agent_claim_v1,
    agent_claims,
    AgentClaimsFieldV1,
    crate::AgentClaimState
);
selected!(
    read_current_industrial_starter_claim_v1,
    starter_oc_claims,
    StarterOcClaimsFieldV1,
    crate::StarterOcClaimState
);
/// Exact outer-root CAS remains valid across other fields' same-TX reads/writes;
/// the caller supplies a fresh strict guard and the real observed member root.
/// The DTO is data, never business permission.
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub fn write_current_industrial_agent_claim_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    id: &str,
    expected: &ReservedValue<CurrentIndustrialClaimRecordV1<crate::AgentClaimState>>,
    after: &crate::AgentClaimState,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    records: &mut u64,
    work: &mut crate::incremental_typed_record_codec::TypedRecordWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        read::check(ctx)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !expected.uses_budget(ctx.values.memory)
            || !expected.memory.same_pool(ctx.values.memory)
            || !tx.uses_budget(ctx.values.memory)
            || !tx.uses_budget(write.memory)
        {
            return Err(invalid().into());
        }
        let field = AgentClaimsFieldV1::descriptor()?;
        let key = ctx.key(field, WorldFieldRecordKindV1::Entry, id)?.0;
        if key != expected.key
            || tx.read(AgentClaimsFieldV1::profile()?.namespace()?, key.as_bytes())?
                != expected.root
        {
            return Err(invalid().into());
        }
        read::check(ctx)?;
        let _phase = write
            .memory
            .reserve(
                id.len().checked_add(32 << 10).ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut result = None;
        prepare_world_native_record_v1::<AgentClaimsFieldV1>(
            &id.to_owned(),
            after,
            write,
            records,
            work,
            &mut |row| {
                if result.is_some() {
                    return Err(invalid());
                }
                result = Some(row);
                Ok(())
            },
        )?;
        let row = result.ok_or_else(invalid)?;
        if row.mutation.key != key.as_bytes()
            || row.namespace != AgentClaimsFieldV1::profile()?.namespace()?
            || row.mutation.after.is_none()
        {
            return Err(invalid().into());
        }
        read::check(ctx)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(row.namespace, key.as_bytes(), row.mutation.after)?;
        read::check(ctx)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(())
    })
}

/// Delete only the selected actual outer member. Absence is not a successful
/// terminal transition, and no caller DTO is used to synthesize a tombstone.
pub fn delete_current_industrial_agent_claim_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    expected: &ReservedValue<CurrentIndustrialClaimRecordV1<crate::AgentClaimState>>,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        read::check(ctx)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !expected.uses_budget(ctx.values.memory)
            || !expected.memory.same_pool(ctx.values.memory)
            || !tx.uses_budget(ctx.values.memory)
            || expected.value().is_none()
            || expected.root.is_none()
        {
            return Err(invalid().into());
        }
        let key = ctx
            .key(
                AgentClaimsFieldV1::descriptor()?,
                WorldFieldRecordKindV1::Entry,
                id,
            )?
            .0;
        let ns = AgentClaimsFieldV1::profile()?.namespace()?;
        if key != expected.key || tx.read(ns, key.as_bytes())? != expected.root {
            return Err(invalid().into());
        }
        read::check(ctx)?;
        tx.write(ns, key.as_bytes(), None)?;
        read::check(ctx)?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok(())
    })
}

#[path = "incremental_industrial_delivery_claim_directory.rs"]
mod directory;
pub use directory::*;
