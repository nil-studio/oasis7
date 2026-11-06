#![cfg(unix)]
use std::{os::unix::fs::PermissionsExt, process::Command};
#[test]
fn operator_cli_rejects_public_directory_and_lists_without_credential_references() {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("catalog-cli-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_oasis7_game_client"))
            .args([
                "dev-test-catalog-list",
                "--directory",
                root.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    assert!(!invoke().status.success());
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let data = serde_json::json!([{"account_ref":"ref-a","account_id":"account-a","environment":"isolated","public_key":"public-key","alias":"A","credential_ref":"/private/secret-config","login_operation":"private-operation"}]);
    std::fs::write(root.join("catalog.json"), data.to_string()).unwrap();
    let result = invoke();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("account-a"));
    assert!(!output.contains("secret-config"));
    assert!(!output.contains("private-operation"));
    std::fs::remove_dir_all(root).unwrap();
}
