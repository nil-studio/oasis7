//! Fail-closed BFT hook for targets without the Unix durable-ledger substrate.
use super::NodeRuntimeExecutionDriver;
use oasis7_consensus::tendermint_v2::{
    BftHeightProposerScheduleV2, VerifiedBftCommitCertificateV2,
};
use oasis7_node::{
    BftBlockV1, BftCommitCertificateV1, NodeBftBuildContextV1, NodeBftBuildContextV2,
    NodeBftExecutionCommitResultV1, NodeBftExecutionCommitResultV2, NodeBftExecutionConfigV1,
    NodeBftExecutionConfigV2, NodeBftExecutionContextV1, NodeBftExecutionContextV2,
    NodeBftExecutionHeadV1, NodeBftExecutionHeadV2, VerifiedBftCommitCertificateV1,
};
use oasis7_proto::distributed_bft::v2::{BftBlockV2, BftCommitCertificateV2};
use std::fs;
use std::path::Path;

pub(crate) struct BftExecutionState;
pub(crate) struct BftExecutionStateV2;

pub(super) fn has_formal_bft_material(records_dir: &Path) -> Result<bool, String> {
    for path in [
        records_dir.join("formal-bft-read-required-v1"),
        records_dir.join("formal-bft-read-v1"),
    ] {
        match fs::symlink_metadata(path.as_path()) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("inspect formal BFT read marker: {error}")),
        }
    }
    Ok(false)
}

impl NodeRuntimeExecutionDriver {
    pub(super) fn has_bft_material(records_dir: &Path) -> Result<bool, String> {
        for path in [
            records_dir.join("bft-execution-required-v1"),
            records_dir.join("bft-execution-v1"),
            records_dir.join("bft-execution-required-v2"),
            records_dir.join("bft-execution-v2"),
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

    pub(super) fn validate_bft_action_payload(&self, _payload: &[u8]) -> Result<(), String> {
        Err("BFT action payload admission is unsupported on this platform".into())
    }

    pub(super) fn commit_bft(
        &mut self,
        _token: VerifiedBftCommitCertificateV1,
    ) -> Result<NodeBftExecutionCommitResultV1, String> {
        Err("durable BFT execution is unsupported on this platform".into())
    }

    pub(super) fn configure_bft_v2(
        &mut self,
        _config: &NodeBftExecutionConfigV2,
    ) -> Result<Option<NodeBftExecutionHeadV2>, String> {
        Err("durable BFT V2 execution is unsupported on this platform".into())
    }

    pub(super) fn has_bft_v2_material(_records_dir: &Path) -> Result<bool, String> {
        Ok(false)
    }

    pub(super) fn current_bft_v2_context(
        &self,
        _schedule: &BftHeightProposerScheduleV2,
    ) -> Result<NodeBftExecutionContextV2, String> {
        Err("durable BFT V2 execution is unsupported on this platform".into())
    }

    pub(super) fn bft_v2_history_page(
        &self,
        _config: &NodeBftExecutionConfigV2,
        _after_height: u64,
        _max_entries: usize,
    ) -> Result<Vec<BftCommitCertificateV2>, String> {
        Err("durable BFT V2 execution is unsupported on this platform".into())
    }

    pub(super) fn validate_bft_v2_action_payload(&self, _payload: &[u8]) -> Result<(), String> {
        Err("BFT V2 action payload admission is unsupported on this platform".into())
    }

    pub(super) fn validate_bft_v2_action_batch(
        &self,
        _schedule: &BftHeightProposerScheduleV2,
        _actions: &[oasis7_node::NodeConsensusAction],
    ) -> Result<(), String> {
        Err("BFT V2 action batch admission is unsupported on this platform".into())
    }

    pub(super) fn build_bft_v2(
        &self,
        _context: NodeBftBuildContextV2,
    ) -> Result<BftBlockV2, String> {
        Err("durable BFT V2 execution is unsupported on this platform".into())
    }

    pub(super) fn preview_bft_v2(
        &self,
        _expected: &NodeBftExecutionContextV2,
        _candidate: &BftBlockV2,
    ) -> Result<(), String> {
        Err("durable BFT V2 execution is unsupported on this platform".into())
    }

    pub(super) fn commit_bft_v2(
        &mut self,
        _token: VerifiedBftCommitCertificateV2,
    ) -> Result<NodeBftExecutionCommitResultV2, String> {
        Err("durable BFT V2 execution is unsupported on this platform".into())
    }
}
