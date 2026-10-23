use oasis7_command_center_core::{CommandCenter, domain::*};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::State;
use tauri_plugin_dialog::DialogExt;
use tokio::sync::Semaphore;

struct Application {
    core: CommandCenter,
    validated: bool,
    local_generation: u64,
}

pub struct Host {
    application: Arc<Mutex<Application>>,
    slots: Arc<Semaphore>,
    local_slots: Arc<Semaphore>,
    pub(crate) project: Arc<Mutex<Option<Project>>>,
}

impl Host {
    pub fn open_with_legacy_privacy(
        path: PathBuf,
        runtime_path: PathBuf,
    ) -> oasis7_command_center_core::Result<Self> {
        let host = Self::open(path)?;
        if runtime_path.exists() {
            host.application
                .lock()
                .map_err(|_| oasis7_command_center_core::Error::Invalid("应用锁不可用".into()))?
                .core
                .use_runtime_privacy_database(&runtime_path)?;
        }
        Ok(host)
    }
    pub fn open(path: PathBuf) -> oasis7_command_center_core::Result<Self> {
        let core = CommandCenter::open(path)?;
        Ok(Self {
            application: Arc::new(Mutex::new(Application {
                core,
                validated: false,
                local_generation: 0,
            })),
            slots: Arc::new(Semaphore::new(64)),
            local_slots: Arc::new(Semaphore::new(1)),
            project: Arc::new(Mutex::new(None)),
        })
    }

    async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Application) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        // Reject excess consumers rather than queueing unbounded blocking jobs.
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| "客户端正在处理请求，请稍后重试".to_string())?;
        let application = self.application.clone();
        let project = self.project.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let _permit = permit;
            let mut application = application
                .lock()
                .map_err(|_| "应用状态不可用".to_string())?;
            let result = work(&mut application);
            // Keep the current validated project identity for tool operations.
            if application.validated {
                *project.lock().map_err(|_| "项目身份不可用".to_string())? =
                    application.core.current_project();
            }
            result
        })
        .await
        .map_err(|_| "后台请求未能完成".to_string())?
    }
}

fn error(error: oasis7_command_center_core::Error) -> String {
    error.to_string()
}

#[tauri::command]
pub async fn import_sign_context(input: String) -> Result<serde_json::Value, String> {
    oasis7_command_center_core::tools::import_sign_context(&input).map_err(error)
}

async fn local_read(host: &Host, path: String, opening: bool) -> Result<Snapshot, String> {
    local_read_with(host, path, opening, CommandCenter::collect_local).await
}
async fn local_read_with(
    host: &Host,
    path: String,
    opening: bool,
    collect: impl FnOnce(&std::path::Path) -> oasis7_command_center_core::LocalRead + Send + 'static,
) -> Result<Snapshot, String> {
    let _permit =
        host.local_slots.clone().try_acquire_owned().map_err(|_| {
            "本地来源正在读取，交互与个人保存仍可使用；请待本次完成后重读".to_string()
        })?;
    let input = path.clone();
    let (expected, generation) = host
        .run(move |app| {
            app.core
                .check_local_path(std::path::Path::new(&input))
                .map_err(error)?;
            app.local_generation += 1;
            Ok((
                app.core
                    .current_project()
                    .map(|p| (p.id, p.path, p.environment)),
                app.local_generation,
            ))
        })
        .await?;
    let candidate =
        tauri::async_runtime::spawn_blocking(move || collect(std::path::Path::new(&path)))
            .await
            .map_err(|_| "本地扫描未完成，旧结果保留".to_string())?;
    host.run(move |app| {
        if app.local_generation != generation
            || app
                .core
                .current_project()
                .map(|p| (p.id, p.path, p.environment))
                != expected
        {
            return Err("本地来源候选已被新范围取代，未发布".into());
        }
        let snapshot = app.core.publish_local(candidate, opening).map_err(error)?;
        app.validated = true;
        Ok(snapshot)
    })
    .await
}
#[tauri::command]
pub async fn open_project(host: State<'_, Host>, path: String) -> Result<Snapshot, String> {
    local_read(&host, path, true).await
}
#[tauri::command]
pub async fn query_view(host: State<'_, Host>) -> Result<Snapshot, String> {
    let (validated, path) = host
        .run(|app| Ok((app.validated, app.core.current_project().map(|p| p.path))))
        .await?;
    if !validated && let Some(path) = path {
        return local_read(&host, path, false).await;
    }
    host.run(|app| {
        app.core.reload_device().map_err(error)?;
        Ok(app.core.snapshot())
    })
    .await
}

#[tauri::command]
pub async fn search_rules(
    host: State<'_, Host>,
    query: String,
    limit: usize,
) -> Result<Vec<DocumentSection>, String> {
    if query.len() > 4096 {
        return Err("搜索文字过长".into());
    }
    host.run(move |app| app.core.search(&query, limit.min(100)).map_err(error))
        .await
}

#[tauri::command]
pub async fn save_preference(
    host: State<'_, Host>,
    preferences: Preferences,
) -> Result<(), String> {
    host.run(move |app| app.core.save_preferences(preferences).map_err(error))
        .await
}

#[tauri::command]
pub async fn read_source(
    host: State<'_, Host>,
    section_id: String,
    revision: String,
    line: usize,
) -> Result<(), String> {
    host.run(move |app| {
        app.core
            .record_reading(&section_id, &revision, line)
            .map_err(error)
    })
    .await
}

#[tauri::command]
pub async fn refresh_source(host: State<'_, Host>) -> Result<Snapshot, String> {
    let path = host
        .run(|app| {
            app.core
                .current_project()
                .map(|p| p.path)
                .ok_or("请先选择项目".into())
        })
        .await?;
    local_read(&host, path, false).await
}

#[tauri::command]
pub async fn rebuild_cache(host: State<'_, Host>) -> Result<Snapshot, String> {
    let path = host
        .run(|app| {
            app.core
                .current_project()
                .map(|p| p.path)
                .ok_or("请先选择项目".into())
        })
        .await?;
    local_read(&host, path, false).await
}

#[tauri::command]
pub async fn choose_project(
    host: State<'_, Host>,
    handle: tauri::AppHandle,
) -> Result<Option<Snapshot>, String> {
    let folder = tauri::async_runtime::spawn_blocking(move || {
        handle
            .dialog()
            .file()
            .set_title("选择 Oasis7 Git 工作树")
            .blocking_pick_folder()
    })
    .await
    .map_err(|_| "项目选择器未完成".to_string())?;
    let Some(folder) = folder else {
        return Ok(None);
    };
    let path = folder
        .into_path()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .into_owned();
    local_read(&host, path, true).await.map(Some)
}

#[tauri::command]
pub async fn read_version(
    host: State<'_, Host>,
    section_id: String,
    revision: String,
) -> Result<Option<DocumentSection>, String> {
    host.run(move |app| app.core.read_version(&section_id, &revision).map_err(error))
        .await
}

/// Identity-only app navigation; no input, parameters, or execution controls.
fn validate_codex_target(target: &str) -> Result<String, String> {
    let id = target
        .strip_prefix("codex://threads/")
        .ok_or("Codex 会话定位无效")?;
    let valid = id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        });
    if !valid {
        return Err("仅支持 codex://threads/<UUID> 原会话定位".into());
    }
    Ok(target.into())
}

#[tauri::command]
pub async fn open_source_target(host: State<'_, Host>, target: String) -> Result<(), String> {
    host.run(move |app| {
        let destination = if target.starts_with("codex:") {
            validate_codex_target(&target)?
        } else if target.starts_with("https:") {
            let url = url::Url::parse(&target).map_err(|_| "来源链接无效".to_string())?;
            if url.scheme() != "https"
                || url.host_str() != Some("github.com")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.port().is_some()
            {
                return Err("只支持 GitHub HTTPS 来源链接".into());
            }
            url.to_string()
        } else {
            app.core
                .document_path(&target)
                .map_err(error)?
                .to_string_lossy()
                .into_owned()
        };
        #[cfg(target_os = "macos")]
        {
            let result = std::process::Command::new("/usr/bin/open")
                .arg("--")
                .arg(destination)
                .status()
                .map_err(|e| e.to_string())?;
            if !result.success() {
                return Err("系统未能打开来源".into());
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = destination;
            Err("来源打开目前支持 macOS".into())
        }
    })
    .await
}

#[tauri::command]
pub async fn query_document_inventory(
    host: State<'_, Host>,
    query: oasis7_command_center_core::document_inventory::DocumentInventoryQuery,
) -> Result<oasis7_command_center_core::document_inventory::DocumentInventoryPage, String> {
    host.run(move |app| app.core.document_inventory(query).map_err(error))
        .await
}

#[tauri::command]
pub async fn query_document_directory(
    host: State<'_, Host>,
    query: oasis7_command_center_core::document_directory::DocumentDirectoryQuery,
) -> Result<oasis7_command_center_core::document_directory::DocumentDirectoryPage, String> {
    host.run(move |app| app.core.document_directory(query).map_err(error))
        .await
}

#[tauri::command]
pub async fn query_document_semantic_tree(
    host: State<'_, Host>,
    query: oasis7_command_center_core::document_semantic_tree::DocumentSemanticTreeQuery,
) -> Result<oasis7_command_center_core::document_semantic_tree::DocumentSemanticTreePage, String> {
    host.run(move |app| app.core.document_semantic_tree(query).map_err(error))
        .await
}

#[tauri::command]
pub async fn load_document_tree_reading(
    host: State<'_, Host>,
    key: oasis7_command_center_core::document_tree_reading::DocumentTreeReadingKey,
) -> Result<oasis7_command_center_core::document_tree_reading::DocumentTreeReadingLoadResult, String>
{
    host.run(move |app| app.core.load_document_tree_reading(key).map_err(error))
        .await
}

#[tauri::command]
pub async fn save_document_tree_reading(
    host: State<'_, Host>,
    request: oasis7_command_center_core::document_tree_reading::DocumentTreeReadingSaveRequest,
) -> Result<oasis7_command_center_core::document_tree_reading::DocumentTreeReadingLoadResult, String>
{
    host.run(move |app| app.core.save_document_tree_reading(request).map_err(error))
        .await
}

#[tauri::command]
pub async fn reset_document_tree_reading(
    host: State<'_, Host>,
    key: oasis7_command_center_core::document_tree_reading::DocumentTreeReadingKey,
) -> Result<oasis7_command_center_core::document_tree_reading::DocumentTreeReadingLoadResult, String>
{
    host.run(move |app| app.core.reset_document_tree_reading(key).map_err(error))
        .await
}

#[tauri::command]
pub async fn load_document_tree_last_view(
    host: State<'_, Host>,
    scope: oasis7_command_center_core::document_tree_reading::DocumentTreeReadingScope,
) -> Result<oasis7_command_center_core::document_tree_reading::DocumentTreeLastView, String> {
    host.run(move |app| app.core.load_document_tree_last_view(scope).map_err(error))
        .await
}

#[cfg(test)]
mod concurrency_tests {
    use super::*;
    #[test]
    fn saturated_short_requests_reject_without_entering_application_lock() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let host = Host::open(temp.path().join("db")).unwrap();
            let permit = host.slots.clone().acquire_many_owned(64).await.unwrap();
            assert!(host.run(|_| Ok(())).await.unwrap_err().contains("稍后重试"));
            drop(permit);
            assert!(host.run(|_| Ok(())).await.is_ok());
        });
    }
    #[test]
    fn slow_local_source_does_not_lock_device_preferences_and_cancel_rejects_publication() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let host = Arc::new(Host::open(temp.path().join("db")).unwrap());
            let (started_tx, started_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let reader = host.clone();
            let path = temp.path().to_string_lossy().to_string();
            let task = tokio::spawn(async move {
                local_read_with(&reader, path, true, move |path| {
                    let _ = started_tx.send(());
                    release_rx.recv().unwrap();
                    CommandCenter::collect_local(path)
                })
                .await
            });
            started_rx.await.unwrap();
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                host.run(|app| {
                    let mut prefs = app.core.snapshot().preferences;
                    prefs.view = "rules".into();
                    app.core.save_preferences(prefs).map_err(error)?;
                    app.local_generation += 1;
                    Ok(())
                }),
            )
            .await;
            assert!(result.is_ok_and(|r| r.is_ok()));
            assert!(
                local_read_with(
                    &host,
                    temp.path().to_string_lossy().into(),
                    true,
                    CommandCenter::collect_local
                )
                .await
                .unwrap_err()
                .contains("正在读取")
            );
            release_tx.send(()).unwrap();
            assert!(task.await.unwrap().unwrap_err().contains("未发布"));
            assert_eq!(
                host.run(|app| Ok(app.core.snapshot().preferences.view))
                    .await
                    .unwrap(),
                "rules"
            );
        });
    }
}

#[cfg(test)]
mod external_codex_tests {
    use super::validate_codex_target;
    #[test]
    fn startup_preserves_retired_database_and_does_not_create_a_new_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let main = temp.path().join("main.sqlite");
        let legacy = temp.path().join("command-center-runtime.sqlite");
        drop(super::Host::open_with_legacy_privacy(main.clone(), legacy.clone()).unwrap());
        assert!(!legacy.exists());
    }
    #[test]
    fn opens_only_identity_navigation_without_parameters() {
        let target = "codex://threads/019ea241-685c-79b3-a0b0-97ac2aa9f7e2";
        assert_eq!(validate_codex_target(target).unwrap(), target);
        for suffix in ["?prompt=run", "#fragment", "/turn", "%2f", "?"] {
            assert!(validate_codex_target(&format!("{target}{suffix}")).is_err());
        }
        for invalid in [
            "codex://user@threads/019ea241-685c-79b3-a0b0-97ac2aa9f7e2",
            "codex://threads:80/019ea241-685c-79b3-a0b0-97ac2aa9f7e2",
            "Codex://threads/019ea241-685c-79b3-a0b0-97ac2aa9f7e2",
            "codex://threads/../run",
            "codex://threads/not-a-thread",
        ] {
            assert!(validate_codex_target(invalid).is_err());
        }
    }
}
