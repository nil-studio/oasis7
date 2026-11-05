use super::*;
fn fields(
    auth: bool,
    rich: bool,
) -> Vec<(SupplyScalarFieldV1, Option<SupplyScalarValueV1>, Vec<u8>)> {
    use SupplyScalarValueV1 as V;
    if auth {
        use SupplyAuthorizationFieldV1 as A;
        use SupplyCrisisFieldV1 as C;
        let a = SupplyScalarFieldV1::Authorization;
        let c = |field| a(A::CrisisEvidence(field));
        vec![
            (
                a(A::Id),
                Some(V::Text { utf8_bytes: 10 }),
                b"inner-auth".to_vec(),
            ),
            (
                a(A::ProviderId),
                Some(V::Text { utf8_bytes: 8 }),
                b"provider".to_vec(),
            ),
            (
                a(A::CrisisId),
                Some(V::Text { utf8_bytes: 6 }),
                b"crisis".to_vec(),
            ),
            (a(A::Material), Some(V::Text { utf8_bytes: 0 }), vec![]),
            (
                a(A::ReviewProposal),
                Some(V::Text { utf8_bytes: 0 }),
                vec![],
            ),
            (
                a(A::PolicyVersion),
                Some(V::U32(if rich { u32::MAX } else { 0 })),
                vec![],
            ),
            (a(A::StartsAt), Some(V::U64(0)), vec![]),
            (a(A::ExpiresAt), Some(V::U64(1)), vec![]),
            (
                a(A::RemainingEscrow),
                Some(V::I64(if rich { i64::MIN } else { 0 })),
                vec![],
            ),
            (a(A::Revision), Some(V::U64(0)), vec![]),
            (a(A::Active), Some(V::Bool(false)), vec![]),
            (
                c(C::CrisisId),
                Some(V::Text { utf8_bytes: 8 }),
                b"evidence".to_vec(),
            ),
            (c(C::Kind), Some(V::Text { utf8_bytes: 0 }), vec![]),
            (
                c(C::Severity),
                Some(V::U32(if rich { u32::MAX } else { 0 })),
                vec![],
            ),
            (
                c(C::Status),
                Some(V::Status(CrisisStatus::TimedOut)),
                vec![],
            ),
            (c(C::OpenedAt), Some(V::U64(0)), vec![]),
            (c(C::ExpiresAt), Some(V::U64(1)), vec![]),
            (
                c(C::ResolverAgentId),
                if rich {
                    Some(V::Text { utf8_bytes: 0 })
                } else {
                    None
                },
                vec![],
            ),
            (c(C::Strategy), None, vec![]),
            (c(C::Success), Some(V::Bool(false)), vec![]),
            (
                c(C::Impact),
                Some(V::I64(if rich { i64::MAX } else { 0 })),
                vec![],
            ),
            (c(C::ResolvedAt), Some(V::U64(0)), vec![]),
        ]
    } else {
        use SupplyBatchFieldV1 as B;
        let b = SupplyScalarFieldV1::Batch;
        vec![
            (
                b(B::Id),
                Some(V::Text { utf8_bytes: 11 }),
                b"inner-batch".to_vec(),
            ),
            (
                b(B::AuthorizationId),
                Some(V::Text { utf8_bytes: 10 }),
                b"inner-auth".to_vec(),
            ),
            (b(B::FrozenAt), Some(V::U64(0)), vec![]),
            (b(B::AuthorizationRevision), Some(V::U64(0)), vec![]),
            (b(B::FrozenEscrow), Some(V::I64(0)), vec![]),
            (b(B::Allocated), Some(V::Bool(false)), vec![]),
        ]
    }
}
/// These are independent point operations, each with a finite physical session.
/// No scope budget is reset inside an owner reader or qualifier.
pub(super) fn assert_fields(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    owner: SupplyRootKeyV1<'_>,
    proof: &ReservedValue<SupplyRecordQualificationV1>,
    class: MemoryClass,
    rich: bool,
) {
    let _outputs = memory.reserve(16 << 10, class).unwrap();
    for (field, expected, bytes) in fields(matches!(owner, SupplyRootKeyV1::Authorization(_)), rich)
    {
        let state = store.state_session(physical(), class).unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
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
        let mut output = Vec::new();
        assert_eq!(
            read_qualified_supply_scalar_v1(
                SupplyScalarRequestV1 {
                    owner,
                    field,
                    qualification: proof
                },
                &mut context,
                165,
                &mut output
            )
            .unwrap(),
            expected
        );
        assert_eq!(output, bytes);
    }
}
