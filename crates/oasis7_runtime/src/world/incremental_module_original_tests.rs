use super::*;
fn event(state: Vec<u8>, id: u64, cause: Option<CausedBy>) -> WorldEvent {
    WorldEvent {
        id,
        time: id,
        caused_by: cause,
        body: WorldEventBody::ModuleStateUpdated(oasis7_wasm_abi::ModuleStateUpdate {
            module_id: "m#2\0雪".into(),
            trace_id: "trace-\0🦀".into(),
            state,
        }),
    }
}
#[test]
fn streamed_canonical_original_matches_maintained_abi_at_integer_and_array_boundaries() {
    let memory = MemoryBudget::new(2 << 20, 0).unwrap();
    for (len, id) in [
        (0, 0),
        (1, 23),
        (23, 24),
        (24, 255),
        (255, 256),
        (256, 65535),
        (65535, 65536),
        (65536, u64::MAX),
    ] {
        for cause in [
            None,
            Some(CausedBy::Action(id)),
            Some(CausedBy::Effect {
                intent_id: "原\0因".into(),
            }),
        ] {
            let event = event((0..len).map(|n| n as u8).collect(), id, cause);
            let expected = oasis7_wasm_abi::encode_canonical_cbor(&event).unwrap();
            let plan = ModuleStateOriginal::new(&event, expected.len() as u64, &memory).unwrap();
            assert_eq!(plan.length, expected.len() as u64);
            assert_eq!(
                plan.digest(|| false).unwrap(),
                *blake3::hash(&expected).as_bytes()
            );
            for chunk in [1, 2, 7, 1024, 65536] {
                let mut reader = plan.reader();
                let mut actual = Vec::new();
                let mut buffer = vec![0; chunk];
                assert_eq!(reader.read(&mut []).unwrap(), 0);
                loop {
                    let n = reader.read(&mut buffer).unwrap();
                    if n == 0 {
                        break;
                    }
                    actual.extend_from_slice(&buffer[..n]);
                }
                assert_eq!(actual, expected);
                assert_eq!(reader.read(&mut buffer).unwrap(), 0);
            }
            drop(plan);
            assert_eq!(memory.used(), 0);
            assert!(ModuleStateOriginal::new(&event, expected.len() as u64 - 1, &memory).is_err());
            assert_eq!(memory.used(), 0);
        }
    }
}
#[test]
fn admission_and_midstream_cancel_do_not_materialize_original_or_retain_workspace() {
    let event = event(vec![255; 2 << 20], 1, None);
    let memory = MemoryBudget::new(128 << 10, 0).unwrap();
    let plan = ModuleStateOriginal::new(&event, 8 << 20, &memory).unwrap();
    assert!(memory.used() < 64 << 10);
    let mut calls = 0;
    assert!(matches!(
        plan.digest(|| {
            calls += 1;
            calls == 3
        }),
        Err(BodyMutationError::Value(ValueTreeError::Cancelled))
    ));
    drop(plan);
    assert_eq!(memory.used(), 0);
    let insufficient = MemoryBudget::new(1024, 0).unwrap();
    assert!(ModuleStateOriginal::new(&event, 8 << 20, &insufficient).is_err());
    assert_eq!(insufficient.used(), 0);
}
