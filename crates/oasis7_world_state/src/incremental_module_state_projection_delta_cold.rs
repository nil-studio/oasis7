//! Complete qualified opaque byte-state enumeration; never interprets the
//! payload as CBOR or a World and never invents a pending write.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{StateScanBudget, StateScanContext, StateScanError},
};
pub struct QualifiedModuleStateProjectionColumnsV1 {
    states: u64,
    logical_bytes: u64,
    data: CurrentModuleStateProjectionDeltaV1,
}
impl QualifiedModuleStateProjectionColumnsV1 {
    pub fn states(&self) -> u64 {
        self.states
    }
    pub fn logical_bytes(&self) -> u64 {
        self.logical_bytes
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_bootstrap_read_only_v1(live)?;
        self.data.validate(tx, memory, live)
    }
    pub fn visit_with_work_v1<E: From<ValueTreeError>>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, &[u8], &[u8], &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.data.rows {
            work(1, 0)?;
            emit(
                "module_states",
                &row.semantic,
                &row.key,
                row.value.as_deref().map(Vec::as_slice).ok_or_else(bad)?,
            )?;
        }
        Ok(())
    }
}
pub fn read_qualified_module_state_projection_columns_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<ModuleStateFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    content: &mut ModuleStateContentBudgetV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<QualifiedModuleStateProjectionColumnsV1, BodyMutationError> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        ctx.transaction.validate_bootstrap_read_only_v1(live)?;
        validate_module_state_transaction_v1(parent, ctx.transaction, ctx.values.memory)?;
        parent.validate_binding(roots)?;
        if ctx.values.class != MemoryClass::Ordinary {
            return Err(bad().into());
        }
        let memory = ctx.values.memory.clone();
        let count = usize::try_from(parent.states()).map_err(|_| limit())?;
        pay(
            processing,
            count.checked_mul(size_of::<Pending>()).ok_or_else(limit)?,
            &mut ctx.cancelled,
        )?;
        let mut pending_fee = memory
            .reserve(
                count.checked_mul(size_of::<Pending>()).ok_or_else(limit)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut pending = Vec::with_capacity(count);
        if pending.capacity() != count {
            return Err(limit().into());
        }
        {
            let stop = RefCell::new(&mut ctx.cancelled);
            StateScanContext {
                source: state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: scan,
            }
            .visit_entries(
                roots.state_root(Ns::Business),
                |entry| {
                    pay(processing, entry.key.len(), &mut || (stop.borrow_mut())())?;
                    if !ModuleStateKeyV1::is_state_key(&entry.key) {
                        return Ok(());
                    }
                    ModuleStateKeyV1::state_key_from_bytes(&entry.key)
                        .map_err(ValueTreeError::Invalid)?;
                    if entry.namespace != Ns::Business || pending.len() == count {
                        return Err(bad().into());
                    }
                    pay(processing, entry.key.len(), &mut || (stop.borrow_mut())())?;
                    let mut fee = memory
                        .reserve(entry.key.len(), MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    pending_fee
                        .merge(&mut fee)
                        .map_err(ValueTreeError::Invalid)?;
                    let mut key = Vec::with_capacity(entry.key.len());
                    if key.capacity() != entry.key.len() {
                        return Err(limit().into());
                    }
                    for chunk in entry.key.chunks(8192) {
                        pay(processing, chunk.len(), &mut || (stop.borrow_mut())())?;
                        key.extend_from_slice(chunk);
                    }
                    pending.push(Pending {
                        key,
                        after: Some(entry.value),
                    });
                    Ok::<_, BodyMutationError>(())
                },
                || (stop.borrow_mut())(),
            )
            .map_err(|e| match e {
                StateScanError::Visitor(e) => e,
                StateScanError::Memory(e) => ValueTreeError::Memory(e).into(),
                StateScanError::ResourceLimited => limit().into(),
                StateScanError::Cancelled => ValueTreeError::Cancelled.into(),
                StateScanError::Invalid(e) => ValueTreeError::Invalid(e).into(),
                StateScanError::Tree(e) => {
                    BodyMutationError::Original(Box::new(io::Error::other(e)))
                }
            })?;
        }
        if pending.len() != count {
            return Err(bad().into());
        }
        let n = count
            .checked_mul(size_of::<Row>())
            .and_then(|n| n.checked_add(size_of::<QualifiedModuleStateProjectionColumnsV1>()))
            .ok_or_else(limit)?;
        pay(processing, n, &mut ctx.cancelled)?;
        let fee = memory
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut rows = Vec::with_capacity(count);
        if rows.capacity() != count {
            return Err(limit().into());
        }
        let mut logical_bytes = 0u64;
        for row in pending {
            let row = decode_projection_row_v1(row, ctx, content, processing)?;
            if row.value.is_none() {
                return Err(bad().into());
            }
            logical_bytes = logical_bytes
                .checked_add(row.logical_bytes)
                .ok_or_else(limit)?;
            rows.push(row)
        }
        if logical_bytes != parent.logical_bytes() {
            return Err(bad().into());
        }
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        pay(processing, 0, &mut ctx.cancelled)?;
        let out = QualifiedModuleStateProjectionColumnsV1 {
            states: parent.states(),
            logical_bytes,
            data: CurrentModuleStateProjectionDeltaV1 {
                rows,
                guard: ctx.transaction.guard(),
                parent: ctx.transaction.parent_fence().parent_roots,
                memory,
                _fee: fee,
            },
        };
        out.validate(ctx.transaction, ctx.values.memory, live)?;
        Ok(out)
    })();
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    result
}
