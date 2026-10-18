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
    local_generation: u64,
}

pub struct Host {
    application: Arc<Mutex<Application>>,
    slots: Arc<Semaphore>,
    github_slots: Arc<Semaphore>,
    local_slots: Arc<Semaphore>,
    projection_slots: Arc<Semaphore>,
    pub(crate) project: Arc<Mutex<Option<Project>>>,
}

impl Host {
    pub fn open_with_runtime_privacy(
        path: PathBuf,
        runtime_path: PathBuf,
    ) -> oasis7_command_center_core::Result<Self> {
        let host = Self::open(path)?;
        host.application
            .lock()
            .map_err(|_| oasis7_command_center_core::Error::Invalid("应用锁不可用".into()))?
            .core
            .use_runtime_privacy_database(&runtime_path)?;
        Ok(host)
    }
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
                local_generation: 0,
            })),
            slots: Arc::new(Semaphore::new(64)),
            github_slots: Arc::new(Semaphore::new(1)),
            local_slots: Arc::new(Semaphore::new(1)),
            projection_slots: Arc::new(Semaphore::new(2)),
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
pub async fn import_sign_context(input: String) -> Result<serde_json::Value, String> {
    oasis7_command_center_core::tools::import_sign_context(&input).map_err(error)
}
#[tauri::command]
pub async fn cancel_source_reads(host: State<'_, Host>) -> Result<(), String> {
    host.run(|app| {
        app.local_generation += 1;
        app.github_generation += 1;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn query_daily_work(
    host: State<'_, Host>,
) -> Result<oasis7_command_center_core::daily_work::DailyWorkState, String> {
    host.run(|app| app.core.public_daily_work().map_err(error))
        .await
}
#[tauri::command]
pub async fn save_daily_work(
    host: State<'_, Host>,
    request: oasis7_command_center_core::daily_work::DailyWorkRequest,
) -> Result<oasis7_command_center_core::daily_work::DailyWorkState, String> {
    host.run(move |app| {
        app.core.save_daily_work(request).map_err(error)?;
        app.core.public_daily_work().map_err(error)
    })
    .await
}

#[tauri::command]
pub async fn add_worktree_reference(
    host: State<'_, Host>,
    path: String,
    expected_revision: u64,
) -> Result<oasis7_command_center_core::daily_work::DailyWorkState, String> {
    let _permit = host
        .local_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| "本地来源正在读取，工作树引用未保存；请待本次完成后重试".to_string())?;
    let input = path.clone();
    let expected = host
        .run(move |app| {
            app.core
                .check_local_path(std::path::Path::new(&input))
                .map_err(error)?;
            app.core
                .current_project()
                .map(|p| p.id)
                .ok_or("请先选择项目".into())
        })
        .await?;
    let reference = tauri::async_runtime::spawn_blocking(move || {
        CommandCenter::inspect_worktree(std::path::Path::new(&path))
    })
    .await
    .map_err(|_| "工作树身份读取未完成".to_string())?
    .map_err(error)?;
    host.run(move |app| {
        if app.core.current_project().map(|p| p.id).as_ref() != Some(&expected) {
            return Err("项目已切换，未保存引用".into());
        }
        app.core
            .remember_worktree(reference, expected_revision)
            .map_err(error)?;
        app.core.public_daily_work().map_err(error)
    })
    .await
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
        let previous = app.core.current_project().map(|p| (p.id, p.path));
        let snapshot = app.core.publish_local(candidate, opening).map_err(error)?;
        app.validated = true;
        if previous
            != snapshot
                .project
                .as_ref()
                .map(|p| (p.id.clone(), p.path.clone()))
        {
            app.github_generation += 1;
            app.github.reset();
            if let Some(cache) = app.core.load_github_cache().map_err(error)? {
                app.github = GitHubClient::restore(cache);
            }
        }
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
        app.core.reload_personal().map_err(error)?;
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
pub async fn replace_decision(
    host: State<'_, Host>,
    draft: DecisionDraft,
    previous_id: String,
) -> Result<DecisionRecord, String> {
    host.run(move |app| app.core.replace_decision(draft, previous_id).map_err(error))
        .await
}
#[tauri::command]
pub async fn query_decision_history(
    host: State<'_, Host>,
    id: String,
) -> Result<Vec<DecisionRecord>, String> {
    host.run(move |app| app.core.decision_history(&id).map_err(error))
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
pub async fn read_github_window(
    host: State<'_, Host>,
    gh_path: Option<String>,
    page: Option<u32>,
    number: Option<u64>,
) -> Result<GitHubSnapshot, String> {
    if page.is_some() == number.is_some() {
        return Err("请选择一次明确分页或一个已知 PR".into());
    }
    let _permit = host
        .github_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| "GitHub 正在读取，请稍后继续".to_string())?;
    let (project, generation, previous) = host
        .run(|app| {
            let p = app.core.current_project().ok_or("请先选择项目")?;
            app.github_generation += 1;
            Ok((p, app.github_generation, app.github.snapshot()))
        })
        .await?;
    let root = project.path.clone();
    let candidate = tauri::async_runtime::spawn_blocking(move || {
        let mut client = GitHubClient::restore(previous);
        if let Some(page) = page {
            client.append_page(std::path::Path::new(&root), gh_path.as_deref(), page)
        } else {
            client.read_known_pull(
                std::path::Path::new(&root),
                gh_path.as_deref(),
                number.unwrap(),
            )
        }
    })
    .await
    .map_err(|_| "GitHub 后台读取未完成".to_string())?
    .map_err(|e| oasis7_command_center_core::github::public_error(&e))?;
    host.run(move |app| {
        let current = app.core.current_project().ok_or("项目已切换")?;
        if generation != app.github_generation
            || current.id != project.id
            || current.path != project.path
            || current.environment != project.environment
        {
            return Err("读取已被原范围变化取代，未覆盖当前列表".into());
        }
        app.core.save_github_cache(&candidate).map_err(error)?;
        app.github = GitHubClient::from_observation(candidate.clone());
        Ok(candidate)
    })
    .await
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
    let _permit = host
        .projection_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| "工作投影读取繁忙；保留最近结果，请稍后重读".to_string())?;
    let (base, references) = host
        .run(|app| {
            Ok((
                app.core.snapshot(),
                app.core
                    .daily_work()
                    .map(|s| s.worktrees)
                    .unwrap_or_default(),
            ))
        })
        .await?;
    let expected = base
        .project
        .as_ref()
        .map(|p| (p.id.clone(), p.path.clone(), p.environment.clone()));
    let current = base.project.clone().ok_or("请先选择项目")?;
    let observed = runtime
        .try_read(move |runtime| runtime.public_snapshot(&current).map_err(|e| e.to_string()))
        .await;
    let mut additional = Vec::new();
    for reference in references {
        if base
            .project
            .as_ref()
            .is_some_and(|p| p.id == reference.project_id)
        {
            continue;
        }
        let path = reference.path.clone();
        let candidate = tauri::async_runtime::spawn_blocking(move || {
            CommandCenter::inspect_worktree(std::path::Path::new(&path))
        })
        .await;
        let allowed = match candidate {
            Ok(Ok(project))
                if project.id == reference.project_id
                    && base
                        .project
                        .as_ref()
                        .is_some_and(|p| p.git_common_dir == project.git_common_dir) =>
            {
                Some(project)
            }
            _ => None,
        };
        if let Some(project) = allowed {
            let check = project.clone();
            let allowed = host
                .run(move |app| {
                    app.core
                        .check_local_path(std::path::Path::new(&check.path))
                        .map_err(error)
                })
                .await
                .is_ok();
            if allowed {
                let target = project.clone();
                if let Ok(source) = runtime
                    .try_read(move |r| r.public_snapshot(&target).map_err(|e| e.to_string()))
                    .await
                {
                    let mut snapshot = base.clone();
                    snapshot.project = Some(project);
                    snapshot.sections.clear();
                    snapshot.associations.clear();
                    snapshot.decisions.clear();
                    if let Ok(mut extra) = oasis7_command_center_core::work_projection::project_work(
                        &snapshot,
                        &GitHubClient::new().snapshot(),
                        Some(&source),
                        &oasis7_command_center_core::now(),
                    ) {
                        extra.sources.retain(|s| s.kind == "codex");
                        for s in &mut extra.sources {
                            s.coverage = format!("显式工作树 {} · {}", reference.path, s.coverage);
                        }
                        additional.push(extra);
                    }
                    continue;
                }
            }
        }
        additional.push(
            oasis7_command_center_core::work_projection::WorkProjection {
                view_revision: None,
                personal_revision: None,
                schema_version: 1,
                project_id: reference.project_id,
                environment: reference.environment,
                read_at: oasis7_command_center_core::now(),
                items: vec![],
                sources: vec![oasis7_command_center_core::work_projection::WorkSource {
                    kind: "codex".into(),
                    state: "error".into(),
                    coverage: format!(
                        "显式工作树 {} 未能核实，原身份保留；不能推断没有会话",
                        reference.path
                    ),
                    verified_at: Some(reference.verified_at),
                    error: Some("工作树不可读、身份变化或运行来源未接入".into()),
                }],
            },
        );
    }
    host.run(move |app| {
        if app
            .core
            .current_project()
            .map(|p| (p.id, p.path, p.environment))
            != expected
        {
            return Err("工作范围已变化，未发布旧投影".into());
        }
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
        let permitted = app
            .core
            .daily_work()
            .map(|s| s.worktrees)
            .unwrap_or_default();
        for extra in additional {
            if permitted
                .iter()
                .any(|r| r.project_id == extra.project_id && r.environment == extra.environment)
            {
                projection.items.extend(extra.items);
                projection.sources.extend(extra.sources);
            }
        }
        let personal = app.core.public_daily_work();
        if personal.is_err() {
            projection
                .sources
                .push(oasis7_command_center_core::work_projection::WorkSource {
                    kind: "personal".into(),
                    state: "error".into(),
                    coverage:
                        "个人记录未能按当前版本安全组合；来源事实独立保留，未推断没有关系或决定"
                            .into(),
                    verified_at: None,
                    error: Some("请在个人记录入口核对版本或存储错误；未覆盖原记录".into()),
                });
        }
        for external in personal.as_ref().ok().into_iter().flat_map(|p| &p.external) {
            if projection
                .items
                .iter()
                .any(|w| w.reference == external.work)
            {
                continue;
            }
            let evidence = oasis7_command_center_core::work_projection::SourceFact {
                source: external.work.clone(),
                revision: None,
                verified_at: None,
                event_at: None,
                read_at: projection.read_at.clone(),
                coverage: "用户明确保存的普通原工具定位；源会话、消息和结果均未读取".into(),
            };
            projection
                .items
                .push(oasis7_command_center_core::work_projection::WorkItem {
                    reference: external.work.clone(),
                    purpose: oasis7_command_center_core::work_projection::FactCell {
                        value: external.title.clone(),
                        evidence: evidence.clone(),
                    },
                    result: oasis7_command_center_core::work_projection::FactCell {
                        value: "原工具结果未知 · 只有定位记录".into(),
                        evidence,
                    },
                    remaining: "复制原定位，在原工具处理并核实；客户端没有外部控制权".into(),
                    source_url: None,
                    goals: vec![],
                    checks: vec![],
                    checks_source: None,
                    runs: vec![],
                });
        }
        projection.personal_revision = personal.as_ref().ok().map(|p| p.revision);
        // The revision binds this exact published source window and personal record version.
        let mut facts = serde_json::to_value((
            &projection.project_id,
            &projection.environment,
            &snapshot.git,
            &snapshot.sections,
            &projection.sources,
            &projection.items,
            projection.personal_revision,
        ))
        .map_err(|e| e.to_string())?;
        fn without_read_time(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(fields) => {
                    fields.remove("readAt");
                    for item in fields.values_mut() {
                        without_read_time(item);
                    }
                }
                serde_json::Value::Array(items) => {
                    for item in items {
                        without_read_time(item);
                    }
                }
                _ => {}
            }
        }
        without_read_time(&mut facts);
        use sha2::{Digest, Sha256};
        projection.view_revision = Some(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&facts).map_err(|e| e.to_string())?)
        ));
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
    fn slow_local_source_does_not_lock_personal_writes_and_cancel_rejects_publication() {
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
