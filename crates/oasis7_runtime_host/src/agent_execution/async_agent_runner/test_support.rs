use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use super::{AgentBehavior, AgentDecision, Observation};

pub(super) struct BuiltinWaitBehavior {
    agent_id: String,
}

impl BuiltinWaitBehavior {
    pub(super) fn new(agent_id: String) -> Self {
        Self { agent_id }
    }
}

impl AgentBehavior for BuiltinWaitBehavior {
    fn agent_id(&self) -> &str {
        &self.agent_id
    }

    fn decide(&mut self, _observation: &Observation) -> AgentDecision {
        AgentDecision::Wait
    }
}

pub(super) struct BlockingProviderBehavior {
    agent_id: String,
    cancelled: Arc<AtomicBool>,
}

impl BlockingProviderBehavior {
    pub(super) fn new(agent_id: String, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            agent_id,
            cancelled,
        }
    }
}

impl AgentBehavior for BlockingProviderBehavior {
    fn agent_id(&self) -> &str {
        &self.agent_id
    }

    fn decide(&mut self, _observation: &Observation) -> AgentDecision {
        while !self.cancelled.load(Ordering::Acquire) {
            thread::sleep(std::time::Duration::from_millis(1));
        }
        AgentDecision::Wait
    }
}

impl Drop for BlockingProviderBehavior {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
