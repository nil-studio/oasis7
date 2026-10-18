use serde::{Deserialize, Serialize};

use super::GameplayModuleKind;

pub const GAMEPLAY_BASELINE_KINDS: [GameplayModuleKind; 5] = [
    GameplayModuleKind::War,
    GameplayModuleKind::Governance,
    GameplayModuleKind::Crisis,
    GameplayModuleKind::Economic,
    GameplayModuleKind::Meta,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveGameplayModule {
    pub module_id: String,
    pub version: String,
    pub kind: GameplayModuleKind,
    pub game_modes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameplayKindCoverage {
    pub kind: GameplayModuleKind,
    pub active_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameplayModeReadiness {
    pub mode: String,
    #[serde(default)]
    pub active_modules: Vec<ActiveGameplayModule>,
    #[serde(default)]
    pub coverage: Vec<GameplayKindCoverage>,
    #[serde(default)]
    pub missing_kinds: Vec<GameplayModuleKind>,
}

impl GameplayModeReadiness {
    pub fn is_ready(&self) -> bool {
        self.missing_kinds.is_empty()
    }
}
