//! Native L4 owner for original archive-bearing Simulator hosts.
//! This is not the L2 pure kernel: Runtime events, manifests, sidecars and journal matter.
pub mod archive;
pub mod kernel_host;
pub mod persist;
#[cfg(test)]
mod tests;
