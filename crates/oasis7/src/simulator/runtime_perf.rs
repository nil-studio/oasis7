//! Backward-compatible exports of the deterministic performance sample aggregator.
//!
//! Wall-clock sampling remains the responsibility of the host AgentRunner. The
//! L2 simulator receives only the caller-supplied Duration samples and derives
//! reproducible windows, percentiles and health from those input sequences.
pub use oasis7_simulator::{
    RuntimePerfBottleneck, RuntimePerfHealth, RuntimePerfSeriesSnapshot, RuntimePerfSnapshot,
    unsupported_runtime_perf_snapshot,
};

#[cfg(target_arch = "wasm32")]
pub use oasis7_simulator::RuntimePerfCollector;
