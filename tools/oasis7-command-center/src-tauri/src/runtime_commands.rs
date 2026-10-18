mod lifecycle;
mod private_input;
pub use private_input::*;

use crate::commands::Host;
use oasis7_command_center_core::{
    codex::{Runtime, SessionConfig, dto::*},
    domain::Project,
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, State};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct RuntimeHost {
    runtime: Arc<Mutex<Option<Runtime>>>,
    jobs: Arc<oasis7_command_center_core::tools::jobs::ToolJobs>,
    failure: Arc<Option<String>>,
    slots: Arc<Semaphore>,
    approved_exit: Arc<AtomicBool>,
    exit_checking: Arc<AtomicBool>,
    waiting: Arc<AtomicBool>,
    watching: Arc<AtomicBool>,
}

fn with_tool_status(
    mut status: QuitStatus,
    jobs: &oasis7_command_center_core::tools::jobs::ToolJobs,
) -> Result<QuitStatus, String> {
    let operations = jobs.list().map_err(|e| e.to_string())?;
    let active = operations
        .iter()
        .filter(|job| job.state == "running")
        .count();
    let unknown = operations
        .iter()
        .filter(|job| job.state == "unknown")
        .count();
    status.active += active;
    status.unknown += unknown;
    if active + unknown > 0 {
        status.safe = false;
        status.reason = format!(
            "本机工具仍有 {active} 项执行中、{unknown} 项结果未知；{}",
            status.reason
        );
    }
    Ok(status)
}

fn exit_ready(status: &QuitStatus) -> bool {
    status.safe && status.active == 0 && status.unknown == 0 && status.pending_requests == 0
}

impl RuntimeHost {
    pub(crate) async fn try_read<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Runtime) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let runtime = self.runtime.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut guard = runtime
                .try_lock()
                .map_err(|_| "运行控制正在处理，普通投影保留其他来源并稍后回读".to_string())?;
            let runtime = guard.as_mut().ok_or("运行来源不可用".to_string())?;
            work(runtime)
        })
        .await
        .map_err(|_| "运行只读查询未完成".to_string())?
    }
    #[cfg(test)]
    pub fn open(path: PathBuf) -> oasis7_command_center_core::Result<Self> {
        Self::open_with_jobs(
            path,
            Arc::new(oasis7_command_center_core::tools::jobs::ToolJobs::open(
                ":memory:",
            )?),
        )
    }

    pub fn open_with_jobs(
        path: PathBuf,
        jobs: Arc<oasis7_command_center_core::tools::jobs::ToolJobs>,
    ) -> oasis7_command_center_core::Result<Self> {
        let (runtime, failure) = match Runtime::open(&path) {
            Ok(runtime) => (Some(runtime), None),
            Err(error) => (
                None,
                Some(format!(
                    "执行数据库不可用：{error}。历史自管工作的终止尚未确认。请保留并备份 {}，修复数据库后重开；不要删除记录或重发未知操作。",
                    path.display()
                )),
            ),
        };
        Ok(Self {
            runtime: Arc::new(Mutex::new(runtime)),
            jobs,
            failure: Arc::new(failure),
            slots: Arc::new(Semaphore::new(4)),
            approved_exit: Arc::new(AtomicBool::new(false)),
            exit_checking: Arc::new(AtomicBool::new(false)),
            waiting: Arc::new(AtomicBool::new(false)),
            watching: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(crate) async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Runtime) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| "执行请求繁忙，请稍后重试".to_string())?;
        let runtime = self.runtime.clone();
        let failure = self.failure.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let _permit = permit;
            let mut runtime = runtime.lock().map_err(|_| "执行状态不可用".to_string())?;
            let runtime = runtime.as_mut().ok_or_else(|| {
                failure
                    .as_ref()
                    .clone()
                    .unwrap_or_else(|| "执行状态不可用".into())
            })?;
            work(runtime)
        })
        .await
        .map_err(|_| "执行请求未能完成".to_string())?
    }

    pub(crate) async fn project_work(
        &self,
        host: &Host,
        work: impl FnOnce(&mut Runtime, &Project) -> oasis7_command_center_core::Result<RuntimeSnapshot>
        + Send
        + 'static,
    ) -> Result<RuntimeSnapshot, String> {
        let identity = host.project.clone();
        let expected = identity
            .lock()
            .map_err(|_| "项目身份不可用".to_string())?
            .clone()
            .ok_or("请先打开并验证项目")?;
        let approved = self.approved_exit.clone();
        self.run(move |runtime| {
            if approved.load(Ordering::Acquire) {
                return Err("客户端已确认退出".into());
            }
            let project = identity
                .lock()
                .map_err(|_| "项目身份不可用".to_string())?
                .clone()
                .ok_or("请先打开并验证项目")?;
            if project.id != expected.id
                || project.path != expected.path
                || project.environment != expected.environment
                || project.head != expected.head
            {
                return Err("项目上下文已变化，请刷新后重试".into());
            }
            work(runtime, &project).map_err(|e| e.to_string())
        })
        .await
    }

    fn check_exit(&self) -> Result<QuitStatus, String> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| "执行状态不可用".to_string())?;
        self.jobs.freeze().map_err(|e| e.to_string())?;
        let status = (|| {
            let r = runtime
                .as_mut()
                .ok_or_else(|| self.unavailable_quit_status().reason)?;
            let status =
                with_tool_status(r.shutdown_if_idle().map_err(|e| e.to_string())?, &self.jobs)?;
            if exit_ready(&status) {
                self.approved_exit.store(true, Ordering::Release);
            }
            Ok(status)
        })();
        if status.is_err() && !self.approved_exit.load(Ordering::Acquire) {
            self.jobs.unfreeze().map_err(|e| e.to_string())?;
        }
        status
    }

    fn unavailable_quit_status(&self) -> QuitStatus {
        QuitStatus {
            safe: false,
            active: 0,
            unknown: 1,
            pending_requests: 0,
            waiting: false,
            reason: self
                .failure
                .as_ref()
                .clone()
                .unwrap_or_else(|| "执行状态不可用".into()),
        }
    }

    // Serialize cancellation with the watcher that can approve process exit.
    // A missing runtime is an unknown outcome, but must not disable ordinary tools.
    fn cancel_wait(&self) -> Result<QuitStatus, String> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| "执行状态不可用".to_string())?;
        if self.approved_exit.load(Ordering::Acquire) {
            return Err("客户端已确认退出".into());
        }
        if self.exit_checking.load(Ordering::Acquire) {
            return Err("正在核对退出状态，请稍后取消".into());
        }
        self.jobs.unfreeze().map_err(|e| e.to_string())?;
        self.waiting.store(false, Ordering::Release);
        let status = runtime
            .as_mut()
            .map(|r| r.cancel_exit_wait())
            .unwrap_or_else(|| self.unavailable_quit_status());
        with_tool_status(status, &self.jobs)
    }

    async fn interrupt_turn(
        &self,
        host: &Host,
        target: RuntimeTarget,
    ) -> Result<RuntimeSnapshot, String> {
        let jobs = self.jobs.clone();
        self.project_work(host, move |runtime, project| {
            jobs.with_open(|| runtime.interrupt_turn(project, target))
        })
        .await
    }

    async fn answer_request(
        &self,
        host: &Host,
        reply: RequestAnswer,
    ) -> Result<RuntimeSnapshot, String> {
        let jobs = self.jobs.clone();
        self.project_work(host, move |runtime, project| {
            jobs.with_open(|| runtime.answer_request(project, reply))
        })
        .await
    }
}

fn invalidate(app: &tauri::AppHandle) {
    let _ = app.emit_to("main", "command-center:runtime-invalidated", ());
}

#[tauri::command]
pub async fn get_runtime(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
) -> Result<RuntimeSnapshot, String> {
    runtime
        .project_work(&host, |runtime, project| runtime.snapshot(project))
        .await
}

#[tauri::command]
pub async fn reconcile_runtime_operation(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
    operation_id: String,
) -> Result<RuntimeSnapshot, String> {
    let result = runtime
        .project_work(&host, move |r, p| r.reconcile_operation(p, &operation_id))
        .await;
    invalidate(&handle);
    result
}

#[tauri::command]
pub async fn set_runtime_enabled(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
    enabled: bool,
) -> Result<RuntimeSnapshot, String> {
    let result = runtime
        .project_work(&host, move |r, p| r.set_enabled(p, enabled))
        .await;
    invalidate(&handle);
    result
}

#[tauri::command]
pub async fn connect_runtime(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
    binary_path: Option<String>,
) -> Result<RuntimeSnapshot, String> {
    let path = binary_path
        .filter(|path| !path.trim().is_empty())
        .ok_or("请选择 Codex 0.137.0 原生程序的绝对路径；npm/Node 启动脚本不适用于 Finder")?;
    if !std::path::Path::new(&path).is_absolute() {
        return Err("请选择 Codex 0.137.0 原生程序的绝对路径".into());
    }
    let jobs = runtime.jobs.clone();
    let result = runtime
        .project_work(&host, move |r, p| jobs.with_open(|| r.connect(p, &path)))
        .await;
    invalidate(&handle);
    result
}

#[tauri::command]
pub async fn create_runtime_session(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
    mut config: SessionConfig,
) -> Result<RuntimeSnapshot, String> {
    let jobs = runtime.jobs.clone();
    let result = runtime
        .project_work(&host, move |r, p| {
            config.basis_revision = p.head.clone();
            jobs.with_open(|| r.create_session(p, config))
        })
        .await;
    invalidate(&handle);
    result
}

#[tauri::command]
pub async fn start_work(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
    intent_id: String,
    config: SessionConfig,
    input: String,
) -> Result<RuntimeSnapshot, String> {
    let jobs = runtime.jobs.clone();
    let result = runtime
        .project_work(&host, move |r, p| {
            jobs.with_open(|| r.start_work(p, &intent_id, config, &input))
        })
        .await;
    invalidate(&handle);
    result
}

#[tauri::command]
pub async fn query_start_work_records(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
) -> Result<Vec<oasis7_command_center_core::codex::StartWorkRecord>, String> {
    let project = host
        .project
        .lock()
        .map_err(|_| "项目身份不可用".to_string())?
        .clone()
        .ok_or("请先选择项目")?;
    runtime
        .try_read(move |r| {
            r.public_start_work_records(&project)
                .map_err(|e| e.to_string())
        })
        .await
}

#[tauri::command]
pub async fn get_public_runtime(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
) -> Result<RuntimeSnapshot, String> {
    let project = host
        .project
        .lock()
        .map_err(|_| "项目身份不可用".to_string())?
        .clone()
        .ok_or("请先验证项目".to_string())?;
    runtime
        .try_read(move |r| r.public_snapshot(&project).map_err(|e| e.to_string()))
        .await
}

#[tauri::command]
pub async fn continue_runtime_session(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
    thread_id: String,
) -> Result<RuntimeSnapshot, String> {
    let jobs = runtime.jobs.clone();
    let result = runtime
        .project_work(&host, move |r, p| {
            jobs.with_open(|| r.continue_session(p, &thread_id))
        })
        .await;
    invalidate(&handle);
    result
}

#[tauri::command]
pub async fn start_runtime_turn(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
    thread_id: String,
    input: String,
) -> Result<RuntimeSnapshot, String> {
    let jobs = runtime.jobs.clone();
    let result = runtime
        .project_work(&host, move |r, p| {
            jobs.with_open(|| r.start_turn(p, &thread_id, &input))
        })
        .await;
    invalidate(&handle);
    result
}

#[tauri::command]
pub async fn steer_runtime_turn(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
    target: RuntimeTarget,
    input: String,
) -> Result<RuntimeSnapshot, String> {
    let jobs = runtime.jobs.clone();
    let result = runtime
        .project_work(&host, move |r, p| {
            jobs.with_open(|| r.steer_turn(p, target, &input))
        })
        .await;
    invalidate(&handle);
    result
}

#[tauri::command]
pub async fn interrupt_runtime_turn(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
    target: RuntimeTarget,
) -> Result<RuntimeSnapshot, String> {
    let result = runtime.interrupt_turn(&host, target).await;
    invalidate(&handle);
    result
}

#[tauri::command]
pub async fn answer_runtime_request(
    host: State<'_, Host>,
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
    reply: RequestAnswer,
) -> Result<RuntimeSnapshot, String> {
    let result = runtime.answer_request(&host, reply).await;
    invalidate(&handle);
    result
}

#[tauri::command]
pub async fn get_exit_status(runtime: State<'_, RuntimeHost>) -> Result<QuitStatus, String> {
    if runtime.failure.is_some() {
        return with_tool_status(runtime.unavailable_quit_status(), &runtime.jobs);
    }
    let jobs = runtime.jobs.clone();
    runtime
        .run(move |r| with_tool_status(r.quit_status(), &jobs))
        .await
}

#[tauri::command]
pub async fn request_exit_wait(
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
) -> Result<QuitStatus, String> {
    let jobs = runtime.jobs.clone();
    let checking = runtime.exit_checking.clone();
    let approved = runtime.approved_exit.clone();
    let waiting = runtime.waiting.clone();
    let status = runtime
        .run(move |r| {
            if checking.load(Ordering::Acquire) || approved.load(Ordering::Acquire) {
                return Err("正在核对或已确认退出".into());
            }
            jobs.freeze().map_err(|e| e.to_string())?;
            waiting.store(true, Ordering::Release);
            Ok(r.begin_wait_for_exit())
        })
        .await;
    let status = match status.and_then(|status| with_tool_status(status, &runtime.jobs)) {
        Ok(status) => status,
        Err(error) => {
            runtime.cancel_wait()?;
            return Err(error);
        }
    };
    runtime.watch_exit(handle.clone());
    invalidate(&handle);
    Ok(status)
}

#[tauri::command]
pub async fn request_exit_stop(
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
) -> Result<QuitStatus, String> {
    let jobs = runtime.jobs.clone();
    let checking = runtime.exit_checking.clone();
    let approved = runtime.approved_exit.clone();
    let waiting = runtime.waiting.clone();
    let result = runtime
        .run(move |r| {
            if checking.load(Ordering::Acquire) || approved.load(Ordering::Acquire) {
                return Err("正在核对或已确认退出".into());
            }
            jobs.freeze().map_err(|e| e.to_string())?;
            waiting.store(true, Ordering::Release);
            r.begin_wait_for_exit();
            r.stop_for_exit().map_err(|e| e.to_string())
        })
        .await;
    let result = result.and_then(|status| with_tool_status(status, &runtime.jobs));
    if result.is_err() {
        runtime.cancel_wait()?;
    } else {
        runtime.watch_exit(handle.clone());
    }
    invalidate(&handle);
    result
}

#[tauri::command]
pub async fn cancel_exit_wait(
    runtime: State<'_, RuntimeHost>,
    handle: tauri::AppHandle,
) -> Result<QuitStatus, String> {
    let owner = runtime.inner().clone();
    let status = tauri::async_runtime::spawn_blocking(move || owner.cancel_wait())
        .await
        .map_err(|_| "退出取消未完成".to_string())??;
    invalidate(&handle);
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_cancel_and_exit_check_never_reopen_approved_admission() {
        for _ in 0..32 {
            let runtime = RuntimeHost::open(PathBuf::from(":memory:")).unwrap();
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let cancel_host = runtime.clone();
            let cancel_barrier = barrier.clone();
            let cancel = std::thread::spawn(move || {
                cancel_barrier.wait();
                cancel_host.cancel_wait()
            });
            barrier.wait();
            let status = runtime.check_exit().unwrap();
            let _ = cancel.join().unwrap();
            assert!(exit_ready(&status));
            assert!(runtime.exit_is_approved());
            assert!(runtime.jobs.with_open(|| Ok(())).is_err());
            assert!(runtime.cancel_wait().is_err());
        }
    }

    #[test]
    fn request_answers_are_rejected_by_exit_admission_before_runtime_work() {
        let host = Host::open(PathBuf::from(":memory:")).unwrap();
        *host.project.lock().unwrap() = Some(Project {
            id: "fixture".into(),
            path: "/fixture".into(),
            git_common_dir: "/fixture/.git".into(),
            branch: Some("fixture".into()),
            head: "fixture-head".into(),
            environment: "local".into(),
        });
        let runtime = RuntimeHost::open(PathBuf::from(":memory:")).unwrap();
        let reply = RequestAnswer {
            request_id: "original-request".into(),
            target: RuntimeTarget {
                instance_id: "original-instance".into(),
                thread_id: "original-thread".into(),
                turn_id: "original-turn".into(),
            },
            decision: Some("accept".into()),
            answers: Default::default(),
        };
        runtime.jobs.freeze().unwrap();
        let interrupt =
            tauri::async_runtime::block_on(runtime.interrupt_turn(&host, reply.target.clone()));
        assert!(
            interrupt
                .unwrap_err()
                .contains("exit has frozen new side effects")
        );
        let frozen = tauri::async_runtime::block_on(runtime.answer_request(&host, reply.clone()));
        assert!(
            frozen
                .unwrap_err()
                .contains("exit has frozen new side effects")
        );
        runtime.jobs.unfreeze().unwrap();
        let interrupt =
            tauri::async_runtime::block_on(runtime.interrupt_turn(&host, reply.target.clone()));
        assert!(
            !interrupt
                .unwrap_err()
                .contains("exit has frozen new side effects")
        );
        let reopened = tauri::async_runtime::block_on(runtime.answer_request(&host, reply));
        assert!(
            !reopened
                .unwrap_err()
                .contains("exit has frozen new side effects")
        );
    }

    #[test]
    fn quit_projection_includes_running_and_unknown_tool_operations() {
        let jobs = oasis7_command_center_core::tools::jobs::ToolJobs::open(":memory:").unwrap();
        jobs.admit("install", "hash", "tool", "install").unwrap();
        let status = QuitStatus {
            safe: true,
            active: 0,
            unknown: 0,
            pending_requests: 0,
            waiting: false,
            reason: String::new(),
        };
        let running = with_tool_status(status.clone(), &jobs).unwrap();
        assert_eq!(running.active, 1);
        assert!(!exit_ready(&running));
        jobs.finish("install", "unknown", None).unwrap();
        let unknown = with_tool_status(status.clone(), &jobs).unwrap();
        assert_eq!(unknown.unknown, 1);
        assert!(!exit_ready(&unknown));
        jobs.finish("install", "succeeded", None).unwrap();
        assert!(exit_ready(&with_tool_status(status, &jobs).unwrap()));
    }
    #[test]
    fn exit_requires_idle_known_state_and_no_outstanding_request() {
        let mut status = QuitStatus {
            safe: true,
            active: 0,
            unknown: 0,
            pending_requests: 0,
            waiting: false,
            reason: String::new(),
        };
        assert!(exit_ready(&status));
        status.active = 1;
        assert!(!exit_ready(&status));
        status.active = 0;
        status.unknown = 1;
        assert!(!exit_ready(&status));
        status.unknown = 0;
        status.pending_requests = 1;
        assert!(!exit_ready(&status));
        status.pending_requests = 0;
        status.safe = false;
        assert!(!exit_ready(&status));
    }

    #[test]
    fn runtime_dispatch_requires_a_rust_validated_project() {
        let host = Host::open(PathBuf::from(":memory:")).unwrap();
        let runtime = RuntimeHost::open(PathBuf::from(":memory:")).unwrap();
        let result = tauri::async_runtime::block_on(runtime.project_work(&host, |_, _| {
            panic!("must not call runtime without a validated project")
        }));
        assert!(result.is_err());
    }

    #[test]
    fn saturated_runtime_admission_rejects_instead_of_queueing() {
        let runtime = RuntimeHost::open(PathBuf::from(":memory:")).unwrap();
        let _permits: Vec<_> = (0..4)
            .map(|_| runtime.slots.clone().try_acquire_owned().unwrap())
            .collect();
        let result = tauri::async_runtime::block_on(
            runtime.run::<()>(|_| panic!("must not queue a fifth blocking request")),
        );
        assert!(result.is_err());
    }

    #[test]
    fn corrupted_runtime_storage_preserves_data_and_foundation_availability() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("runtime.sqlite");
        let original = b"damaged-runtime-storage-with-unresolved-history";
        std::fs::write(&path, original).unwrap();
        let runtime = RuntimeHost::open(path.clone()).unwrap();
        let foundation = Host::open(directory.path().join("foundation.sqlite")).unwrap();
        assert!(foundation.project.lock().unwrap().is_none());
        assert!(
            runtime
                .failure
                .as_ref()
                .as_ref()
                .unwrap()
                .contains("历史自管工作的终止尚未确认")
        );
        assert!(!runtime.exit_is_approved());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let result = tauri::async_runtime::block_on(
            runtime.run::<()>(|_| panic!("damaged runtime storage must reject execution")),
        );
        assert!(result.is_err());
        assert!(runtime.check_exit().is_err());
        runtime.jobs.with_open(|| Ok(())).unwrap();
        runtime.jobs.freeze().unwrap();
        runtime.waiting.store(true, Ordering::Release);
        let status = runtime.cancel_wait().unwrap();
        assert!(!status.safe);
        assert_eq!(status.unknown, 1);
        assert!(!status.waiting);
        runtime.jobs.with_open(|| Ok(())).unwrap();
        assert!(!runtime.exit_is_approved());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
