//! Software-only cache provenance and atomic-publication regressions. No
//! ActualWorld admission, network permission or hardware claim is made here.
use super::*;
fn published() -> (
    BftPrivateRetentionMemoryBackendV1,
    BftPrivateRetentionSnapshotV1,
) {
    let (mut memory, request) = backend();
    let original = memory.compare_and_swap_private_retention(&request).unwrap();
    (memory, original)
}
#[test]
fn private_retention_memory_immutable_witness_tampering_never_mints_or_falls_back() {
    for mode in 0..8 {
        let (mut memory, original) = published();
        let counts = memory.inner.lock().unwrap().hash_probe.clone();
        if mode == 6 {
            memory.trusted.creation_marker_digest = "ee".repeat(32);
        }
        {
            let mut actual = memory.inner.lock().unwrap();
            match mode {
                0 => actual.material_epoch += 1,
                1 => {
                    actual.validated_inputs =
                        ValidatedMemoryInputs::empty(&memory.trusted, actual.material_epoch)
                            .unwrap()
                }
                2 => {
                    let current = Arc::make_mut(actual.original.as_mut().unwrap());
                    let input = current.inputs.values_mut().next().unwrap();
                    *input = Arc::new(input.as_ref().clone());
                }
                3 => Arc::make_mut(actual.original.as_mut().unwrap())
                    .inputs
                    .clear(),
                4 => {
                    let current = Arc::make_mut(actual.original.as_mut().unwrap());
                    current.retention.map.entries[0].key.height += 1;
                }
                5 => actual.signer.state.binding_digest = "ed".repeat(32),
                6 => {}
                7 => {
                    let (id, input) = original.inputs.first_key_value().unwrap();
                    actual
                        .sealed_inputs
                        .insert(id.clone(), Arc::new(input.as_ref().clone()));
                }
                _ => unreachable!(),
            }
        }
        let (before_state, before_original, before_epoch, before_witnesses) = {
            let actual = memory.inner.lock().unwrap();
            (
                actual.signer.clone(),
                actual.original.clone().unwrap(),
                actual.material_epoch,
                actual.validated_inputs.clone(),
            )
        };
        let before_hashes = counts.snapshot();
        assert!(
            memory
                .read_for_lease(
                    original.signer.lease_generation,
                    &original.signer.lease_token_digest
                )
                .is_err(),
            "unexpected actual witness/role/key/owner mutation {mode} must fail closed"
        );
        assert_eq!(
            counts.snapshot(),
            before_hashes,
            "read-time rejection cannot mint a replacement ticket by hashing data again"
        );
        let actual = memory.inner.lock().unwrap();
        assert_eq!(actual.signer, before_state);
        assert!(Arc::ptr_eq(
            actual.original.as_ref().unwrap(),
            &before_original
        ));
        assert_eq!(actual.material_epoch, before_epoch);
        assert!(actual.validated_inputs.same_witnesses(&before_witnesses));
    }
}
#[test]
fn private_retention_memory_failed_material_candidate_preserves_published_epoch_and_witnesses() {
    let (mut memory, original) = published();
    let mut map = original.retention.map.clone();
    map.entries[0].lifecycle = BftPrivateRetentionLifecycleV1::Ready;
    let mut copied = continuation(&original, map.clone());
    let input = copied.after.inputs.values_mut().next().unwrap();
    *input = Arc::new(input.as_ref().clone());
    // Valid receipt/map/bytes are insufficient to replace an already published
    // immutable allocation. Reach the candidate-witness check after full CAS validation.
    copied.validate_envelope(&memory.trusted).unwrap();
    let (before_epoch, before_witnesses) = {
        let actual = memory.inner.lock().unwrap();
        (actual.material_epoch, actual.validated_inputs.clone())
    };
    let error = memory
        .compare_and_swap_private_retention(&copied)
        .unwrap_err();
    assert!(
        error.to_string().contains("owner replaced"),
        "must reach immutable publication ownership: {error}"
    );
    {
        let actual = memory.inner.lock().unwrap();
        assert_eq!(actual.signer, original.signer);
        assert_eq!(actual.original.as_ref().unwrap().as_ref(), &original);
        assert_eq!(actual.material_epoch, before_epoch);
        assert!(actual.validated_inputs.same_witnesses(&before_witnesses));
    }
    assert_eq!(
        memory
            .read_for_lease(
                original.signer.lease_generation,
                &original.signer.lease_token_digest
            )
            .unwrap(),
        original.signer
    );
    let ready = continuation(&original, map);
    let installed = memory.compare_and_swap_private_retention(&ready).unwrap();
    ready.validate_exact_readback(&installed).unwrap();
    let actual = memory.inner.lock().unwrap();
    assert_eq!(actual.material_epoch, before_epoch + 1);
    assert!(!actual.validated_inputs.same_witnesses(&before_witnesses));
}
#[test]
fn private_retention_memory_fast_input_reads_still_check_exact_ref_missing_and_lease() {
    let (mut memory, original) = published();
    let input = original.inputs.values().next().unwrap();
    let reference = input.descriptor().unwrap();
    let counts = memory.inner.lock().unwrap().hash_probe.clone();
    let before = counts.snapshot();
    let mut wrong = reference.clone();
    wrong.object.digest = "ea".repeat(32);
    assert!(
        memory
            .load_private_retention_input(&reference.source.binding_digest, &wrong)
            .is_err()
    );
    let mut missing = reference.clone();
    missing.admission_digest = "e9".repeat(32);
    assert!(
        memory
            .load_private_retention_input(&reference.source.binding_digest, &missing)
            .is_err()
    );
    assert!(
        memory
            .read_for_lease(
                original.signer.lease_generation + 1,
                &original.signer.lease_token_digest
            )
            .is_err()
    );
    assert!(
        memory
            .read_for_lease(original.signer.lease_generation, "foreign-token")
            .is_err()
    );
    assert!(Arc::ptr_eq(
        &memory
            .load_private_retention_input(&reference.source.binding_digest, &reference)
            .unwrap(),
        input
    ));
    assert_eq!(
        memory
            .read_for_lease(
                original.signer.lease_generation,
                &original.signer.lease_token_digest
            )
            .unwrap(),
        original.signer
    );
    assert_eq!(
        counts.snapshot(),
        before,
        "exact metadata/fence checks never rehash or self-mint input witnesses"
    );
}
