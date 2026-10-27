use super::*;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupApproveRequestV1 {
    pub api_version: u8,
    pub input: ApprovalAdmissionInput,
    pub plan: OwnerSetupPlanV1,
}
impl core::fmt::Debug for OwnerSetupApproveRequestV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupApproveRequestV1(<private>)")
    }
}
impl OwnerSetupApproveRequestV1 {
    pub fn validate(&self) -> Result<()> {
        self.plan.validate()?;
        if self.api_version != 1
            || self.input.operation_id != self.plan.setup_id
            || self.input.request.intent != self.plan.intent()?
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupDriveRequestV1 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub setup_id: String,
}
impl OwnerSetupDriveRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1
            || [&self.custody_account_id, &self.provision_id, &self.setup_id]
                .iter()
                .any(|v| !id(v))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupProgressV1 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub setup_id: String,
    pub stage: ProvisionStage,
    pub pending_operations: Vec<String>,
    pub blocked_operations: Vec<OwnerSetupBlockedOperationV1>,
    pub transport_remaining: DecimalU64,
}
impl AccountProvisionApprovalState {
    pub fn owner_setup_progress(&self, id: &str) -> Result<OwnerSetupProgressV1> {
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        Ok(OwnerSetupProgressV1 {
            api_version: 1,
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            setup_id: v.plan.setup_id.clone(),
            stage: self.provision.stage(),
            pending_operations: v
                .stages
                .iter()
                .filter(|(_, o)| o.result.is_none())
                .map(|(s, _)| s.name().into())
                .collect(),
            blocked_operations: v
                .stages
                .iter()
                .filter_map(|(stage, o)| {
                    let outcome = o
                        .result
                        .as_ref()
                        .or(o.last_observation.as_ref())
                        .and_then(|r| match r {
                            OwnerSetupResultV1::Canonical { response, .. } => {
                                Some(&response.payload.outcome)
                            }
                            _ => None,
                        })
                        .or_else(|| {
                            o.canonical_history
                                .get(&OwnerSetupCanonicalSourceV1::Lookup)
                                .or_else(|| {
                                    o.canonical_history
                                        .get(&OwnerSetupCanonicalSourceV1::Submit)
                                })
                                .map(|f| &f.response.payload.outcome)
                        });
                    let code = match outcome? {
                        ws::IntentOutcome::Rejected { .. } => OwnerSetupBlockedCodeV1::Rejected,
                        ws::IntentOutcome::Expired => OwnerSetupBlockedCodeV1::ExpiredPending,
                        ws::IntentOutcome::HistoryUnavailable => {
                            OwnerSetupBlockedCodeV1::HistoryUnavailablePending
                        }
                        ws::IntentOutcome::Unknown => OwnerSetupBlockedCodeV1::OutcomeUnknown,
                        ws::IntentOutcome::Pending | ws::IntentOutcome::Received { .. } => {
                            OwnerSetupBlockedCodeV1::AwaitingCanonicalResult
                        }
                        ws::IntentOutcome::Committed { .. } => return None,
                    };
                    Some(OwnerSetupBlockedOperationV1 {
                        stage: *stage,
                        code,
                        operation_ref: ledger::stage_operation_id(&v.plan, *stage).ok()?,
                    })
                })
                .collect(),
            transport_remaining: DecimalU64(
                v.transport
                    .as_ref()
                    .map_or(Ok(v.plan.transport_limit.0), |v| v.remaining())
                    .map_err(|_| invalid())?,
            ),
        })
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerSetupBlockedCodeV1 {
    Rejected,
    ExpiredPending,
    HistoryUnavailablePending,
    OutcomeUnknown,
    AwaitingCanonicalResult,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupBlockedOperationV1 {
    pub stage: OwnerSetupStageV1,
    pub code: OwnerSetupBlockedCodeV1,
    pub operation_ref: String,
}
