//! Immutable trusted Account Current window, never a refreshed authority or grant.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceFinalCurrentWindowV2 {
    at: DecimalU64,
    handoff_until: DecimalU64,
    session_expires_at: DecimalU64,
    session_idle_expires_at: DecimalU64,
}
pub(super) fn required_window<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> core::result::Result<Option<ResourceFinalCurrentWindowV2>, D::Error> {
    Option::deserialize(d)
}
impl ResourceFinalCurrentWindowV2 {
    pub(super) fn capture(current: &OwnerSetupCurrent<'_, '_>, now: u64) -> Self {
        Self {
            at: DecimalU64(now),
            handoff_until: current.provision.approval.handoff_until,
            session_expires_at: current.provision.approval.source.session_expires_at,
            session_idle_expires_at: current.provision.approval.source.session_idle_expires_at,
        }
    }
    pub(super) fn validate(&self, source: &ProvisionOriginInspectionV1, at: u64) -> Result<()> {
        let v = &source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification;
        if self.at.0 != at
            || at >= self.handoff_until.0
            || at >= self.session_expires_at.0
            || at >= self.session_idle_expires_at.0
            || self.session_expires_at != v.source_session_expires_at
            || self.session_idle_expires_at != v.source_session_idle_expires_at
            || v.observed_at.0 > at
            || source.verified_at.is_none_or(|verified| verified.0 > at)
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn deadline(
        &self,
        plan: &OwnerSetupPlanV2,
        source: &ProvisionOriginInspectionV1,
    ) -> Result<u64> {
        self.validate(source, self.at.0)?;
        let source_until = source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification
            .observed_at
            .0
            .checked_add(5000)
            .ok_or_else(invalid)?;
        Ok(plan
            .deadline
            .0
            .min(source_until)
            .min(self.handoff_until.0)
            .min(self.session_expires_at.0)
            .min(self.session_idle_expires_at.0))
    }
}
