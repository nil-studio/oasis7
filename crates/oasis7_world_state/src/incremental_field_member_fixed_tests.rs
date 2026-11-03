//! Independent fixed-width wire boundaries and short-payload rejection.
use super::*;

#[test]
fn fixed_member_reader_never_zero_pads_short_bytes_payloads() {
    // A locator is 56 bytes. Its 40-byte prefix must not acquire a fabricated
    // height/ordinal of zero merely because the destination array starts zeroed.
    for length in [56, 0, 40, 55, 57] {
        let memory = MemoryBudget::new(16 << 20, 0).unwrap();
        let mut pages = Pages::new(&memory);
        let payload = [7; 57];
        let record = write_member_record_v1(
            descriptor(WorldFieldRecordKindV1::Bytes),
            WorldFieldRecordKindV1::Bytes,
            "original-key",
            MemberPayloadRefV1::Bytes(&payload[..length]),
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut quota(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 4096,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 1024,
                    record_bytes: 4096,
                },
                cancelled: || false,
            },
        )
        .unwrap();
        let mut io = quota();
        let mut values = ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        };
        let result = crate::incremental_inventory_value_read::InventoryValueReader {
            values: &mut values,
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            semantic_bytes: 1024,
            record_bytes: 4096,
            cancelled: &mut || false,
        }
        .fixed::<56>(
            &record.value_root,
            &record.field_key,
            record.info.semantic_bytes,
            MemberPayloadCodecV1::Bytes,
        );
        if length == 56 {
            assert_eq!(result.unwrap(), [7; 56]);
        } else {
            assert!(result.is_err(), "short/oversize wire accepted: {length}");
        }
    }
}

#[test]
fn fixed_u64x7_is_entry_only_fixed_length_and_cancellable() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let _output = memory.reserve(8192, MemoryClass::Ordinary).unwrap();

    let mut wrong_kind_pages = Pages::new(&memory);
    let wrong_kind = write_member_record_v1(
        descriptor(WorldFieldRecordKindV1::Bytes),
        WorldFieldRecordKindV1::Bytes,
        "é",
        MemberPayloadRefV1::FixedU64x7([1, 2, 3, 4, 5, 6, 7]),
        &mut MemberRecordWriteContext {
            sink: &mut wrong_kind_pages,
            memory: &memory,
            io: &mut quota(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            limits: limits(),
            cancelled: || false,
        },
    );
    assert!(matches!(
        wrong_kind,
        Err(MemberRecordError::Invalid("member key/payload kind"))
    ));
    assert!(wrong_kind_pages.nodes.borrow().is_empty());

    let mut pages = Pages::new(&memory);
    let mut record = write(
        &mut pages,
        &memory,
        MemberPayloadRefV1::FixedU64x7([1, 2, 3, 4, 5, 6, 7]),
        WorldFieldRecordKindV1::Entry,
    );
    let mut bytes = raw(&pages, &memory, &record.value_root);
    let codec_at = FIELD_MEMBER_DOMAIN.len() + 2;
    bytes[codec_at] = 12;
    record.value_root = store(&mut pages, &memory, &bytes);
    assert!(matches!(
        read(
            &record,
            &pages,
            &memory,
            limits(),
            &mut Vec::new(),
            &mut Vec::new(),
            || false,
        ),
        Err(ValueTreeError::Invalid("member payload codec"))
    ));

    let mut bytes = raw(&pages, &memory, &record.value_root);
    bytes[codec_at] = 11;
    let payload_length_at = FIELD_MEMBER_DOMAIN.len() + 11;
    bytes[payload_length_at..payload_length_at + 8].copy_from_slice(&55u64.to_be_bytes());
    bytes.pop();
    record.value_root = store(&mut pages, &memory, &bytes);
    let mut key = Vec::new();
    let mut payload = Vec::new();
    assert!(matches!(
        read(
            &record,
            &pages,
            &memory,
            limits(),
            &mut key,
            &mut payload,
            || false,
        ),
        Err(ValueTreeError::Invalid("member fixed payload length"))
    ));
    assert!(key.is_empty());
    assert!(payload.is_empty());

    let record = write(
        &mut pages,
        &memory,
        MemberPayloadRefV1::FixedU64x7([1, 2, 3, 4, 5, 6, 7]),
        WorldFieldRecordKindV1::Entry,
    );
    let cancelled = Cell::new(false);
    struct CancelOutput<'a>(&'a Cell<bool>);
    impl Write for CancelOutput<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.set(true);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(
        read(
            &record,
            &pages,
            &memory,
            limits(),
            &mut Vec::new(),
            &mut CancelOutput(&cancelled),
            || cancelled.get(),
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert!(cancelled.get());
}
