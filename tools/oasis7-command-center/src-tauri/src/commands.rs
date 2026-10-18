use oasis7_command_center_core::{
    CommandCenter,
    domain::*,
    github::{GitHubClient, GitHubSnapshot},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::State;
use tauri_plugin_dialog::DialogExt;
use tokio::sync::Semaphore;

struct Application {
    core: CommandCenter,
    github: GitHubClient,
    validated: bool,
    github_generation: u64,
}

pub struct Host {
    application: Arc<Mutex<Application>>,
    slots: Arc<Semaphore>,
    github_slots: Arc<Semaphore>,
    pub(crate) project: Arc<Mutex<Option<Project>>>,
}

impl Host {
    pub fn open(path: PathBuf) -> oasis7_command_center_core::Result<Self> {
        let core = CommandCenter::open(path)?;
        let github = core
            .load_github_cache()?
            .map(GitHubClient::restore)
            .unwrap_or_else(GitHubClient::new);
        Ok(Self {
            application: Arc::new(Mutex::new(Application {
                core,
                github,
                validated: false,
                github_generation: 0,
            })),
            slots: Arc::new(Semaphore::new(4)),
            github_slots: Arc::new(Semaphore::new(1)),
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
            // Keep a fast Rust-owned identity mirror for runtime operations. It
            // never requires the indexing/database mutex on the protocol reader.
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
pub async fn open_project(host: State<'_, Host>, path: String) -> Result<Snapshot, String> {
    let identity = host.project.clone();
    host.run(move |app| {
        *identity.lock().map_err(|_| "项目身份不可用".to_string())? = None;
        let previous = app.core.snapshot().project.map(|p| (p.id, p.path));
        let result = app.core.open_project(path).map_err(error);
        if result.is_ok() {
            app.validated = true;
        }
        let current = app.core.snapshot().project.map(|p| (p.id, p.path));
        if previous != current {
            app.github_generation += 1;
            app.github.reset();
            if let Some(snapshot) = app.core.load_github_cache().map_err(error)? {
                app.github = GitHubClient::restore(snapshot);
            }
        }
        result
    })
    .await
}

#[tauri::command]
pub async fn query_view(host: State<'_, Host>) -> Result<Snapshot, String> {
    host.run(|app| {
        // Restored paths are revalidated on a worker before the first projection.
        // Source failures are retained by Core; actual storage failures propagate.
        if !app.validated {
            if app.core.snapshot().project.is_some() {
                app.core.refresh().map_err(error)?;
            }
            app.validated = true;
        }
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
pub async fn save_decision_draft(
    host: State<'_, Host>,
    draft: DecisionDraft,
) -> Result<DecisionRecord, String> {
    host.run(move |app| app.core.save_decision(draft).map_err(error))
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
    host.run(|app| app.core.refresh().map_err(error)).await
}

#[tauri::command]
pub async fn rebuild_cache(host: State<'_, Host>) -> Result<Snapshot, String> {
    host.run(|app| app.core.rebuild_cache().map_err(error))
        .await
}

#[tauri::command]
pub async fn refresh_github(
    host: State<'_, Host>,
    gh_path: Option<String>,
) -> Result<GitHubSnapshot, String> {
    let _source_permit = host
        .github_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| "GitHub 来源正在读取，请待本次读取完成后重试".to_string())?;
    let (project, generation, previous) = host
        .run(|app| {
            let project = app.core.snapshot().project.ok_or("请先选择项目")?;
            app.github_generation += 1;
            Ok((project, app.github_generation, app.github.snapshot()))
        })
        .await?;
    let root = project.path.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let mut candidate = GitHubClient::restore(previous);
        candidate.refresh(std::path::Path::new(&root), gh_path.as_deref())
    })
    .await
    .map_err(|_| "GitHub 后台读取未能完成".to_string())?;
    host.run(move |app| {
        let current = app
            .core
            .snapshot()
            .project
            .ok_or("项目上下文已变化，请刷新后重试")?;
        if generation != app.github_generation
            || current.id != project.id
            || current.path != project.path
            || current.environment != project.environment
        {
            return Err("GitHub 来源读取已被新请求或项目切换取代，请重新读取当前来源".into());
        }
        app.core.save_github_cache(&snapshot).map_err(error)?;
        app.github = GitHubClient::from_observation(snapshot.clone());
        Ok(snapshot)
    })
    .await
}

#[tauri::command]
pub async fn choose_project(
    host: State<'_, Host>,
    handle: tauri::AppHandle,
) -> Result<Option<Snapshot>, String> {
    let identity = host.project.clone();
    host.run(move |app| {
        let Some(folder) = handle
            .dialog()
            .file()
            .set_title("选择 Oasis7 Git 工作树")
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let path = folder.into_path().map_err(|e| e.to_string())?;
        *identity.lock().map_err(|_| "项目身份不可用".to_string())? = None;
        let previous = app.core.snapshot().project.map(|p| (p.id, p.path));
        let result = app.core.open_project(path).map_err(error);
        if result.is_ok() {
            app.validated = true;
        }
        let current = app.core.snapshot().project.map(|p| (p.id, p.path));
        if previous != current {
            app.github_generation += 1;
            app.github.reset();
            if let Some(snapshot) = app.core.load_github_cache().map_err(error)? {
                app.github = GitHubClient::restore(snapshot);
            }
        }
        result.map(Some)
    })
    .await
}

#[tauri::command]
pub async fn associate_work(
    host: State<'_, Host>,
    work_url: String,
    goal_section_ids: Vec<String>,
) -> Result<WorkAssociation, String> {
    host.run(move |app| {
        app.core
            .associate_work(&work_url, goal_section_ids)
            .map_err(error)
    })
    .await
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

#[tauri::command]
pub async fn get_github_snapshot(host: State<'_, Host>) -> Result<GitHubSnapshot, String> {
    host.run(|app| Ok(app.github.snapshot())).await
}

#[tauri::command]
pub async fn open_source_target(host: State<'_, Host>, target: String) -> Result<(), String> {
    host.run(move |app| {
        let destination = if target.starts_with("https:") {
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

#[tauri::command]
pub async fn query_work_projection(
    host: State<'_, Host>,
    runtime: State<'_, crate::runtime_commands::RuntimeHost>,
) -> Result<oasis7_command_center_core::work_projection::WorkProjection, String> {
    let observed = runtime
        .project_work(&host, |runtime, project| runtime.public_snapshot(project))
        .await;
    host.run(move |app| {
        let snapshot = app.core.snapshot();
        let mut projection = oasis7_command_center_core::work_projection::project_work(
            &snapshot,
            &app.github.snapshot(),
            observed.as_ref().ok(),
            &oasis7_command_center_core::now(),
        )
        .map_err(error)?;
        if observed.is_err() {
            let source = &mut projection.sources[1];
            source.state = "error".into();
            source.error =
                Some("会话来源读取失败；其他工作来源仍可使用，不能据此推断没有会话。".into());
        }
        Ok(projection)
    })
    .await
}

#[tauri::command]
pub async fn refresh_work_checks(
    host: State<'_, Host>,
    number: String,
    gh_path: Option<String>,
) -> Result<GitHubSnapshot, String> {
    let _source_permit = host
        .github_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| "GitHub 来源正在读取，请稍后重试".to_string())?;
    let (project, generation, repository, pull) = host
        .run(move |app| {
            let project = app.core.snapshot().project.ok_or("请先选择项目")?;
            let snapshot = app.github.snapshot();
            let repository = snapshot.repository.ok_or("请先读取 PR 来源")?;
            let pull = snapshot
                .pulls
                .into_iter()
                .find(|pull| pull.id == number)
                .ok_or("原 PR 不在已读取范围，请先刷新列表")?;
            Ok((project, app.github_generation, repository, pull))
        })
        .await?;
    let root = project.path.clone();
    let repo = repository.clone();
    let head = pull.head_oid.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        oasis7_command_center_core::github::read_pull_checks(
            std::path::Path::new(&root),
            gh_path.as_deref(),
            &repo,
            &head,
        )
    })
    .await
    .map_err(|_| "CI 后台读取未能完成".to_string())?;
    host.run(move |app| {
        let current = app.core.snapshot().project.ok_or("项目上下文已变化")?;
        let mut snapshot = app.github.snapshot();
        if generation != app.github_generation
            || current.id != project.id
            || current.path != project.path
            || current.environment != project.environment
            || snapshot.repository.as_deref() != Some(&repository)
        {
            return Err("CI 来源已变化，请刷新后重试".into());
        }
        let target = snapshot
            .pulls
            .iter_mut()
            .find(|target| target.id == pull.id && target.head_oid == pull.head_oid)
            .ok_or("PR HEAD 已变化，请重新读取检查")?;
        let source = match result {
            Ok((checks, coverage)) => {
                target.checks = checks;
                oasis7_command_center_core::github::PullChecksSource {
                    schema_version: 1,
                    state: if target.checks.is_empty() {
                        "empty"
                    } else {
                        "partial"
                    }
                    .into(),
                    observed_at: Some(oasis7_command_center_core::now()),
                    coverage,
                    error: None,
                }
            }
            Err(error) => oasis7_command_center_core::github::PullChecksSource {
                schema_version: 1,
                state: "error".into(),
                observed_at: target
                    .checks_source
                    .as_ref()
                    .and_then(|source| source.observed_at.clone()),
                coverage: "本次读取失败；保留该 HEAD 最近已读取检查，不推导完整成功。".into(),
                error: Some(oasis7_command_center_core::github::public_error(&error)),
            },
        };
        target.checks_source = Some(source);
        app.core.save_github_cache(&snapshot).map_err(error)?;
        app.github = GitHubClient::from_observation(snapshot.clone());
        Ok(snapshot)
    })
    .await
}
