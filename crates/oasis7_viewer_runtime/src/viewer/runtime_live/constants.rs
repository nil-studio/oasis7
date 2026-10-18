use std::time::Duration;
pub(super) const AUTHORITATIVE_BATCH_CONFIRM_DELAY_TICKS: u64 = 1;
pub(super) const AUTHORITATIVE_BATCH_FINALITY_WINDOW_TICKS: u64 = 2;
pub(super) const MAX_AUTHORITATIVE_BATCH_HISTORY: usize = 256;
pub(super) const MAX_AUTHORITATIVE_CHALLENGE_HISTORY: usize = 512;
pub(super) const MAX_AUTHORITATIVE_STABLE_CHECKPOINTS: usize = 64;
pub(super) const LLM_GAMEPLAY_REQUIRED_HINT: &str =
    "enable --llm and configure a reachable LLM provider before retrying gameplay controls";
pub(super) const LLM_PROVIDER_GATEWAY_TIMEOUT_HINT: &str = "local LLM provider gateway timed out; inspect output/local-letai-game-test/*/local-letai-provider-bridge.log, confirm proxy/upstream LetAI reachability, then rerun scripts/run-local-letai-game-test.sh or its chat probe/bridge smoke before retrying gameplay controls";
pub(super) const RUNTIME_CONTROL_REQUIRED_HINT: &str = "inspect the reported runtime failure, repair the broken world/module state, then retry the control";
pub(super) const BACKGROUND_PLAY_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(2);
pub(super) const BACKGROUND_PLAY_TRANSIENT_FAILURE_BUDGET: u8 = 12;
pub(super) const BACKGROUND_PLAY_TRANSIENT_FAILURE_RETRY_DELAY: Duration = Duration::from_secs(5);
