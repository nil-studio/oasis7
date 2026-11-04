//! Structural closure of the three original flat WorldRules children only.
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
    incremental_utf8_output::read_text_value,
    incremental_world_rule_parent::{descriptor, read_parent_fixed},
    incremental_world_rule_record::SupplyRecordKeyV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[path = "incremental_world_rule_remainder_family.rs"]
mod family;
#[path = "incremental_world_rule_remainder_receiver.rs"]
mod receiver;
pub use family::{qualify_world_rule_remainder_v1, readmit_world_rule_remainder_ordinary_v1};
pub use receiver::accept_world_rule_remainder_transition_v1;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("world rules flat remainder")
}
pub struct WorldRuleRemainderWorkV1 {
    pub cells_remaining: u64,
    pub max_semantic_bytes: usize,
    pub max_payload_bytes: u64,
}
impl WorldRuleRemainderWorkV1 {
    fn step(&mut self, c: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        if c() {
            return Err(ValueTreeError::Cancelled);
        }
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("rule remainder cells"))?;
        Ok(())
    }
}
pub struct WorldRuleRemainderQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    present: bool,
    counts: [u64; 3],
}
impl WorldRuleRemainderQualificationV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn rules_present(&self) -> bool {
        self.present
    }
    pub fn record_counts(&self) -> [u64; 3] {
        self.counts
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
}
fn key(raw: &[u8]) -> Result<Option<WorldFieldKeyV1>, ValueTreeError> {
    if !raw.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let p = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
    if p.owner != FieldOwner::WorldState || p.name != "world_rules" {
        return Ok(None);
    }
    let k = WorldFieldKeyV1::new(descriptor()?, p.kind, p.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if k.as_bytes() != raw {
        return Err(invalid());
    }
    Ok(Some(k))
}
fn shape<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    record: SupplyRecordKeyV1<'_>,
    cols: &[Option<&'static str>],
    present: bool,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRuleRemainderWorkV1,
) -> Result<(), ValueTreeError> {
    let mut exists = 0;
    for column in cols {
        work.step(&mut ctx.cancelled)?;
        let (key, _) = record.fixed_key(*column, ctx)?;
        if ctx.lookup(&key)?.is_some() {
            exists += 1;
        }
    }
    if (exists != 0 && exists != cols.len()) || (!present && exists != 0) {
        return Err(invalid());
    }
    Ok(())
}
// Return only the count-bearing marker's branch; every flat column is still
// checked, and complete sibling presence is point-checked at the candidate.
fn inspect<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    parent_present: bool,
    validate: bool,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRuleRemainderWorkV1,
) -> Result<Option<usize>, ValueTreeError> {
    work.step(&mut ctx.cancelled)?;
    ctx.check()?;
    let binding = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    if binding.kind == WorldFieldRecordKindV1::Metadata {
        if validate {
            ctx.records().presence(value, key, 0)?;
        }
        return Ok(None);
    }
    if binding.kind != WorldFieldRecordKindV1::Entry {
        return Err(invalid());
    }
    let max = work
        .max_semantic_bytes
        .min(usize::try_from(ctx.semantic_bytes).unwrap_or(usize::MAX));
    let _charge = ctx
        .values
        .memory
        .reserve(max.checked_add(128).ok_or_else(invalid)?, ctx.values.class)?;
    let mut semantic = Vec::with_capacity(max);
    let info = read_member_record_key_v1(
        value,
        key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: max as u64,
            payload_bytes: ctx.record_bytes,
            record_bytes: ctx.record_bytes,
        },
        &mut semantic,
        &mut ctx.cancelled,
    )?;
    if semantic.capacity() > max {
        return Err(invalid());
    }
    let decoded = read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?;
    if matches!(decoded, SemanticKeyRefV1::Unit) {
        return Ok(None);
    }
    let SemanticKeyRefV1::Pair { left, right } = decoded else {
        return Err(invalid());
    };
    let first = left.decode().map_err(ValueTreeError::Invalid)?;
    let (branch, id, column) = match first {
        SemanticKeyRefV1::Text(branch) => (
            branch,
            right.decode().map_err(ValueTreeError::Invalid)?,
            None,
        ),
        SemanticKeyRefV1::Pair { left, right: id } => {
            let SemanticKeyRefV1::Text(branch) = left.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Err(invalid());
            };
            let SemanticKeyRefV1::Text(column) = right.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Err(invalid());
            };
            (
                branch,
                id.decode().map_err(ValueTreeError::Invalid)?,
                Some(column),
            )
        }
        _ => return Ok(None),
    };
    let (index, codec, bytes) = match (branch, id, column) {
        ("governance_effects", SemanticKeyRefV1::Text(_), None) => {
            (0, MemberPayloadCodecV1::Text, None)
        }
        ("supply_lineages", SemanticKeyRefV1::Text(id), column) => {
            shape(
                SupplyRecordKeyV1::Lineage(id),
                &[None, Some("target"), Some("fulfilled"), Some("reserved")],
                parent_present,
                ctx,
                work,
            )?;
            match column {
                None => (1, MemberPayloadCodecV1::Unit, Some(0)),
                Some("target" | "fulfilled" | "reserved") => {
                    (1, MemberPayloadCodecV1::I64, Some(8))
                }
                _ => return Err(invalid()),
            }
        }
        ("supply_shipments", SemanticKeyRefV1::Unsigned(id), column) => {
            shape(
                SupplyRecordKeyV1::Shipment(id),
                &[
                    None,
                    Some("authorization_id"),
                    Some("batch_id"),
                    Some("beneficiary_id"),
                    Some("lineage_id"),
                    Some("input_amount"),
                    Some("delivered_amount"),
                    Some("settled"),
                ],
                parent_present,
                ctx,
                work,
            )?;
            match column {
                None => (2, MemberPayloadCodecV1::Unit, Some(0)),
                Some("authorization_id" | "batch_id" | "beneficiary_id" | "lineage_id") => {
                    (2, MemberPayloadCodecV1::Text, None)
                }
                Some("input_amount" | "delivered_amount") => {
                    (2, MemberPayloadCodecV1::I64, Some(8))
                }
                Some("settled") => (2, MemberPayloadCodecV1::Bool, Some(1)),
                _ => return Err(invalid()),
            }
        }
        ("governance_effects" | "supply_lineages" | "supply_shipments", _, _) => {
            return Err(invalid());
        }
        _ => return Ok(None),
    };
    if validate {
        ctx.values
            .verify_closure(value, ctx.record_bytes, &mut ctx.cancelled)?;
        let profile = MemberReadProfileV1 {
            codec,
            semantic_bytes: info.semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: ctx.semantic_bytes,
                payload_bytes: bytes.unwrap_or(work.max_payload_bytes),
                record_bytes: ctx.record_bytes,
            },
        };
        if codec == MemberPayloadCodecV1::Text {
            read_text_value(
                value,
                key,
                &mut ctx.values,
                profile,
                &mut std::io::sink(),
                "rule remainder UTF-8",
                &mut ctx.cancelled,
            )?;
        } else {
            let mut payload = [0u8; 8];
            let mut out = &mut payload[..];
            let read = read_member_record_profile_v1(
                value,
                key,
                &mut ctx.values,
                profile,
                &mut std::io::sink(),
                &mut out,
                &mut ctx.cancelled,
            )?;
            if Some(read.payload_bytes) != bytes {
                return Err(invalid());
            }
            if codec == MemberPayloadCodecV1::Bool && payload[0] > 1 {
                return Err(invalid());
            }
        }
    }
    ctx.check()?;
    Ok(if index == 0 || column.is_none() {
        Some(index)
    } else {
        None
    })
}

#[path = "incremental_world_rule_closed_children.rs"]
mod children;
pub use children::{WORLD_RULE_CHILDREN_V1, world_rule_child_from_semantic_v1};

#[cfg(test)]
#[path = "incremental_world_rule_remainder_tests.rs"]
mod tests;
