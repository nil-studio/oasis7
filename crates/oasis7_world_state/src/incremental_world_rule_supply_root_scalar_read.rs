//! Field-local reads from an exact qualified record root, including cold roots.
use super::*;
use crate::CrisisStatus;
#[derive(Clone, Copy)]
pub enum SupplyAuthorizationFieldV1 {
    Id,
    ProviderId,
    CrisisId,
    Material,
    ReviewProposal,
    PolicyVersion,
    StartsAt,
    ExpiresAt,
    RemainingEscrow,
    Revision,
    Active,
    CrisisEvidence(SupplyCrisisFieldV1),
}
#[derive(Clone, Copy)]
pub enum SupplyCrisisFieldV1 {
    CrisisId,
    Kind,
    Severity,
    Status,
    OpenedAt,
    ExpiresAt,
    ResolverAgentId,
    Strategy,
    Success,
    Impact,
    ResolvedAt,
}
#[derive(Clone, Copy)]
pub enum SupplyBatchFieldV1 {
    Id,
    AuthorizationId,
    FrozenAt,
    AuthorizationRevision,
    FrozenEscrow,
    Allocated,
}
#[derive(Clone, Copy)]
pub enum SupplyScalarFieldV1 {
    Authorization(SupplyAuthorizationFieldV1),
    Batch(SupplyBatchFieldV1),
}
impl SupplyScalarFieldV1 {
    fn slot(self, owner: SupplyRootKeyV1<'_>) -> Result<usize, ValueTreeError> {
        Ok(match (self, owner) {
            (Self::Authorization(field), SupplyRootKeyV1::Authorization(_)) => match field {
                SupplyAuthorizationFieldV1::Id => 1,
                SupplyAuthorizationFieldV1::ProviderId => 2,
                SupplyAuthorizationFieldV1::CrisisId => 3,
                SupplyAuthorizationFieldV1::Material => 4,
                SupplyAuthorizationFieldV1::ReviewProposal => 5,
                SupplyAuthorizationFieldV1::PolicyVersion => 6,
                SupplyAuthorizationFieldV1::StartsAt => 7,
                SupplyAuthorizationFieldV1::ExpiresAt => 8,
                SupplyAuthorizationFieldV1::RemainingEscrow => 9,
                SupplyAuthorizationFieldV1::Revision => 10,
                SupplyAuthorizationFieldV1::Active => 11,
                SupplyAuthorizationFieldV1::CrisisEvidence(field) => match field {
                    SupplyCrisisFieldV1::CrisisId => 12,
                    SupplyCrisisFieldV1::Kind => 13,
                    SupplyCrisisFieldV1::Severity => 14,
                    SupplyCrisisFieldV1::Status => 15,
                    SupplyCrisisFieldV1::OpenedAt => 16,
                    SupplyCrisisFieldV1::ExpiresAt => 17,
                    SupplyCrisisFieldV1::ResolverAgentId => 19,
                    SupplyCrisisFieldV1::Strategy => 21,
                    SupplyCrisisFieldV1::Success => 23,
                    SupplyCrisisFieldV1::Impact => 24,
                    SupplyCrisisFieldV1::ResolvedAt => 26,
                },
            },
            (Self::Batch(field), SupplyRootKeyV1::Batch(_)) => match field {
                SupplyBatchFieldV1::Id => 1,
                SupplyBatchFieldV1::AuthorizationId => 2,
                SupplyBatchFieldV1::FrozenAt => 3,
                SupplyBatchFieldV1::AuthorizationRevision => 4,
                SupplyBatchFieldV1::FrozenEscrow => 5,
                SupplyBatchFieldV1::Allocated => 6,
            },
            _ => return Err(ValueTreeError::Invalid("supply scalar field owner")),
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyScalarValueV1 {
    Text { utf8_bytes: u64 },
    U64(u64),
    U32(u32),
    I64(i64),
    Bool(bool),
    Status(CrisisStatus),
}
pub struct SupplyScalarRequestV1<'a> {
    pub owner: SupplyRootKeyV1<'a>,
    pub field: SupplyScalarFieldV1,
    pub qualification: &'a ReservedValue<SupplyRecordQualificationV1>,
}
/// `None` is only an original absent Option value. Text streams are provisional
/// until the whole call and final cancellation check succeed.
pub fn read_qualified_supply_scalar_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: SupplyScalarRequestV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload_bytes: u64,
    mut output: &mut dyn std::io::Write,
) -> Result<Option<SupplyScalarValueV1>, ValueTreeError> {
    context.check()?;
    if !request.qualification.uses_budget(context.values.memory) {
        return Err(ValueTreeError::Invalid("supply scalar read pool"));
    }
    let outer = request
        .owner
        .outer()
        .fixed_path(RecordPathV1::Record, context)?;
    let digest = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid("supply scalar owner digest"))?;
    request
        .qualification
        .validate_binding(request.owner, &digest, context.root)?;
    let _fixed = context.values.memory.reserve(8192, context.values.class)?;
    let slot = request.field.slot(request.owner)?;
    let spec = request.owner.spec(slot);
    let key = request.owner.relative().fixed_path(spec.path, context)?;
    let root = context.lookup(&key.0)?;
    if let Some(flag) = spec.flag {
        let flag = request
            .owner
            .relative()
            .fixed_path(request.owner.spec(flag).path, context)?;
        let value = context
            .lookup(&flag.0)?
            .ok_or(ValueTreeError::Invalid("supply scalar option flag"))?;
        let present = closure::primitive(
            Kind::Bool,
            &value,
            &flag,
            &mut context.values,
            context.record_bytes,
            1,
            &mut context.cancelled,
        )?
        .ok_or(ValueTreeError::Invalid("supply scalar option bool"))?;
        if present != root.is_some() {
            return Err(ValueTreeError::Invalid("supply scalar option relationship"));
        }
        if !present {
            context.check()?;
            return Ok(None);
        }
    }
    let root = root.ok_or(ValueTreeError::Invalid("supply scalar required column"))?;
    let codec = match spec.kind {
        Kind::Text | Kind::Status => MemberPayloadCodecV1::Text,
        Kind::U64 | Kind::U32 => MemberPayloadCodecV1::U64,
        Kind::I64 => MemberPayloadCodecV1::I64,
        Kind::Bool => MemberPayloadCodecV1::Bool,
        Kind::Unit => return Err(ValueTreeError::Invalid("supply scalar unit field")),
    };
    let profile = MemberReadProfileV1 {
        codec,
        semantic_bytes: key.1,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: key.1,
            payload_bytes: match spec.kind {
                Kind::Text => max_payload_bytes,
                Kind::Status => 9,
                Kind::Bool => 1,
                _ => 8,
            },
            record_bytes: context.record_bytes,
        },
    };
    let mut bytes = [0; 9];
    let value = if matches!(spec.kind, Kind::Text) {
        SupplyScalarValueV1::Text {
            utf8_bytes: crate::incremental_utf8_output::read_text_value(
                &root,
                &key.0,
                &mut context.values,
                profile,
                &mut output,
                "supply scalar UTF-8",
                &mut context.cancelled,
            )?,
        }
    } else if matches!(spec.kind, Kind::Status) {
        let length = crate::incremental_utf8_output::read_text_value(
            &root,
            &key.0,
            &mut context.values,
            profile,
            &mut Cursor::new(bytes.as_mut_slice()),
            "supply scalar status UTF-8",
            &mut context.cancelled,
        )?;
        SupplyScalarValueV1::Status(match &bytes[..length as usize] {
            b"active" => CrisisStatus::Active,
            b"resolved" => CrisisStatus::Resolved,
            b"timed_out" => CrisisStatus::TimedOut,
            _ => return Err(ValueTreeError::Invalid("supply scalar status")),
        })
    } else {
        read_member_record_profile_v1(
            &root,
            &key.0,
            &mut context.values,
            profile,
            &mut std::io::sink(),
            &mut Cursor::new(bytes.as_mut_slice()),
            &mut context.cancelled,
        )?;
        let number = u64::from_be_bytes(bytes[..8].try_into().unwrap());
        match spec.kind {
            Kind::U64 => SupplyScalarValueV1::U64(number),
            Kind::U32 => SupplyScalarValueV1::U32(
                u32::try_from(number).map_err(|_| ValueTreeError::Invalid("supply scalar u32"))?,
            ),
            Kind::I64 => {
                SupplyScalarValueV1::I64(i64::from_be_bytes(bytes[..8].try_into().unwrap()))
            }
            Kind::Bool => SupplyScalarValueV1::Bool(match bytes[0] {
                0 => false,
                1 => true,
                _ => return Err(ValueTreeError::Invalid("supply scalar bool")),
            }),
            _ => return Err(ValueTreeError::Invalid("supply scalar primitive")),
        }
    };
    context.check()?;
    Ok(Some(value))
}
