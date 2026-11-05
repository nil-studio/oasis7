//! Borrowed complete genesis records, streamed into bounded inner COW steps.
//! The caller owns the frozen World and discards all emitted outer writes on error.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{StateDeltaV1, StatePageWriter, StateWriteV1},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::cell::RefCell;
fn body<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    owner: OrganizationRootKeyV1<'_>,
    source: super::super::WorldRuleOrganizationScalarsRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    max_pending: usize,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _operation = context
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut root = StateRootV1::empty(StateNamespaceV1::Authorization)
        .map_err(|_| MemberRecordError::Invalid("organization genesis empty tree"))?;
    visit_relative_body(source, &mut |record, path, payload| {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        *remaining = remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "organization genesis cells",
            ))?;
        let row = record.write_path(path, payload, context)?;
        let admission = inner_tree::workspace(context.memory, 1)?;
        let delta = {
            let source = inner_tree::Source {
                source: &*context.sink,
                memory: context.memory,
                class: MemoryClass::Ordinary,
                io: RefCell::new(&mut *context.io),
                cancelled: RefCell::new(&mut context.cancelled),
                error: RefCell::new(None),
            };
            // Every original relative path is unique in the closed visitor;
            // previous inner pages are already durable and readable by this sink.
            admission.retain(source.finish(StateDeltaV1::build(
                root.clone(),
                vec![StateWriteV1 {
                    key: row.field_key.as_bytes().to_vec(),
                    before: None,
                    after: Some(row.value_root),
                }],
                &source,
                max_pending,
            ))?)
        };
        inner_tree::persist(&delta, context)?;
        root = delta.after.clone();
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })?;
    write_organization_root_reference_v1(owner, &root, context)
}
/// Emits only the original outer Case/Estate keys, each bound to its own inner
/// root. Dynamic members are independent leaves; no Case or collection clone.
/// This is genesis production, not qualification of arbitrary restored roots.
pub fn prepare_qualified_organization_genesis_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &super::super::WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    max_pending_node_bytes: usize,
    sink: &mut S,
) -> Result<super::super::WorldRuleOrganizationGenesisCountsV1, MemberRecordError> {
    crate::incremental_world_rule_parent::descriptor()?;
    let mut counts = super::super::WorldRuleOrganizationGenesisCountsV1::default();
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(rules) = &source.world_rules {
        let (cases, estates) = super::super::genesis::owners(rules);
        for (key, value) in estates {
            let row = body(
                OrganizationRootKeyV1::Estate(key),
                super::super::WorldRuleOrganizationScalarsRefV1::Estate(value),
                context,
                remaining,
                max_pending_node_bytes,
            )?;
            *remaining = remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "organization genesis outer cells",
                ))?;
            crate::incremental_field_write::emit(row.field_key, row.value_root, context, sink)?;
            counts.estate = counts
                .estate
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid("organization estate count"))?;
        }
        for (key, value) in cases {
            let row = body(
                OrganizationRootKeyV1::Case(key),
                super::super::WorldRuleOrganizationScalarsRefV1::Case(value),
                context,
                remaining,
                max_pending_node_bytes,
            )?;
            *remaining = remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "organization genesis outer cells",
                ))?;
            crate::incremental_field_write::emit(row.field_key, row.value_root, context, sink)?;
            counts.cases = counts
                .cases
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid("organization case count"))?;
            counts.grants = counts
                .grants
                .checked_add(
                    u64::try_from(value.grants.len())
                        .map_err(|_| MemberRecordError::Invalid("organization grant count"))?,
                )
                .ok_or(MemberRecordError::Invalid("organization grant count"))?;
            counts.recovery = counts
                .recovery
                .checked_add(
                    u64::try_from(value.requests.len())
                        .map_err(|_| MemberRecordError::Invalid("organization recovery count"))?,
                )
                .ok_or(MemberRecordError::Invalid("organization recovery count"))?;
        }
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(counts)
}
