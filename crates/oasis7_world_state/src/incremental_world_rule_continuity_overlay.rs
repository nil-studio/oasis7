//! Fixed typed owner operations; canonical paths are assembled inside poison scope.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_world_rule_composite_family::{
        CompositeBranchCodecV1, WorldRuleCompositeQualificationV1,
    },
    incremental_world_rule_composite_overlay::{
        CompositePathPatchV1, WorldRuleCompositeOverlayV1, patch_world_rule_branch_v1,
        prepare_semantic_path_key_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1};
use oasis7_proto::memory_budget::MemoryClass;
pub type OrganizationContinuityOverlayV1 =
    WorldRuleCompositeOverlayV1<crate::world_rules::OrganizationContinuityStateV1>;
pub type EmergencySupplyOverlayV1 =
    WorldRuleCompositeOverlayV1<crate::world_rules::EmergencySupplyStateV1>;
enum Key<'a> {
    Field(&'static str),
    Entry(&'a str),
}

fn fixed_scalar<
    'a,
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
    const N: usize,
>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<T>>,
    roots: &RootSetV1,
    current: Option<&ReservedValue<WorldRuleCompositeOverlayV1<T>>>,
    keys: [Key<'a>; N],
    expected: [u8; 32],
    after: MemberPayloadRefV1<'a>,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut WorldRuleCompositeReadWorkV1,
    write_work: &mut WorldRuleCompositeWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<WorldRuleCompositeOverlayV1<T>>>, WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let _path_metadata = write
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if N > 8 {
            return Err(ValueTreeError::Invalid("fixed owner path depth").into());
        }
        let mut paths = Vec::with_capacity(N);
        {
            let mut combined = || (context.cancelled)() || (write.cancelled)();
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: &mut combined,
            };
            for key in keys {
                paths.push(match key {
                    Key::Field(name) => prepare_semantic_path_key_v1(name, &mut output)?,
                    Key::Entry(id) => prepare_semantic_path_key_v1(&("entry", id), &mut output)?,
                });
            }
        }
        let path: Vec<&[u8]> = paths.iter().map(|p| p.as_slice()).collect();
        let mut local = WorldFieldOverlayContextV1 {
            transaction: tx,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: context.values.source,
                memory: context.values.memory,
                class: context.values.class,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            take_state_error: &mut context.take_state_error,
            cancelled: &mut context.cancelled,
        };
        patch_world_rule_branch_v1(
            parent,
            roots,
            current,
            [CompositePathPatchV1 {
                path: &path,
                expected: Some(expected),
                after: Some(Encoded::Scalar(after)),
            }],
            state,
            &mut local,
            write,
            scan,
            read_work,
            write_work,
            guard,
            live,
        )
    })
}

/// `expected` is the exact original selected leaf value ID; unrelated records
/// and original map keys remain untouched. A failed call poisons the transaction.
pub fn set_world_rule_continuity_case_phase_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    parent: &ReservedValue<
        WorldRuleCompositeQualificationV1<crate::world_rules::OrganizationContinuityStateV1>,
    >,
    roots: &RootSetV1,
    current: Option<
        &ReservedValue<
            WorldRuleCompositeOverlayV1<crate::world_rules::OrganizationContinuityStateV1>,
        >,
    >,
    id: &str,
    expected: [u8; 32],
    after: crate::world_rules::OrganizationContinuityPhaseV1,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut WorldRuleCompositeReadWorkV1,
    write_work: &mut WorldRuleCompositeWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    Option<
        ReservedValue<
            WorldRuleCompositeOverlayV1<crate::world_rules::OrganizationContinuityStateV1>,
        >,
    >,
    WorldFieldOverlayErrorV1,
> {
    use crate::world_rules::OrganizationContinuityPhaseV1 as P;
    let tag = match after {
        P::AwaitingNotice => 0,
        P::Notice => 1,
        P::Protection => 2,
        P::CandidateCollection => 3,
        P::Protected => 4,
        P::Disposition => 5,
        P::Resolved => 6,
        P::Cancelled => 7,
    };
    let after = MemberPayloadRefV1::U64(tag);
    fixed_scalar(
        parent,
        roots,
        current,
        [
            Key::Field("cases"),
            Key::Entry(id),
            Key::Field("phase"),
            Key::Field("tag"),
        ],
        expected,
        after,
        state,
        context,
        write,
        scan,
        read_work,
        write_work,
        guard,
        live,
    )
}

/// `expected` is the exact original selected leaf value ID; unrelated records
/// and original map keys remain untouched. A failed call poisons the transaction.
pub fn set_world_rule_emergency_review_decision_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    parent: &ReservedValue<
        WorldRuleCompositeQualificationV1<crate::world_rules::EmergencySupplyStateV1>,
    >,
    roots: &RootSetV1,
    current: Option<
        &ReservedValue<WorldRuleCompositeOverlayV1<crate::world_rules::EmergencySupplyStateV1>>,
    >,
    id: &str,
    expected: [u8; 32],
    after: Option<bool>,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut WorldRuleCompositeReadWorkV1,
    write_work: &mut WorldRuleCompositeWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    Option<ReservedValue<WorldRuleCompositeOverlayV1<crate::world_rules::EmergencySupplyStateV1>>>,
    WorldFieldOverlayErrorV1,
> {
    let after = match after {
        None => MemberPayloadRefV1::Unit,
        Some(value) => MemberPayloadRefV1::Bool(value),
    };
    fixed_scalar(
        parent,
        roots,
        current,
        [
            Key::Field("reviews"),
            Key::Entry(id),
            Key::Field("decision"),
        ],
        expected,
        after,
        state,
        context,
        write,
        scan,
        read_work,
        write_work,
        guard,
        live,
    )
}

#[cfg(test)]
#[path = "incremental_world_rule_continuity_overlay_tests.rs"]
mod tests;
