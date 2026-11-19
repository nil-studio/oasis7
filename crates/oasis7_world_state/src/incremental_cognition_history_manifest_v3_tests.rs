use super::*;
use oasis7_formal_protocol::incremental_state::{StateNamespaceV1, StateRootV1};
fn disabled() -> (
    CognitionHistoryViewV1,
    CognitionHistoryDerivedRefsV2,
    CognitionHistorySourceSummaryV3,
) {
    (
        CognitionHistoryViewV1 {
            roots: [9; 32],
            enabled: false,
            revision: 0,
            previous_view: None,
            heads: std::array::from_fn(|_| CognitionHistoryHeadV1 {
                sequence: 0,
                digest: None,
            }),
            indexes: std::array::from_fn(|_| CognitionHistoryIndexRefV1 {
                present: false,
                records: 0,
                root: StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
            }),
        },
        CognitionHistoryDerivedRefsV2 {
            caches: [None; 3],
            lifecycle: None,
        },
        CognitionHistorySourceSummaryV3 {
            mode: CognitionHistorySourceModeV3::Bootstrap,
            cache_state_digests: [None; 3],
            last: [None, None, None],
            lifecycle_keys: 0,
            lifecycle_journal_records: 0,
        },
    )
}
#[test]
fn v3_preserves_exact_v2_payload_and_rejects_ambiguous_framing() {
    let memory = MemoryBudget::new(2 << 20, 1 << 20).unwrap();
    let (view, derived, source) = disabled();
    let v2 = cognition_history_view_v2_bytes(&view, &derived, &memory, || false).unwrap();
    let v3 = cognition_history_view_v3_bytes(&view, &derived, &source, &memory, || false).unwrap();
    let begin = COGNITION_HISTORY_VIEW_V3_MAGIC.len() + 8;
    assert_eq!(&v3[begin..begin + v2.len()], v2.as_slice());
    let decoded =
        decode_cognition_history_view_v3(v3.as_slice(), &memory, MemoryClass::Recovery, || false)
            .unwrap();
    assert_eq!(*decoded.view, view);
    assert_eq!(decoded.derived, derived);
    assert_eq!(**decoded.source, source);
    let suffix = begin + COGNITION_HISTORY_VIEW_V2_BYTES;
    for offset in [
        COGNITION_HISTORY_VIEW_V3_MAGIC.len(),
        suffix,
        suffix + 32,
        suffix + 33,
        suffix + 65,
        suffix + 66,
        COGNITION_HISTORY_VIEW_V3_BYTES - 1,
    ] {
        let mut altered = *v3;
        altered[offset] = 255;
        assert!(
            decode_cognition_history_view_v3(&altered, &memory, MemoryClass::Recovery, || false)
                .is_err(),
            "offset={offset}"
        );
    }
    let mut trailing = v3.to_vec();
    trailing.push(0);
    assert!(
        decode_cognition_history_view_v3(&trailing, &memory, MemoryClass::Recovery, || false)
            .is_err()
    );
    assert!(
        decode_cognition_history_view_v3(
            &v3[..v3.len() - 1],
            &memory,
            MemoryClass::Recovery,
            || false
        )
        .is_err()
    );
    assert!(matches!(
        decode_cognition_history_view_v3(v3.as_slice(), &memory, MemoryClass::Recovery, || true),
        Err(ValueTreeError::Cancelled)
    ));
}
