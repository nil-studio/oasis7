//! Explicit IWS continuation DATA; never a registered-turn or admission permit.
//! The raw layout is shared with AgentContinuation, but V1 validators continue
//! to reject this schema and its V2 conditions. No implicit migration occurs.
use super::*;
use oasis7_agent_api::iws_scheduler_v2::{
    IWS_CURRENT_COMMITMENT_SCHEME_V1, IwsWakeConditionValidatorV2,
};
pub const IWS_CONTINUATION_SCHEMA_V2: &str = "iws-agent-continuation.v2";
pub const IWS_CONTINUATION_STATUS_DOMAIN_V2: &str = "oasis7.cognition.iws-continuation-status.v2";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IwsAgentContinuationV2 {
    raw: AgentContinuation,
}
impl IwsAgentContinuationV2 {
    /// Structural data conversion only; actual turn/Context/Current admission is
    /// mandatory in Runtime before the value can enter a scheduler or a lease.
    pub fn try_from_data(raw: AgentContinuation) -> Result<Self, WakeConditionError> {
        let value = Self { raw };
        value.validate_fields()?;
        Ok(value)
    }
    pub fn as_data(&self) -> &AgentContinuation {
        &self.raw
    }
    pub fn as_mut_data(&mut self) -> &mut AgentContinuation {
        &mut self.raw
    }
    pub fn validate_fields(&self) -> Result<(), WakeConditionError> {
        validate_iws_continuation_fields_v2(&self.raw)
    }
    pub fn status_digest_input(&self) -> impl Serialize + '_ {
        iws_continuation_status_digest_input_v2(&self.raw)
    }
    pub fn status_digest(&self) -> String {
        h_v1(
            IWS_CONTINUATION_STATUS_DOMAIN_V2,
            &self.status_digest_input(),
        )
    }
    pub fn validate_data(&self) -> Result<(), WakeConditionError> {
        self.validate_fields()?;
        if self.raw.continuation_status_digest.as_deref() != Some(self.status_digest().as_str()) {
            return Err(WakeConditionError::new("recovery_pending"));
        }
        Ok(())
    }
}

/// Borrowed structural validation; no DTO clone precedes rejection of an
/// oversized or malformed inherited continuation. This grants no source right.
pub fn validate_iws_continuation_fields_v2(
    data: &AgentContinuation,
) -> Result<(), WakeConditionError> {
    data.validate_identity_for_schema_v1(IWS_CONTINUATION_SCHEMA_V2)?;
    IwsWakeConditionValidatorV2::validate_data_rows(&data.wake_conditions)?;
    let terminal = matches!(
        data.status,
        ContinuationStatusV1::Completed
            | ContinuationStatusV1::Cancelled
            | ContinuationStatusV1::Invalidated
            | ContinuationStatusV1::Expired
            | ContinuationStatusV1::Rejected
    );
    if terminal != data.terminal_disposition.is_some() {
        return Err(WakeConditionError::new("recovery_pending"));
    }
    Ok(())
}
/// The one original V2 projection is shared by owned DATA helpers and the
/// Runtime paid encoder. Returning references allocates no continuation graph.
pub fn iws_continuation_status_digest_input_v2(data: &AgentContinuation) -> impl Serialize + '_ {
    #[derive(Serialize)]
    struct Status<'a, T> {
        schema_version: &'static str,
        commitment_scheme: &'static str,
        body: T,
        wake_conditions: &'a [WakeConditionV1],
    }
    Status {
        schema_version: "iws-continuation-status.v2",
        commitment_scheme: IWS_CURRENT_COMMITMENT_SCHEME_V1,
        body: data.status_digest_input_v1(),
        wake_conditions: &data.wake_conditions,
    }
}
