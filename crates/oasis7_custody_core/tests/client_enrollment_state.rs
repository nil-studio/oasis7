use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::{
    approval_admission::*, client_enrollment::*, http_pop::*, time_units::*,
};
use sha2::{Digest, Sha256};
struct Strict;
impl ApprovalSignatureVerifier for Strict {
    fn verify(&mut self, c: &ExactSignatureChecks) -> Result<(), ApprovalAdmissionError> {
        for j in c.checks() {
            let mut pk = [0; 32];
            for (i, p) in j.public_key_hex().as_bytes().chunks_exact(2).enumerate() {
                pk[i] = u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap();
            }
            let key = ed25519_dalek::VerifyingKey::from_bytes(&pk).unwrap();
            let sig = ed25519_dalek::Signature::from_slice(
                &URL_SAFE_NO_PAD.decode(j.signature_base64url()).unwrap(),
            )
            .unwrap();
            key.verify_strict(j.canonical_bytes(), &sig)
                .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
        }
        Ok(())
    }
}
const START: u64 = 1_700_000_000_000;
fn physical() -> EnrollmentPhysicalCurrent<'static> {
    EnrollmentPhysicalCurrent {
        actual_actor_id: "actor-one",
        expected_actor_id: "actor-one",
        actual_realm: "owner-enrollment",
        expected_realm: "owner-enrollment",
        public_origin: "https://custody.example",
    }
}
fn initial() -> ClientEnrollmentStateV1 {
    let key = SigningKey::from_bytes(&[8; 32]);
    let public = key
        .verifying_key()
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let request = ClientEnrollmentCreateRequestV1 {
        api_version: 1,
        request_id: "create-one".into(),
        target: PairingInviteTargetRefV1 {
            custody_account_id: "account-one".into(),
            provision_id: "provision-one".into(),
            invite_id: "invite-one".into(),
            invite_digest: "ab".repeat(32),
        },
        descriptor: HostDeploymentDescriptorV1 {
            api_version: 1,
            environment: "test".into(),
            transport_profile: HostTransportProfileV1::NativeBrokerCustodyV1,
            tool_profile: "custody-native-v1".into(),
            host_instance_id: "host-one".into(),
            client_public_key: public,
            build_digest: "bc".repeat(32),
            skill_manifest_digest: "cd".repeat(32),
            tool_manifest_digest: "de".repeat(32),
            https_origin: "https://host.example".into(),
            restricted_runtime_identity: "synthetic-principal-not-isolation-acceptance".into(),
            isolation_mechanism: HostIsolationMechanismV1::SeparateOsPrincipal,
            isolation_acceptance_refs: vec!["synthetic-no-CT008-PASS".into()],
        },
    };
    ClientEnrollmentStateV1::new(
        &serde_json::to_vec(&request).unwrap(),
        EnrollmentServerIds {
            enrollment_id: "enrollment-one",
            key_id: "client-one",
            nonce: &URL_SAFE_NO_PAD.encode([2; 32]),
        },
        &physical(),
        UnixMillis::new(START),
    )
    .unwrap()
}
fn request(s: &ClientEnrollmentStateV1) -> ClientEnrollmentProofRequestV1 {
    let r = s.registration();
    ClientEnrollmentProofRequestV1 {
        api_version: 1,
        enrollment_id: r.enrollment_id.clone(),
        request_id: "prove-one".into(),
        target: r.request.target.clone(),
        descriptor_digest: r.descriptor_digest.clone(),
        client_public_key: r.request.descriptor.client_public_key.clone(),
        server_nonce: r.server_nonce.clone(),
        challenge_expires_at: r.challenge_expires_at,
    }
}
fn possession(s: &ClientEnrollmentStateV1, body: &[u8]) -> HttpPossessionReceipt {
    let r = s.registration();
    let target = CanonicalHttpTarget::new(
        &r.public_origin,
        HttpPopRoute::EnrollmentProve(r.enrollment_id.clone()),
    )
    .unwrap();
    let request: ClientEnrollmentProofRequestV1 = serde_json::from_slice(body).unwrap();
    let material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile: HttpPopProfileV1::EnrollmentProof,
        target: &target,
        key_id: &r.key_id,
        binding_id: &r.enrollment_id,
        request_id: &request.request_id,
        nonce: Some(&r.server_nonce),
        token: None,
        created: UnixSeconds::new(START / 1000),
        expires: UnixSeconds::new(START / 1000 + 29),
        body,
    })
    .unwrap();
    let headers = material.signed_headers(
        &SigningKey::from_bytes(&[8; 32])
            .sign(material.signature_base())
            .to_bytes(),
    );
    let raw = headers
        .iter()
        .map(|h| HttpHeader {
            name: &h.name,
            value: &h.value,
        })
        .collect::<Vec<_>>();
    let empty: [u8; 32] = Sha256::digest(b"").into();
    let proof = prepare_http_pop(
        &HttpPopMessage {
            method: "POST",
            target_uri: target.uri(),
            headers: &raw,
            representation: HttpHeaderRepresentation::RawInstances,
            body,
        },
        &HttpPopBinding {
            profile: HttpPopProfileV1::EnrollmentProof,
            target: &target,
            key_id: &r.key_id,
            public_key_hex: &r.request.descriptor.client_public_key,
            binding_id: &r.enrollment_id,
            request_id: &request.request_id,
            nonce: Some(&r.server_nonce),
            credential_digest: &empty,
            valid_until: r.challenge_expires_at,
        },
        UnixMillis::new(START + 1000),
    )
    .unwrap();
    verify_http_possession(proof, UnixMillis::new(START + 1000), &mut Strict).unwrap()
}
#[test]
fn directed_enrollment_proof_is_immutable_historical_and_not_a_grant() {
    let mut s = initial();
    let body = serde_json::to_vec(&request(&s)).unwrap();
    let receipt = possession(&s, &body);
    let fact = s
        .prove(&body, receipt, &physical(), UnixMillis::new(START + 1000))
        .unwrap();
    assert_eq!(
        fact.registration.request.target.custody_account_id,
        "account-one"
    );
    let checks = fact.checks().unwrap();
    Strict.verify(&checks).unwrap();
    let before = s.clone();
    let receipt = possession(&s, &body);
    assert_eq!(
        s.prove(&body, receipt, &physical(), UnixMillis::new(START + 2000))
            .unwrap(),
        fact
    );
    assert_eq!(s, before);
    assert_eq!(
        serde_json::from_value::<ClientEnrollmentStateV1>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
    // The historical fact survives ten-minute expiry, without a live-grant API.
    assert!(s.proved().is_some());
    let mut raw = serde_json::to_value(&s).unwrap();
    raw["proved"]["prove_request"]["target"]["custody_account_id"] = "other-account".into();
    assert!(serde_json::from_value::<ClientEnrollmentStateV1>(raw).is_err());
    assert!(
        HostDeploymentAllowlist::default()
            .configured_metadata(&fact.registration.request.descriptor)
            .unwrap()
            .is_none()
    );
    assert!(
        fact.http_evidence
            .signature_base
            .contains("@signature-params")
    );
    assert!(!fact.http_evidence.signature_base.contains("authorization"));
    assert_eq!(
        STANDARD
            .decode("47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=")
            .unwrap(),
        Sha256::digest(b"").to_vec()
    );
}
#[test]
fn valid_signature_for_another_target_or_actor_and_stale_final_clock_reject_atomically() {
    for case in 0..4 {
        let mut s = initial();
        let mut req = request(&s);
        if case == 0 {
            req.target.custody_account_id = "other-account".into();
        }
        let body = serde_json::to_vec(&req).unwrap();
        let receipt = possession(&s, &body);
        let mut current = physical();
        if case == 1 {
            current.actual_actor_id = "other-actor";
            current.expected_actor_id = "other-actor";
        }
        if case == 2 {
            current.actual_realm = "other-realm";
            current.expected_realm = "other-realm";
        }
        let now = if case == 3 {
            START + 600001
        } else {
            START + 1000
        };
        let before = s.clone();
        assert!(
            s.prove(&body, receipt, &current, UnixMillis::new(now))
                .is_err(),
            "case {case}"
        );
        assert_eq!(s, before);
    }
}
