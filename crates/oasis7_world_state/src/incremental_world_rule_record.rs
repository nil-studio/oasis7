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
    Authorization(&'a str),
    AuthorizationInner,
    BatchInner,
    Batch(&'a str),
    OrganizationCase(&'a str),
    OrganizationCaseInner,
    OrganizationEstateInner,
    OrganizationGrantInner(&'a str),
    OrganizationRecoveryInner(&'a str),
    OrganizationEstate(&'a str),
}
#[derive(Clone, Copy)]
pub(crate) enum RecordPathV1<'a> {
    Record,
    Field(&'static str),
    Nested(&'static str, &'static str),
    OptionPresence(&'static str, &'static str),
    OptionValue(&'static str, &'static str),
    Collection(&'static str),
    Member(&'static str, &'a str),
    MemberField(&'static str, &'a str, &'static str),
    Ordinal(&'static str, u64),
    MemberPair(&'static str, &'a str, &'a str),
    FieldOptionPresence(&'static str),
    FieldOptionValue(&'static str),
}
// Both macros select only the existing explicit tuple key profiles. They never
// invoke Serialize, concatenate paths or allocate an intermediate key tree.
macro_rules! with_record_key {
    ($source:expr, $root:ident => $body:expr) => {
        match $source {
            SupplyRecordKeyV1::Lineage(key) => {
                let $root = ("supply_lineages", key);
                $body
            }
            SupplyRecordKeyV1::Shipment(key) => {
                let $root = ("supply_shipments", key);
                $body
            }
            SupplyRecordKeyV1::AuthorizationInner
            | SupplyRecordKeyV1::BatchInner
            | SupplyRecordKeyV1::OrganizationCaseInner
            | SupplyRecordKeyV1::OrganizationEstateInner => {
                let $root = ();
                $body
            }
            SupplyRecordKeyV1::OrganizationGrantInner(key) => {
                let $root = (((), "grants"), key);
                $body
            }
            SupplyRecordKeyV1::OrganizationRecoveryInner(key) => {
                let $root = (((), "requests"), key);
                $body
            }
            SupplyRecordKeyV1::OrganizationCase(key) => {
                let $root = ("organization_cases", key);
                $body
            }
            SupplyRecordKeyV1::OrganizationEstate(key) => {
                let $root = ("organization_estate", key);
                $body
            }
            SupplyRecordKeyV1::Batch(key) => {
                let $root = ("supply_batches", key);
                $body
            }
            SupplyRecordKeyV1::Authorization(key) => {
                let $root = ("supply_authorizations", key);
                $body
            }
        }
    };
}
macro_rules! with_record_path {
    ($root:expr, $path:expr, $key:ident => $body:expr) => {
        match $path {
            RecordPathV1::Record => {
                let $key = &$root;
                $body
            }
            RecordPathV1::Field(field) => {
                let $key = &($root, field);
                $body
            }
            RecordPathV1::Nested(parent, field) => {
                let $key = &(($root, parent), field);
                $body
            }
            RecordPathV1::OptionPresence(parent, field) => {
                let $key = &((($root, parent), field), ());
                $body
            }
            RecordPathV1::OptionValue(parent, field) => {
                let $key = &((($root, parent), field), "value");
                $body
            }
            RecordPathV1::Collection(collection) => {
                let $key = &(($root, collection), ());
                $body
            }
            RecordPathV1::FieldOptionPresence(field) => {
                let $key = &(($root, field), ());
                $body
            }
            RecordPathV1::FieldOptionValue(field) => {
                let $key = &(($root, field), "value");
                $body
            }
            RecordPathV1::Ordinal(collection, index) => {
                let $key = &(($root, collection), index);
                $body
            }
            RecordPathV1::MemberPair(collection, first, second) => {
                let $key = &(($root, collection), (first, second));
                $body
            }
            RecordPathV1::MemberField(collection, member, field) => {
                let $key = &((($root, collection), member), field);
                $body
            }
            RecordPathV1::Member(collection, member) => {
                let $key = &(($root, collection), member);
                $body
            }
        }
    };
}
impl SupplyRecordKeyV1<'_> {
    pub(crate) fn fixed_key<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        self,
        column: Option<&'static str>,
        context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        self.fixed_path(
            column.map_or(RecordPathV1::Record, RecordPathV1::Field),
            context,
        )
    }
    pub(crate) fn fixed_path<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        self,
        path: RecordPathV1<'_>,
        context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        let field = descriptor()?;
        with_record_key!(self, root => with_record_path!(root, path, key => context.key(field, WorldFieldRecordKindV1::Entry, key)))
    }
    pub(crate) fn overlay_key<R: ValueNodeReader, C: FnMut() -> bool>(
        self,
        column: Option<&'static str>,
        records: &mut InventoryValueReader<'_, '_, R, C>,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        self.overlay_path(
            column.map_or(RecordPathV1::Record, RecordPathV1::Field),
            records,
        )
    }
    pub(crate) fn overlay_path<R: ValueNodeReader, C: FnMut() -> bool>(
        self,
        path: RecordPathV1<'_>,
        records: &mut InventoryValueReader<'_, '_, R, C>,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        let field = descriptor()?;
        with_record_key!(self, root => with_record_path!(root, path, key => records.key(field, WorldFieldRecordKindV1::Entry, key)))
    }
    pub(crate) fn write<W: ValueNodeWriter, C: FnMut() -> bool>(
        self,
        column: Option<&'static str>,
        payload: MemberPayloadRefV1<'_>,
        context: &mut MemberRecordWriteContext<'_, W, C>,
    ) -> Result<MemberRecordV1, MemberRecordError> {
        self.write_path(
            column.map_or(RecordPathV1::Record, RecordPathV1::Field),
            payload,
            context,
        )
    }
    pub(crate) fn write_path<W: ValueNodeWriter, C: FnMut() -> bool>(
        self,
        path: RecordPathV1<'_>,
        payload: MemberPayloadRefV1<'_>,
        context: &mut MemberRecordWriteContext<'_, W, C>,
    ) -> Result<MemberRecordV1, MemberRecordError> {
        let field = descriptor()?;
        with_record_key!(self, root => with_record_path!(root, path, key => write_member_record_v1(field, WorldFieldRecordKindV1::Entry, key, payload, context)))
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
