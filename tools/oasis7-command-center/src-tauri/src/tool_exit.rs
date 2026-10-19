//! Exit protection belongs to local side-effecting tools, not Codex sessions.
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
pub struct ToolExit {
    approved: AtomicBool,
    checking: AtomicBool,
}
impl ToolExit {
    pub fn approved(&self) -> bool {
        self.approved.load(Ordering::Acquire)
    }
}
pub fn request_exit(app: tauri::AppHandle) {
    if app
        .state::<ToolExit>()
        .checking
        .swap(true, Ordering::AcqRel)
    {
        return;
    }
    std::thread::spawn(move || {
        let jobs = app.state::<crate::tool_commands::ToolHost>().jobs.clone();
        let result = check_exit(&jobs);
        match result {
            Ok(()) => {
                app.state::<ToolExit>()
                    .approved
                    .store(true, Ordering::Release);
                app.exit(0);
            }
            Err(error) => {
                let _ = jobs.unfreeze();
                crate::show_main(&app);
                app.dialog()
                    .message(error.to_string())
                    .title("核对本机工具后退出")
                    .blocking_show();
                app.state::<ToolExit>()
                    .checking
                    .store(false, Ordering::Release);
            }
        }
    });
}

fn check_exit(
    jobs: &oasis7_command_center_core::tools::jobs::ToolJobs,
) -> oasis7_command_center_core::Result<()> {
    jobs.freeze().and_then(|()| {
            let pending = jobs.list()?.into_iter().filter(|job| matches!(job.state.as_str(), "running" | "unknown")).collect::<Vec<_>>();
            if pending.is_empty() { Ok(()) } else {
                Err(oasis7_command_center_core::Error::Invalid(format!(
                    "本地工具仍有未确认操作，暂未退出：\n{}\n\n请打开左侧“本机工具”，核对原操作及其结果后再退出。不会自动中断或重放操作。",
                    pending.iter().map(|job| format!("{} · {} · {}", job.kind, job.id, job.state)).collect::<Vec<_>>().join("\n")
                )))
            }
        })
}

#[cfg(test)]
mod tests {
    use super::check_exit;
    use oasis7_command_center_core::tools::jobs::ToolJobs;
    #[test]
    fn pending_tool_blocks_exit_and_frozen_gate_prevents_new_side_effects() {
        let temp = tempfile::tempdir().unwrap();
        let jobs = ToolJobs::open(temp.path().join("jobs.sqlite")).unwrap();
        jobs.admit("original", "fingerprint", "resource", "signer")
            .unwrap();
        let error = check_exit(&jobs).unwrap_err().to_string();
        assert!(error.contains("original"));
        assert!(error.contains("本机工具"));
        assert!(jobs.ensure_open().is_err());
        jobs.unfreeze().unwrap();
        assert!(jobs.ensure_open().is_ok());
        jobs.finish("original", "unknown", None).unwrap();
        assert!(check_exit(&jobs).is_err());
        jobs.finish("original", "succeeded", None).unwrap();
        assert!(check_exit(&jobs).is_ok());
        assert!(jobs.ensure_open().is_err());
    }
}
