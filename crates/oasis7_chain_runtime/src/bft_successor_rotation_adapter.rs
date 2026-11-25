//! Compatibility-only bridge from a qualified original H to the dormant Node
//! successor rotation candidate. Never grants signing, install, or live admission.
use oasis7_distfs::protected_bft_v3::{
    BftSignerBindingV3, BftSuccessorWindowSourceV1, successor_source_from_binding_v3,
};
use oasis7_node::{
    BftSuccessorRotationMachineV1, BftWindowHeadPinV1, NodeConsensusAction,
};

#[derive(Debug, Clone)]
pub struct CandidateSuccessorWindowBridgeV1 {
    candidate: BftSuccessorRotationMachineV1,
}

impl CandidateSuccessorWindowBridgeV1 {
    /// Cold-validate a candidate against the *independently qualified* original
    /// signer binding and the original durable minimum/driver head.
    /// The caller must still enforce its actual protected signer and install
    /// authority. This method can never authorize an active worker cutover.
    pub fn from_qualified_original(
        candidate: BftSuccessorRotationMachineV1,
        binding: &BftSignerBindingV3,
        durable_minimum_sequence: u64,
        durable_minimum_digest: &str,
        current_lease_generation: u64,
        durable_driver_head: Option<&BftWindowHeadPinV1>,
    ) -> Result<Self, String> {
        let source = successor_source_from_binding_v3(binding).map_err(|e| e.to_string())?;
        Self::check_candidate(
            candidate,
            &source,
            durable_minimum_sequence,
            durable_minimum_digest,
            current_lease_generation,
            durable_driver_head,
        )
    }

    fn check_candidate(
        candidate: BftSuccessorRotationMachineV1,
        source: &BftSuccessorWindowSourceV1,
        durable_minimum_sequence: u64,
        durable_minimum_digest: &str,
        current_lease_generation: u64,
        durable_driver_head: Option<&BftWindowHeadPinV1>,
    ) -> Result<Self, String> {
        let candidate = candidate
            .verify_cold_resume(
                source,
                durable_minimum_sequence,
                durable_minimum_digest,
                current_lease_generation,
                durable_driver_head,
            )
            .map_err(|e| e.to_string())?;
        Ok(Self { candidate })
    }

    /// This is a local observation, not a permission to submit or sign.
    pub fn next_expected_height(&self) -> Result<u64, String> {
        self.candidate
            .next_expected_height()
            .map_err(|e| e.to_string())
    }

    /// Local queue projection keeps the Node's original duplicate/error rules.
    /// No action is committed or removed from a real worker by this method.
    pub fn retain_uncommitted(
        &self,
        pending: &[NodeConsensusAction],
    ) -> Result<Vec<NodeConsensusAction>, String> {
        self.candidate
            .retain_uncommitted_pending(pending)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_proto::distributed_bft::v2::BftScopeV2;

    fn digest(value: u8) -> String {
        format!("{value:02x}").repeat(32)
    }

    fn source() -> BftSuccessorWindowSourceV1 {
        BftSuccessorWindowSourceV1 {
            scope: BftScopeV2 {
                world_id: "world".into(),
                chain_id: "chain".into(),
                genesis_hash: digest(1),
            },
            binding_digest: digest(2),
            handoff_position: 100,
            handoff_record_id: digest(3),
            handoff_snapshot_root: digest(4),
            target_digest: digest(5),
            activation_digest: digest(6),
            material_identity: digest(7),
        }
    }

    #[test]
    fn candidate_cold_read_preserves_origin_and_durable_minimum() {
        let source = source();
        let original = source.origin_digest().unwrap();
        let candidate =
            BftSuccessorRotationMachineV1::from_source(source.clone(), 1, digest(8), 3)
                .unwrap();
        let checked = CandidateSuccessorWindowBridgeV1::check_candidate(
            candidate.clone(),
            &source,
            0,
            &original,
            3,
            None,
        )
        .unwrap();
        assert_eq!(checked.next_expected_height().unwrap(), 101);
        assert!(checked.retain_uncommitted(&[]).unwrap().is_empty());

        let mut other_source = source.clone();
        other_source.activation_digest = digest(9);
        let foreign = CandidateSuccessorWindowBridgeV1::check_candidate(
            candidate.clone(),
            &other_source,
            0,
            &original,
            3,
            None,
        );
        assert!(foreign.is_err());
        let wrong_sequence = CandidateSuccessorWindowBridgeV1::check_candidate(
            candidate.clone(),
            &source,
            1,
            &original,
            3,
            None,
        );
        assert!(wrong_sequence.is_err());
        let wrong_digest = CandidateSuccessorWindowBridgeV1::check_candidate(
            candidate.clone(),
            &source,
            0,
            &digest(10),
            3,
            None,
        );
        assert!(wrong_digest.is_err());
        let stale_owner = CandidateSuccessorWindowBridgeV1::check_candidate(
            candidate,
            &source,
            0,
            &original,
            2,
            None,
        );
        assert!(stale_owner.is_err());
    }
}
