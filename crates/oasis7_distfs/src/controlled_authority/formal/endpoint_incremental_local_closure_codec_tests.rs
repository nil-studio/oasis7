use super::*;
fn record() -> LocalDataClosureRecordV1 {
    LocalDataClosureRecordV1 {
        version: 1,
        instance: [1; 32],
        authority: [2; 32],
        activation: [3; 32],
        epoch: 1,
        record: [4; 32],
        body: [5; 32],
        roots: [6; 32],
        manifest: [7; 32],
        page_lease: (8, 9),
        native_lease: (10, 11),
        native_generation: None,
        cognition: LocalFactLocationV1 {
            height: 0,
            ordinal: 1,
            original: [12; 32],
        },
        runtime: LocalFactLocationV1 {
            height: 0,
            ordinal: 2,
            original: [13; 32],
        },
    }
}
#[test]
fn local_record_fixed_width_rejects_truncation_unknown_version_and_generation_option() {
    let mut record = record();
    for generation in [None, Some([0; 32]), Some([255; 32])] {
        record.native_generation = generation;
        let raw = serde_cbor::to_vec(&record).unwrap();
        assert_eq!(raw.len(), WIRE_BYTES);
        let recovered: LocalDataClosureRecordV1 = serde_cbor::from_slice(&raw).unwrap();
        assert_eq!(recovered.native_generation, generation);
        assert_eq!(serde_cbor::to_vec(&recovered).unwrap(), raw);
        assert!(serde_cbor::from_slice::<LocalDataClosureRecordV1>(&raw[..raw.len() - 1]).is_err());
        let mut bad = raw.clone();
        bad[3 + DOMAIN.len() + 3] = 2;
        assert!(serde_cbor::from_slice::<LocalDataClosureRecordV1>(&bad).is_err());
        let mut bad = raw;
        bad[3 + DOMAIN.len() + 4 + 3 * 32 + 8 + 4 * 32 + 4 * 8] = 2;
        assert!(serde_cbor::from_slice::<LocalDataClosureRecordV1>(&bad).is_err());
    }
}
