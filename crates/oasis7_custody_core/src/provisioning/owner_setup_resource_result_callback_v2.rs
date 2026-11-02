//! The existing paid result RPC has its own V2 once, never legacy common flags.
use super::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceFinalResultCallbackV2 {
    api_version: u8,
    claimed_at: DecimalU64,
    dispatch_record_digest: String,
    original_digest: String,
    source_record_digest: String,
    expires_at: DecimalU64,
}
pub(super) fn required_callback<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> core::result::Result<Option<ResourceFinalResultCallbackV2>, D::Error> {
    Option::deserialize(d)
}
impl ResourceFinalResultCallbackV2 {
    pub(super) fn claimed_at(&self) -> u64 {
        self.claimed_at.0
    }
    fn new(f: &OwnerSetupResourceFinalOutboxV2, now: u64) -> Result<Self> {
        let dispatch = f.dispatch.as_ref().ok_or_else(invalid)?;
        let value = Self {
            api_version: 2,
            claimed_at: DecimalU64(now),
            dispatch_record_digest: digest(dispatch)?,
            original_digest: digest(&(
                "oasis7.custody.resource-final-callback-original.v2",
                f.original_bytes.as_ref().ok_or_else(invalid)?,
            ))?,
            source_record_digest: digest(&f.common.source)?,
            expires_at: DecimalU64(dispatch.expires_at()),
        };
        value.validate(f)?;
        Ok(value)
    }
    pub(super) fn validate(&self, f: &OwnerSetupResourceFinalOutboxV2) -> Result<()> {
        let dispatch = f.dispatch.as_ref().ok_or_else(invalid)?;
        let observed = f
            .common
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification
            .observed_at
            .0;
        if self.api_version != 2
            || self.claimed_at.0 < dispatch.paid_at()
            || self.claimed_at.0 >= self.expires_at.0
            || self.expires_at.0 != dispatch.expires_at()
            || observed > self.claimed_at.0
            || self.claimed_at.0 - observed > 5000
            || self.dispatch_record_digest != digest(dispatch)?
            || self.original_digest
                != digest(&(
                    "oasis7.custody.resource-final-callback-original.v2",
                    f.original_bytes.as_ref().ok_or_else(invalid)?,
                ))?
            || self.source_record_digest != digest(&f.common.source)?
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    /// Local prepayment reservation only. It grants no Current or remote dispatch.
    pub fn reserve_owner_setup_resource_result_callback_v2(
        &mut self,
        id: &str,
        now: u64,
    ) -> Result<()> {
        self.validate_persistence()?;
        self.owner_setup_local(id, now)?;
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        if v.plan.setup_id != id
            || v.plan.selection.resource().is_none()
            || v.book
                .original(id)
                .is_none_or(|r| r.stage() != ApprovalAdmissionStage::Consumed)
        {
            return Err(invalid());
        }
        let f = v.resource_final.as_ref().ok_or_else(invalid)?;
        if f.result_callback.is_some() || f.result.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        let callback = ResourceFinalResultCallbackV2::new(f, now)?;
        let mut next = self.clone();
        next.owner_setup_v2
            .as_mut()
            .ok_or_else(invalid)?
            .resource_final
            .as_mut()
            .ok_or_else(invalid)?
            .result_callback = Some(callback);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    /// Exact historical input/Source for the already paid callback. No Current
    /// assertion, crypto verdict, refresh, external request or caller-made token.
    pub fn owner_setup_resource_result_callback_source_v2(
        &self,
        id: &str,
        now: u64,
    ) -> Result<(&ApprovalAdmissionInput, &SignedApprovalVerificationV1)> {
        self.validate_persistence()?;
        self.owner_setup_local(id, now)?;
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        if v.plan.setup_id != id || v.plan.selection.resource().is_none() {
            return Err(invalid());
        }
        let f = v.resource_final.as_ref().ok_or_else(invalid)?;
        let callback = f.result_callback.as_ref().ok_or_else(invalid)?;
        callback.validate(f)?;
        if f.result.is_some() || now < callback.claimed_at.0 || now >= callback.expires_at.0 {
            return Err(invalid());
        }
        let record = v
            .book
            .original(id)
            .filter(|r| r.stage() == ApprovalAdmissionStage::Consumed)
            .ok_or_else(invalid)?;
        Ok((
            record.input(),
            f.common.source.verification.as_ref().ok_or_else(invalid)?,
        ))
    }
}
