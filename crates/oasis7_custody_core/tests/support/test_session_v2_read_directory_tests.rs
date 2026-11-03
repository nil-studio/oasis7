//! Component fixture: actual Core paid transitions and complete signatures;
//! not a Worker delivery, Directory SQL, CF or complete ReadCurrent proof.
use super::*;
use oasis7_custody_core::{
    provisioning::ReadClaimSourceRouteV3, read_directory_admission_v3::*, read_directory_v3::*,
};
fn directory_material() -> (
    SignedReadDirectoryV3,
    ReadDirectoryVerifierV3,
    ReadDirectoryAdmissionScopeV3,
) {
    let mut p = ready();
    let operation = known(&p);
    let (body, pop) = read_pop(&mut p.f, "directory-source", &operation, 14_010);
    let paid =
        p.f.state
            .settle_test_session_v2_read_current_five(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: "directory-ingress",
                    original_bytes: &body,
                    possession: &pop,
                },
                UnixMillis::new(14_012),
            )
            .unwrap()
            .unwrap();
    let source =
        p.f.state
            .reserve_test_session_v2_read_current_source_five(
                paid,
                &oasis7_custody_core::approval::bytes_digest(b"directory-nonce"),
                UnixMillis::new(14_013),
            )
            .unwrap();
    let (template, mut pin, floor) = material();
    let signer = SigningKey::from_bytes(&[113; 32]);
    let account = "a7".repeat(32);
    let object = "b7".repeat(32);
    let authority =
        p.f.state
            .claim_test_session_v2_read_claim_source_callback(
                source,
                ReadClaimSourceRouteV3 {
                    issuer: template.issuer,
                    account_object_id: account.clone(),
                },
                UnixMillis::new(14_014),
            )
            .unwrap();
    pin.allowed_purpose = AccountMachinePurposeV2::ReadClaimSourceV3;
    let signature = hx(signer.sign(&authority.signing_bytes().unwrap()).to_bytes());
    let coordinator = authority.finish(signature, &pin, &floor, 14_014).unwrap();
    let callback =
        p.f.state
            .accept_test_session_v2_read_claim_source_callback(
                &coordinator,
                &pin,
                &floor,
                &account,
                UnixMillis::new(14_015),
            )
            .unwrap();
    let (_, authority) = callback.issue_directory(object.clone()).unwrap();
    let original = &authority.claim().query.original;
    let mut directory_pin = pin.clone();
    directory_pin.allowed_purpose = AccountMachinePurposeV2::ReadDirectoryV3;
    let verifier = ReadDirectoryVerifierV3 {
        directory_pin,
        coordinator_pin: pin,
        floor,
        native_key_id: original.input.approval.proof.key_id.clone(),
        native_public_key: hx(SigningKey::from_bytes(&[41; 32]).verifying_key().to_bytes()),
    };
    let scope = ReadDirectoryAdmissionScopeV3 {
        object_id: object,
        environment: coordinator.claim.issuer.environment.clone(),
        manager_identity_namespace: authority
            .claim()
            .query
            .manager_reference
            .identity_namespace
            .clone(),
        manager_identity_instance_epoch: authority
            .claim()
            .query
            .manager_reference
            .identity_instance_epoch,
        manager_account_id: authority.claim().query.manager_reference.account_id.clone(),
        issuer_namespace: coordinator.claim.issuer.namespace.clone(),
        issuer_service_id: coordinator.claim.issuer.service_id.clone(),
    };
    let signature = hx(signer.sign(&authority.signing_bytes().unwrap()).to_bytes());
    let proof = authority.finish(signature, &verifier, 14_016).unwrap();
    (proof, verifier, scope)
}
#[test]
fn read_directory_strong_once_cold_disabled_and_partial_fail_closed() {
    let (proof, verifier, scope) = directory_material();
    let account = proof.claim.coordinator.claim.account_object_id.clone();
    let config = ReadDirectoryAdmissionConfigV3 {
        capacity: DecimalU64(2),
        active: Some(verifier.clone()),
        history: vec![verifier.clone()],
    };
    proof
        .verify(&verifier, &scope.object_id, &account, 14_016)
        .unwrap();
    let (rows, first) = ReadDirectoryRowsV3::consume(
        None,
        &config,
        scope.clone(),
        proof.clone(),
        &account,
        14_016,
        |_| Ok(account.clone()),
    )
    .unwrap();
    first.require_observed(14_017, 14_017).unwrap();
    assert!(first.require_observed(19_012, 19_012).is_err());
    assert!(
        ReadDirectoryRowsV3::consume(
            Some(rows.clone()),
            &config,
            scope.clone(),
            proof.clone(),
            &account,
            14_017,
            |_| Ok(account.clone())
        )
        .is_err()
    );
    let wire = proof.wire_bytes().unwrap();
    let decoded = SignedReadDirectoryV3::decode_wire(&wire).unwrap();
    assert!(decoded == proof);
    let stored = rows.storage_transition(&proof).unwrap();
    let cold = ReadDirectoryRowsV3::decode_storage_rows(
        stored[0].as_bytes(),
        stored[1].as_bytes(),
        &[(stored[2].clone(), stored[3].clone())],
        &scope,
        &config,
    )
    .unwrap();
    let mut disabled = config.clone();
    disabled.active = None;
    cold.verify(&disabled, &scope, |_| Ok(account.clone()))
        .unwrap();
    assert!(
        ReadDirectoryRowsV3::consume(
            Some(cold.clone()),
            &disabled,
            scope.clone(),
            proof.clone(),
            &account,
            14_018,
            |_| Ok(account.clone())
        )
        .is_err()
    );
    let mut partial = cold.clone();
    partial.claims.clear();
    assert!(
        partial
            .verify(&config, &scope, |_| Ok(account.clone()))
            .is_err()
    );
    let mut changed = cold.clone();
    changed.header.count = DecimalU64(2);
    assert!(
        changed
            .verify(&config, &scope, |_| Ok(account.clone()))
            .is_err()
    );
    assert!(
        cold.verify(&config, &scope, |_| Ok("c7".repeat(32)))
            .is_err()
    );
    assert!(
        proof
            .verify(&verifier, &"c7".repeat(32), &account, 14_016)
            .is_err()
    );
    let mut bad = proof.clone();
    bad.claim.query.original.verification.signature = "AA".into();
    bad.signature_hex = hx(SigningKey::from_bytes(&[113; 32])
        .sign(&bad.claim.signing_bytes().unwrap())
        .to_bytes());
    assert!(
        bad.verify(&verifier, &scope.object_id, &account, 14_016)
            .is_err()
    );
    assert!(
        read_directory_scan_row_bound_v3().unwrap() >= read_directory_row_bound_v3(&scope).unwrap()
    );
}
