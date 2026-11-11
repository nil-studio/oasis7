//! Phase-local original VM output retention, in actual completion order.
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_wasm_abi::ModuleOutput;
pub(super) struct CapturedRuleOutputV1 {
    pub module_id: String,
    pub trace_id: String,
    pub output: ReservedValue<ModuleOutput>,
    _identity: MemoryReservation,
}
struct PendingIdentity {
    module_id: String,
    trace_id: String,
    fee: MemoryReservation,
}
pub(in crate::world) struct RuleOutputCaptureV1 {
    slots: Vec<Option<CapturedRuleOutputV1>>,
    pending: Vec<Option<PendingIdentity>>,
    max: usize,
    memory: MemoryBudget,
    _metadata: MemoryReservation,
}
impl RuleOutputCaptureV1 {
    pub fn new(memory: &MemoryBudget, max: usize) -> Result<Self, ValueTreeError> {
        let bytes = max
            .checked_mul(
                std::mem::size_of::<Option<CapturedRuleOutputV1>>()
                    + std::mem::size_of::<Option<PendingIdentity>>(),
            )
            .ok_or(ValueTreeError::ResourceLimited("rule capture slots"))?;
        let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
        let mut slots = Vec::new();
        let mut pending = Vec::new();
        slots
            .try_reserve_exact(max)
            .map_err(|_| ValueTreeError::ResourceLimited("rule capture allocation"))?;
        pending
            .try_reserve_exact(max)
            .map_err(|_| ValueTreeError::ResourceLimited("rule capture allocation"))?;
        if slots.capacity() > max || pending.capacity() > max {
            return Err(ValueTreeError::ResourceLimited("rule capture capacity"));
        }
        Ok(Self {
            slots,
            pending,
            max,
            memory: memory.clone(),
            _metadata: fee,
        })
    }
    /// Called immediately after successful VM return, before any output effects.
    pub fn begin(&mut self, module_id: &str, trace_id: &str) -> Result<usize, ValueTreeError> {
        if self.slots.len() == self.max {
            return Err(ValueTreeError::ResourceLimited("rule capture slots"));
        }
        let bytes = module_id
            .len()
            .checked_add(trace_id.len())
            .ok_or(ValueTreeError::ResourceLimited("rule capture identity"))?;
        let fee = self.memory.reserve(bytes, MemoryClass::Ordinary)?;
        let at = self.slots.len();
        self.slots.push(None);
        self.pending.push(Some(PendingIdentity {
            module_id: module_id.into(),
            trace_id: trace_id.into(),
            fee,
        }));
        Ok(at)
    }
    pub fn finish(
        &mut self,
        index: usize,
        output: ReservedValue<ModuleOutput>,
    ) -> Result<(), ValueTreeError> {
        if !output.uses_budget(&self.memory) {
            return Err(ValueTreeError::Invalid("rule capture output pool"));
        }
        let identity = self
            .pending
            .get_mut(index)
            .and_then(Option::take)
            .ok_or(ValueTreeError::Invalid("rule capture occurrence"))?;
        self.slots[index] = Some(CapturedRuleOutputV1 {
            module_id: identity.module_id,
            trace_id: identity.trace_id,
            output,
            _identity: identity.fee,
        });
        Ok(())
    }
    pub fn outputs(&self) -> Result<impl Iterator<Item = &CapturedRuleOutputV1>, ValueTreeError> {
        if self.slots.iter().any(Option::is_none) {
            return Err(ValueTreeError::Invalid("rule capture unfinished VM output"));
        }
        Ok(self.slots.iter().filter_map(Option::as_ref))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_keeps_main_before_the_policy_completed_during_output_application() {
        let memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let _fixture = memory.reserve(256, MemoryClass::Ordinary).unwrap();
        let mut capture = RuleOutputCaptureV1::new(&memory, 2).unwrap();
        let output = || {
            memory
                .reserve(std::mem::size_of::<ModuleOutput>(), MemoryClass::Ordinary)
                .unwrap()
                .retain(ModuleOutput {
                    new_state: None,
                    effects: vec![],
                    emits: vec![],
                    tick_lifecycle: None,
                    output_bytes: 0,
                })
        };
        let main = capture.begin("main", "action-1-main").unwrap();
        let policy = capture.begin("policy", "action-1-main").unwrap();
        capture.finish(policy, output()).unwrap();
        assert!(capture.outputs().is_err());
        capture.finish(main, output()).unwrap();
        let modules: Vec<_> = capture
            .outputs()
            .unwrap()
            .map(|row| row.module_id.as_str())
            .collect();
        assert_eq!(modules, ["main", "policy"]);
    }
}
