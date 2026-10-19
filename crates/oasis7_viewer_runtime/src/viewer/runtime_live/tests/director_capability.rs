use crate::viewer::runtime_live::session_policy::RuntimeSessionPolicy;

fn signer(seed: u8) -> (String, String) {
    let private_key = [seed; 32];
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&private_key);
    (
        hex::encode(signing_key.verifying_key().to_bytes()),
        hex::encode(private_key),
    )
}

#[test]
fn director_capability_is_one_shot_and_session_epoch_bound() {
    let (player_public_key, _) = signer(21);
    let (signer_public_key, signer_private_key) = signer(22);
    let mut policy = RuntimeSessionPolicy::default();
    let first_epoch = policy
        .register_session("player-director", player_public_key.as_str())
        .expect("register player session");
    let grant = crate::viewer::sign_director_capability_grant(
        "player-director",
        player_public_key.as_str(),
        "viewer-live-1",
        first_epoch,
        "director-nonce-once",
        1_000,
        2_000,
        signer_public_key.as_str(),
        signer_private_key.as_str(),
    )
    .expect("sign director grant");

    policy
        .validate_and_consume_director_capability_grant(
            &grant,
            "viewer-live-1",
            signer_public_key.as_str(),
            1_500,
        )
        .expect("first grant use");
    let replay = policy
        .validate_and_consume_director_capability_grant(
            &grant,
            "viewer-live-1",
            signer_public_key.as_str(),
            1_500,
        )
        .expect_err("replayed Director grant must fail closed");
    assert!(replay.contains("nonce replay"));

    policy
        .revoke_session("player-director", None)
        .expect("revoke player session");
    let revoked = policy
        .validate_and_consume_director_capability_grant(
            &grant,
            "viewer-live-1",
            signer_public_key.as_str(),
            1_500,
        )
        .expect_err("revoked session must reject Director grant");
    assert!(revoked.contains("session_revoked"));
}
