//! Closed supply-record identities and record-shape checks, not rule policy.
use crate::{
    incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_world_rule_parent::descriptor,
};
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};

#[derive(Clone, Copy)]
pub(crate) enum SupplyRecordKeyV1<'a> {
    Lineage(&'a str),
    Shipment(u64),
}
impl SupplyRecordKeyV1<'_> {
    pub(crate) fn fixed_key<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        self,
        column: Option<&str>,
        context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        let field = descriptor()?;
        match (self, column) {
            (Self::Lineage(key), None) => context.key(
                field,
                WorldFieldRecordKindV1::Entry,
                &("supply_lineages", key),
            ),
            (Self::Lineage(key), Some(column)) => context.key(
                field,
                WorldFieldRecordKindV1::Entry,
                &(("supply_lineages", key), column),
            ),
            (Self::Shipment(key), None) => context.key(
                field,
                WorldFieldRecordKindV1::Entry,
                &("supply_shipments", key),
            ),
            (Self::Shipment(key), Some(column)) => context.key(
                field,
                WorldFieldRecordKindV1::Entry,
                &(("supply_shipments", key), column),
            ),
        }
    }
    pub(crate) fn overlay_key<R: ValueNodeReader, C: FnMut() -> bool>(
        self,
        column: Option<&str>,
        records: &mut InventoryValueReader<'_, '_, R, C>,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        let field = descriptor()?;
        match (self, column) {
            (Self::Lineage(key), None) => records.key(
                field,
                WorldFieldRecordKindV1::Entry,
                &("supply_lineages", key),
            ),
            (Self::Lineage(key), Some(column)) => records.key(
                field,
                WorldFieldRecordKindV1::Entry,
                &(("supply_lineages", key), column),
            ),
            (Self::Shipment(key), None) => records.key(
                field,
                WorldFieldRecordKindV1::Entry,
                &("supply_shipments", key),
            ),
            (Self::Shipment(key), Some(column)) => records.key(
                field,
                WorldFieldRecordKindV1::Entry,
                &(("supply_shipments", key), column),
            ),
        }
    }
    pub(crate) fn write<W: ValueNodeWriter, C: FnMut() -> bool>(
        self,
        column: Option<&str>,
        payload: MemberPayloadRefV1<'_>,
        context: &mut MemberRecordWriteContext<'_, W, C>,
    ) -> Result<MemberRecordV1, MemberRecordError> {
        let field = descriptor()?;
        match (self, column) {
            (Self::Lineage(key), None) => write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                &("supply_lineages", key),
                payload,
                context,
            ),
            (Self::Lineage(key), Some(column)) => write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                &(("supply_lineages", key), column),
                payload,
                context,
            ),
            (Self::Shipment(key), None) => write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                &("supply_shipments", key),
                payload,
                context,
            ),
            (Self::Shipment(key), Some(column)) => write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                &(("supply_shipments", key), column),
                payload,
                context,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecordPresenceV1 {
    RulesAbsent,
    RecordAbsent,
    Present,
}
pub(crate) fn shape<const N: usize>(
    present: bool,
    roots: &[Option<[u8; 32]>; N],
) -> Result<RecordPresenceV1, ValueTreeError> {
    if !present {
        if roots.iter().any(Option::is_some) {
            return Err(ValueTreeError::Invalid("orphan world rule record"));
        }
        return Ok(RecordPresenceV1::RulesAbsent);
    }
    if roots[0].is_none() {
        if roots[1..].iter().any(Option::is_some) {
            return Err(ValueTreeError::Invalid("orphan world rule record column"));
        }
        return Ok(RecordPresenceV1::RecordAbsent);
    }
    if roots[1..].iter().any(Option::is_none) {
        return Err(ValueTreeError::Invalid("world rule record missing column"));
    }
    Ok(RecordPresenceV1::Present)
}
pub(crate) fn marker<R: ValueNodeReader>(
    root: &[u8; 32],
    key: &WorldFieldKeyV1,
    bytes: u64,
    values: &mut ValueReadContext<'_, R>,
    record_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    read_member_record_profile_v1(
        root,
        key,
        values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Unit,
            semantic_bytes: bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: bytes,
                payload_bytes: 0,
                record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut std::io::sink(),
        &mut *cancelled,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
