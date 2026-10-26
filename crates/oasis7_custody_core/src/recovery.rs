//! Monotonic independent fence; it cannot restore grants or authorize recovery.
use crate::DecimalU64;
use serde::{Deserialize, Serialize};
mod activation;
pub use activation::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryFence {
    epoch: DecimalU64,
    disabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryError {
    EpochRollback,
    Reenable,
    StaleOrDisabled,
}

impl RecoveryFence {
    pub fn new(epoch: u64, disabled: bool) -> Self {
        Self {
            epoch: DecimalU64(epoch),
            disabled,
        }
    }
    pub fn epoch(&self) -> u64 {
        self.epoch.0
    }
    pub fn disabled(&self) -> bool {
        self.disabled
    }
    pub fn tighten(&mut self, epoch: u64, disabled: bool) -> Result<(), RecoveryError> {
        if epoch < self.epoch.0 {
            return Err(RecoveryError::EpochRollback);
        }
        if self.disabled && !disabled {
            return Err(RecoveryError::Reenable);
        }
        self.epoch = DecimalU64(epoch);
        self.disabled = disabled;
        Ok(())
    }
    pub fn check_epoch(&self, epoch: u64) -> Result<(), RecoveryError> {
        if !self.disabled && self.epoch.0 == epoch {
            Ok(())
        } else {
            Err(RecoveryError::StaleOrDisabled)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fence_only_tightens() {
        let mut fence = RecoveryFence::new(1, false);
        fence.tighten(2, false).unwrap();
        assert_eq!(fence.check_epoch(1), Err(RecoveryError::StaleOrDisabled));
        assert_eq!(fence.tighten(1, true), Err(RecoveryError::EpochRollback));
        assert!(!fence.disabled());
        fence.tighten(2, true).unwrap();
        assert_eq!(fence.tighten(3, false), Err(RecoveryError::Reenable));
        assert_eq!(fence.check_epoch(2), Err(RecoveryError::StaleOrDisabled));
    }
}
