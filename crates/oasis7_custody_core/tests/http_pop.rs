use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::{
    approval_admission::{ApprovalAdmissionError, ApprovalSignatureVerifier, ExactSignatureChecks},
    http_pop::*,
    time_units::*,
};
const TOKEN: &str = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE";
const NONCE: &str = "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI";
struct StrictPort;
impl ApprovalSignatureVerifier for StrictPort {
    fn verify(&mut self, checks: &ExactSignatureChecks) -> Result<(), ApprovalAdmissionError> {
        for c in checks.checks() {
            let mut key = [0u8; 32];
            for (i, pair) in c.public_key_hex().as_bytes().chunks_exact(2).enumerate() {
                key[i] = u8::from_str_radix(
                    std::str::from_utf8(pair).map_err(|_| ApprovalAdmissionError::Invalid)?,
                    16,
                )
                .map_err(|_| ApprovalAdmissionError::Invalid)?;
            }
            let key = ed25519_dalek::VerifyingKey::from_bytes(&key)
                .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
            let bytes = URL_SAFE_NO_PAD
                .decode(c.signature_base64url())
                .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
            let signature = ed25519_dalek::Signature::from_slice(&bytes)
                .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
            key.verify_strict(c.canonical_bytes(), &signature)
                .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
        }
        Ok(())
    }
}
struct Fixture {
    target: CanonicalHttpTarget,
    profile: HttpPopProfileV1,
    headers: Vec<OwnedHttpHeader>,
    body: Vec<u8>,
    public: String,
    digest: [u8; 32],
    binding: String,
}
impl Fixture {
    fn new(profile: HttpPopProfileV1, route: HttpPopRoute) -> Self {
        let target = CanonicalHttpTarget::new("https://custody.example", route).unwrap();
        let key = SigningKey::from_bytes(&[7; 32]);
        let public = key
            .verifying_key()
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let body = if target.method() == "GET" {
            vec![]
        } else {
            b"{}".to_vec()
        };
        let binding = if profile == HttpPopProfileV1::EnrollmentProof {
            "enrollment-one"
        } else {
            "session-one"
        }
        .to_owned();
        let token = if matches!(
            profile,
            HttpPopProfileV1::EnrollmentProof
                | HttpPopProfileV1::CredentialClaim
                | HttpPopProfileV1::ClaimChallenge
        ) {
            None
        } else {
            Some(TOKEN)
        };
        let nonce = if profile.is_challenge() {
            None
        } else {
            Some(NONCE)
        };
        let material = build_http_pop_signing_material(&HttpPopSigningInput {
            profile,
            target: &target,
            key_id: "client-one",
            binding_id: &binding,
            request_id: "request-one",
            nonce,
            token,
            created: UnixSeconds::new(1_700_000_000),
            expires: UnixSeconds::new(1_700_000_030),
            body: &body,
        })
        .unwrap();
        let headers = material.signed_headers(&key.sign(material.signature_base()).to_bytes());
        let digest = token.map_or_else(
            || {
                use sha2::Digest;
                sha2::Sha256::digest(b"").into()
            },
            |t| token_digest(t).unwrap(),
        );
        Self {
            target,
            profile,
            headers,
            body,
            public,
            digest,
            binding,
        }
    }
    fn prepare(
        &self,
        representation: HttpHeaderRepresentation,
        now: u64,
        deadline: u64,
    ) -> Result<PreparedHttpPop, HttpPopError> {
        let headers = self
            .headers
            .iter()
            .map(|h| HttpHeader {
                name: &h.name,
                value: &h.value,
            })
            .collect::<Vec<_>>();
        prepare_http_pop(
            &HttpPopMessage {
                method: self.target.method(),
                target_uri: self.target.uri(),
                headers: &headers,
                representation,
                body: &self.body,
            },
            &HttpPopBinding {
                profile: self.profile,
                target: &self.target,
                key_id: "client-one",
                public_key_hex: &self.public,
                binding_id: &self.binding,
                request_id: "request-one",
                nonce: if self.profile.is_challenge() {
                    None
                } else {
                    Some(NONCE)
                },
                credential_digest: &self.digest,
                valid_until: UnixMillis::new(deadline),
            },
            UnixMillis::new(now),
        )
    }
    fn set(&mut self, name: &str, value: String) {
        self.headers
            .iter_mut()
            .find(|h| h.name == name)
            .unwrap()
            .value = value;
    }
}
#[test]
fn http_pop_fixed_rfc_base_independent_digest_vector_and_direct_signature() {
    let f = Fixture::new(HttpPopProfileV1::Custody, HttpPopRoute::Actions);
    let material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile: f.profile,
        target: &f.target,
        key_id: "client-one",
        binding_id: "session-one",
        request_id: "request-one",
        nonce: Some(NONCE),
        token: Some(TOKEN),
        created: UnixSeconds::new(1_700_000_000),
        expires: UnixSeconds::new(1_700_000_030),
        body: b"{}",
    })
    .unwrap();
    // Digests independently computed with Python hashlib; the base follows RFC
    // 9421 section 2.5, not the World canonical-CBOR signature codec.
    let expected = concat!(
        "\"@method\": POST\n",
        "\"@target-uri\": https://custody.example/v1/actions\n",
        "\"content-type\": application/json\n",
        "\"content-digest\": sha-256=:RBNvo1WzZ4oRRq0W9+hknpT7T8If536DEMBg9hyq/4o=:\n",
        "\"oasis7-token-digest\": :VtX6czP210fbQsI5QH5dpMMvTHnzXQkrE0/TWkAtnFw=:\n",
        "\"oasis7-test-session\": session-one\n",
        "\"oasis7-request-id\": request-one\n",
        "\"@signature-params\": (\"@method\" \"@target-uri\" \"content-type\" \"content-digest\" \"oasis7-token-digest\" \"oasis7-test-session\" \"oasis7-request-id\");created=1700000000;expires=1700000030;nonce=\"AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI\";alg=\"ed25519\";keyid=\"client-one\";tag=\"oasis7-custody-client-v1\""
    );
    assert_eq!(material.signature_base(), expected.as_bytes());
    let proof = f
        .prepare(
            HttpHeaderRepresentation::RawInstances,
            1_700_000_001_000,
            1_700_000_030_999,
        )
        .unwrap();
    let checks = proof.signature_checks().unwrap();
    assert_eq!(checks.checks()[0].canonical_bytes(), expected.as_bytes());
    let receipt =
        verify_http_possession(proof, UnixMillis::new(1_700_000_001_001), &mut StrictPort).unwrap();
    assert_eq!(receipt.nonce(), Some(NONCE));
    assert_eq!(receipt.request_id(), "request-one");
    assert_eq!(receipt.method(), "POST");
    assert_eq!(receipt.binding_id(), "session-one");
}
#[test]
fn http_pop_profiles_and_get_empty_are_distinct_not_empty_token_actions() {
    for (p, r) in [
        (HttpPopProfileV1::Mcp, HttpPopRoute::Mcp),
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        (
            HttpPopProfileV1::McpChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        (
            HttpPopProfileV1::ClaimChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        (
            HttpPopProfileV1::EnrollmentProof,
            HttpPopRoute::EnrollmentProve("enrollment-one".into()),
        ),
        (
            HttpPopProfileV1::CredentialClaim,
            HttpPopRoute::EnrollmentClaim("enrollment-one".into()),
        ),
        (
            HttpPopProfileV1::Custody,
            HttpPopRoute::Operation("operation-one".into()),
        ),
    ] {
        let f = Fixture::new(p, r);
        let proof = f
            .prepare(
                HttpHeaderRepresentation::WorkerMergedSingletons,
                1_700_000_001_000,
                1_700_000_030_000,
            )
            .unwrap();
        let receipt =
            verify_http_possession(proof, UnixMillis::new(1_700_000_001_001), &mut StrictPort)
                .unwrap();
        assert_eq!(receipt.profile(), p);
        assert_eq!(receipt.nonce().is_none(), p.is_challenge());
    }
    let f = Fixture::new(HttpPopProfileV1::Custody, HttpPopRoute::Actions);
    let wrong = build_http_pop_signing_material(&HttpPopSigningInput {
        profile: f.profile,
        target: &f.target,
        key_id: "client-one",
        binding_id: "session-one",
        request_id: "request-one",
        nonce: Some(NONCE),
        token: None,
        created: UnixSeconds::new(1),
        expires: UnixSeconds::new(2),
        body: b"{}",
    });
    assert!(wrong.is_err());
    assert_eq!(
        content_digest(b""),
        "sha-256=:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=:"
    );
}
#[test]
fn http_pop_tamper_multi_label_unknown_param_and_body_octets_reject() {
    for case in 0..11 {
        let mut f = Fixture::new(HttpPopProfileV1::Custody, HttpPopRoute::Actions);
        match case {
            0 => f.body = b"{ }".to_vec(),
            1 => f.set("content-digest", "sha-512=:AA==:".into()),
            2 => f.set("oasis7-request-id", "other-request".into()),
            3 => f.set("oasis7-test-session", "other-session".into()),
            4 => f.set(
                "oasis7-token-digest",
                format!(":{}:", STANDARD.encode([0; 32])),
            ),
            5 => {
                let old = f
                    .headers
                    .iter()
                    .find(|h| h.name == "signature-input")
                    .unwrap()
                    .value
                    .clone();
                f.set("signature-input", format!("{old}, second=()"));
            }
            6 => {
                let old = f
                    .headers
                    .iter()
                    .find(|h| h.name == "signature-input")
                    .unwrap()
                    .value
                    .clone();
                f.set("signature-input", format!("{old};created=1700000000"));
            }
            7 => {
                let old = f
                    .headers
                    .iter()
                    .find(|h| h.name == "signature-input")
                    .unwrap()
                    .value
                    .replace("alg=\"ed25519\"", "alg=\"ed25519ph\"");
                f.set("signature-input", old);
            }
            8 => f.set("content-type", "application/json;charset=utf-8".into()),
            9 => f.headers.push(OwnedHttpHeader {
                name: "Content-Encoding".into(),
                value: "identity".into(),
            }),
            _ => f.set("signature", "oasis7=:AAAA:".into()),
        };
        assert!(
            f.prepare(
                HttpHeaderRepresentation::RawInstances,
                1_700_000_001_000,
                1_700_000_030_000
            )
            .is_err(),
            "mutation {case}"
        );
    }
}
#[test]
fn http_pop_raw_duplicates_and_merged_singleton_ambiguity_never_split() {
    for name in [
        "content-type",
        "signature-input",
        "signature",
        "oasis7-capability",
        "oasis7-token-digest",
        "oasis7-test-session",
        "oasis7-request-id",
    ] {
        for empty in [false, true] {
            let mut f = Fixture::new(HttpPopProfileV1::Custody, HttpPopRoute::Actions);
            let old = f
                .headers
                .iter()
                .find(|h| h.name == name)
                .unwrap()
                .value
                .clone();
            f.headers.push(OwnedHttpHeader {
                name: name.to_ascii_uppercase(),
                value: if empty { String::new() } else { old.clone() },
            });
            assert!(
                f.prepare(
                    HttpHeaderRepresentation::RawInstances,
                    1_700_000_001_000,
                    1_700_000_030_000
                )
                .is_err()
            );
            f.headers.pop();
            for merged in [
                format!("{old}, {old}"),
                format!("{old},"),
                format!(",{old}"),
                ",".into(),
            ] {
                f.set(name, merged);
                assert!(
                    f.prepare(
                        HttpHeaderRepresentation::WorkerMergedSingletons,
                        1_700_000_001_000,
                        1_700_000_030_000
                    )
                    .is_err()
                );
            }
        }
    }
    let mut f = Fixture::new(HttpPopProfileV1::Custody, HttpPopRoute::Actions);
    f.headers.push(OwnedHttpHeader {
        name: "accept".into(),
        value: "application/json, text/plain".into(),
    });
    assert!(
        f.prepare(
            HttpHeaderRepresentation::WorkerMergedSingletons,
            1_700_000_001_000,
            1_700_000_030_000
        )
        .is_ok()
    );
}
#[test]
fn http_pop_deadline_and_time_units_do_not_extend_authority() {
    let f = Fixture::new(HttpPopProfileV1::Custody, HttpPopRoute::Actions);
    assert!(
        f.prepare(
            HttpHeaderRepresentation::RawInstances,
            1_700_000_030_000,
            1_700_000_050_000
        )
        .is_err()
    );
    assert!(
        f.prepare(
            HttpHeaderRepresentation::RawInstances,
            1_700_000_001_000,
            1_700_000_029_999
        )
        .is_err()
    );
    assert_eq!(
        UnixSeconds::new(u64::MAX).checked_millis(),
        Err(TimeUnitError::Overflow)
    );
    assert_eq!(UnixMillis::new(1999).floor_seconds().get(), 1);
    assert!(http_proof_window(UnixMillis::new(1999), UnixMillis::new(2999)).is_err());
    let (_, expires) = http_proof_window(UnixMillis::new(1000), UnixMillis::new(2999)).unwrap();
    assert_eq!(expires.get(), 2);
    assert!(expires.checked_millis().unwrap().get() <= 2999);
    assert!(serde_json::from_str::<UnixSeconds>("123").is_err());
    assert_eq!(
        serde_json::from_str::<UnixMillis>("\"18446744073709551615\"")
            .unwrap()
            .get(),
        u64::MAX
    );
}

#[test]
fn http_pop_closed_targets_bounds_and_after_async_expiry_reject() {
    for origin in [
        "http://custody.example",
        "https://Custody.example",
        "https://user@custody.example",
        "https://custody.example/",
        "https://custody.example?x=1",
        "https://custody.example:443",
        "https://custody.example:0443",
    ] {
        assert!(CanonicalHttpTarget::new(origin, HttpPopRoute::Actions).is_err());
    }
    for id in [".", "..", "%2e%2e", "x/y", "x?y", "x\\y"] {
        assert!(
            CanonicalHttpTarget::new(
                "https://custody.example",
                HttpPopRoute::Operation(id.into())
            )
            .is_err()
        );
    }
    let mut f = Fixture::new(HttpPopProfileV1::Custody, HttpPopRoute::Actions);
    f.body = vec![b'x'; MAX_HTTP_BODY_BYTES + 1];
    assert!(
        f.prepare(
            HttpHeaderRepresentation::RawInstances,
            1_700_000_001_000,
            1_700_000_030_000
        )
        .is_err()
    );
    let mut f = Fixture::new(HttpPopProfileV1::Custody, HttpPopRoute::Actions);
    f.headers.push(OwnedHttpHeader {
        name: "host".into(),
        value: "other.example".into(),
    });
    assert!(
        f.prepare(
            HttpHeaderRepresentation::RawInstances,
            1_700_000_001_000,
            1_700_000_030_000
        )
        .is_err()
    );
    let f = Fixture::new(HttpPopProfileV1::Custody, HttpPopRoute::Actions);
    let proof = f
        .prepare(
            HttpHeaderRepresentation::RawInstances,
            1_700_000_001_000,
            1_700_000_030_000,
        )
        .unwrap();
    assert_eq!(
        verify_http_possession(proof, UnixMillis::new(1_700_000_030_000), &mut StrictPort)
            .unwrap_err(),
        HttpPopError::Time
    );
}

#[test]
fn nonce_lookup_hint_is_bounded_singleton_and_never_accepts_alternate_nonce_grammar() {
    let extract = |headers: &[OwnedHttpHeader], representation| {
        let raw = headers
            .iter()
            .map(|h| HttpHeader {
                name: &h.name,
                value: &h.value,
            })
            .collect::<Vec<_>>();
        http_pop_nonce_hint(&raw, representation)
    };
    let f = Fixture::new(
        HttpPopProfileV1::CredentialClaim,
        HttpPopRoute::EnrollmentClaim("enrollment-one".into()),
    );
    for representation in [
        HttpHeaderRepresentation::RawInstances,
        HttpHeaderRepresentation::WorkerMergedSingletons,
    ] {
        assert_eq!(
            extract(&f.headers, representation).unwrap().as_deref(),
            Some(NONCE)
        );
        let challenge = Fixture::new(
            HttpPopProfileV1::ClaimChallenge,
            HttpPopRoute::ProofChallenges,
        );
        assert_eq!(extract(&challenge.headers, representation).unwrap(), None);
        for mutation in 0..7 {
            let mut bad = f.headers.clone();
            let input = bad
                .iter_mut()
                .find(|h| h.name == "signature-input")
                .unwrap();
            input.value = match mutation {
                0 => input
                    .value
                    .replace(";alg=", &format!(";nonce=\"{NONCE}\";alg=")),
                1 => format!("{},other=(\"@method\")", input.value),
                2 => input.value.replace(NONCE, &format!("{NONCE}=")),
                3 => input.value.replace(NONCE, "invalid"),
                4 => input.value.replace("oasis7=", "other="),
                5 => input.value.replace("nonce=", "Nonce="),
                _ => "a".repeat(MAX_HTTP_HEADER_VALUE_BYTES + 1),
            };
            assert!(
                extract(&bad, representation).is_err(),
                "mutation {mutation}"
            );
        }
        let mut duplicate = f.headers.clone();
        let mut input = duplicate
            .iter()
            .find(|h| h.name == "signature-input")
            .unwrap()
            .clone();
        input.name = "SIGNATURE-INPUT".into();
        duplicate.push(input);
        assert!(extract(&duplicate, representation).is_err());
        let mut too_many = f.headers.clone();
        for n in 0..MAX_HTTP_HEADER_INSTANCES {
            too_many.push(OwnedHttpHeader {
                name: format!("x-bound-{n}"),
                value: "x".into(),
            });
        }
        assert!(extract(&too_many, representation).is_err());
        let mut total = f.headers.clone();
        for n in 0..8 {
            total.push(OwnedHttpHeader {
                name: format!("x-size-{n}"),
                value: "x".repeat(1100),
            });
        }
        assert!(extract(&total, representation).is_err());
    }
}
