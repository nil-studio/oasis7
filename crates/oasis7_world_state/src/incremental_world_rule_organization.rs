//! Closed organization owner records; this module grants no business authority.
use crate::{
    incremental_field_member::*,
    incremental_world_rule_record::{RecordPathV1, SupplyRecordKeyV1 as K, marker},
    state::WorldState,
    world_rules::{
        OrganizationCaseV1, OrganizationDispositionPhaseV1, OrganizationEstateDepositV1,
        OrganizationGrantV1, OrganizationProtectionPhaseV1, OrganizationRecoveryRequestV1,
        WorldRulesStateV1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::Cursor;
#[path = "incremental_world_rule_organization_layout.rs"]
mod layout;
use layout::*;
/// A borrowed scalar projection. Dynamic collections are not compared or replaced.
/// The original object and strings remain caller-admitted and borrowed.
#[derive(Clone, Copy)]
pub enum WorldRuleOrganizationScalarsRefV1<'a> {
    Case(&'a OrganizationCaseV1),
    Estate(&'a OrganizationEstateDepositV1),
    Grant(&'a OrganizationGrantV1),
    Recovery(&'a OrganizationRecoveryRequestV1),
}
impl WorldRuleOrganizationScalarsRefV1<'_> {
    fn profile(self) -> Profile {
        match self {
            Self::Case(_) => Profile::Case,
            Self::Estate(_) => Profile::Estate,
            Self::Grant(_) => Profile::Grant,
            Self::Recovery(_) => Profile::Recovery,
        }
    }
    fn empty(self) -> bool {
        match self {
            Self::Case(v) => {
                v.members.is_empty()
                    && v.grants.is_empty()
                    && v.requests.is_empty()
                    && v.stage_receipts.is_empty()
            }
            Self::Grant(v) => v.scopes.is_empty(),
            Self::Estate(_) | Self::Recovery(_) => true,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    Absent,
    Unit,
    Text(u64),
    U64(u64),
    I64(i64),
    Bool(bool),
    Phase(OrganizationProtectionPhaseV1),
    Disposition(OrganizationDispositionPhaseV1),
}
fn profile(
    codec: MemberPayloadCodecV1,
    semantic_bytes: u64,
    payload_bytes: u64,
    record_bytes: u64,
) -> MemberReadProfileV1 {
    MemberReadProfileV1 {
        codec,
        semantic_bytes,
        limits: MemberRecordLimitsV1 {
            semantic_bytes,
            payload_bytes,
            record_bytes,
        },
    }
}
fn fixed<R: ValueNodeReader, const N: usize>(
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    codec: MemberPayloadCodecV1,
    values: &mut ValueReadContext<'_, R>,
    record_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<[u8; N], ValueTreeError> {
    let mut bytes = [0; N];
    read_member_record_profile_v1(
        root,
        &key.0,
        values,
        profile(codec, key.1, N as u64, record_bytes),
        &mut std::io::sink(),
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut *cancelled,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(bytes)
}
fn bool_value<R: ValueNodeReader>(
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    values: &mut ValueReadContext<'_, R>,
    record_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<bool, ValueTreeError> {
    match fixed::<_, 1>(
        root,
        key,
        MemberPayloadCodecV1::Bool,
        values,
        record_bytes,
        cancelled,
    )?[0]
    {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ValueTreeError::Invalid("organization bool")),
    }
}
#[derive(Clone, Copy)]
struct ReadLimits {
    record_bytes: u64,
    max_payload: u64,
}
#[path = "incremental_world_rule_organization_genesis.rs"]
mod genesis;
#[path = "incremental_world_rule_organization_overlay.rs"]
mod overlay;
pub use genesis::WorldRuleOrganizationGenesisCountsV1;
#[path = "incremental_world_rule_organization_members.rs"]
mod members;
#[path = "incremental_world_rule_organization_receipts.rs"]
mod receipts;
pub use receipts::{WorldRuleOrganizationReceiptChangeV1, WorldRuleOrganizationReceiptValueV1};
#[path = "incremental_world_rule_organization_root.rs"]
pub mod root;
