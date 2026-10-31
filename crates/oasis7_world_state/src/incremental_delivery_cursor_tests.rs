use super::*;

#[test]
fn delivery_cursor_codec_preserves_era_and_event_id_in_fixed_order() {
    use crate::state::ProductValidationDeliveryCursor;
    for (era, event) in [(0, 0), (0, u64::MAX), (u64::MAX, 0), (7, 19)] {
        let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
        let mut pages = Pages::new(&memory);
        let cursor = ProductValidationDeliveryCursor {
            event_id_era: era,
            routed_through_event_id: event,
        };
        let record = write(
            &mut pages,
            &memory,
            MemberPayloadRefV1::DeliveryCursor(&cursor),
            WorldFieldRecordKindV1::Entry,
        );
        let bytes = raw(&pages, &memory, &record.value_root);
        assert_eq!(bytes[FIELD_MEMBER_DOMAIN.len() + 2], 8);
        assert_eq!(
            &bytes[bytes.len() - 16..bytes.len() - 8],
            &era.to_be_bytes()
        );
        assert_eq!(&bytes[bytes.len() - 8..], &event.to_be_bytes());
        let mut output = [0; 16];
        read_member_record_profile_v1(
            &record.value_root,
            &record.field_key,
            &mut ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota(),
            },
            MemberReadProfileV1 {
                codec: MemberPayloadCodecV1::DeliveryCursor,
                semantic_bytes: record.info.semantic_bytes,
                limits: limits(),
            },
            &mut std::io::sink(),
            &mut &mut output[..],
            || false,
        )
        .unwrap();
        assert_eq!(&output[..8], &era.to_be_bytes());
        assert_eq!(&output[8..], &event.to_be_bytes());
        let mut malformed = bytes;
        let start = FIELD_MEMBER_DOMAIN.len() + 11;
        malformed[start..start + 8].copy_from_slice(&15u64.to_be_bytes());
        malformed.pop();
        let root = store(&mut pages, &memory, &malformed);
        let error = read_member_record_v1(
            &root,
            &record.field_key,
            &mut ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota(),
            },
            MemberRecordLimitsV1 {
                payload_bytes: 0,
                ..limits()
            },
            &mut std::io::sink(),
            &mut std::io::sink(),
            || false,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ValueTreeError::Invalid("member fixed payload length")
        ));
    }
}
