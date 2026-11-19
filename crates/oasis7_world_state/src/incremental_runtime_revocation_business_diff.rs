//! Fixed semantic business-ledger delta over authenticated Current columns.
//! Equal immutable subtrees are pruned by the original typed changed-path
//! walker. Keys come from the original member envelope, never from a hash guess.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use crate::incremental_inventory_value_read::InventoryValueReader;
use crate::incremental_typed_record_diff::{self as diff, ChangeDecoder};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation, ReservedValue};

/// Caller-owned fine processing allowances; IO retains its independent lanes.
pub struct BusinessDiffProcessingWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
}
impl BusinessDiffProcessingWorkV1 {
    fn spend(&mut self, n: usize) -> Result<(), ValueTreeError> {
        let n = u64::try_from(n).map_err(|_| invalid())?;
        self.steps_remaining = self
            .steps_remaining
            .checked_sub(n.checked_add(1).ok_or_else(invalid)?)
            .ok_or(ValueTreeError::ResourceLimited(
                "business diff processing steps",
            ))?;
        self.bytes_remaining =
            self.bytes_remaining
                .checked_sub(n)
                .ok_or(ValueTreeError::ResourceLimited(
                    "business diff processing bytes",
                ))?;
        Ok(())
    }
}
pub struct CurrentBusinessResultChangeV1 {
    key: ReservedValue<ColumnKeyV1>,
    before: Option<RevocationRecoveredV1<serde_json::Value>>,
    after: Option<RevocationRecoveredV1<serde_json::Value>>,
    _slot: MemoryReservation,
}
impl CurrentBusinessResultChangeV1 {
    pub fn key(&self) -> &str {
        semantic_key(&self.key).expect("fixed business key")
    }
    pub fn before(&self) -> Option<&serde_json::Value> {
        self.before.as_ref().map(|v| v.value())
    }
    pub fn after(&self) -> Option<&serde_json::Value> {
        self.after.as_ref().map(|v| v.value())
    }
    pub fn retained_allocation_bytes_v1(
        &self,
        cells: &mut u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<usize, ValueTreeError> {
        let mut n = self
            .key
            .reserved_bytes()
            .checked_add(self._slot.bytes())
            .ok_or_else(invalid)?;
        for row in self.before.iter().chain(self.after.iter()) {
            n = n
                .checked_add(row.retained_allocation_bytes_v1(cells, &mut *cancelled)?)
                .ok_or_else(invalid)?;
        }
        Ok(n)
    }
}
pub struct CurrentBusinessResultDeltaV1 {
    changes: Vec<CurrentBusinessResultChangeV1>,
    guard: PreparedGuard,
    parent: [u8; 32],
    before: ReservedValue<crate::incremental_record_columns_read::ColumnRowV1>,
    after: ReservedValue<crate::incremental_record_columns_read::ColumnRowV1>,
    _fee: MemoryReservation,
}
impl CurrentBusinessResultDeltaV1 {
    pub fn changes(&self) -> &[CurrentBusinessResultChangeV1] {
        &self.changes
    }
    pub fn before_len(&self) -> u64 {
        match &self.before.cell {
            ColumnCellV1::Root(r) => r.leaf_count.saturating_sub(2),
            _ => unreachable!(),
        }
    }
    pub fn after_len(&self) -> u64 {
        match &self.after.cell {
            ColumnCellV1::Root(r) => r.leaf_count.saturating_sub(2),
            _ => unreachable!(),
        }
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if self.parent != tx.parent_fence().parent_roots
            || !self._fee.uses_budget(memory)
            || !tx.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        Ok(())
    }
}
fn semantic_key(key: &ColumnKeyV1) -> Option<&str> {
    match key {
        ColumnKeyV1::Pair(a, b) => match (&**a, &**b) {
            (ColumnKeyV1::Text(a), ColumnKeyV1::Text(b)) if a == "entry" => Some(b),
            _ => None,
        },
        _ => None,
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_business_result_delta_v1<M, R, E, C, P, V, ER, CR>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentBusinessResultDeltaV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    overlay.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if overlay.values.class != MemoryClass::Ordinary
            || read.values.class != MemoryClass::Ordinary
            || qualification.allocation_class() != MemoryClass::Ordinary
            || !qualification.uses_budget(overlay.values.memory)
            || !read.values.memory.same_pool(overlay.values.memory)
            || !tx.uses_budget(overlay.values.memory)
        {
            return Err(invalid().into());
        }
        qualification.validate_digest(&tx.parent_fence().parent_roots)?;
        read.check()?;
        let fee = overlay
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentBusinessResultDeltaV1>() + 8192,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let body = {
            let mut records = InventoryValueReader {
                values: &mut overlay.values,
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                cancelled: &mut overlay.cancelled,
            };
            current_identity::current_body(tx, &mut records, &mut overlay.take_state_error)?
        };
        current.check(tx, &body, overlay.values.memory)?;
        let before = current_identity::map_root(
            qualification.body(),
            "world_service_results",
            read,
            scan,
            work,
        )?;
        let after = current_identity::map_root(&body, "world_service_results", read, scan, work)?;
        let (ColumnCellV1::Root(before_root), ColumnCellV1::Root(after_root)) =
            (&before.cell, &after.cell)
        else {
            return Err(invalid().into());
        };
        let profile = profile();
        let trees_remaining = work.max_retained_trees;
        let mut decoder = ChangeDecoder {
            ctx: read,
            scan,
            work,
            profile,
            trees_remaining,
            tree_depth: 0,
        };
        let mut changes = Vec::new();
        diff::visit_entries_with_processing(
            before_root,
            after_root,
            &mut decoder,
            |old, new, d| {
                let Some(cell) = new.or(old) else {
                    return Err(invalid());
                };
                // Metadata is authenticated by the selected container opener. Only
                // actual Entry rows contain original semantic keys.
                let entry = match cell {
                    diff::TypedCellRefV1::Entry(e) => e,
                    diff::TypedCellRefV1::Body(_) => return Err(invalid()),
                };
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if parsed.kind == WorldFieldRecordKindV1::Metadata {
                    return Ok(());
                }
                let key = diff::key(&profile, cell, d)?;
                if matches!(&*key,ColumnKeyV1::Text(v)if v=="_length") {
                    return Ok(());
                }
                if semantic_key(&key).is_none() {
                    return Err(invalid());
                }
                if let Some(old) = old {
                    let oldkey = diff::key(&profile, old, d)?;
                    if *oldkey != *key {
                        return Err(invalid());
                    }
                }
                let mut recover = |cell: Option<&diff::TypedCellRefV1<'_>>| -> Result<
                    Option<RevocationRecoveredV1<serde_json::Value>>,
                    ValueTreeError,
                > {
                    let Some(cell) = cell else { return Ok(None) };
                    let row = diff::row(&profile, cell, d)?;
                    let ColumnCellV1::Root(root) = &row.cell else {
                        return Err(invalid());
                    };
                    Ok(Some(
                        crate::incremental_typed_record_codec::recover_typed_record_body_v1(
                            root, &profile, d.ctx, d.scan, d.work,
                        )?,
                    ))
                };
                let old = recover(old)?;
                let new = recover(new)?;
                // One complete BTree-free Vec slot per changed leaf. Reserve before
                // growth; doubling slack is retained with its source until drop.
                let slot = d.ctx.values.memory.reserve(
                    2 * std::mem::size_of::<CurrentBusinessResultChangeV1>(),
                    MemoryClass::Ordinary,
                )?;
                changes.try_reserve_exact(1).map_err(|_| invalid())?;
                changes.push(CurrentBusinessResultChangeV1 {
                    key,
                    before: old,
                    after: new,
                    _slot: slot,
                });
                Ok(())
            },
            &mut |n| processing.spend(n),
        )?;
        read.check()?;
        if (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(e) = (overlay.take_state_error)() {
            return Err(e.into());
        }
        if let Some(e) = (read.take_state_error)() {
            return Err(e.into());
        }
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        Ok(CurrentBusinessResultDeltaV1 {
            changes,
            guard,
            parent: tx.parent_fence().parent_roots,
            before,
            after,
            _fee: fee,
        })
    })
}

/// Exact account-key point read for the original formal participant audience.
#[allow(clippy::too_many_arguments)]
pub fn read_current_business_participant_v1<M, R, E, C, P, V, ER, CR>(
    account: &str,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    Option<RevocationRecoveredV1<oasis7_agent_api::game::GameParticipantReceiptV1>>,
    WorldFieldOverlayErrorV1,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    overlay.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        qualification.validate_digest(&tx.parent_fence().parent_roots)?;
        if qualification.allocation_class() != MemoryClass::Ordinary
            || !qualification.uses_budget(overlay.values.memory)
            || !read.values.memory.same_pool(overlay.values.memory)
            || !tx.uses_budget(overlay.values.memory)
        {
            return Err(invalid().into());
        }
        let body = {
            let mut records = InventoryValueReader {
                values: &mut overlay.values,
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                cancelled: &mut overlay.cancelled,
            };
            current_identity::current_body(tx, &mut records, &mut overlay.take_state_error)?
        };
        current.check(tx, &body, overlay.values.memory)?;
        let profile = profile();
        let trees_remaining = work.max_retained_trees;
        let participants = {
            let mut d = ChangeDecoder {
                ctx: read,
                scan,
                work,
                profile,
                trees_remaining,
                tree_depth: 0,
            };
            diff::point_typed_column_v1(&profile, &body, "participants", &mut d, 8192)?
        };
        let ColumnCellV1::Root(participants) = &participants.cell else {
            return Err(invalid().into());
        };
        let row = current_identity::map_entry(participants, "bindings", account, read, scan, work)?;
        let out = match row.as_ref().map(|r| &r.cell) {
            None => None,
            Some(ColumnCellV1::Root(root)) => Some(
                crate::incremental_typed_record_codec::recover_typed_record_body_v1(
                    root, &profile, read, scan, work,
                )?,
            ),
            _ => return Err(invalid().into()),
        };
        read.check()?;
        if (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(e) = (overlay.take_state_error)() {
            return Err(e.into());
        }
        if let Some(e) = (read.take_state_error)() {
            return Err(e.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(out)
    })
}

/// Fixed monotone authority-generation observation. The scalar base is DATA;
/// Runtime must independently bind its real resident parent before using it.
pub struct CurrentAuthorityGenerationV1 {
    generation: u64,
    guard: PreparedGuard,
    _fee: MemoryReservation,
}
impl CurrentAuthorityGenerationV1 {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self._fee.uses_budget(memory) || !tx.uses_budget(memory) {
            return Err(invalid().into());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
#[expect(
    clippy::possible_missing_else,
    reason = "Independent typed-record validation guards preserve their first-error order"
)]
pub fn read_current_authority_generation_v1<M, R, E, C, P, V, ER, CR>(
    parent_max: u64,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentAuthorityGenerationV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    overlay.transaction.contribute(|tx|{
  tx.validate_guard(guard,live)?;qualification.validate_digest(&tx.parent_fence().parent_roots)?;
  if qualification.allocation_class()!=MemoryClass::Ordinary||!qualification.uses_budget(overlay.values.memory)||overlay.values.class!=MemoryClass::Ordinary||read.values.class!=MemoryClass::Ordinary||!read.values.memory.same_pool(overlay.values.memory)||!tx.uses_budget(overlay.values.memory){return Err(invalid().into())}
  let fee=overlay.values.memory.reserve(std::mem::size_of::<CurrentAuthorityGenerationV1>()+8192,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let body={let mut records=InventoryValueReader{values:&mut overlay.values,key_work:&mut*overlay.key_work,semantic_bytes:overlay.semantic_bytes,record_bytes:overlay.record_bytes,cancelled:&mut overlay.cancelled};current_identity::current_body(tx,&mut records,&mut overlay.take_state_error)?};current.check(tx,&body,overlay.values.memory)?;
  let before=current_identity::map_root(qualification.body(),"authority_records",read,scan,work)?;let after=current_identity::map_root(&body,"authority_records",read,scan,work)?;
  let(ColumnCellV1::Root(before),ColumnCellV1::Root(after))=(&before.cell,&after.cell)else{return Err(invalid().into())};
  let profile=profile();let trees_remaining=work.max_retained_trees;let mut d=ChangeDecoder{ctx:read,scan,work,profile,trees_remaining,tree_depth:0};let mut generation=parent_max;
  diff::visit_entries_with_processing(before,after,&mut d,|old,new,d|{
   let Some(cell)=new.or(old)else{return Err(invalid())};let entry=match cell{diff::TypedCellRefV1::Entry(e)=>e,_=>return Err(invalid())};let parsed=WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;if parsed.kind==WorldFieldRecordKindV1::Metadata{return Ok(())}
   let key=diff::key(&profile,cell,d)?;if matches!(&*key,ColumnKeyV1::Text(v)if v=="_length"){return Ok(())}if semantic_key(&key).is_none(){return Err(invalid())}
   let mut epoch=|cell:Option<&diff::TypedCellRefV1<'_>>|->Result<Option<u64>,ValueTreeError>{let Some(cell)=cell else{return Ok(None)};let oldkey=diff::key(&profile,cell,d)?;if *oldkey!=*key{return Err(invalid())}let row=diff::row(&profile,cell,d)?;let ColumnCellV1::Root(root)=&row.cell else{return Err(invalid())};let record=crate::incremental_typed_record_codec::recover_typed_record_body_v1::<crate::CapabilityAuthorityRecord,_,_,_,_>(root,&profile,d.ctx,d.scan,d.work)?;Ok(Some(record.value().governance_epoch))};
   let before=epoch(old)?;let after=epoch(new)?.ok_or(ValueTreeError::Invalid("authority source record removed"))?;
   if before.is_some_and(|n|n>parent_max||after<n){return Err(ValueTreeError::Invalid("authority source epoch decreased"))}
   d.work.json.cells_remaining=d.work.json.cells_remaining.checked_sub(1).ok_or(ValueTreeError::ResourceLimited("authority generation comparisons"))?;generation=generation.max(after);Ok(())
  },&mut|n|processing.spend(n))?;
  read.check()?;if(overlay.cancelled)(){return Err(ValueTreeError::Cancelled.into())}if let Some(e)=(overlay.take_state_error)(){return Err(e.into())}if let Some(e)=(read.take_state_error)(){return Err(e.into())}
  let guard=tx.guard();tx.validate_guard(&guard,live)?;Ok(CurrentAuthorityGenerationV1{generation,guard,_fee:fee})
 })
}
