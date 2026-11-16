//! Supply lifecycle rows derived only after the original closed record proof.
//! Unchanged records share both their paid row and their genuine record proof.
use super::*;
use crate::incremental_world_rules_family::{
    LifecycleSupplyAuthorizationV1, LifecycleSupplyBatchV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation};
use std::{
    collections::BTreeMap,
    mem::{align_of, size_of},
    sync::Arc,
};

fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("supply lifecycle projection")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("supply lifecycle projection")
}
fn step(cells: &mut u64, bytes: usize) -> Result<(), ValueTreeError> {
    *cells = cells
        .checked_sub(
            u64::try_from(bytes)
                .map_err(|_| limit())?
                .checked_add(1)
                .ok_or_else(limit)?,
        )
        .ok_or_else(limit)?;
    Ok(())
}
fn add(
    fee: &mut MemoryReservation,
    memory: &MemoryBudget,
    class: MemoryClass,
    bytes: usize,
) -> Result<(), ValueTreeError> {
    let mut next = memory.reserve(bytes, class)?;
    fee.merge(&mut next).map_err(ValueTreeError::Invalid)
}
fn arc_bytes<T>() -> usize {
    size_of::<ReservedValue<T>>() + 2 * size_of::<usize>() + 4 * align_of::<ReservedValue<T>>()
}
fn push<T>(
    out: &mut Vec<T>,
    value: T,
    fee: &mut MemoryReservation,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<(), ValueTreeError> {
    if out.len() == out.capacity() {
        let capacity = out.capacity().checked_mul(2).ok_or_else(limit)?.max(4);
        add(
            fee,
            memory,
            class,
            capacity.checked_mul(size_of::<T>()).ok_or_else(limit)?,
        )?;
        out.try_reserve_exact(capacity - out.len())
            .map_err(|_| limit())?;
        if out.capacity() > capacity {
            return Err(limit());
        }
    }
    out.push(value);
    Ok(())
}
#[derive(Debug)]
pub(in super::super) struct SupplyLifecycleRowsV1 {
    pub(super) authorizations: Vec<Arc<ReservedValue<LifecycleSupplyAuthorizationV1>>>,
    pub(super) batches: Vec<Arc<ReservedValue<LifecycleSupplyBatchV1>>>,
    authorization_records: Vec<Arc<ReservedValue<SupplyRecordQualificationV1>>>,
    batch_records: Vec<Arc<ReservedValue<SupplyRecordQualificationV1>>>,
    fee: MemoryReservation,
}
impl SupplyLifecycleRowsV1 {
    pub(super) fn empty(memory: &MemoryBudget, class: MemoryClass) -> Result<Self, ValueTreeError> {
        Ok(Self {
            authorizations: Vec::new(),
            batches: Vec::new(),
            authorization_records: Vec::new(),
            batch_records: Vec::new(),
            fee: memory.reserve(size_of::<Self>() + 64, class)?,
        })
    }
    pub(super) fn record(
        &self,
        owner: SupplyRootKeyV1<'_>,
    ) -> Option<&ReservedValue<SupplyRecordQualificationV1>> {
        match owner {
            SupplyRootKeyV1::Authorization(id) => self
                .authorizations
                .binary_search_by(|r| r.outer_id.as_str().cmp(id))
                .ok()
                .map(|n| &*self.authorization_records[n]),
            SupplyRootKeyV1::Batch(id) => self
                .batches
                .binary_search_by(|r| r.outer_id.as_str().cmp(id))
                .ok()
                .map(|n| &*self.batch_records[n]),
        }
    }
    pub(super) fn share(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
        cells: &mut u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        let mut next = Self::empty(memory, class)?;
        for (row, record) in self.authorizations.iter().zip(&self.authorization_records) {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            step(cells, row.outer_id.len())?;
            let q = record.clone();
            push(
                &mut next.authorizations,
                row.clone(),
                &mut next.fee,
                memory,
                class,
            )?;
            push(
                &mut next.authorization_records,
                q,
                &mut next.fee,
                memory,
                class,
            )?;
        }
        for (row, record) in self.batches.iter().zip(&self.batch_records) {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            step(cells, row.outer_id.len())?;
            let q = record.clone();
            push(&mut next.batches, row.clone(), &mut next.fee, memory, class)?;
            push(&mut next.batch_records, q, &mut next.fee, memory, class)?;
        }
        Ok(next)
    }
    pub(in super::super) fn insert<M, R, E, C>(
        &mut self,
        outer_id: String,
        kind: usize,
        qualification: ReservedValue<SupplyRecordQualificationV1>,
        ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        scan: &mut StateScanBudget,
        cells: &mut u64,
        max: u64,
    ) -> Result<(), ValueTreeError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        let memory = ctx.values.memory;
        let class = ctx.values.class;
        step(
            cells,
            outer_id
                .len()
                .checked_mul(usize::BITS as usize + 2)
                .and_then(|n| {
                    n.checked_add(
                        self.authorizations
                            .len()
                            .max(self.batches.len())
                            .checked_mul(2 * size_of::<usize>())?,
                    )
                })
                .ok_or_else(limit)?,
        )?;
        let owner = if kind == 0 {
            SupplyRootKeyV1::Authorization(&outer_id)
        } else if kind == 1 {
            SupplyRootKeyV1::Batch(&outer_id)
        } else {
            return Err(bad());
        };
        let mut fee = memory.reserve(4096 + outer_id.capacity(), class)?;
        let (id_field, flag_field) = match owner {
            SupplyRootKeyV1::Authorization(_) => (
                SupplyScalarFieldV1::Authorization(SupplyAuthorizationFieldV1::ProviderId),
                SupplyScalarFieldV1::Authorization(SupplyAuthorizationFieldV1::Active),
            ),
            SupplyRootKeyV1::Batch(_) => (
                SupplyScalarFieldV1::Batch(SupplyBatchFieldV1::AuthorizationId),
                SupplyScalarFieldV1::Batch(SupplyBatchFieldV1::Allocated),
            ),
        };
        let provider_or_authorization =
            text(owner, id_field, &qualification, ctx, cells, max, &mut fee)?;
        let flag = match read_qualified_supply_scalar_v1(
            SupplyScalarRequestV1 {
                owner,
                field: flag_field,
                qualification: &qualification,
            },
            ctx,
            max,
            &mut std::io::sink(),
        )? {
            Some(SupplyScalarValueV1::Bool(value)) => value,
            _ => return Err(bad()),
        };
        if kind == 0 {
            let id = text(
                owner,
                SupplyScalarFieldV1::Authorization(SupplyAuthorizationFieldV1::Id),
                &qualification,
                ctx,
                cells,
                max,
                &mut fee,
            )?;
            add(
                &mut fee,
                memory,
                class,
                arc_bytes::<LifecycleSupplyAuthorizationV1>(),
            )?;
            let row = Arc::new(fee.retain(LifecycleSupplyAuthorizationV1 {
                outer_id,
                id,
                provider_id: provider_or_authorization,
                active: flag,
            }));
            let at = self
                .authorizations
                .binary_search_by(|r| r.outer_id.cmp(&row.outer_id));
            add(&mut self.fee, memory, class, 2 * size_of::<usize>() + 64)?;
            let q = Arc::new(qualification);
            match at {
                Ok(n) => {
                    self.authorizations[n] = row;
                    self.authorization_records[n] = q;
                }
                Err(n) => {
                    push(&mut self.authorizations, row, &mut self.fee, memory, class)?;
                    push(
                        &mut self.authorization_records,
                        q,
                        &mut self.fee,
                        memory,
                        class,
                    )?;
                    self.authorizations[n..].rotate_right(1);
                    self.authorization_records[n..].rotate_right(1);
                }
            }
        } else {
            let allocation = match self
                .batches
                .binary_search_by(|r| r.outer_id.as_str().cmp(&outer_id))
            {
                Ok(n) => changed_allocations(
                    owner,
                    &self.batches[n].allocation,
                    self.batch_records[n].root(),
                    ctx,
                    scan,
                    cells,
                    max,
                    &mut fee,
                )?,
                Err(_) => allocations(owner, ctx, scan, cells, max, &mut fee)?,
            };
            add(
                &mut fee,
                memory,
                class,
                arc_bytes::<LifecycleSupplyBatchV1>(),
            )?;
            let row = Arc::new(fee.retain(LifecycleSupplyBatchV1 {
                outer_id,
                authorization_id: provider_or_authorization,
                allocated: flag,
                allocation,
            }));
            let at = self
                .batches
                .binary_search_by(|r| r.outer_id.cmp(&row.outer_id));
            add(&mut self.fee, memory, class, 2 * size_of::<usize>() + 64)?;
            let q = Arc::new(qualification);
            match at {
                Ok(n) => {
                    self.batches[n] = row;
                    self.batch_records[n] = q;
                }
                Err(n) => {
                    push(&mut self.batches, row, &mut self.fee, memory, class)?;
                    push(&mut self.batch_records, q, &mut self.fee, memory, class)?;
                    self.batches[n..].rotate_right(1);
                    self.batch_records[n..].rotate_right(1);
                }
            }
        }
        ctx.check()
    }
}
fn text<M, R, E, C>(
    owner: SupplyRootKeyV1<'_>,
    field: SupplyScalarFieldV1,
    qualification: &ReservedValue<SupplyRecordQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
    fee: &mut MemoryReservation,
) -> Result<String, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let n = match read_qualified_supply_scalar_v1(
        SupplyScalarRequestV1 {
            owner,
            field,
            qualification,
        },
        ctx,
        max,
        &mut std::io::sink(),
    )? {
        Some(SupplyScalarValueV1::Text { utf8_bytes }) => {
            usize::try_from(utf8_bytes).map_err(|_| limit())?
        }
        _ => return Err(bad()),
    };
    step(cells, n)?;
    add(
        fee,
        ctx.values.memory,
        ctx.values.class,
        n.checked_add(size_of::<String>()).ok_or_else(limit)?,
    )?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(n).map_err(|_| limit())?;
    if bytes.capacity() > n {
        return Err(limit());
    }
    read_qualified_supply_scalar_v1(
        SupplyScalarRequestV1 {
            owner,
            field,
            qualification,
        },
        ctx,
        max,
        &mut bytes,
    )?;
    if bytes.len() != n || bytes.capacity() > n {
        return Err(bad());
    }
    String::from_utf8(bytes).map_err(|_| bad())
}
fn allocations<M, R, E, C>(
    owner: SupplyRootKeyV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
    fee: &mut MemoryReservation,
) -> Result<BTreeMap<String, i64>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut output = BTreeMap::new();
    // The original record closure has already accepted every path. Reuse its
    // exact semantic decoder, retaining only actual allocation member values.
    let stop = std::cell::RefCell::new(&mut ctx.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: ctx.root,
        state: ctx.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: || (stop.borrow_mut())(),
    };
    let result = oasis7_formal_protocol::incremental_state::StateScanContext {
        source: ctx.state,
        memory: local.values.memory,
        class: local.values.class,
        io: scan,
    }
    .visit_entries(
        ctx.root,
        |entry| {
            step(cells, entry.key.len())?;
            let (key, len, semantic) =
                crate::incremental_world_rule_organization::root::closure::identity(
                    entry, &mut local, max,
                )?;
            if let schema::Key::Member("allocation", member) = schema::inspect(owner, &semantic)? {
                let node = 2usize
                    .checked_mul(
                        11 * (size_of::<String>() + size_of::<i64>())
                            + 12 * size_of::<usize>()
                            + 64,
                    )
                    .and_then(|n| n.checked_add(member.len()))
                    .ok_or_else(limit)?;
                step(
                    cells,
                    member
                        .len()
                        .checked_mul(usize::BITS as usize + 2)
                        .ok_or_else(limit)?,
                )?;
                add(fee, local.values.memory, local.values.class, node)?;
                let mut bytes = [0u8; 8];
                read_member_record_profile_v1(
                    &entry.value,
                    &key,
                    &mut local.values,
                    MemberReadProfileV1 {
                        codec: MemberPayloadCodecV1::I64,
                        semantic_bytes: len,
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: local.semantic_bytes,
                            payload_bytes: 8,
                            record_bytes: local.record_bytes,
                        },
                    },
                    &mut std::io::sink(),
                    &mut Cursor::new(&mut bytes[..]),
                    &mut local.cancelled,
                )?;
                if output
                    .insert(member.to_owned(), i64::from_be_bytes(bytes))
                    .is_some()
                {
                    return Err(bad());
                }
            }
            Ok(())
        },
        || (stop.borrow_mut())(),
    );
    let original = (local.take_state_error)();
    if (stop.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(e) = original {
        return Err(e);
    }
    match result {
        Ok(_) => {}
        Err(oasis7_formal_protocol::incremental_state::StateScanError::Visitor(e)) => {
            return Err(e);
        }
        Err(oasis7_formal_protocol::incremental_state::StateScanError::Memory(e)) => {
            return Err(e.into());
        }
        Err(oasis7_formal_protocol::incremental_state::StateScanError::Cancelled) => {
            return Err(ValueTreeError::Cancelled);
        }
        Err(oasis7_formal_protocol::incremental_state::StateScanError::ResourceLimited) => {
            return Err(limit());
        }
        Err(_) => return Err(bad()),
    }
    Ok(output)
}

fn changed_allocations<M, R, E, C>(
    owner: SupplyRootKeyV1<'_>,
    previous: &BTreeMap<String, i64>,
    before: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
    fee: &mut MemoryReservation,
) -> Result<BTreeMap<String, i64>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    // Only the resident selected allocation map is copied. The cold body is
    // never rescanned for an unchanged frozen-ledger/demand member.
    for key in previous.keys() {
        ctx.check()?;
        step(cells, key.len().checked_mul(2).ok_or_else(limit)?)?;
        let node = 2usize
            .checked_mul(
                11 * (size_of::<String>() + size_of::<i64>()) + 12 * size_of::<usize>() + 64,
            )
            .and_then(|n| n.checked_add(key.len()))
            .ok_or_else(limit)?;
        add(fee, ctx.values.memory, ctx.values.class, node)?;
    }
    let mut output = previous.clone();
    let stop = std::cell::RefCell::new(&mut ctx.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: ctx.root,
        state: ctx.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: || (stop.borrow_mut())(),
    };
    let mut cancel = || (stop.borrow_mut())();
    let result=crate::incremental_world_rule_organization::root::OrganizationChangedTreeContextV1 {source:ctx.state,memory:local.values.memory,class:local.values.class,budget:scan,cancelled:&mut cancel}.visit(before,ctx.root,|old,new| {
        let Some(entry)=new.or(old) else {return Err(bad());};
        step(cells,entry.key.len())?;
        let(key,len,semantic)=crate::incremental_world_rule_organization::root::closure::identity(entry,&mut local,max)?;
        let schema::Key::Member("allocation",member)=schema::inspect(owner,&semantic)? else {return Ok(());};
        step(cells,member.len().checked_mul(usize::BITS as usize+2).ok_or_else(limit)?)?;
        let mut number=|entry:&oasis7_formal_protocol::incremental_state::StateEntryV1|->Result<i64,ValueTreeError> {
            let mut bytes=[0u8;8];
            read_member_record_profile_v1(&entry.value,&key,&mut local.values,MemberReadProfileV1 {codec:MemberPayloadCodecV1::I64,semantic_bytes:len,limits:MemberRecordLimitsV1 {semantic_bytes:local.semantic_bytes,payload_bytes:8,record_bytes:local.record_bytes}},&mut std::io::sink(),&mut Cursor::new(&mut bytes[..]),&mut local.cancelled)?;
            Ok(i64::from_be_bytes(bytes))
        };
        let expected=old.map(&mut number).transpose()?;
        if output.get(member).copied()!=expected {return Err(bad());}
        match new {
            None=>{output.remove(member);},
            Some(entry)=>{
                let value=number(entry)?;
                let node=2usize.checked_mul(11*(size_of::<String>()+size_of::<i64>())+12*size_of::<usize>()+64).and_then(|n|n.checked_add(member.len())).ok_or_else(limit)?;
                add(fee,local.values.memory,local.values.class,node)?;
                output.insert(member.to_owned(),value);
            },
        }
        Ok(())
    });
    let original = (local.take_state_error)();
    if (stop.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = original {
        return Err(error);
    }
    result?;
    Ok(output)
}
