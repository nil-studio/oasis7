//! Independent writer plan acknowledgement and irreversible epoch seal.
//! P/R endpoint signatures cannot stand in for either writer statement.
use super::{crypto::*, *};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

const ACK_DOMAIN: &str = "oasis7.formal.writer.recovery_plan_ack.sign.v1";
const SEAL_DOMAIN: &str = "oasis7.formal.writer.epoch_seal.sign.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterRecoveryPlanAckBodyV1 {
    pub schema_version: u32,
    pub world: FormalWorldIdentityV1,
    pub authority_id: String,
    pub authority_epoch: u64,
    pub operation_id: String,
    pub plan_digest: String,
    pub pending_handoff_root: String,
    pub revision: u64,
}
pub fn writer_recovery_plan_ack_signing_bytes(
    body: &WriterRecoveryPlanAckBodyV1,
) -> Result<Vec<u8>, FormalError> {
    bytes(ACK_DOMAIN, body)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedWriterRecoveryPlanAckV1 {
    pub body: WriterRecoveryPlanAckBodyV1,
    pub signature_hex: String,
}
impl SignedWriterRecoveryPlanAckV1 {
    pub fn sign(body: WriterRecoveryPlanAckBodyV1, key: &SigningKey) -> Result<Self, FormalError> {
        let signature_hex = hex::encode(
            key.sign(&writer_recovery_plan_ack_signing_bytes(&body)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            signature_hex,
        })
    }
    pub fn digest(&self) -> Result<String, FormalError> {
        hash("oasis7.formal.writer.recovery_plan_ack.v1", &self.body)
    }
    pub fn verify(
        &self,
        plan: &SignedRecoveryPlanV1,
        authority: &VerifiedAuthorityV1,
    ) -> Result<(), FormalError> {
        plan.verify(authority)?;
        if self.body.schema_version != 1
            || self.body.revision == 0
            || self.body.world != authority.definition().world
            || self.body.authority_id != authority.authority_id()?
            || self.body.authority_epoch != authority.definition().epoch
            || self.body.operation_id != plan.body.intent.operation_id
            || self.body.plan_digest != plan.digest()?
            || self.body.pending_handoff_root != plan.body.pending_handoff_root
        {
            return Err(invalid(
                "writer plan acknowledgement original authority/plan/pending binding",
            ));
        }
        verify(
            ACK_DOMAIN,
            &self.body,
            &self.signature_hex,
            &authority.definition().writer_public_key,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterEpochSealBodyV1 {
    pub schema_version: u32,
    pub world: FormalWorldIdentityV1,
    pub authority_id: String,
    pub authority_epoch: u64,
    pub operation_id: String,
    pub plan_digest: String,
    pub pending_handoff_root: String,
    pub cut: FormalHeadRefV1,
    pub next_epoch: u64,
    pub plan_ack_digest: String,
    pub drained_proof_digest: String,
    pub revision: u64,
}
pub fn writer_epoch_seal_signing_bytes(
    body: &WriterEpochSealBodyV1,
) -> Result<Vec<u8>, FormalError> {
    bytes(SEAL_DOMAIN, body)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedWriterEpochSealV1 {
    pub body: WriterEpochSealBodyV1,
    pub plan_ack: SignedWriterRecoveryPlanAckV1,
    pub drained: RecoveryDrainedProofV1,
    pub signature_hex: String,
}
impl SignedWriterEpochSealV1 {
    pub fn sign(
        body: WriterEpochSealBodyV1,
        plan_ack: SignedWriterRecoveryPlanAckV1,
        drained: RecoveryDrainedProofV1,
        key: &SigningKey,
    ) -> Result<Self, FormalError> {
        if body.plan_ack_digest != plan_ack.digest()?
            || body.revision <= plan_ack.body.revision
            || body.drained_proof_digest != drained.digest()?
        {
            return Err(invalid(
                "writer seal must follow its durable plan acknowledgement",
            ));
        }
        let signature_hex = hex::encode(
            key.sign(&writer_epoch_seal_signing_bytes(&body)?)
                .to_bytes(),
        );
        Ok(Self {
            body,
            plan_ack,
            drained,
            signature_hex,
        })
    }
    pub fn digest(&self) -> Result<String, FormalError> {
        hash("oasis7.formal.writer.epoch_seal.v1", &self.body)
    }
    pub fn verify(
        &self,
        plan: &SignedRecoveryPlanV1,
        authority: &VerifiedAuthorityV1,
    ) -> Result<(), FormalError> {
        self.plan_ack.verify(plan, authority)?;
        let cut = self.drained.verify(plan, authority)?;
        if self.body.schema_version != 1
            || self.body.revision <= self.plan_ack.body.revision
            || self.body.world != authority.definition().world
            || self.body.authority_id != authority.authority_id()?
            || self.body.authority_epoch != authority.definition().epoch
            || self.body.operation_id != plan.body.intent.operation_id
            || self.body.plan_digest != plan.digest()?
            || self.body.pending_handoff_root != plan.body.pending_handoff_root
            || self.body.cut != cut
            || self.body.next_epoch != plan.body.intent.successor.epoch
            || self.body.plan_ack_digest != self.plan_ack.digest()?
            || self.body.drained_proof_digest != self.drained.digest()?
        {
            return Err(invalid(
                "writer epoch seal original plan/cut/successor/ack binding",
            ));
        }
        verify(
            SEAL_DOMAIN,
            &self.body,
            &self.signature_hex,
            &authority.definition().writer_public_key,
        )
    }
    pub fn verify_with_ack(
        &self,
        plan: &SignedRecoveryPlanV1,
        authority: &VerifiedAuthorityV1,
        ack: &SignedWriterRecoveryPlanAckV1,
    ) -> Result<(), FormalError> {
        self.verify(plan, authority)?;
        if &self.plan_ack != ack {
            return Err(invalid(
                "writer seal changed the endpoint's original writer plan acknowledgement",
            ));
        }
        Ok(())
    }
}
