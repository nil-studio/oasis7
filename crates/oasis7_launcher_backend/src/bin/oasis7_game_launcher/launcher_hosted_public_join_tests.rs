use std::path::Path;

use super::{
    BUILTIN_LLM_DECISION_SOURCE, CliOptions, DEFAULT_INTERACTIVE_LLM_TIMEOUT_MS, DEFAULT_SCENARIO,
    LLM_TIMEOUT_MS_ENV, build_oasis7_viewer_live_command, parse_options,
};

#[test]
fn build_viewer_live_command_keeps_explicit_chain_status_bind_for_hosted_public_join() {
    if !crate::hosted_test_env::with_issuer(None) {
        return;
    }
    let options = parse_options(
        [
            "--deployment-mode",
            "hosted_public_join",
            "--chain-execution-world-dir",
            "output/test-observer-root",
            "--chain-status-bind",
            "39.104.204.172:6631",
            "--chain-link-policy",
            "enforcing",
        ]
        .into_iter(),
    )
    .expect("hosted public join should parse");
    assert!(!options.chain_enabled);
    assert!(options.chain_status_bind_explicit);

    let command = build_oasis7_viewer_live_command(Path::new("/bin/echo"), &options, false, false);
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    assert!(args.contains(&"--deployment-mode".to_string()));
    assert!(args.contains(&"hosted_public_join".to_string()));
    assert!(args.contains(&"--chain-status-bind".to_string()));
    assert!(args.contains(&"39.104.204.172:6631".to_string()));
    assert!(args.contains(&"--chain-link-policy".to_string()));
    assert!(args.contains(&"enforcing".to_string()));
}

#[test]
fn build_viewer_live_command_omits_chain_status_bind_when_hosted_chain_is_disabled() {
    if !crate::hosted_test_env::with_issuer(None) {
        return;
    }
    let options = parse_options(["--deployment-mode", "hosted_public_join"].into_iter())
        .expect("hosted public join should parse");
    assert!(!options.chain_enabled);
    assert!(!options.chain_status_bind_explicit);

    let command = build_oasis7_viewer_live_command(Path::new("/bin/echo"), &options, false, false);
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    assert!(!args.contains(&"--chain-status-bind".to_string()));
    assert!(!args.contains(&"--chain-link-policy".to_string()));
}

#[test]
fn build_viewer_live_command_derives_trusted_registration_issuer_key() {
    if !crate::hosted_test_env::with_issuer(Some(81)) {
        return;
    }
    let options = parse_options(["--deployment-mode", "hosted_public_join"].into_iter())
        .expect("hosted public join should parse");

    let command = build_oasis7_viewer_live_command(Path::new("/bin/echo"), &options, false, false);
    let private_key_env = command
        .get_envs()
        .find(|(name, _)| {
            *name == oasis7_client_api::viewer_auth::HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY_ENV
        })
        .map(|(_, value)| value.map(|value| value.to_os_string()));
    let trusted_public_key = command
        .get_envs()
        .find_map(|(name, value)| {
            (name == oasis7_client_api::viewer_auth::HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV)
                .then(|| value.map(|value| value.to_string_lossy().into_owned()))
                .flatten()
        })
        .expect("trusted issuer public key env");

    assert_eq!(
        private_key_env,
        Some(None),
        "the hosted viewer child must explicitly remove the issuer private key"
    );
    assert_eq!(
        trusted_public_key,
        hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&[81_u8; 32])
                .verifying_key()
                .to_bytes()
        )
    );
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_viewer_live_command_wires_llm_timeout_default_into_spawn_path() {
    if !crate::hosted_test_env::with_issuer(None) {
        return;
    }
    let mut options = CliOptions::default();
    options.agent_decision_source = BUILTIN_LLM_DECISION_SOURCE.to_string();
    let command = build_oasis7_viewer_live_command(Path::new("/bin/echo"), &options, false, false);
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    assert!(args.contains(&"--llm".to_string()));
    assert!(args.contains(&"--chain-status-bind".to_string()));
    assert!(args.contains(&options.chain_status_bind));
    assert!(args.contains(&"--chain-link-policy".to_string()));
    assert!(args.contains(&options.chain_link_policy));
    assert!(!args.iter().any(|arg| arg.is_empty()));
    assert!(!args.iter().any(|arg| arg == DEFAULT_SCENARIO));
    let timeout_env = command
        .get_envs()
        .find(|(key, _)| key.to_string_lossy() == LLM_TIMEOUT_MS_ENV)
        .map(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()));
    assert_eq!(timeout_env, Some(Some("30000".to_string())));
    assert_eq!(DEFAULT_INTERACTIVE_LLM_TIMEOUT_MS, 30_000);
}
