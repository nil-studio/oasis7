//! Private opt-in observation at the genuine final Consume sole persistence boundary.
use super::*;
impl RuntimeLlmSidecar {
    pub(super) fn test_final_budget_root(&self) -> Result<Option<std::path::PathBuf>, String> {
        let Some(root) = std::env::var_os("PRE2_FINAL_BUDGET_FS_ROOT") else {
            return Ok(None);
        };
        let root = std::path::PathBuf::from(root);
        if !root.is_absolute() || !root.is_dir() {
            return Err("test final budget private directory required".into());
        }
        if root.join("final-budget-before-persist").exists() {
            return Ok(None);
        }
        Ok(Some(root))
    }
    pub(super) fn test_final_budget_state(
        &self,
        digests: [String; 5],
    ) -> Result<serde_json::Value, String> {
        let mut state = self.test_resume_handoff_state()?;
        let fields = state
            .as_object_mut()
            .ok_or("test final budget snapshot invalid")?;
        fields.insert(
            "terminal_states".into(),
            serde_json::to_value(&self.provider_terminal_states).map_err(|e| e.to_string())?,
        );
        fields.insert(
            "runtime_ledgers".into(),
            serde_json::to_value(digests).map_err(|e| e.to_string())?,
        );
        Ok(state)
    }
}
