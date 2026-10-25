//! Fail-closed BFT hook for targets without the Unix durable-ledger substrate.
use super::NodeRuntimeExecutionDriver;
use oasis7_node::{
    BftBlockV1, BftCommitCertificateV1, NodeBftBuildContextV1, NodeBftExecutionCommitResultV1,
    NodeBftExecutionConfigV1, NodeBftExecutionContextV1, NodeBftExecutionHeadV1,
    VerifiedBftCommitCertificateV1,
};
use std::fs;
use std::path::Path;

pub(crate) struct BftExecutionState;

impl NodeRuntimeExecutionDriver {
    pub(super) fn has_bft_material(records_dir: &Path) -> Result<bool, String> {
        for path in [
            records_dir.join("bft-execution-required-v1"),
            records_dir.join("bft-execution-v1"),
        ] {
            match fs::symlink_metadata(path.as_path()) {
                Ok(_) => return Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("inspect BFT execution marker: {error}")),
            }
        }
        Ok(false)
    }

    pub(super) fn configure_bft(
        &mut self,
        _config: &NodeBftExecutionConfigV1,
    ) -> Result<Option<NodeBftExecutionHeadV1>, String> {
        Err("durable BFT execution is unsupported on this platform".into())
    }

    pub(super) fn bft_history_page(
        &self,
        _config: &NodeBftExecutionConfigV1,
        _after_height: u64,
        _max_entries: usize,
    ) -> Result<Vec<BftCommitCertificateV1>, String> {
        Err("durable BFT execution is unsupported on this platform".into())
    }

    pub(super) fn current_bft_context(
        &self,
        _scope: &oasis7_node::BftScopeV1,
        _height: u64,
        _parent_commit_ref: &str,
        _authority_epoch: u64,
    ) -> Result<NodeBftExecutionContextV1, String> {
        Err("durable BFT execution is unsupported on this platform".into())
    }

    pub(super) fn build_bft(&self, _context: NodeBftBuildContextV1) -> Result<BftBlockV1, String> {
        Err("durable BFT execution is unsupported on this platform".into())
    }

    pub(super) fn preview_bft(
        &self,
        _expected: &NodeBftExecutionContextV1,
        _candidate: &BftBlockV1,
    ) -> Result<(), String> {
        Err("durable BFT execution is unsupported on this platform".into())
    }

    pub(super) fn commit_bft(
        &mut self,
        _token: VerifiedBftCommitCertificateV1,
    ) -> Result<NodeBftExecutionCommitResultV1, String> {
        Err("durable BFT execution is unsupported on this platform".into())
    }
}
