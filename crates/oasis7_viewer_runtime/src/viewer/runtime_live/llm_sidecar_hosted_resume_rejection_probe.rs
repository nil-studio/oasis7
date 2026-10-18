//! Opt-in probes at the real sole Resume rejection persistence boundary.
use super::*;
impl RuntimeLlmSidecar {
    pub(super) fn test_resume_rejection_root(&self) -> Result<Option<std::path::PathBuf>, String> {
        let Some(root) = std::env::var_os("PRE2_RESUME_REJECTION_FS_ROOT") else {
            return Ok(None);
        };
        let root = std::path::PathBuf::from(root);
        if !root.is_absolute() || !root.is_dir() {
            return Err("test rejected Resume private directory required".into());
        }
        if root.join("resume-rejection-before-persist").exists() {
            return Ok(None);
        }
        Ok(Some(root))
    }
    pub(super) fn test_resume_rejection_state(
        &self,
        digests: [String; 5],
    ) -> Result<serde_json::Value, String> {
        let mut state = self.test_resume_handoff_state()?;
        let fields = state
            .as_object_mut()
            .ok_or("test rejected Resume snapshot invalid")?;
        fields.insert(
            "terminal_states".into(),
            serde_json::to_value(&self.provider_terminal_states)
                .map_err(|_| "test rejected Resume terminal snapshot failed")?,
        );
        fields.insert(
            "runtime_ledgers".into(),
            serde_json::to_value(digests)
                .map_err(|_| "test rejected Resume ledger snapshot failed")?,
        );
        Ok(state)
    }
}
