use super::{ChainRuntimeStatus, ClientLauncherApp, OASIS7_CLIENT_LAUNCHER_SCREENSHOT_MODAL_ENV};
use eframe::egui;
use std::sync::{Mutex, OnceLock};

fn launcher_screenshot_modal_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn heavy_modal_windows_render_one_real_egui_frame() {
    let ctx = egui::Context::default();
    let mut app = ClientLauncherApp::default();
    app.config.chain_enabled = true;
    app.chain_runtime_status = ChainRuntimeStatus::Ready;
    app.transfer_window_open = true;
    app.explorer_window_open = true;
    app.peer_details_window_open = true;

    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        let ctx = ui.ctx();
        app.show_transfer_window(ctx);
        app.show_explorer_window(ctx);
        app.show_peer_details_window(ctx);
    });

    assert!(app.transfer_window_open);
    assert!(app.explorer_window_open);
    assert!(app.peer_details_window_open);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn screenshot_modal_override_seeds_without_opening_real_windows() {
    let _guard = launcher_screenshot_modal_env_lock()
        .lock()
        .expect("screenshot modal env lock");
    let previous = std::env::var(OASIS7_CLIENT_LAUNCHER_SCREENSHOT_MODAL_ENV).ok();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        std::env::set_var(OASIS7_CLIENT_LAUNCHER_SCREENSHOT_MODAL_ENV, "transfer");
    }

    let mut app = ClientLauncherApp::default();
    app.onboarding_state.open = true;

    app.apply_screenshot_modal_override();

    if let Some(previous) = previous {
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            std::env::set_var(OASIS7_CLIENT_LAUNCHER_SCREENSHOT_MODAL_ENV, previous);
        }
    } else {
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            std::env::remove_var(OASIS7_CLIENT_LAUNCHER_SCREENSHOT_MODAL_ENV);
        }
    }

    assert!(!app.onboarding_state.open);
    assert!(!app.config_window_open);
    assert!(!app.feedback_window_open);
    assert!(!app.transfer_window_open);
    assert!(!app.explorer_window_open);
    assert!(!app.peer_details_window_open);
    assert!(app.config.chain_enabled);
    assert!(matches!(
        app.chain_runtime_status,
        ChainRuntimeStatus::Ready
    ));
    assert_eq!(app.transfer_draft.amount, "12");
}
