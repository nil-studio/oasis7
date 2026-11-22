use std::env;
use std::fs;
use std::path::Path;

use super::super::{
    DeploymentMode, VIEWER_AUTH_BOOTSTRAP_OBJECT, VIEWER_AUTH_PRIVATE_KEY_ENV,
    VIEWER_AUTH_PUBLIC_KEY_ENV, VIEWER_PLAYER_ID_ENV, ViewerAuthBootstrap,
    build_viewer_auth_bootstrap_script, resolve_viewer_auth_bootstrap_for_embedded_server,
    resolve_viewer_auth_bootstrap_from_path, sanitize_index_html_for_embedded_server,
};
use super::viewer_static_dir_tests::make_temp_dir;

fn assert_removed_old_brand_viewer_auth_env_absent(text: &str) {
    assert!(!text.contains(removed_old_brand_viewer_auth_bootstrap_object().as_str()));
    for key in removed_old_brand_viewer_auth_env_keys() {
        assert!(!text.contains(key.as_str()));
    }
}

fn removed_old_brand_viewer_auth_bootstrap_object() -> String {
    format!(
        "__{}",
        ["AGENT", "WORLD", "VIEWER", "AUTH", "ENV"].join("_")
    )
}

fn removed_old_brand_viewer_auth_env_keys() -> [String; 3] {
    [
        ["AGENT", "WORLD", "VIEWER", "PLAYER", "ID"].join("_"),
        ["AGENT", "WORLD", "VIEWER", "AUTH", "PUBLIC", "KEY"].join("_"),
        ["AGENT", "WORLD", "VIEWER", "AUTH", "PRIVATE", "KEY"].join("_"),
    ]
}

#[test]
fn sanitize_index_html_for_embedded_server_injects_viewer_auth_bootstrap() {
    let html = "<html><head></head><body><div id=\"app\"></div></body></html>";
    let auth = ViewerAuthBootstrap {
        player_id: "viewer-player".to_string(),
        public_key: "pub-hex".to_string(),
        private_key: "priv-hex".to_string(),
    };
    let sanitized = sanitize_index_html_for_embedded_server(
        Path::new("index.html"),
        html.as_bytes(),
        Some(&auth),
    );
    let sanitized = String::from_utf8(sanitized).expect("utf-8");
    assert!(sanitized.contains(VIEWER_AUTH_BOOTSTRAP_OBJECT));
    assert!(sanitized.contains(VIEWER_PLAYER_ID_ENV));
    assert!(sanitized.contains(VIEWER_AUTH_PUBLIC_KEY_ENV));
    assert!(sanitized.contains(VIEWER_AUTH_PRIVATE_KEY_ENV));
    assert_removed_old_brand_viewer_auth_env_absent(&sanitized);
    assert!(sanitized.contains("viewer-player"));
    assert!(sanitized.contains("pub-hex"));
    assert!(sanitized.contains("priv-hex"));
}

#[test]
fn sanitize_index_html_for_embedded_server_injects_viewer_auth_bootstrap_into_non_index_html() {
    let html = "<html><head></head><body><div id=\"safe\"></div></body></html>";
    let auth = ViewerAuthBootstrap {
        player_id: "viewer-player".to_string(),
        public_key: "pub-hex".to_string(),
        private_key: "priv-hex".to_string(),
    };
    let sanitized = sanitize_index_html_for_embedded_server(
        Path::new("software_safe.html"),
        html.as_bytes(),
        Some(&auth),
    );
    let sanitized = String::from_utf8(sanitized).expect("utf-8");
    assert!(sanitized.contains(VIEWER_AUTH_BOOTSTRAP_OBJECT));
    assert_removed_old_brand_viewer_auth_env_absent(&sanitized);
    assert!(sanitized.contains("viewer-player"));
    assert!(sanitized.contains("pub-hex"));
    assert!(sanitized.contains("priv-hex"));
}

#[test]
fn build_viewer_auth_bootstrap_script_contains_expected_window_object() {
    let auth = ViewerAuthBootstrap {
        player_id: "viewer-player".to_string(),
        public_key: "public".to_string(),
        private_key: "private".to_string(),
    };
    let script = build_viewer_auth_bootstrap_script(&auth);
    assert!(script.contains("window."));
    assert!(script.contains(VIEWER_AUTH_BOOTSTRAP_OBJECT));
    assert!(script.contains(VIEWER_PLAYER_ID_ENV));
    assert!(script.contains(VIEWER_AUTH_PUBLIC_KEY_ENV));
    assert!(script.contains(VIEWER_AUTH_PRIVATE_KEY_ENV));
    assert_removed_old_brand_viewer_auth_env_absent(&script);
}

#[test]
fn resolve_viewer_auth_bootstrap_from_path_reads_node_keypair() {
    let temp_dir = make_temp_dir("viewer_auth_bootstrap");
    let config_path = temp_dir.join("config.toml");
    fs::write(
        &config_path,
        "[node]\nprivate_key = \"private-key-hex\"\npublic_key = \"public-key-hex\"\n",
    )
    .expect("write config");

    let auth =
        resolve_viewer_auth_bootstrap_from_path(config_path.as_path(), None).expect("resolve auth");
    assert_eq!(auth.public_key, "public-key-hex");
    assert_eq!(auth.private_key, "private-key-hex");
    assert!(!auth.player_id.trim().is_empty());
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn resolve_viewer_auth_bootstrap_from_path_uses_chain_node_id_fallback() {
    let temp_dir = make_temp_dir("viewer_auth_bootstrap_chain_player");
    let config_path = temp_dir.join("config.toml");
    fs::write(
        &config_path,
        "[node]\nprivate_key = \"private-key-hex\"\npublic_key = \"public-key-hex\"\n",
    )
    .expect("write config");

    let auth = resolve_viewer_auth_bootstrap_from_path(config_path.as_path(), Some("chain-a"))
        .expect("resolve auth");
    assert_eq!(auth.player_id, "chain-a");
    assert_eq!(auth.public_key, "public-key-hex");
    assert_eq!(auth.private_key, "private-key-hex");
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn hosted_public_join_disables_viewer_auth_bootstrap_resolution() {
    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    struct CwdRestore(std::path::PathBuf);
    impl Drop for CwdRestore {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.0);
        }
    }

    let _cwd_guard = CWD_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let temp_dir = make_temp_dir("hosted_public_join_no_bootstrap");
    let config_path = temp_dir.join("config.toml");
    fs::write(
        &config_path,
        "[node]\nprivate_key = \"private-key-hex\"\npublic_key = \"public-key-hex\"\n",
    )
    .expect("write config");

    let old_cwd = env::current_dir().expect("cwd");
    let _cwd_restore = CwdRestore(old_cwd);
    env::set_current_dir(&temp_dir).expect("chdir");
    let auth = resolve_viewer_auth_bootstrap_for_embedded_server(
        DeploymentMode::HostedPublicJoin,
        Some("chain-a"),
    );
    drop(_cwd_restore);

    assert!(auth.is_none());
    let _ = fs::remove_dir_all(temp_dir);
}
