use super::*;

fn locator() -> BusinessOutputLocatorV1 {
    BusinessOutputLocatorV1 {
        commit_body: [1; 32],
        descriptor: OriginalArtifactRefV1 {
            id: [2; 32],
            byte_len: 1,
        },
        delta: OriginalArtifactRefV1 {
            id: [3; 32],
            byte_len: 64 * 1024 * 1024,
        },
    }
}

#[test]
fn business_locator_keeps_exact_framing_and_its_output_reservation() {
    let memory = MemoryBudget::new(4096, 0).unwrap();
    let value = locator();
    let encoded = value
        .canonical_bytes_with_work(&memory, &mut |_, _| Ok(()))
        .unwrap();
    let mut expected = b"oasis7.iws.business-output.v1\0".to_vec();
    expected.extend_from_slice(&112u32.to_be_bytes());
    expected.extend_from_slice(&[1; 32]);
    expected.extend_from_slice(&[2; 32]);
    expected.extend_from_slice(&1u64.to_be_bytes());
    expected.extend_from_slice(&[3; 32]);
    expected.extend_from_slice(&(64u64 * 1024 * 1024).to_be_bytes());
    assert_eq!(*encoded, expected);
    assert_eq!(
        BusinessOutputLocatorV1::from_canonical_bytes_with_work(&encoded, &mut |_, _| Ok(()))
            .unwrap(),
        value
    );
    assert!(encoded.uses_budget(&memory));
    assert_eq!(memory.used(), encoded.reserved_bytes());
    drop(encoded);
    assert_eq!(memory.used(), 0);

    expected.push(0);
    assert!(
        BusinessOutputLocatorV1::from_canonical_bytes_with_work(&expected, &mut |_, _| Ok(()))
            .is_err()
    );
    expected.pop();
    expected[0] ^= 1;
    assert!(
        BusinessOutputLocatorV1::from_canonical_bytes_with_work(&expected, &mut |_, _| Ok(()))
            .is_err()
    );
}

#[test]
fn business_locator_rejects_unbounded_or_aliased_originals() {
    let memory = MemoryBudget::new(4096, 0).unwrap();
    for mode in 0..5 {
        let mut value = locator();
        match mode {
            0 => value.descriptor.byte_len = 0,
            1 => value.delta.byte_len = 0,
            2 => value.descriptor.byte_len = MAX_ORIGINAL_BYTES + 1,
            3 => value.delta.byte_len = MAX_ORIGINAL_BYTES + 1,
            _ => value.delta.id = value.descriptor.id,
        }
        assert!(
            value
                .canonical_bytes_with_work(&memory, &mut |_, _| Ok(()))
                .is_err()
        );
        assert_eq!(memory.used(), 0);
    }
}

#[test]
fn business_locator_preserves_first_work_failure_and_admission_failure() {
    let memory = MemoryBudget::new(4096, 0).unwrap();
    let mut calls = 0;
    let error = locator()
        .canonical_bytes_with_work(&memory, &mut |_, _| {
            calls += 1;
            if calls == 3 {
                Err(ExecutionError::Invalid("original work failure"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert!(matches!(
        error,
        ExecutionError::Invalid("original work failure")
    ));
    assert_eq!(
        calls, 3,
        "later fields must not debit after the first writer failure"
    );
    assert_eq!(memory.used(), 0);
    let tiny = MemoryBudget::new(1, 0).unwrap();
    assert!(matches!(
        locator().canonical_bytes_with_work(&tiny, &mut |_, _| Ok(())),
        Err(ExecutionError::Memory(_))
    ));
    assert_eq!(tiny.used(), 0);
}
