use super::*;
pub(super) fn exercise(
    owner: OrganizationRootKeyV1<'_>,
    proof: &ReservedValue<OrganizationRecordQualificationV1>,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    class: MemoryClass,
) {
    use OrganizationFieldValueV1 as V;
    use OrganizationRelativeRecordV1 as R;
    use OrganizationScalarFieldV1 as F;
    for (record, field, expected, bytes) in [
        (R::Case, F::Id, V::Text { utf8_bytes: 10 }, "case-inner"),
        (
            R::Case,
            F::OrganizationId,
            V::Text { utf8_bytes: 6 },
            "org\0é",
        ),
        (R::Case, F::FounderId, V::Text { utf8_bytes: 0 }, ""),
        (R::Case, F::PolicyVersion, V::U32(u32::MAX), ""),
        (R::Case, F::Revision, V::U64(u64::MAX), ""),
        (R::Case, F::Phase, V::Phase(Phase::Protected), ""),
        (
            R::Case,
            F::Disposition,
            V::Disposition(Disposition::Obligations),
            "",
        ),
        (R::Case, F::NotifiedAt, V::OptionalU64(None), ""),
        (R::Case, F::ProtectedAt, V::OptionalU64(Some(0)), ""),
        (R::Estate, F::Id, V::Text { utf8_bytes: 12 }, "estate-inner"),
        (
            R::Estate,
            F::OrganizationId,
            V::Text { utf8_bytes: 13 },
            "different-org",
        ),
        (
            R::Estate,
            F::ContributorId,
            V::Text { utf8_bytes: 11 },
            "contributor",
        ),
        (R::Estate, F::Material, V::Text { utf8_bytes: 0 }, ""),
        (R::Estate, F::Amount, V::I64(i64::MIN), ""),
        (R::Estate, F::HeldInTrust, V::Bool(true), ""),
        (R::Estate, F::Disposed, V::Bool(false), ""),
        (
            R::Estate,
            F::EffectReceipt,
            V::OptionalText {
                utf8_bytes: Some(0),
            },
            "",
        ),
        (
            R::Grant(GRANT),
            F::Id,
            V::Text { utf8_bytes: 11 },
            "grant-inner",
        ),
        (
            R::Grant(GRANT),
            F::DelegateId,
            V::Text { utf8_bytes: 8 },
            "delegate",
        ),
        (R::Grant(GRANT), F::Revision, V::U64(0), ""),
        (R::Grant(GRANT), F::ExpiresAt, V::U64(u64::MAX), ""),
        (
            R::Grant(GRANT),
            F::SourceProposal,
            V::Text { utf8_bytes: 0 },
            "",
        ),
        (
            R::Grant(GRANT),
            F::Replaces,
            V::OptionalText {
                utf8_bytes: Some(0),
            },
            "",
        ),
        (
            R::Recovery(REQUEST),
            F::RequesterId,
            V::Text { utf8_bytes: 15 },
            "requester-inner",
        ),
        (R::Recovery(REQUEST), F::Appeal, V::Bool(true), ""),
        (
            R::Recovery(REQUEST),
            F::Approved,
            V::OptionalBool(Some(false)),
            "",
        ),
        (
            R::Recovery(REQUEST),
            F::ResolvedBy,
            V::OptionalText {
                utf8_bytes: Some(0),
            },
            "",
        ),
    ] {
        if matches!(owner, OrganizationRootKeyV1::Estate(_)) != matches!(record, R::Estate) {
            continue;
        }
        let state = store.state_session(physical(), class).unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut read = WorldFieldReadContextV1 {
            root: proof.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let _scratch = memory.reserve(256, class).unwrap();
        let mut output = Vec::with_capacity(256);
        assert_eq!(
            read_qualified_organization_field_v1(
                OrganizationFieldRequestV1 {
                    owner,
                    record,
                    field,
                    qualification: proof,
                },
                &mut read,
                128,
                &mut output
            )
            .unwrap(),
            OrganizationFieldReadV1::Value(expected)
        );
        assert_eq!(output, bytes.as_bytes());
    }
}
