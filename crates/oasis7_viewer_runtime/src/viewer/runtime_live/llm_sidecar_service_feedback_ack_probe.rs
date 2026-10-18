//! Test-tier observation/fault boundary around the actual sole durable write.
use super::*;
fn root() -> Option<std::path::PathBuf> {
    (std::env::var("PRE2_APP_ADMISSION").ok()?.as_str() == "memory-ack-write-failure")
        .then(|| std::env::var_os("PRE2_METADATA_DIR").map(std::path::PathBuf::from))
        .flatten()
}
fn write(root: &std::path::Path, name: &str, value: &serde_json::Value) -> Result<(), String> {
    let path = root.join(name);
    std::fs::write(&path, serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn wait(root: &std::path::Path, name: &str) -> Result<(), String> {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !root.join(name).exists() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    if root.join(name).exists() {
        Ok(())
    } else {
        Err(format!("actual ACK probe timed out: {name}"))
    }
}
impl RuntimeLlmSidecar {
    pub(super) fn feedback_ack_persist_probe(
        &self,
        memory: &MemoryWriteStore,
        pending: &BTreeMap<String, PendingProviderServiceIntent>,
    ) -> Result<(), String> {
        let Some(root) = root() else { return Ok(()) };
        if root.join("feedback-ack-before-persist").exists() {
            return Ok(());
        }
        let native_before: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("feedback-ack-native-before.json"))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let native_staged: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("feedback-ack-native-staged.json"))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        write(
            &root,
            "feedback-ack-before.json",
            &serde_json::json!({"memory":memory,"pending":pending,"runtime_ledgers":native_before,"model_calls":self.service_test_model_calls.values().map(|s|s.lock().unwrap().recorded_requests.len()).sum::<usize>()}),
        )?;
        write(
            &root,
            "feedback-ack-staged.json",
            &serde_json::json!({"memory":self.provider_memory_store,"pending":self.provider_service_pending,"runtime_ledgers":native_staged,"model_calls":self.service_test_model_calls.values().map(|s|s.lock().unwrap().recorded_requests.len()).sum::<usize>()}),
        )?;
        write(
            &root,
            "feedback-ack-before-persist",
            &serde_json::json!({"actual_staged_consumption":true}),
        )?;
        wait(&root, "feedback-ack-persist-release")
    }
    pub(super) fn feedback_ack_rollback_probe(&mut self, error: &str) -> Result<(), String> {
        let Some(root) = root() else { return Ok(()) };
        if !root.join("feedback-ack-before-persist").exists()
            || root.join("feedback-ack-rollback").exists()
        {
            return Ok(());
        }
        let native = self
            .runner
            .as_mut()
            .and_then(|r| r.async_runner_mut())
            .ok_or("actual rollback runner absent")?
            .rejected_wait_test_ledger_digests();
        write(
            &root,
            "feedback-ack-after.json",
            &serde_json::json!({"memory":self.provider_memory_store,"pending":self.provider_service_pending,"runtime_ledgers":native,"model_calls":self.service_test_model_calls.values().map(|s|s.lock().unwrap().recorded_requests.len()).sum::<usize>()}),
        )?;
        write(
            &root,
            "feedback-ack-rollback",
            &serde_json::json!({"actual_error":error}),
        )?;
        wait(&root, "feedback-ack-retry-release")
    }
    pub(super) fn feedback_ack_missing_runner_rejected_probe(
        &self,
        error: &str,
    ) -> Result<(), String> {
        if std::env::var("PRE2_APP_ADMISSION").ok().as_deref() != Some("memory-ack-missing-runner")
        {
            return Ok(());
        }
        let root = std::env::var_os("PRE2_METADATA_DIR")
            .map(std::path::PathBuf::from)
            .ok_or("missing runner root")?;
        write(
            &root,
            "feedback-ack-missing-runner-refused",
            &serde_json::json!({"actual_error":error,"runner_absent":self.runner.is_none(),"memory":self.provider_memory_store,"pending":self.provider_service_pending}),
        )
    }
    pub(super) fn feedback_ack_missing_runner_probe(&mut self) -> Result<(), String> {
        if std::env::var("PRE2_APP_ADMISSION").ok().as_deref() != Some("memory-ack-missing-runner")
        {
            return Ok(());
        }
        self.runner.take();
        let root = std::env::var_os("PRE2_METADATA_DIR")
            .map(std::path::PathBuf::from)
            .ok_or("missing runner actual process root absent")?;
        if !self.provider_memory_store.entries().is_empty() {
            return Err("missing runner test entered after memory accepted".into());
        }
        if self
            .provider_service_pending
            .values()
            .any(|p| p.feedback_ack.is_some())
        {
            return Err("missing runner test entered after ACK checkpoint".into());
        }
        write(
            &root,
            "feedback-ack-missing-runner-prepared",
            &serde_json::json!({"runner_absent":self.runner.is_none(),"memory":self.provider_memory_store,"pending":self.provider_service_pending}),
        )?;
        Ok(())
    }
}
