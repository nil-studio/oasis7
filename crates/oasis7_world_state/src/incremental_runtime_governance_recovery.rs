//! Closed Runtime governance DTO recovery from authenticated column trees.
use super::*;
pub use crate::incremental_record_columns_read::RecoveredColumnsV1;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_record_columns_read::{
        self as columns, ColumnCellV1 as Cell, ColumnKeyV1 as Key, ColumnReadProfileV1,
    },
};
use oasis7_formal_protocol::incremental_state::{MerkleNodeReader, StateScanBudget};
use std::collections::BTreeMap;
pub enum RecoveredRuntimeGovernanceRecordV1 {
    Policy(GovernanceExecutionPolicy),
    EpochSnapshot(GovernanceFinalityEpochSnapshot),
    IdentityPenalty(GovernanceIdentityPenaltyRecord),
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime governance closed record")
}
fn narrow(value: u64) -> Result<u16, ValueTreeError> {
    u16::try_from(value).map_err(|_| invalid())
}
fn pair(a: &str, b: &str) -> Key {
    Key::pair(Key::text(a), Key::text(b))
}
fn count(map: &mut BTreeMap<Key, Cell>, name: &str) -> Result<usize, ValueTreeError> {
    let Some(Cell::U64(value)) = map.remove(&pair(name, "count")) else {
        return Err(invalid());
    };
    let value = usize::try_from(value).map_err(|_| invalid())?;
    if value > map.len() {
        return Err(invalid());
    }
    Ok(value)
}
pub fn recover_runtime_governance_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: RuntimeGovernanceKindV1,
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    max_heap: usize,
) -> Result<RecoveredColumnsV1<RecoveredRuntimeGovernanceRecordV1>, ValueTreeError> {
    let _scratch = ctx.values.memory.reserve(32 << 10, ctx.values.class)?;
    let mut body = columns::read_columns_v1(
        ColumnReadProfileV1 {
            field: kind.columns(),
            schema: kind.schema(),
            groups: &[],
        },
        root,
        ctx,
        scan,
        max_heap,
    )?;
    let result = match kind {
        RuntimeGovernanceKindV1::Policy => {
            let map = body.cells_mut();
            RecoveredRuntimeGovernanceRecordV1::Policy(GovernanceExecutionPolicy {
                timelock_ticks: columns::unsigned(map, "timelock_ticks")?,
                epoch_length_ticks: columns::unsigned(map, "epoch_length_ticks")?,
                activation_delay_epochs: columns::unsigned(map, "activation_delay_epochs")?,
                emergency_brake_guardian_threshold: narrow(columns::unsigned(
                    map,
                    "emergency_brake_guardian_threshold",
                )?)?,
                emergency_veto_guardian_threshold: narrow(columns::unsigned(
                    map,
                    "emergency_veto_guardian_threshold",
                )?)?,
                emergency_brake_max_ticks: columns::unsigned(map, "emergency_brake_max_ticks")?,
            })
        }
        RuntimeGovernanceKindV1::EpochSnapshot => {
            let signer_count = count(body.cells_mut(), "signer_node_ids")?;
            let stake_count = count(body.cells_mut(), "validator_stakes")?;
            body.spend_output(
                signer_count
                    .checked_mul(std::mem::size_of::<String>())
                    .and_then(|n| stake_count.checked_mul(160).and_then(|m| n.checked_add(m)))
                    .ok_or_else(invalid)?,
            )?;
            let map = body.cells_mut();
            let mut value = GovernanceFinalityEpochSnapshot {
                epoch_id: columns::unsigned(map, "epoch_id")?,
                threshold_bps: narrow(columns::unsigned(map, "threshold_bps")?)?,
                min_unique_signers: narrow(columns::unsigned(map, "min_unique_signers")?)?,
                validator_set_hash: columns::text(map, "validator_set_hash")?,
                stake_root: columns::text(map, "stake_root")?,
                threshold: narrow(columns::unsigned(map, "threshold")?)?,
                signer_node_ids: Vec::with_capacity(signer_count),
                validator_stakes: BTreeMap::new(),
            };
            for ordinal in 0..signer_count {
                let key = Key::pair(Key::text("signer_node_ids"), Key::Ordinal(ordinal as u64));
                let Some(Cell::Text(id)) = map.remove(&key) else {
                    return Err(invalid());
                };
                value.signer_node_ids.push(id);
            }
            for _ in 0..stake_count {
                let Some((Key::Pair(field, member), Cell::U64(stake))) = map.pop_first() else {
                    return Err(invalid());
                };
                if *field != Key::text("validator_stakes") {
                    return Err(invalid());
                }
                let Key::Pair(tag, id) = *member else {
                    return Err(invalid());
                };
                if *tag != Key::text("member") {
                    return Err(invalid());
                }
                let Key::Text(id) = *id else {
                    return Err(invalid());
                };
                if value.validator_stakes.insert(id, stake).is_some() {
                    return Err(invalid());
                }
            }
            RecoveredRuntimeGovernanceRecordV1::EpochSnapshot(value)
        }
        RuntimeGovernanceKindV1::IdentityPenalty => {
            let map = body.cells_mut();
            RecoveredRuntimeGovernanceRecordV1::IdentityPenalty(GovernanceIdentityPenaltyRecord {
                penalty_id: columns::unsigned(map, "penalty_id")?,
                target_agent_id: columns::text(map, "target_agent_id")?,
                evidence_hash: columns::text(map, "evidence_hash")?,
                reason: columns::text(map, "reason")?,
                slash_stake: columns::unsigned(map, "slash_stake")?,
                appeal_deadline_tick: columns::unsigned(map, "appeal_deadline_tick")?,
                status: match columns::unsigned(map, "status")? {
                    0 => GovernanceIdentityPenaltyStatus::Applied,
                    1 => GovernanceIdentityPenaltyStatus::Appealed,
                    2 => GovernanceIdentityPenaltyStatus::AppealAccepted,
                    3 => GovernanceIdentityPenaltyStatus::AppealRejected,
                    _ => return Err(invalid()),
                },
                identity_status_before: match columns::unsigned(map, "identity_status_before")? {
                    0 => GovernanceIdentityStatus::Active,
                    1 => GovernanceIdentityStatus::Frozen,
                    2 => GovernanceIdentityStatus::Expelled,
                    _ => return Err(invalid()),
                },
                detection_source: columns::text(map, "detection_source")?,
                detection_risk_score: columns::signed(map, "detection_risk_score")?,
                detection_incident_id: columns::text(map, "detection_incident_id")?,
                evidence_chain_hash: columns::text(map, "evidence_chain_hash")?,
                appeal_evidence_hash: columns::opt_text(map, "appeal_evidence_hash")?,
                resolution_evidence_hash: columns::opt_text(map, "resolution_evidence_hash")?,
                appellant: columns::opt_text(map, "appellant")?,
                appeal_reason: columns::opt_text(map, "appeal_reason")?,
                resolved_by: columns::opt_text(map, "resolved_by")?,
                resolution_reason: columns::opt_text(map, "resolution_reason")?,
                resolved_at_tick: columns::opt_unsigned(map, "resolved_at_tick")?,
            })
        }
    };
    ctx.check()?;
    body.finish(result, Vec::new())
}
