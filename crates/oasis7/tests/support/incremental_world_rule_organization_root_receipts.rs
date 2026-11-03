use super::*;
/// Exercise duplicate Vec values as separate ordinals, empty Text, and tail-only
/// length changes in the same outer transaction. Cold owner acceptance follows.
pub(super) fn receipts<
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    mut proof: ReservedValue<OrganizationRecordQualificationV1>,
    state: &M,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> ReservedValue<OrganizationRecordQualificationV1> {
    use WorldRuleOrganizationReceiptChangeV1 as C;
    for length in 0..2 {
        let guard = context.transaction.guard();
        proof = change_qualified_organization_receipt_v1(
            C::Append {
                case: CASE,
                expected_len: length,
                after: "duplicate",
            },
            &proof,
            4 << 20,
            context,
            write,
            &guard,
            live,
        )
        .unwrap()
        .unwrap();
    }
    for ordinal in 0..2 {
        let mut output = Vec::new();
        let mut read = WorldFieldReadContextV1 {
            root: proof.root(),
            state,
            values: ValueReadContext {
                source: context.values.source,
                memory: context.values.memory,
                class: MemoryClass::Ordinary,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            take_state_error: &mut context.take_state_error,
            cancelled: &mut context.cancelled,
        };
        assert_eq!(
            read_qualified_organization_receipt_v1(
                CASE,
                &proof,
                ordinal,
                Some(2),
                &mut read,
                128,
                &mut output
            )
            .unwrap(),
            WorldRuleOrganizationReceiptValueV1::Value {
                length: 2,
                utf8_bytes: 9
            }
        );
        assert_eq!(output, b"duplicate");
    }
    let guard = context.transaction.guard();
    proof = change_qualified_organization_receipt_v1(
        C::Replace {
            case: CASE,
            ordinal: 0,
            expected_len: 2,
            expected: "duplicate",
            after: "",
            max_current_payload_bytes: 128,
        },
        &proof,
        4 << 20,
        context,
        write,
        &guard,
        live,
    )
    .unwrap()
    .unwrap();
    for (length, expected) in [(2, "duplicate"), (1, "")] {
        let guard = context.transaction.guard();
        proof = change_qualified_organization_receipt_v1(
            C::Pop {
                case: CASE,
                expected_len: length,
                expected,
                max_current_payload_bytes: 128,
            },
            &proof,
            4 << 20,
            context,
            write,
            &guard,
            live,
        )
        .unwrap()
        .unwrap();
    }
    let mut output = Vec::new();
    let mut read = WorldFieldReadContextV1 {
        root: proof.root(),
        state,
        values: ValueReadContext {
            source: context.values.source,
            memory: context.values.memory,
            class: MemoryClass::Ordinary,
            io: &mut *context.values.io,
        },
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        take_state_error: &mut context.take_state_error,
        cancelled: &mut context.cancelled,
    };
    assert_eq!(
        read_qualified_organization_receipt_v1(CASE, &proof, 0, Some(0), &mut read, 0, &mut output)
            .unwrap(),
        WorldRuleOrganizationReceiptValueV1::OutOfBounds { length: 0 }
    );
    assert!(output.is_empty());
    proof
}
