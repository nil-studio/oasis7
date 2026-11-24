//! Compatibility facade for the canonical deterministic memory selector.
//!
//! The shipping LLM prompt path keeps its public API while executing the
//! single Simulator owner. The original algorithm and five regression tests
//! live in oasis7_simulator::agent_memory_selector.
pub use oasis7_simulator::agent_memory_selector::{MemorySelector, MemorySelectorConfig};

#[cfg(test)]
mod tests {
    use super::{MemorySelector, MemorySelectorConfig};
    use oasis7_simulator::AgentMemory;
    use oasis7_simulator::agent_memory_selector as canonical;

    #[test]
    fn legacy_memory_selector_is_the_canonical_owner() {
        let config: canonical::MemorySelectorConfig = MemorySelectorConfig::default();
        let memory = AgentMemory::with_capacities(8, 8);
        let result: canonical::MemorySelectorResult = MemorySelector::select(&memory, 0, &config);
        assert_eq!(result, canonical::MemorySelector::select(&memory, 0, &config));
    }
}
