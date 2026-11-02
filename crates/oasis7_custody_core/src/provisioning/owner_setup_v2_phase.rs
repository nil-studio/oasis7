//! Internal SDK phase reuse; never a V1 approval or a copied Prepared state.
use super::*;
use crate::budget::BudgetLedger;
/// This projection contains the unchanged SDK four-write request fields only.
/// The actual V2 plan/Book/Source is always kept in the separate V2 header.
pub fn derive_owner_setup_move0_sdk_plan_v2(p: &OwnerSetupPlanV2) -> Result<OwnerSetupPlanV1> {
    p.validate().map_err(|_| invalid())?;
    if !p.selection.is_exact_move0() {
        return Err(invalid());
    }
    derive_owner_setup_write_projection_v2(p)
}
/// Fixed SDK first-five-stage projection for an actually selected Resource
/// setup. It carries no V1 approval, final observation or execution grant.
pub fn derive_owner_setup_resource_sdk_plan_v2(p: &OwnerSetupPlanV2) -> Result<OwnerSetupPlanV1> {
    p.validate().map_err(|_| invalid())?;
    if p.selection.resource().is_none() {
        return Err(invalid());
    }
    derive_owner_setup_write_projection_v2(p)
}
// Pure SDK write projection. The enclosing actual V2 Book remains the only
// approval authority; selection never becomes resource_refs or installs a grant.
fn derive_owner_setup_write_projection_v2(p: &OwnerSetupPlanV2) -> Result<OwnerSetupPlanV1> {
    p.validate().map_err(|_| invalid())?;
    let projection = OwnerSetupPlanV1 {
        api_version: 1,
        setup_id: p.setup_id.clone(),
        provision_id: p.provision_id.clone(),
        participant_receipt_digest: p.participant_receipt_digest.clone(),
        world_identity: p.world_identity.clone(),
        owner_key_ref: p.owner_key_ref.clone(),
        executor_key_ref: p.executor_key_ref.clone(),
        bootstrap_config_version: p.bootstrap_config_version,
        goal_content: p.goal_content.clone(),
        create_operation_id: p.create_operation_id.clone(),
        prepare_operation_id: p.prepare_operation_id.clone(),
        prepare_nonce: p.prepare_nonce.clone(),
        publish_operation_id: p.publish_operation_id.clone(),
        delegation_operation_id: p.delegation_operation_id.clone(),
        bind_operation_id: p.bind_operation_id.clone(),
        executor_id: p.executor_id.clone(),
        executor_public_key: p.executor_public_key.clone(),
        delegation_generation: p.delegation_generation,
        delegation_nonce: p.delegation_nonce,
        resource_refs: vec![],
        deadline: p.deadline,
        request_limit: p.request_limit,
        transport_limit: p.transport_limit,
        write_limit: p.write_limit,
    };
    projection.validate()?;
    Ok(projection)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerSetupSdkPhaseProjectionV2 {
    record: OwnerSetupApprovalV1,
}
impl OwnerSetupSdkPhaseProjectionV2 {
    pub(super) fn from_consumed(
        plan: &OwnerSetupPlanV2,
        book: &ApprovalAdmissionBook,
        approval_charge: &BudgetLedger,
        session: ParticipantSessionRefV1,
        source_nonce: String,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<Self> {
        let r = book.original(&plan.setup_id).ok_or_else(invalid)?;
        if r.stage() != ApprovalAdmissionStage::Consumed {
            return Err(invalid());
        }
        Ok(Self {
            record: OwnerSetupApprovalV1 {
                plan: derive_owner_setup_write_projection_v2(plan)?,
                book: book.clone(),
                transport: Some(approval_charge.clone()),
                source: Some(ProvisionOriginInspectionV1 {
                    operation_id: digest(&(
                        "oasis7.custody.owner-setup-source.v2",
                        &plan.setup_id,
                        plan.digest().map_err(|_| invalid())?,
                    ))?,
                    verify_nonce: source_nonce,
                    created_at: DecimalU64(now),
                    expires_at: DecimalU64(
                        plan.deadline
                            .0
                            .min(current.approval.source.session_expires_at.0)
                            .min(current.approval.source.session_idle_expires_at.0)
                            .min(now.checked_add(30_000).ok_or_else(invalid)?),
                    ),
                    claimed_at: None,
                    request_bytes: None,
                    verification: None,
                    verified_at: None,
                }),
                session: Some(session),
                stages: BTreeMap::new(),
            },
        })
    }
    pub(super) fn validate_header(
        &self,
        plan: &OwnerSetupPlanV2,
        book: &ApprovalAdmissionBook,
    ) -> Result<()> {
        if self.record.plan != derive_owner_setup_write_projection_v2(plan)?
            || &self.record.book != book
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn owner_setup_engine_record(
        &self,
        id: &str,
    ) -> Result<&OwnerSetupApprovalV1> {
        if let Some(v2) = &self.owner_setup_v2 {
            if !self.owner_setups.is_empty() || v2.plan.setup_id != id {
                return Err(ProvisionApprovalError::Conflict);
            }
            let p = v2.phase.as_ref().ok_or_else(invalid)?;
            p.validate_header(&v2.plan, &v2.book)?;
            return Ok(&p.record);
        }
        self.owner_setups.get(id).ok_or_else(invalid)
    }
    pub(in crate::provisioning) fn owner_setup_engine_record_mut(
        &mut self,
        id: &str,
    ) -> Result<&mut OwnerSetupApprovalV1> {
        if self.owner_setup_v2.is_some() {
            if !self.owner_setups.is_empty() {
                return Err(ProvisionApprovalError::Conflict);
            }
            let v2 = self.owner_setup_v2.as_mut().ok_or_else(invalid)?;
            if v2.plan.setup_id != id {
                return Err(ProvisionApprovalError::Conflict);
            }
            let p = v2.phase.as_mut().ok_or_else(invalid)?;
            return Ok(&mut p.record);
        }
        self.owner_setups.get_mut(id).ok_or_else(invalid)
    }
    pub(in crate::provisioning) fn owner_setup_engine_records(
        &self,
    ) -> impl Iterator<Item = (&String, &OwnerSetupApprovalV1)> {
        self.owner_setups.iter().chain(
            self.owner_setup_v2
                .iter()
                .filter_map(|v| v.phase.as_ref().map(|p| (&v.plan.setup_id, &p.record))),
        )
    }
    pub(in crate::provisioning) fn owner_setup_signed_plan_digest(
        &self,
        id: &str,
    ) -> Result<String> {
        if let Some(v2) = &self.owner_setup_v2 {
            self.owner_setup_engine_record(id)?;
            return v2.plan.digest().map_err(|_| invalid());
        }
        self.owner_setup_engine_record(id)?.plan.digest()
    }
    pub(in crate::provisioning) fn owner_setup_phase_body_digest(
        &self,
        id: &str,
    ) -> Result<String> {
        if let Some(v2) = &self.owner_setup_v2 {
            self.owner_setup_engine_record(id)?;
            return v2.plan.digest().map_err(|_| invalid());
        }
        digest(&self.owner_setup_engine_record(id)?.plan)
    }
    pub(in crate::provisioning) fn owner_setup_engine_intent(
        &self,
        id: &str,
    ) -> Result<CustodyIntentV1> {
        if let Some(v2) = &self.owner_setup_v2 {
            self.owner_setup_engine_record(id)?;
            return v2.plan.intent().map_err(|_| invalid());
        }
        self.owner_setup_engine_record(id)?.plan.intent()
    }
    pub(in crate::provisioning) fn owner_setup_source_operation_id(
        &self,
        id: &str,
    ) -> Result<String> {
        if let Some(v2) = &self.owner_setup_v2 {
            self.owner_setup_engine_record(id)?;
            return digest(&(
                "oasis7.custody.owner-setup-source.v2",
                id,
                v2.plan.digest().map_err(|_| invalid())?,
            ));
        }
        digest(&(
            "oasis7.custody.owner-setup-source.v1",
            id,
            &self.owner_setup_engine_record(id)?.plan.setup_id,
        ))
    }
    pub fn owner_setup_engine_version(&self, id: &str) -> Result<u8> {
        self.owner_setup_engine_record(id)?;
        Ok(if self.owner_setup_v2.is_some() { 2 } else { 1 })
    }
    pub fn owner_setup_approved_header_v2(&self, id: &str) -> Option<&OwnerSetupPlanV2> {
        let v2 = self.owner_setup_v2.as_ref()?;
        (v2.plan.setup_id == id && self.owner_setup_engine_record(id).is_ok()).then_some(&v2.plan)
    }
    pub(in crate::provisioning) fn owner_setup_engine_authority_revision(
        &self,
        id: &str,
        authority: &crate::approval::ApprovalContextV1,
        source_revision: &str,
    ) -> Result<String> {
        if let Some(v2) = &self.owner_setup_v2 {
            self.owner_setup_engine_record(id)?;
            return digest(&(
                "oasis7.custody.owner-setup-authority.v2",
                &v2.plan,
                authority,
                source_revision,
            ));
        }
        digest(&(
            "oasis7.custody.owner-setup-authority.v1",
            &self.owner_setup_engine_record(id)?.plan,
            authority,
            source_revision,
        ))
    }
    /// SDK data only, never a V1 approval or Prepared status projection.
    pub fn owner_setup_phase_sdk_plan(&self, id: &str) -> Result<&OwnerSetupPlanV1> {
        Ok(&self.owner_setup_engine_record(id)?.plan)
    }
    pub fn owner_setup_phase_original(&self, id: &str) -> Result<&ApprovalAdmissionRecord> {
        self.owner_setup_engine_record(id)?
            .book
            .original(id)
            .ok_or_else(invalid)
    }
    pub fn owner_setup_phase_source(&self, id: &str) -> Result<&ProvisionOriginInspectionV1> {
        self.owner_setup_engine_record(id)?
            .source
            .as_ref()
            .ok_or_else(invalid)
    }
}
