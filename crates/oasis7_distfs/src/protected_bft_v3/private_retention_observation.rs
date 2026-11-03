//! Current authenticated lease observation; a historical operation is not this type.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::private_retention::BftPrivateRetentionAttestorV1;
use super::private_retention_backend::BftPrivateRetentionLedgerV1;
use super::state::BftSignerCheckpointV3;

/// Constructed only after RemoteLease validates one live Get reply and its
/// checkpoint. No Deserialize or public raw-DTO constructor.
pub struct ValidatedBftPrivateRetentionObservationV1 {
    checkpoint: BftSignerCheckpointV3,
    ledger: BftPrivateRetentionLedgerV1,
    reserved_response_bytes: u64,
    latest_seal: Option<super::private_retention_current_anchor::BftPrivateRetentionSealAnchorV1>,
}

impl ValidatedBftPrivateRetentionObservationV1 {
    pub(super) fn from_validated_reply(
        binding: &BftSignerBindingV3,
        role: &BftPrivateRetentionAttestorV1,
        generation: u64,
        checkpoint: BftSignerCheckpointV3,
        ledger: BftPrivateRetentionLedgerV1,
        latest_seal: Option<
            super::private_retention_current_anchor::BftPrivateRetentionSealAnchorV1,
        >,
        counter: u64,
    ) -> Result<Self, BftSignerErrorV3> {
        super::archive_transfer::validate_successor_archive_budget(
            &checkpoint.state,
            Some(
                &super::archive_transfer::BftSuccessorArchiveTransferLedgerV1 {
                    schema_version: 1,
                    reserved_response_bytes: counter,
                },
            ),
        )?;
        super::private_retention_current_anchor::verify_current_anchor(
            role,
            &ledger,
            latest_seal.as_ref(),
        )?;
        let body = &ledger.receipt.body;
        if checkpoint.state.binding_digest != binding.digest()?
            || checkpoint.provider_public_key != role.provider_public_key
            || checkpoint.lease_generation != generation
            || generation < body.original_lease_generation
            || checkpoint.state.private_retention.as_ref() != Some(&body.head)
            || checkpoint.state.revision < body.after_revision
            || (checkpoint.state.revision == body.after_revision
                && checkpoint.state.state_digest != body.after_state_digest)
            || counter == 0
        {
            return Err(invalid("current retention observation binding mismatch"));
        }
        Ok(Self {
            checkpoint,
            ledger,
            reserved_response_bytes: counter,
            latest_seal,
        })
    }

    pub fn latest_seal(
        &self,
    ) -> Option<&super::private_retention_current_anchor::BftPrivateRetentionSealAnchorV1> {
        self.latest_seal.as_ref()
    }
    pub fn checkpoint(&self) -> &BftSignerCheckpointV3 {
        &self.checkpoint
    }
    pub fn ledger(&self) -> &BftPrivateRetentionLedgerV1 {
        &self.ledger
    }
    pub fn reserved_response_bytes(&self) -> u64 {
        self.reserved_response_bytes
    }
}
