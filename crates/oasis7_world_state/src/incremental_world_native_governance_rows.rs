//! Complete original governance rows, authenticated once and path-copied only
//! for actual changed members. Iterating output never re-decodes old records.
use super::*;
use crate::incremental_world_native_governance::*;
use oasis7_proto::memory_budget::MemoryReservation;
use std::{cmp::Ordering, sync::Arc};
pub(crate) enum GovernanceRecordV1 {
    Admission(WorldNativeRecoveredRecordV1<crate::governance::GovernanceValidatorAdmissionRecord>),
    Rotation(WorldNativeRecoveredRecordV1<crate::governance::FinalityKeyRotationRecordV1>),
    Bootstrap(
        WorldNativeRecoveredRecordV1<crate::governance::BootstrapFinalityKeyRotationRecordV1>,
    ),
}
pub(crate) struct GovernanceRowV1 {
    pub key: String,
    pub outer: [u8; 32],
    pub record: GovernanceRecordV1,
    pub bytes: usize,
}
#[path = "incremental_world_native_governance_tree.rs"]
mod tree;
#[derive(Clone)]
pub(crate) struct GovernanceRowsV1 {
    root: Option<tree::NodeRef>,
    pub count: u64,
    class: MemoryClass,
}
pub(crate) fn tracked<F: WorldNativeFieldSpecV1>() -> bool {
    matches!(
        F::NAME,
        "governance_validator_admissions"
            | "governance_finality_key_rotations"
            | "governance_bootstrap_finality_key_rotations"
    )
}
fn step(work: &mut u64, c: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
    if c() {
        return Err(ValueTreeError::Cancelled);
    }
    *work = work
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("governance resident work"))?;
    Ok(())
}
fn compare(
    a: &str,
    b: &str,
    work: &mut u64,
    c: &mut impl FnMut() -> bool,
) -> Result<Ordering, ValueTreeError> {
    step(work, c)?;
    *work =
        work.checked_sub(a.len().min(b.len()) as u64)
            .ok_or(ValueTreeError::ResourceLimited(
                "governance resident comparison",
            ))?;
    Ok(a.cmp(b))
}
impl GovernanceRowsV1 {
    pub(crate) fn retained_bytes(&self) -> usize {
        self.root.as_ref().map_or(0, |r| r.bytes)
    }
    pub(crate) fn empty(class: MemoryClass) -> Self {
        Self {
            root: None,
            count: 0,
            class,
        }
    }
    pub(crate) fn coverage(
        &self,
        memory: &MemoryBudget,
    ) -> Result<Option<MemoryReservation>, ValueTreeError> {
        if self.class == MemoryClass::Ordinary {
            Ok(None)
        } else {
            Ok(Some(memory.reserve(
                self.root.as_ref().map_or(0, |n| n.bytes),
                MemoryClass::Ordinary,
            )?))
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Bounded inputs are independently validated"
    )]
    pub(crate) fn replace<
        F: WorldNativeFieldSpecV1,
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        &mut self,
        key: &str,
        outer: Option<[u8; 32]>,
        body: Option<&StateRootV1>,
        ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        scan: &mut StateScanBudget,
        typed: &mut crate::incremental_typed_record_codec::TypedRecordReadWorkV1,
        cells: &mut u64,
    ) -> Result<(), ValueTreeError> {
        if let Some(body) = body {
            let (record, bytes) = match F::NAME {
                "governance_validator_admissions" => {
                    let r = recover_typed_record_body_v1::<
                        WorldNativeRecordV1<crate::governance::GovernanceValidatorAdmissionRecord>,
                        _,
                        _,
                        _,
                        _,
                    >(
                        body,
                        &GovernanceValidatorAdmissionsFieldV1::profile()?,
                        ctx,
                        scan,
                        typed,
                    )?;
                    let n = r.retained_allocation_bytes_v1(
                        &mut typed.json.cells_remaining,
                        &mut ctx.cancelled,
                    )?;
                    (GovernanceRecordV1::Admission(r), n)
                }
                "governance_finality_key_rotations" => {
                    let r = recover_typed_record_body_v1::<
                        WorldNativeRecordV1<crate::governance::FinalityKeyRotationRecordV1>,
                        _,
                        _,
                        _,
                        _,
                    >(
                        body,
                        &GovernanceFinalityKeyRotationsFieldV1::profile()?,
                        ctx,
                        scan,
                        typed,
                    )?;
                    let n = r.retained_allocation_bytes_v1(
                        &mut typed.json.cells_remaining,
                        &mut ctx.cancelled,
                    )?;
                    (GovernanceRecordV1::Rotation(r), n)
                }
                "governance_bootstrap_finality_key_rotations" => {
                    let r = recover_typed_record_body_v1::<
                        WorldNativeRecordV1<
                            crate::governance::BootstrapFinalityKeyRotationRecordV1,
                        >,
                        _,
                        _,
                        _,
                        _,
                    >(
                        body,
                        &GovernanceBootstrapFinalityKeyRotationsFieldV1::profile()?,
                        ctx,
                        scan,
                        typed,
                    )?;
                    let n = r.retained_allocation_bytes_v1(
                        &mut typed.json.cells_remaining,
                        &mut ctx.cancelled,
                    )?;
                    (GovernanceRecordV1::Bootstrap(r), n)
                }
                _ => return Err(invalid()),
            };
            let outerbytes = std::mem::size_of::<GovernanceRowV1>()
                .checked_add(key.len())
                .and_then(|n| n.checked_add(4096))
                .ok_or_else(invalid)?;
            let bytes = bytes.checked_add(outerbytes).ok_or_else(invalid)?;
            ctx.key_work.bytes_remaining = ctx
                .key_work
                .bytes_remaining
                .checked_sub(outerbytes as u64)
                .ok_or(ValueTreeError::ResourceLimited("governance row retain"))?;
            let fee = ctx.values.memory.reserve(outerbytes, ctx.values.class)?;
            let row = Arc::new(fee.retain(GovernanceRowV1 {
                key: key.to_owned(),
                outer: outer.ok_or_else(invalid)?,
                record,
                bytes,
            }));
            let (root, new) = tree::put(
                &self.root,
                row,
                ctx.values.memory,
                ctx.values.class,
                cells,
                &mut ctx.cancelled,
            )?;
            self.root = Some(root);
            if new {
                self.count = self.count.checked_add(1).ok_or_else(invalid)?;
            }
        } else {
            let (root, removed) = tree::remove(
                &self.root,
                key,
                ctx.values.memory,
                ctx.values.class,
                cells,
                &mut ctx.cancelled,
            )?;
            if !removed {
                return Err(invalid());
            }
            self.root = root;
            self.count = self.count.checked_sub(1).ok_or_else(invalid)?;
        }
        Ok(())
    }
    pub(crate) fn visit<E: From<ValueTreeError>>(
        &self,
        cells: &mut u64,
        c: &mut impl FnMut() -> bool,
        emit: &mut impl FnMut(&str, &GovernanceRecordV1, &mut u64) -> Result<(), E>,
    ) -> Result<(), E> {
        tree::visit(&self.root, cells, c, &mut |row, work| {
            emit(&row.key, &row.record, work)
        })
    }
    pub(crate) fn find(
        &self,
        key: &str,
        cells: &mut u64,
        c: &mut impl FnMut() -> bool,
    ) -> Result<Option<&GovernanceRowV1>, ValueTreeError> {
        tree::find(&self.root, key, cells, c)
    }
}
pub(crate) fn original(bytes: &[u8]) -> Result<&str, ValueTreeError> {
    match crate::incremental_semantic_key::read_semantic_key_v1(bytes)
        .map_err(ValueTreeError::Invalid)?
    {
        crate::incremental_semantic_key::SemanticKeyRefV1::Text(id) => Ok(id),
        _ => Err(invalid()),
    }
}
