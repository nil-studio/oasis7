//! Explicit expected-source proof of an entire relative organization body.
//! This is not raw recovery or family creation qualification.
use super::*;
use crate::incremental_expected_bytes::Compare;
use oasis7_proto::memory_budget::ReservedValue;

#[derive(Debug)]
pub struct ExpectedOrganizationRecordProofV1 {
    schema: [u8; 32],
    owner: [u8; 32],
    root: StateRootV1,
}
impl ExpectedOrganizationRecordProofV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn validate_binding(
        &self,
        schema: &[u8; 32],
        owner: &[u8; 32],
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if &self.schema != schema || &self.owner != owner || &self.root != root {
            return Err(ValueTreeError::Invalid(
                "organization qualified record binding",
            ));
        }
        Ok(())
    }
}
/// Authenticates every expected closed field and dynamic member, followed by a
/// full actual tree traversal and exact cardinality. The borrowed source is
/// caller-admitted. It is exhaustively projected by the owner; it cannot name a
/// subset while hiding additional actual leaves. Normal updates must use the
/// incremental receiver, never this whole-record expected path.
pub fn verify_expected_organization_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: OrganizationRootKeyV1<'_>,
    source: super::super::WorldRuleOrganizationScalarsRefV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells_remaining: &mut u64,
) -> Result<ReservedValue<ExpectedOrganizationRecordProofV1>, MemberRecordError> {
    use super::super::WorldRuleOrganizationScalarsRefV1 as V;
    if !matches!(
        (owner, source),
        (OrganizationRootKeyV1::Case(_), V::Case(_))
            | (OrganizationRootKeyV1::Estate(_), V::Estate(_))
    ) {
        return Err(MemberRecordError::Invalid("organization expected profile"));
    }
    context.check()?;
    if context.root.namespace != StateNamespaceV1::Authorization {
        return Err(ValueTreeError::Invalid("organization expected namespace").into());
    }
    let retained = context
        .values
        .memory
        .reserve(512, context.values.class)
        .map_err(ValueTreeError::Memory)?;
    let (owner_key, _) = owner.outer().fixed_path(RecordPathV1::Record, context)?;
    let owner_digest = WorldFieldKeyV1::from_bytes(owner_key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid("organization expected owner"))?;
    let mut expected_count = 0u64;
    visit_relative_body(source, &mut |record, path, payload| {
        *cells_remaining =
            cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "organization expected cells",
                ))?;
        expected_count = expected_count
            .checked_add(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "organization expected count",
            ))?;
        let (key, semantic_bytes) = record.fixed_path(path, context)?;
        let value = context.lookup(&key)?.ok_or(ValueTreeError::Invalid(
            "organization expected missing field",
        ))?;
        compare_payload(
            payload,
            &value,
            &key,
            semantic_bytes,
            &mut context.values,
            context.record_bytes,
            &mut context.cancelled,
        )?;
        Ok(())
    })?;
    let empty = StateRootV1::empty(StateNamespaceV1::Authorization)
        .map_err(|_| ValueTreeError::Invalid("organization expected empty root"))?;
    let result = tree::OrganizationChangedTreeContextV1 {
        source: context.state,
        memory: context.values.memory,
        class: context.values.class,
        budget: scan,
        cancelled: &mut context.cancelled,
    }
    .visit(&empty, context.root, |_, _| Ok(()));
    let source_error = (context.take_state_error)();
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(error) = source_error {
        return Err(error.into());
    }
    result?;
    if expected_count != context.root.leaf_count {
        return Err(ValueTreeError::Invalid("organization expected undeclared field").into());
    }
    context.check()?;
    Ok(retained.retain(ExpectedOrganizationRecordProofV1 {
        schema: owner.schema(),
        owner: owner_digest,
        root: context.root.clone(),
    }))
}

/// Exact complete payload comparison shared by whole expected-record proof and
/// finite closed-record deletion. Semantic identity is authenticated by reader.
pub(super) fn compare_payload<R: ValueNodeReader, C: FnMut() -> bool>(
    payload: MemberPayloadRefV1<'_>,
    value: &[u8; 32],
    key: &WorldFieldKeyV1,
    semantic_bytes: u64,
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    record_bytes: u64,
    cancelled: &mut C,
) -> Result<(), MemberRecordError> {
    let mut fixed = [0u8; 8];
    let (codec, expected): (_, &[u8]) = match payload {
        MemberPayloadRefV1::Unit => (MemberPayloadCodecV1::Unit, &[]),
        MemberPayloadRefV1::Text(text) => (MemberPayloadCodecV1::Text, text.as_bytes()),
        MemberPayloadRefV1::Bool(value) => {
            fixed[0] = u8::from(value);
            (MemberPayloadCodecV1::Bool, &fixed[..1])
        }
        MemberPayloadRefV1::U64(value) => {
            fixed = value.to_be_bytes();
            (MemberPayloadCodecV1::U64, &fixed)
        }
        MemberPayloadRefV1::I64(value) => {
            fixed = value.to_be_bytes();
            (MemberPayloadCodecV1::I64, &fixed)
        }
        _ => return Err(MemberRecordError::Invalid("organization expected payload")),
    };
    let mut compare = Compare::new(expected);
    let info = read_member_record_profile_v1(
        value,
        key,
        values,
        MemberReadProfileV1 {
            codec,
            semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes,
                payload_bytes: expected.len() as u64,
                record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut compare,
        &mut *cancelled,
    )?;
    if info.payload_bytes != expected.len() as u64 || !compare.is_equal() {
        return Err(ValueTreeError::Invalid("organization expected expected field").into());
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
