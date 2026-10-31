//! Point reads authenticate the exact qualified owner, ordinal and length.
use super::*;
#[derive(Clone, Copy)]
pub enum DiplomacyMemberColumnV1 {
    AllianceMember,
    OutcomeAgentId,
    OutcomeElectricityDelta,
    OutcomeDataDelta,
    OutcomeReputationDelta,
}
fn bind<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if !matches!(field, DiplomacyFieldV1::Alliances | DiplomacyFieldV1::Wars)
        || !proof.uses_budget(ctx.values.memory)
    {
        return Err(closure::invalid());
    }
    let key = ctx.key(field.descriptor()?, WorldFieldRecordKindV1::Entry, id)?;
    proof.validate_binding(field, &access::digest(&key.0)?, ctx.root)
}
pub fn read_qualified_diplomacy_length_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<u64, ValueTreeError> {
    bind(field, id, proof, ctx)?;
    let length = closure::length(field, ctx)?;
    ctx.check()?;
    Ok(length)
}
pub struct DiplomacyMemberReadV1<'a> {
    pub field: DiplomacyFieldV1,
    pub id: &'a str,
    pub ordinal: u64,
    pub column: DiplomacyMemberColumnV1,
    pub qualification: &'a ReservedValue<DiplomacyRecordQualificationV1>,
    pub max_payload_bytes: u64,
}
pub fn read_qualified_diplomacy_member_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    query: DiplomacyMemberReadV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    output: &mut dyn std::io::Write,
) -> Result<Option<DiplomacyValueV1>, ValueTreeError> {
    let DiplomacyMemberReadV1 {
        field,
        id,
        ordinal,
        column,
        qualification: proof,
        max_payload_bytes: max,
    } = query;
    bind(field, id, proof, ctx)?;
    let (path, kind) = match (field, column) {
        (DiplomacyFieldV1::Alliances, DiplomacyMemberColumnV1::AllianceMember) => {
            (Path::Ordinal("members", ordinal), Kind::Text)
        }
        (DiplomacyFieldV1::Wars, column) => match column {
            DiplomacyMemberColumnV1::OutcomeAgentId => {
                (Path::OutcomeField(ordinal, "agent_id"), Kind::Text)
            }
            DiplomacyMemberColumnV1::OutcomeElectricityDelta => {
                (Path::OutcomeField(ordinal, "electricity_delta"), Kind::I64)
            }
            DiplomacyMemberColumnV1::OutcomeDataDelta => {
                (Path::OutcomeField(ordinal, "data_delta"), Kind::I64)
            }
            DiplomacyMemberColumnV1::OutcomeReputationDelta => {
                (Path::OutcomeField(ordinal, "reputation_delta"), Kind::I64)
            }
            _ => return Err(closure::invalid()),
        },
        _ => return Err(closure::invalid()),
    };
    if ordinal >= closure::length(field, ctx)? {
        ctx.check()?;
        return Ok(None);
    }
    let key = path.read(field, ctx)?;
    let value = ctx.lookup(&key.0)?.ok_or_else(closure::invalid)?;
    let result = closure::primitive(kind, &value, &key, &mut ctx.records(), max, output)?;
    ctx.check()?;
    Ok(Some(result))
}
