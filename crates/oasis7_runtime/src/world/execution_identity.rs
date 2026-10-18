use super::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldExecutionStateIdentity {
    pub state_root_hash: String,
    pub logical_tick: u64,
}
impl World {
    /// Inspect this World's semantic state root and tick from one immutable view.
    /// Callers must separately establish committed proof and authority.
    pub fn execution_state_identity(&self) -> Result<WorldExecutionStateIdentity, WorldError> {
        Ok(WorldExecutionStateIdentity { state_root_hash: self.current_state_root_hash()?, logical_tick: self.state.time })
    }
}
