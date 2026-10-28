use super::*;
#[test]
fn rfc9421_appendix_b26_direct_ed25519_vector() {
    // RFC 9421 B.1.4 public test key and B.2.6 signature, no private material.
    let public = URL_SAFE_NO_PAD
        .decode("JrQLj5P_89iXES9-vFgrIy29clF9CC_oPPsw3c5D0bs")
        .unwrap();
    let public = public.iter().map(|b| format!("{b:02x}")).collect();
    let signature: [u8;64]=STANDARD.decode("wqcAqbmYJ2ji2glfAMaRy4gruYYnx2nEFN2HN6jrnDnQCK1u02Gb04v9EDgwUPiu4A0w6vuQv5lIp5WPpBKRCw==").unwrap().try_into().unwrap();
    let base = concat!(
        "\"date\": Tue, 20 Apr 2021 02:07:55 GMT\n",
        "\"@method\": POST\n",
        "\"@path\": /foo\n",
        "\"@authority\": example.com\n",
        "\"content-type\": application/json\n",
        "\"content-length\": 18\n",
        "\"@signature-params\": (\"date\" \"@method\" \"@path\" \"@authority\" \"content-type\" \"content-length\");created=1618884473;keyid=\"test-key-ed25519\""
    );
    // This tests the raw crypto primitive, not acceptance of another profile's
    // covered components or metadata by prepare_http_pop.
    let mut proof = PreparedHttpPop {
        key_id: "test-key-ed25519".into(),
        public_key_hex: public,
        signature,
        base: base.as_bytes().to_vec(),
        profile: HttpPopProfileV1::Custody,
        binding_id: "not-a-grant".into(),
        request_id: "not-a-grant".into(),
        nonce: None,
        method: "POST".into(),
        target_uri: "https://example.com/foo".into(),
        body_digest: [0; 32],
        token_digest: [0; 32],
        created: UnixSeconds::new(1618884473),
        expires: UnixSeconds::new(1618884480),
        valid_until: UnixMillis::new(1618884480000),
    };
    verify_ed25519(&proof).unwrap();
    proof.base = sha256(&proof.base).to_vec();
    assert_eq!(verify_ed25519(&proof), Err(HttpPopError::Signature));
}
