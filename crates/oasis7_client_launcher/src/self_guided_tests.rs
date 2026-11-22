use super::*;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(target_arch = "wasm32"))]
fn unique_temp_dir(label: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "oasis7_client_launcher_{label}_{}_{}",
        std::process::id(),
        stamp
    ))
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn load_launcher_ux_state_ignores_noncanonical_state_path() {
    const NONCANONICAL_UX_STATE_PATH: &str = ".legacy_launcher_ux_state.json";
    struct CwdRestore(std::path::PathBuf);
    impl Drop for CwdRestore {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    let _guard = crate::transfer_auth::TRANSFER_AUTH_ENV_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let temp_dir = unique_temp_dir("ux_state");
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let old_cwd = std::env::current_dir().expect("current dir");
    let _cwd_restore = CwdRestore(old_cwd);

    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        std::env::set_current_dir(&temp_dir)?;

        let removed_old_brand_state = LauncherUxState {
            expert_mode: true,
            ..LauncherUxState::default()
        };
        std::fs::write(
            NONCANONICAL_UX_STATE_PATH,
            serde_json::to_vec(&removed_old_brand_state)?,
        )?;
        assert_eq!(load_launcher_ux_state(), LauncherUxState::default());

        let current_state = LauncherUxState {
            onboarding_completed: true,
            expert_mode: true,
            ..LauncherUxState::default()
        };
        std::fs::write(UX_STATE_PATH, serde_json::to_vec(&current_state)?)?;
        assert_eq!(load_launcher_ux_state(), current_state);
        Ok(())
    })();

    drop(_cwd_restore);
    std::fs::remove_dir_all(&temp_dir).ok();
    result.expect("ux state checks should succeed");
}
