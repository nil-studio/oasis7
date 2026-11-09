use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Path {
    Root,
    Field(&'static str),
    Flag(&'static str),
    Value(&'static str),
    Meta(&'static str),
    Length(&'static str),
    Ordinal(&'static str, u64),
    Outcome(u64),
    OutcomeField(u64, &'static str),
}
macro_rules! path_key {
    ($p:expr,$key:ident=>$expr:expr) => {
        match $p {
            Path::Root => {
                let $key = &();
                $expr
            }
            Path::Field(n) => {
                let $key = &((), n);
                $expr
            }
            Path::Flag(n) => {
                let $key = &(((), n), ());
                $expr
            }
            Path::Value(n) => {
                let $key = &(((), n), "value");
                $expr
            }
            Path::Meta(n) => {
                let $key = &(((), n), ());
                $expr
            }
            Path::Length(n) => {
                let $key = &(((), n), "length");
                $expr
            }
            Path::Ordinal(n, i) => {
                let $key = &(((), n), i);
                $expr
            }
            Path::Outcome(i) => {
                let $key = &(((), "participant_outcomes"), i);
                $expr
            }
            Path::OutcomeField(i, n) => {
                let $key = &((((), "participant_outcomes"), i), n);
                $expr
            }
        }
    };
}
impl Path {
    pub(super) fn read<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        self,
        field: DiplomacyFieldV1,
        ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        path_key!(self,k=>ctx.key(field.descriptor()?,WorldFieldRecordKindV1::Entry,k))
    }
    pub(super) fn overlay<R: ValueNodeReader, C: FnMut() -> bool>(
        self,
        field: DiplomacyFieldV1,
        ctx: &mut InventoryValueReader<'_, '_, R, C>,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        path_key!(self,k=>ctx.key(field.descriptor()?,WorldFieldRecordKindV1::Entry,k))
    }
    pub(super) fn write<
        W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
        C: FnMut() -> bool,
    >(
        self,
        field: DiplomacyFieldV1,
        payload: MemberPayloadRefV1<'_>,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
    ) -> Result<MemberRecordV1, MemberRecordError> {
        path_key!(self,k=>write_member_record_v1(field.descriptor()?,WorldFieldRecordKindV1::Entry,k,payload,ctx))
    }
}
