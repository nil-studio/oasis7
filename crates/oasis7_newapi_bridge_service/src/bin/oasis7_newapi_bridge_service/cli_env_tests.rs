use super::*;

#[test]
fn parse_cli_options_accepts_letai_platform_key_env() {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    struct EnvRestore {
        key: &'static str,
        previous: Option<String>,
    }
    impl Drop for EnvRestore {
        fn drop(&mut self) {
            // SAFETY: serialized by ENV_LOCK across tests in this binary.
            unsafe {
                match &self.previous {
                    Some(value) => env::set_var(self.key, value),
                    None => env::remove_var(self.key),
                }
            }
        }
    }

    const ENV_NAME: &str = "OASIS7_TEST_NEWAPI_BRIDGE_PLATFORM_KEY_ENV";
    let _lock = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let _restore = EnvRestore {
        key: ENV_NAME,
        previous: env::var(ENV_NAME).ok(),
    };
    // SAFETY: serialized by ENV_LOCK across tests in this binary.
    unsafe {
        env::set_var(ENV_NAME, "platform-key-from-env");
    }
    let options = parse_cli_options(vec![
        "--letai-platform-key-env".to_string(),
        ENV_NAME.to_string(),
    ])
    .expect("parse env platform key");
    assert_eq!(
        options.letai_platform_key.as_deref(),
        Some("platform-key-from-env")
    );
}
