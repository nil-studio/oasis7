#[test]
fn launcher_environment_mutators_are_child_scoped() {
    assert_eq!(
        oasis7_hosted_access::hosted_test_env::ISSUER,
        oasis7_client_api::viewer_auth::HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY_ENV
    );
    // Inspect source only: executing the old writers to demonstrate a race
    // would itself violate the process-environment safety precondition.
    let sources = [
        (
            "oasis7_game_launcher.rs",
            include_str!("../oasis7_game_launcher.rs"),
        ),
        (
            "hosted_player_session_tests.rs",
            include_str!("hosted_player_session_tests.rs"),
        ),
        (
            "hosted_test_env.rs",
            include_str!("../../../../oasis7_hosted_access/src/hosted_test_env.rs"),
        ),
        (
            "launcher_hosted_public_join_tests.rs",
            include_str!("launcher_hosted_public_join_tests.rs"),
        ),
        (
            "launcher_static_http_tests.rs",
            include_str!("launcher_static_http_tests.rs"),
        ),
        (
            "launcher_visibility_policy_tests.rs",
            include_str!("launcher_visibility_policy_tests.rs"),
        ),
        (
            "oasis7_game_launcher_tests.rs",
            include_str!("oasis7_game_launcher_tests.rs"),
        ),
        (
            "provider_lineage_tests.rs",
            include_str!("provider_lineage_tests.rs"),
        ),
        (
            "hosted_strong_auth.rs",
            include_str!("hosted_strong_auth.rs"),
        ),
        (
            "hosted_access.rs",
            include_str!("../../../../oasis7_hosted_access/src/lib.rs"),
        ),
    ];
    // Fragments avoid matching the guard's own source literals. Whitespace
    // is ignored so splitting a qualified call across lines cannot hide it.
    let prefixes = ["std::env", "env", "oasis7_client_storage::env_mut"];
    let writers = ["set_var", "remove_var"];
    let mut violations = std::collections::BTreeSet::new();
    for (path, source) in sources {
        let mut compact = String::new();
        let mut locations = Vec::new();
        for (line, text) in source.lines().enumerate() {
            for (column, character) in text.char_indices() {
                if !character.is_whitespace() {
                    compact.push(character);
                    locations.extend(std::iter::repeat_n(
                        (line + 1, column + 1),
                        character.len_utf8(),
                    ));
                }
            }
        }
        for prefix in prefixes {
            for writer in writers {
                let pattern = format!("{prefix}::{writer}(");
                for (offset, _) in compact.match_indices(&pattern) {
                    // Do not also report the env suffix of std::env.
                    if offset > 0 {
                        let previous = compact.as_bytes()[offset - 1];
                        if previous == b':' || previous == b'_' || previous.is_ascii_alphanumeric()
                        {
                            continue;
                        }
                    }
                    let (line, column) = locations[offset];
                    violations.insert(format!("{path}:{line}:{column}: {prefix}::{writer}"));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "launcher scenario environment must be configured on child Commands; process-global writers:\n{}",
        violations.into_iter().collect::<Vec<_>>().join("\n")
    );
}
