//! Fixed system-lifecycle sparse writer for the seven continuity collections.
//! This helper has no arbitrary path or raw branch-patch input.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_typed_record_codec::{Codec, Encoded},
    incremental_world_rule_composite_family::CompositeBranchCodecV1,
    incremental_world_rule_composite_overlay::{
        CompositePathPatchV1, WorldRuleCompositeOverlayV1, patch_world_rule_branch_v1,
        prepare_semantic_path_key_v1,
    },
    incremental_world_rule_continuity::CurrentOrganizationContinuityV1,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionError, ExecutionFence, PreparedGuard, RootSetV1,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::mem::size_of;

fn system_bad() -> ValueTreeError {
    ValueTreeError::Invalid("organization continuity system Current writer")
}

#[allow(clippy::too_many_arguments)]
fn changed_field<'a, T, M, R, E, C, W, CW>(
    name: &'static str,
    after: &'a T,
    actual: &StateRootV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    write_work: &mut TypedRecordWriteWorkV1,
) -> Result<Option<(ReservedValue<Vec<u8>>, Option<[u8; 32]>, Encoded<'a>)>, WorldFieldOverlayErrorV1>
where
    T: Codec + PartialEq,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
{
    let current = point(
        actual,
        &[Key::Field(name)],
        &mut context.values,
        &mut *context.key_work,
        context.semantic_bytes,
        context.record_bytes,
        &mut context.take_state_error,
        &mut context.cancelled,
    )?;
    let (expected, previous_root) = match current {
        None => (None, None),
        Some((value, ColumnCellV1::Root(root))) => (Some(value), Some(root)),
        Some(_) => return Err(system_bad().into()),
    };
    let path = prepare_semantic_path_key_v1(name, write)?;
    let encoded = after.encode(&State::BRANCH.profile(), write, write_work)?;
    // Encoding uses the original closed codec and its normal writer meter. The
    // resulting canonical field root gives a bounded equality test without an
    // uncharged recursive `PartialEq` over a potentially large collection.
    if let (Some(previous), Encoded::Tree(next)) = (&previous_root, &encoded)
        && previous == next.root()
    {
        return Ok(None);
    }
    Ok(Some((path, expected, encoded)))
}

/// Apply only a fixed system-reducer result to the actual Current continuity
/// branch. The before state is always read from `current`; callers supply no
/// expected roots, field names, or arbitrary patch list.
#[allow(clippy::too_many_arguments)]
pub fn apply_current_organization_continuity_system_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    parent: &ReservedValue<OrganizationContinuityQualificationV1>,
    roots: &RootSetV1,
    current: &CurrentOrganizationContinuityV1,
    after: &State,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut WorldRuleCompositeReadWorkV1,
    write_work: &mut TypedRecordWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<WorldRuleCompositeOverlayV1<State>>>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        context.transaction.validate_guard(guard, live)?;
        current.validate_transaction_identity_v1(context.transaction, write.memory, live)?;
        parent.check_budget(write.memory, MemoryClass::Ordinary)?;
        if !parent.uses_budget(write.memory)
            || !context.transaction.uses_budget(write.memory)
            || !context.values.memory.same_pool(write.memory)
            || context.values.class != MemoryClass::Ordinary
            || !write.memory.same_pool(context.values.memory)
        {
            return Err(system_bad().into());
        }
        if current.value().is_none() {
            return Err(system_bad().into());
        }
        let actual = current.proof().ok_or_else(system_bad)?;
        // Read-set growth in unrelated selected facts does not change this
        // branch. Authenticate its actual Current root again before encoding
        // any diff, including the semantic no-change path.
        let live_guard = context.transaction.guard();
        let observed =
            crate::incremental_world_rule_composite_overlay::bind_current_world_rule_branch_v1(
                parent,
                roots,
                state,
                context,
                scan,
                read_work,
                &live_guard,
                live,
            )?;
        if observed.body() != actual.body() {
            return Err(system_bad().into());
        }

        let _layout = write
            .memory
            .reserve(
                7 * (size_of::<ReservedValue<Vec<u8>>>()
                    + size_of::<Option<[u8; 32]>>()
                    + size_of::<Option<Encoded<'_>>>()
                    + size_of::<Vec<&[u8]>>()
                    + size_of::<CompositePathPatchV1<'_>>())
                    + size_of::<Vec<ReservedValue<Vec<u8>>>>()
                    + size_of::<Vec<Option<[u8; 32]>>>()
                    + size_of::<Vec<Option<Encoded<'_>>>>()
                    + size_of::<Vec<Vec<&[u8]>>>()
                    + size_of::<Vec<CompositePathPatchV1<'_>>>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;

        let mut keys = Vec::with_capacity(7);
        let mut expected = Vec::with_capacity(7);
        let mut encoded = Vec::with_capacity(7);
        macro_rules! push_field {
            ($name:literal, $field:ident) => {
                if let Some((key, root, value)) = changed_field(
                    $name,
                    &after.$field,
                    actual.body(),
                    context,
                    write,
                    write_work,
                )? {
                    keys.push(key);
                    expected.push(root);
                    encoded.push(Some(value));
                }
            };
        }
        push_field!("registrations", registrations);
        push_field!("availability", availability);
        push_field!("activity_receipts", activity_receipts);
        push_field!("membership_history", membership_history);
        push_field!("assets", assets);
        push_field!("cases", cases);
        push_field!("consumed_effects", consumed_effects);
        if keys.capacity() != 7 || expected.capacity() != 7 || encoded.capacity() != 7 {
            return Err(ValueTreeError::Invalid("organization writer capacity").into());
        }
        if keys.is_empty() {
            return Ok(None);
        }
        let views: Vec<Vec<&[u8]>> = keys.iter().map(|key| vec![key.as_slice()]).collect();
        if views.capacity() > 7 || views.iter().any(|path| path.capacity() > 1) {
            return Err(ValueTreeError::Invalid("organization writer path capacity").into());
        }
        let mut changes = Vec::with_capacity(keys.len());
        for index in 0..keys.len() {
            changes.push(CompositePathPatchV1 {
                path: &views[index],
                expected: expected[index],
                after: encoded[index].take(),
            });
        }
        let current_guard = context.transaction.guard();
        let result = patch_world_rule_branch_v1(
            parent,
            roots,
            Some(actual),
            changes,
            state,
            context,
            write,
            scan,
            read_work,
            write_work,
            &current_guard,
            live,
        )?;
        Ok(result)
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
