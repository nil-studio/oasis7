//! Healthy replica replacement authorization, distinct from disaster recovery.
use super::{crypto::*, *};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalDurableSetTransitionBodyV1 {
    pub schema_version: u32,
    pub world: FormalWorldIdentityV1,
    pub predecessor_authority_id: String,
    pub predecessor: FormalHeadRefV1,
    pub boundary_position: u64,
    pub operation_id: String,
    pub successor: AuthorityDefinitionV1,
    /// Pre-H copy contract identity, NOT proof that final H objects were copied.
    /// Final inventory is separately attested by the trusted installation provider.
    pub independent_closure_root: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNormalDurableSetTransitionV1 {
    pub body: NormalDurableSetTransitionBodyV1,
    pub writer_signature_hex: String,
}
impl NormalDurableSetTransitionBodyV1 {
    pub fn copy_contract_digest(&self) -> Result<String, FormalError> {
        hash(
            "oasis7.formal.durable_set_copy_contract.v1",
            &(
                &self.world,
                &self.predecessor_authority_id,
                &self.predecessor,
            ),
        )
    }
    pub fn digest(&self) -> Result<String, FormalError> {
        hash("oasis7.formal.normal_durable_set_transition.v1", self)
    }
    pub fn validate(&self, authority: &VerifiedAuthorityV1) -> Result<(), FormalError> {
        let old = authority.definition();
        self.successor.validate()?;
        self.successor.verify_policy(authority.policy())?;
        for digest in [
            &self.operation_id,
            &self.independent_closure_root,
            &self.predecessor.record_id,
        ] {
            hash_hex(digest)?;
        }
        let mut preserved_primary = old.primary.clone();
        preserved_primary.instance.authority_epoch = self.successor.epoch;
        if self.independent_closure_root != self.copy_contract_digest()?
            || self.schema_version != 1
            || self.world != old.world
            || self.predecessor_authority_id != authority.authority_id()?
            || self.predecessor.position.checked_add(1) != Some(self.boundary_position)
            || old.epoch.checked_add(1) != Some(self.successor.epoch)
            || self.boundary_position.checked_add(1) != Some(self.successor.valid_from_position)
            || self.successor.predecessor_authority_id.as_ref()
                != Some(&self.predecessor_authority_id)
            || self.successor.world != old.world
            || self.successor.profile != old.profile
            || self.successor.profile_version != old.profile_version
            || self.successor.recovery_policy_digest != old.recovery_policy_digest
            || self.successor.writer_public_key != old.writer_public_key
            || self.successor.primary != preserved_primary
        {
            return Err(invalid(
                "healthy durable set exact predecessor/writer/primary boundary",
            ));
        }
        let new = &self.successor.replica.instance;
        for previous in [&old.primary.instance, &old.replica.instance] {
            if new.endpoint_public_key == previous.endpoint_public_key
                || new.instance_id == previous.instance_id
                || new.storage_id == previous.storage_id
                || new.storage_fingerprint == previous.storage_fingerprint
            {
                return Err(invalid(
                    "healthy durable set requires independent new replica",
                ));
            }
        }
        Ok(())
    }
}
pub fn normal_durable_set_transition_signing_bytes(
    body: &NormalDurableSetTransitionBodyV1,
) -> Result<Vec<u8>, FormalError> {
    bytes(
        "oasis7.formal.normal_durable_set_transition.writer.v1",
        body,
    )
}
/// Constructed only after the old configuration's exact Qualified boundary.
pub struct VerifiedNormalDurableSetTransitionV1 {
    pub(super) body: NormalDurableSetTransitionBodyV1,
    pub(super) old: VerifiedAuthorityV1,
    pub(super) boundary: VerifiedFormalCommitV1,
    pub(super) digest: String,
}
impl VerifiedNormalDurableSetTransitionV1 {
    pub fn predecessor_authority(&self) -> &VerifiedAuthorityV1 {
        &self.old
    }
    pub fn body(&self) -> &NormalDurableSetTransitionBodyV1 {
        &self.body
    }
    pub fn boundary(&self) -> &VerifiedFormalCommitV1 {
        &self.boundary
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
}
pub fn verify_normal_durable_set_transition(
    signed: &SignedNormalDurableSetTransitionV1,
    authority: &VerifiedAuthorityV1,
    predecessor: &VerifiedFormalCommitV1,
    boundary: &VerifiedFormalCommitV1,
) -> Result<VerifiedNormalDurableSetTransitionV1, FormalError> {
    signed.body.validate(authority)?;
    verify(
        "oasis7.formal.normal_durable_set_transition.writer.v1",
        &signed.body,
        &signed.writer_signature_hex,
        &authority.definition().writer_public_key,
    )?;
    let digest = signed.body.digest()?;
    let record = boundary.record();
    if signed.body.predecessor != predecessor.head()
        || predecessor.record().world != signed.body.world
        || predecessor.finalizing_authority_id() != authority.authority_id()?
        || boundary.finalizing_authority_id() != authority.authority_id()?
        || !matches!(record.schema_version, 2 | 4 | 6)
        || record.kind != FormalRecordKindV1::Execution
        || record.position != signed.body.boundary_position
        || record.parent_record_id != predecessor.head().record_id
        || record.world != signed.body.world
        || record.original_authority_id != authority.authority_id()?
        || record.original_epoch != authority.definition().epoch
        || record.activation_ref != authority.activation_ref()
        || record.durable_set_transition_digest.as_ref() != Some(&digest)
        || record.handoff.is_some()
    {
        return Err(invalid(
            "healthy durable set exact Qualified boundary commitment",
        ));
    }
    Ok(VerifiedNormalDurableSetTransitionV1 {
        body: signed.body.clone(),
        old: authority.clone(),
        boundary: boundary.clone(),
        digest,
    })
}
