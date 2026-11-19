//! Complete qualified WorldRules13 columns, only at explicit cold startup.
//! Absent old cells below are a decoder enumeration scope, never a fabricated
//! accepted predecessor or transaction pending-write.
use super::*;
use oasis7_formal_protocol::incremental_state::{StateScanContext, StateScanError};
pub struct QualifiedWorldRuleProjectionColumnsV1 {
    build: Build,
    rules_present: bool,
    roots: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl QualifiedWorldRuleProjectionColumnsV1 {
    pub fn rules_present(&self) -> bool {
        self.rules_present
    }
    pub fn branch_emission(&self) -> [Option<bool>; 2] {
        self.build.emission
    }
    pub fn validate_context(
        &self,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| bad())?
            || !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
        {
            return Err(bad());
        }
        Ok(())
    }
    pub fn visit_with_work_v1<E>(
        &self,
        work: &mut dyn FnMut(usize) -> Result<(), E>,
        mut emit: impl FnMut(&[String], &[u8], &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.build.rows {
            work(0)?;
            emit(
                &row.path,
                &row.semantic,
                row.json.as_ref().expect("ALL actual WorldRule cell"),
            )?
        }
        Ok(())
    }
}
fn branch<
    T: ProjectBranch,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<T>>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    p: &Processing<'_>,
    b: &mut Build,
) -> Result<(), ValueTreeError> {
    parent.validate_binding(roots)?;
    parent.check_budget(&b.memory, MemoryClass::Ordinary)?;
    let Some(body) = parent.body() else {
        if parent.parent_present() {
            return Err(bad());
        }
        if T::BRANCH == Branch::IndustrialWork {
            b.emission[0] = Some(false)
        }
        if T::BRANCH == Branch::Substitution {
            b.emission[1] = Some(false)
        }
        return Ok(());
    };
    let profile = T::BRANCH.profile();
    let trees_remaining = work.max_retained_trees;
    let mut local = WorldFieldReadContextV1 {
        root: body,
        state: read.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: read.values.source,
            memory: read.values.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &mut read.cancelled,
    };
    let mut d = ChangeDecoder {
        ctx: &mut local,
        scan,
        work,
        profile,
        trees_remaining,
        tree_depth: 0,
    };
    if matches!(T::BRANCH, Branch::IndustrialWork | Branch::Substitution) {
        let mut columns = read_columns_v1(
            ColumnReadProfileV1 {
                field: profile.field,
                schema: profile.schema,
                groups: &[],
                reference_schema: profile.reference_schema,
            },
            body,
            d.ctx,
            d.scan,
            d.work.max_tree_heap,
        )?;
        let mut empty = true;
        for value in columns.cells_mut().values() {
            spend(p, 0, &mut d.ctx.cancelled)?;
            let ColumnCellV1::Root(root) = value else {
                return Err(bad());
            };
            if root.leaf_count != 2 {
                empty = false
            }
        }
        b.emission[if T::BRANCH == Branch::IndustrialWork {
            0
        } else {
            1
        }] = Some(!empty)
    }
    T::project(
        &profile,
        None,
        Some(&TypedCellRefV1::Body(body)),
        &mut d,
        p,
        b,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn read_qualified_world_rule_projection_columns_v1<M, S, R, E, C>(
    parent: &ReservedValue<WorldRulesQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    typed_scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    live: &ExecutionFence,
) -> Result<QualifiedWorldRuleProjectionColumnsV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let original = ctx;
    let stop = std::cell::RefCell::new(&mut original.cancelled);
    let mut cancelled = || (stop.borrow_mut())();
    let mut local = WorldFieldOverlayContextV1 {
        transaction: &mut *original.transaction,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: original.values.source,
            memory: original.values.memory,
            class: original.values.class,
            io: &mut *original.values.io,
        },
        key_work: &mut *original.key_work,
        semantic_bytes: original.semantic_bytes,
        record_bytes: original.record_bytes,
        take_state_error: &mut original.take_state_error,
        cancelled: &mut cancelled,
    };
    let ctx = &mut local;
    ctx.transaction.validate_bootstrap_read_only_v1(live)?;
    parent.validate_binding(roots)?;
    let memory = ctx.values.memory.clone();
    let digest = roots.digest().map_err(|_| bad())?;
    if digest != ctx.transaction.parent_fence().parent_roots
        || !parent.uses_budget(&memory)
        || parent.allocation_class() != MemoryClass::Ordinary
        || ctx.values.class != MemoryClass::Ordinary
    {
        return Err(bad().into());
    }
    let p = Processing {
        bank: std::cell::RefCell::new(processing),
    };
    let wrapped = ProcessingState {
        source: state,
        processing: &p,
        first: std::cell::RefCell::new(None),
    };
    let fee = memory
        .reserve(
            size_of::<QualifiedWorldRuleProjectionColumnsV1>() + 4096,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut b = Build {
        emission: [None, None],
        rows: Vec::new(),
        slots: memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?,
        memory: memory.clone(),
        sources: Vec::new(),
        source_slots: memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?,
    };
    let rules_present = {
        let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        crate::incremental_world_rule_parent::read_parent_overlay(
            ctx.transaction,
            &mut records,
            &mut ctx.take_state_error,
        )?
        .1
    };
    if rules_present != parent.rules_present() {
        return Err(bad().into());
    }
    let scan_result = StateScanContext {
        source: &wrapped,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: outer_scan,
    }
    .visit_entries(
        roots.state_root(StateNamespaceV1::Authorization),
        |entry| -> Result<(), WorldFieldOverlayErrorV1> {
            spend(
                &p,
                entry
                    .key
                    .len()
                    .checked_mul(16)
                    .and_then(|v| v.checked_add(256))
                    .ok_or_else(limit)?,
                &mut ctx.cancelled,
            )?;
            if !relevant(&entry.key)? {
                return Ok(());
            }
            let parsed =
                WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            if parsed.kind == WorldFieldRecordKindV1::Metadata {
                return Ok(());
            }
            if parsed.kind != WorldFieldRecordKindV1::Entry || !rules_present {
                return Err(bad().into());
            }
            let key = WorldFieldKeyV1::new(
                crate::incremental_world_rule_parent::descriptor()?,
                parsed.kind,
                parsed.semantic_key,
            )
            .map_err(ValueTreeError::Invalid)?;
            let limits = MemberRecordLimitsV1 {
                semantic_bytes: ctx.semantic_bytes,
                payload_bytes: ctx.record_bytes,
                record_bytes: ctx.record_bytes,
            };
            let header = read_member_record_header_v1(
                &entry.value,
                &key,
                &mut ctx.values,
                limits,
                &mut ctx.cancelled,
            )?;
            let n = usize::try_from(header.semantic_bytes).map_err(|_| limit())?;
            spend(&p, n.checked_mul(16).ok_or_else(limit)?, &mut ctx.cancelled)?;
            let _semantic = memory
                .reserve(
                    n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let mut semantic = Vec::new();
            semantic.try_reserve_exact(n).map_err(|_| limit())?;
            if semantic.capacity() > n {
                return Err(limit().into());
            }
            read_member_record_key_v1(
                &entry.value,
                &key,
                &mut ctx.values,
                limits,
                &mut semantic,
                &mut ctx.cancelled,
            )?;
            if semantic.len() != n {
                return Err(bad().into());
            }
            let child = crate::incremental_world_rule_effects::world_rule_child_from_semantic_v1(
                &semantic,
            )?;
            if !Branch::ALL
                .iter()
                .any(|branch| Some(branch.name()) == child)
            {
                if b.sources.len() == b.sources.capacity() {
                    let cap = b
                        .sources
                        .capacity()
                        .checked_mul(2)
                        .ok_or_else(limit)?
                        .max(4);
                    let bytes = cap.checked_mul(size_of::<Source>()).ok_or_else(limit)?;
                    spend(&p, bytes, &mut ctx.cancelled)?;
                    let mut source_fee = memory
                        .reserve(bytes, MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    b.sources
                        .try_reserve_exact(cap - b.sources.len())
                        .map_err(|_| limit())?;
                    if b.sources.capacity() > cap {
                        return Err(limit().into());
                    }
                    b.source_slots
                        .merge(&mut source_fee)
                        .map_err(ValueTreeError::Invalid)?;
                }
                flat::project_qualified(
                    entry, &semantic, child, roots, &wrapped, ctx, typed_scan, work, &p, &mut b,
                    live,
                )?
            }
            Ok(())
        },
        || (stop.borrow_mut())(),
    );
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    if let Some(error) = wrapped.first.borrow_mut().take() {
        return Err(error.into());
    }
    match scan_result {
        Ok(_) => {}
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e).into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled.into()),
        Err(StateScanError::ResourceLimited) => return Err(limit().into()),
        _ => return Err(bad().into()),
    }
    if rules_present {
        let mut read = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Authorization),
            state: &wrapped,
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
            cancelled: &mut ctx.cancelled,
        };
        branch(
            parent.requests(),
            roots,
            &mut read,
            typed_scan,
            work,
            &p,
            &mut b,
        )?;
        branch(
            parent.orders(),
            roots,
            &mut read,
            typed_scan,
            work,
            &p,
            &mut b,
        )?;
        branch(
            parent.continuity(),
            roots,
            &mut read,
            typed_scan,
            work,
            &p,
            &mut b,
        )?;
        branch(
            parent.emergency(),
            roots,
            &mut read,
            typed_scan,
            work,
            &p,
            &mut b,
        )?;
        branch(
            parent.industrial(),
            roots,
            &mut read,
            typed_scan,
            work,
            &p,
            &mut b,
        )?;
        branch(
            parent.substitution(),
            roots,
            &mut read,
            typed_scan,
            work,
            &p,
            &mut b,
        )?;
    }
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    if let Some(error) = wrapped.first.borrow_mut().take() {
        return Err(error.into());
    }
    ctx.transaction.validate_bootstrap_read_only_v1(live)?;
    Ok(QualifiedWorldRuleProjectionColumnsV1 {
        build: b,
        rules_present,
        roots: digest,
        memory,
        _fee: fee,
    })
}

impl QualifiedWorldRuleProjectionColumnsV1 {
    /// Complete logical inventory from this reader's authenticated full scan and
    /// six original typed branch decoders. It never accepts caller paths/counts.
    pub fn visit_original_logical_rows_v1<
        V: super::seed::QualifiedWorldRuleLogicalRowsVisitorV1,
    >(
        &self,
        visitor: &mut V,
    ) -> Result<(), V::Error> {
        use super::seed::{WorldRuleLogicalColumnV1 as Column, WorldRuleLogicalTopSlotV1 as Top};
        visitor.work(1, 0)?;
        if !self.rules_present {
            return Ok(());
        }
        visitor.work(2, 0)?;
        let emission = [
            self.build.emission[0].ok_or_else(bad)?,
            self.build.emission[1].ok_or_else(bad)?,
        ];
        // The inventory and its branch-relative order are the original seed table.
        // Count each real projection row once under its unique logical column.
        let mut consumed = 0usize;
        for top in Top::ALL {
            visitor.work(1, 0)?;
            let emitted = match top {
                Top::IndustrialWork => emission[0],
                Top::Substitution => emission[1],
                _ => true,
            };
            visitor.top(top, top.original_emission_v1(), emitted)?;
            for &column in Column::ALL {
                visitor.work(1, column.path()[1].len().saturating_add(top.name().len()))?;
                if column.path()[1] != top.name() {
                    continue;
                }
                let mut count = 0usize;
                for row in &self.build.rows {
                    visitor.work(1, 0)?;
                    if cold_column_matches(column, &row.path, visitor)? {
                        count = count.checked_add(1).ok_or_else(limit)?
                    }
                }
                visitor.column(column, count)?;
                for row in &self.build.rows {
                    visitor.work(1, 0)?;
                    if !cold_column_matches(column, &row.path, visitor)? {
                        continue;
                    }
                    let json = row.json.as_ref().ok_or_else(bad)?;
                    visitor.row(column, &row.path, &row.semantic, json)?;
                    consumed = consumed.checked_add(1).ok_or_else(limit)?;
                }
            }
        }
        visitor.work(1, 0)?;
        if consumed != self.build.rows.len() {
            return Err(bad().into());
        }
        Ok(())
    }
}
fn cold_column_matches<V: super::seed::QualifiedWorldRuleLogicalRowsVisitorV1>(
    column: super::seed::WorldRuleLogicalColumnV1,
    path: &[String],
    visitor: &mut V,
) -> Result<bool, V::Error> {
    let expected = column.path();
    visitor.work(1, 0)?;
    if path.len() < expected.len() || path.len() > expected.len() + 1 {
        return Ok(false);
    }
    for (actual, expected) in path.iter().zip(expected) {
        visitor.work(1, actual.len().saturating_add(expected.len()))?;
        if actual != expected {
            return Ok(false);
        }
    }
    Ok(true)
}
