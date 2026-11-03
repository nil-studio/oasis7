//! Test faults wrap an actual lease; no authority or observations are synthesized.
use oasis7_consensus::tendermint_v3::{
    BftProposalIntentV3, BftPublicProposalV3, BftVoteIntentV3,
};
use oasis7_distfs::protected_bft_v3::*;
use oasis7_formal_protocol::bft::FormalBftCertifiedHeaderV3;
use oasis7_proto::distributed_bft::v3::BftVoteV3;
macro_rules! lease_boundary {
    ($name:ident, $stop:expr, $deny:expr) => {
        pub(super) struct $name<'a> {
            inner: &'a mut dyn BftSignerLeaseV3,
        }
        impl<'a> $name<'a> {
            pub(super) fn new(inner: &'a mut dyn BftSignerLeaseV3) -> Self {
                Self { inner }
            }
        }
        impl BftSignerLeaseV3 for $name<'_> {
            fn observe_private_retention(
                &mut self,
                _deadline: std::time::Instant,
            ) -> Result<ValidatedBftPrivateRetentionObservationV1, BftSignerErrorV3> {
                self.inner.observe_private_retention(_deadline)
            }
            fn reserve_private_retention_input(
                &mut self,
                _reference: BftPrivateRetentionAdmissionInputRefV1,
                _deadline: std::time::Instant,
            ) -> Result<BftPrivateRetentionLedgerV1, BftSignerErrorV3> {
                if $deny {
                    Err(BftSignerErrorV3(
                        "cold recovery attempted another Reserve".into(),
                    ))
                } else {
                    self.inner
                        .reserve_private_retention_input(_reference, _deadline)
                }
            }
            fn select_private_retention(
                &mut self,
                _request: BftPrivateRetentionSelectRequestV1,
                _deadline: std::time::Instant,
            ) -> Result<BftPrivateRetentionSelectionResultV1, BftSignerErrorV3> {
                self.inner.select_private_retention(_request, _deadline)
            }
            fn ready_private_retention_original_local(
                &mut self,
                _key: BftPrivateRetentionKeyV1,
                _deadline: std::time::Instant,
            ) -> Result<BftPrivateRetentionLedgerV1, BftSignerErrorV3> {
                if $stop {
                    Err(BftSignerErrorV3(
                        "actual Active cold test stopped before first Ready".into(),
                    ))
                } else {
                    self.inner
                        .ready_private_retention_original_local(_key, _deadline)
                }
            }
            fn ready_private_retention(
                &mut self,
                _key: BftPrivateRetentionKeyV1,
                _deadline: std::time::Instant,
            ) -> Result<BftPrivateRetentionLedgerV1, BftSignerErrorV3> {
                if $stop {
                    Err(BftSignerErrorV3(
                        "actual Active cold test stopped before first Ready".into(),
                    ))
                } else {
                    self.inner.ready_private_retention(_key, _deadline)
                }
            }
            fn verified_active_window(
                &mut self,
            ) -> Result<Option<VerifiedBftSignerActiveWindowV1>, BftSignerErrorV3> {
                self.inner.verified_active_window()
            }
            fn current_verified_schedule(
                &mut self,
            ) -> Result<oasis7_formal_protocol::bft::BftHeaderHeightScheduleV3, BftSignerErrorV3>
            {
                self.inner.current_verified_schedule()
            }
            fn read_private_retention_selection_original(
                &mut self,
                _current: &BftPrivateRetentionLedgerV1,
                _operation_digest: String,
                _deadline: std::time::Instant,
            ) -> Result<BftPrivateRetentionOperationProofV1, BftSignerErrorV3> {
                self.inner.read_private_retention_selection_original(
                    _current,
                    _operation_digest,
                    _deadline,
                )
            }
            fn checkpoint(&mut self) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
                self.inner.checkpoint()
            }
            fn protected_time_ms(&mut self) -> Result<i64, BftSignerErrorV3> {
                self.inner.protected_time_ms()
            }
            fn start_height_timer(
                &mut self,
                expected_revision: u64,
            ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
                self.inner.start_height_timer(expected_revision)
            }
            fn advance(
                &mut self,
                expected_revision: u64,
                expected_state_digest: &str,
                candidate: BftSignerCandidateV3,
            ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
                self.inner
                    .advance(expected_revision, expected_state_digest, candidate)
            }
            fn sign_proposal(
                &mut self,
                expected_revision: u64,
                intent: &BftProposalIntentV3,
            ) -> Result<(BftPublicProposalV3, BftSignerCheckpointV3), BftSignerErrorV3> {
                self.inner.sign_proposal(expected_revision, intent)
            }
            fn sign_vote(
                &mut self,
                expected_revision: u64,
                intent: &BftVoteIntentV3,
            ) -> Result<(BftVoteV3, BftSignerCheckpointV3), BftSignerErrorV3> {
                self.inner.sign_vote(expected_revision, intent)
            }
            fn confirm_applied(
                &mut self,
                expected_revision: u64,
                expected_state_digest: &str,
                height: u64,
                header_id: &str,
                driver_install_digest: &str,
            ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
                self.inner.confirm_applied(
                    expected_revision,
                    expected_state_digest,
                    height,
                    header_id,
                    driver_install_digest,
                )
            }
            fn append_committed_certificate(
                &mut self,
                expected_revision: u64,
                expected_state_digest: &str,
                certificate: FormalBftCertifiedHeaderV3,
            ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
                self.inner.append_committed_certificate(
                    expected_revision,
                    expected_state_digest,
                    certificate,
                )
            }
        }
    };
}
lease_boundary!(StopBeforeReady, true, false);
lease_boundary!(NoReserveOnRecovery, false, true);
