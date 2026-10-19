use std::path::Path;

const DEFAULT_VIEWER_PLAYER_ID: &str = "viewer-player";
const VIEWER_PLAYER_ID_ENV: &str = "OASIS7_VIEWER_PLAYER_ID";
const VIEWER_AUTH_PUBLIC_KEY_ENV: &str = "OASIS7_VIEWER_AUTH_PUBLIC_KEY";
const VIEWER_AUTH_PRIVATE_KEY_ENV: &str = "OASIS7_VIEWER_AUTH_PRIVATE_KEY";
const VIEWER_AUTH_BOOTSTRAP_OBJECT: &str = "__OASIS7_VIEWER_AUTH_ENV";
const NODE_CONFIG_FILE_NAME: &str = "config.toml";
const NODE_TABLE_KEY: &str = "node";
const NODE_PRIVATE_KEY_FIELD: &str = "private_key";
const NODE_PUBLIC_KEY_FIELD: &str = "public_key";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ViewerAuthBootstrap {
    player_id: String,
    public_key: String,
    private_key: String,
}

pub(super) fn resolve_optional_viewer_auth_bootstrap() -> Option<ViewerAuthBootstrap> {
    resolve_from_env().or_else(|| resolve_from_path(Path::new(NODE_CONFIG_FILE_NAME)).ok())
}

pub(super) fn inject_viewer_auth_bootstrap_if_html(
    body: &[u8],
    content_type: &str,
    auth: Option<&ViewerAuthBootstrap>,
) -> Vec<u8> {
    if !content_type.starts_with("text/html") {
        return body.to_vec();
    }
    let Some(auth) = auth else {
        return body.to_vec();
    };
    let html = String::from_utf8_lossy(body);
    let script = build_viewer_auth_bootstrap_script(auth);
    let insert_at = html
        .rfind("</head>")
        .or_else(|| html.rfind("</body>"))
        .unwrap_or(html.len());
    let mut injected = String::with_capacity(html.len() + script.len() + 1);
    injected.push_str(&html[..insert_at]);
    injected.push_str(script.as_str());
    injected.push_str(&html[insert_at..]);
    injected.into_bytes()
}

fn resolve_from_env() -> Option<ViewerAuthBootstrap> {
    let public_key = std::env::var(VIEWER_AUTH_PUBLIC_KEY_ENV)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty());
    let private_key = std::env::var(VIEWER_AUTH_PRIVATE_KEY_ENV)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty());
    match (public_key, private_key) {
        (Some(public_key), Some(private_key)) => Some(ViewerAuthBootstrap {
            player_id: resolve_viewer_player_id_override(std::env::var(VIEWER_PLAYER_ID_ENV).ok()),
            public_key,
            private_key,
        }),
        _ => None,
    }
}

fn resolve_from_path(path: &Path) -> Result<ViewerAuthBootstrap, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|err| format!("read {} failed: {err}", path.display()))?;
    let value: toml::Value = toml::from_str(content.as_str())
        .map_err(|err| format!("parse {} failed: {err}", path.display()))?;
    let node = value
        .get(NODE_TABLE_KEY)
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("{NODE_TABLE_KEY} table is missing in {}", path.display()))?;
    Ok(ViewerAuthBootstrap {
        player_id: resolve_viewer_player_id_override(std::env::var(VIEWER_PLAYER_ID_ENV).ok()),
        public_key: resolve_required_toml_string(node, NODE_PUBLIC_KEY_FIELD, "node.public_key")?,
        private_key: resolve_required_toml_string(
            node,
            NODE_PRIVATE_KEY_FIELD,
            "node.private_key",
        )?,
    })
}

fn resolve_viewer_player_id_override(value: Option<String>) -> String {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_VIEWER_PLAYER_ID.to_string())
}

fn resolve_required_toml_string(
    table: &toml::value::Table,
    key: &str,
    label: &str,
) -> Result<String, String> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("{label} is missing or empty"))
}

fn build_viewer_auth_bootstrap_script(auth: &ViewerAuthBootstrap) -> String {
    let payload = serde_json::json!({
        VIEWER_PLAYER_ID_ENV: auth.player_id,
        VIEWER_AUTH_PUBLIC_KEY_ENV: auth.public_key,
        VIEWER_AUTH_PRIVATE_KEY_ENV: auth.private_key,
    });
    let payload = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    format!(
        "<script>const __oasis7ViewerAuthEnv=Object.freeze({payload});window.{VIEWER_AUTH_BOOTSTRAP_OBJECT}=__oasis7ViewerAuthEnv;</script>"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        VIEWER_AUTH_PRIVATE_KEY_ENV, VIEWER_AUTH_PUBLIC_KEY_ENV, ViewerAuthBootstrap,
        inject_viewer_auth_bootstrap_if_html, resolve_optional_viewer_auth_bootstrap,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn viewer_auth_test_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct ViewerAuthEnvRestore {
        public_key: Option<String>,
        private_key: Option<String>,
    }

    impl ViewerAuthEnvRestore {
        fn clear() -> Self {
            let restore = Self {
                public_key: std::env::var(VIEWER_AUTH_PUBLIC_KEY_ENV).ok(),
                private_key: std::env::var(VIEWER_AUTH_PRIVATE_KEY_ENV).ok(),
            };
            // SAFETY: This test/setup code mutates process environment in a controlled scope.
            unsafe {
                std::env::remove_var(VIEWER_AUTH_PUBLIC_KEY_ENV);
            }
            // SAFETY: This test/setup code mutates process environment in a controlled scope.
            unsafe {
                std::env::remove_var(VIEWER_AUTH_PRIVATE_KEY_ENV);
            }
            restore
        }
    }

    impl Drop for ViewerAuthEnvRestore {
        fn drop(&mut self) {
            restore_env_var(VIEWER_AUTH_PUBLIC_KEY_ENV, self.public_key.as_deref());
            restore_env_var(VIEWER_AUTH_PRIVATE_KEY_ENV, self.private_key.as_deref());
        }
    }

    fn restore_env_var(name: &str, value: Option<&str>) {
        match value {
            Some(value) => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    std::env::set_var(name, value);
                }
            }
            None => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    std::env::remove_var(name);
                }
            }
        }
    }

    fn temp_config_path(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        path.push(format!(
            "oasis7_web_launcher_viewer_auth_{label}_{}_{}.toml",
            std::process::id(),
            stamp
        ));
        path
    }

    #[test]
    fn inject_viewer_auth_bootstrap_if_html_inserts_script() {
        let html = inject_viewer_auth_bootstrap_if_html(
            b"<html><head></head><body>ok</body></html>",
            "text/html; charset=utf-8",
            Some(&ViewerAuthBootstrap {
                player_id: "viewer-player".to_string(),
                public_key: "pub".to_string(),
                private_key: "priv".to_string(),
            }),
        );
        let html = String::from_utf8(html).expect("utf8");
        assert!(html.contains("__OASIS7_VIEWER_AUTH_ENV"));
        assert!(html.contains("OASIS7_VIEWER_AUTH_PUBLIC_KEY"));
        assert!(html.contains("OASIS7_VIEWER_AUTH_PRIVATE_KEY"));
    }

    #[test]
    fn resolve_optional_viewer_auth_bootstrap_prefers_env_keys() {
        let _guard = viewer_auth_test_env_lock().lock().expect("env lock");
        let _restore = ViewerAuthEnvRestore::clear();
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            std::env::set_var(VIEWER_AUTH_PUBLIC_KEY_ENV, "env-public");
        }
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            std::env::set_var(VIEWER_AUTH_PRIVATE_KEY_ENV, "env-private");
        }
        let auth = resolve_optional_viewer_auth_bootstrap().expect("auth");
        assert_eq!(auth.public_key, "env-public");
        assert_eq!(auth.private_key, "env-private");
    }

    #[test]
    fn resolve_optional_viewer_auth_bootstrap_uses_config_file() {
        let _guard = viewer_auth_test_env_lock().lock().expect("env lock");
        let _restore = ViewerAuthEnvRestore::clear();
        let temp_root = temp_config_path("config");
        fs::create_dir_all(&temp_root).expect("mkdir");
        let config_path = temp_root.join("config.toml");
        fs::write(
            &config_path,
            "[node]\nprivate_key = \"private-key-hex\"\npublic_key = \"public-key-hex\"\n",
        )
        .expect("write config");
        let old_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(config_path.parent().expect("parent")).expect("chdir");
        let auth = resolve_optional_viewer_auth_bootstrap().expect("auth");
        assert_eq!(auth.public_key, "public-key-hex");
        assert_eq!(auth.private_key, "private-key-hex");
        std::env::set_current_dir(old_cwd).expect("restore cwd");
        let _ = fs::remove_dir_all(temp_root);
    }
}
