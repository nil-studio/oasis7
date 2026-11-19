//! Closed governance column addresses shared by GEN, ordinary updates and receivers.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeGovernanceValueV1<'a> {
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
    Text(&'a str),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyColumnV1 {
    TimelockTicks,
    EpochLengthTicks,
    ActivationDelayEpochs,
    EmergencyBrakeGuardianThreshold,
    EmergencyVetoGuardianThreshold,
    EmergencyBrakeMaxTicks,
}
impl PolicyColumnV1 {
    pub const ALL: [Self; 6] = [
        Self::TimelockTicks,
        Self::EpochLengthTicks,
        Self::ActivationDelayEpochs,
        Self::EmergencyBrakeGuardianThreshold,
        Self::EmergencyVetoGuardianThreshold,
        Self::EmergencyBrakeMaxTicks,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::TimelockTicks => "timelock_ticks",
            Self::EpochLengthTicks => "epoch_length_ticks",
            Self::ActivationDelayEpochs => "activation_delay_epochs",
            Self::EmergencyBrakeGuardianThreshold => "emergency_brake_guardian_threshold",
            Self::EmergencyVetoGuardianThreshold => "emergency_veto_guardian_threshold",
            Self::EmergencyBrakeMaxTicks => "emergency_brake_max_ticks",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpochColumnV1 {
    EpochId,
    ThresholdBps,
    MinUniqueSigners,
    ValidatorSetHash,
    StakeRoot,
    Threshold,
}
impl EpochColumnV1 {
    pub const ALL: [Self; 6] = [
        Self::EpochId,
        Self::ThresholdBps,
        Self::MinUniqueSigners,
        Self::ValidatorSetHash,
        Self::StakeRoot,
        Self::Threshold,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::EpochId => "epoch_id",
            Self::ThresholdBps => "threshold_bps",
            Self::MinUniqueSigners => "min_unique_signers",
            Self::ValidatorSetHash => "validator_set_hash",
            Self::StakeRoot => "stake_root",
            Self::Threshold => "threshold",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenaltyColumnV1 {
    PenaltyId,
    TargetAgentId,
    EvidenceHash,
    Reason,
    SlashStake,
    AppealDeadlineTick,
    Status,
    IdentityStatusBefore,
    DetectionSource,
    DetectionRiskScore,
    DetectionIncidentId,
    EvidenceChainHash,
}
impl PenaltyColumnV1 {
    pub const ALL: [Self; 12] = [
        Self::PenaltyId,
        Self::TargetAgentId,
        Self::EvidenceHash,
        Self::Reason,
        Self::SlashStake,
        Self::AppealDeadlineTick,
        Self::Status,
        Self::IdentityStatusBefore,
        Self::DetectionSource,
        Self::DetectionRiskScore,
        Self::DetectionIncidentId,
        Self::EvidenceChainHash,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::PenaltyId => "penalty_id",
            Self::TargetAgentId => "target_agent_id",
            Self::EvidenceHash => "evidence_hash",
            Self::Reason => "reason",
            Self::SlashStake => "slash_stake",
            Self::AppealDeadlineTick => "appeal_deadline_tick",
            Self::Status => "status",
            Self::IdentityStatusBefore => "identity_status_before",
            Self::DetectionSource => "detection_source",
            Self::DetectionRiskScore => "detection_risk_score",
            Self::DetectionIncidentId => "detection_incident_id",
            Self::EvidenceChainHash => "evidence_chain_hash",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GovernanceOptionV1 {
    AppealEvidenceHash,
    ResolutionEvidenceHash,
    Appellant,
    AppealReason,
    ResolvedBy,
    ResolutionReason,
    ResolvedAtTick,
}
impl GovernanceOptionV1 {
    pub const ALL: [Self; 7] = [
        Self::AppealEvidenceHash,
        Self::ResolutionEvidenceHash,
        Self::Appellant,
        Self::AppealReason,
        Self::ResolvedBy,
        Self::ResolutionReason,
        Self::ResolvedAtTick,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::AppealEvidenceHash => "appeal_evidence_hash",
            Self::ResolutionEvidenceHash => "resolution_evidence_hash",
            Self::Appellant => "appellant",
            Self::AppealReason => "appeal_reason",
            Self::ResolvedBy => "resolved_by",
            Self::ResolutionReason => "resolution_reason",
            Self::ResolvedAtTick => "resolved_at_tick",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeGovernanceColumnV1<'a> {
    Policy(PolicyColumnV1),
    Epoch(EpochColumnV1),
    Penalty(PenaltyColumnV1),
    SignerCount,
    Signer(u64),
    StakeCount,
    Stake(&'a str),
    OptionPresence(GovernanceOptionV1),
    OptionValue(GovernanceOptionV1),
}
impl RuntimeGovernanceColumnV1<'_> {
    pub fn kind(self) -> RuntimeGovernanceKindV1 {
        match self {
            Self::Policy(_) => RuntimeGovernanceKindV1::Policy,
            Self::Epoch(_)
            | Self::SignerCount
            | Self::Signer(_)
            | Self::StakeCount
            | Self::Stake(_) => RuntimeGovernanceKindV1::EpochSnapshot,
            _ => RuntimeGovernanceKindV1::IdentityPenalty,
        }
    }
    pub(crate) fn scalar(
        kind: RuntimeGovernanceKindV1,
        name: &str,
    ) -> Result<Self, MemberRecordError> {
        match kind {
            RuntimeGovernanceKindV1::Policy => PolicyColumnV1::ALL
                .into_iter()
                .find(|c| c.name() == name)
                .map(Self::Policy),
            RuntimeGovernanceKindV1::EpochSnapshot => EpochColumnV1::ALL
                .into_iter()
                .find(|c| c.name() == name)
                .map(Self::Epoch),
            RuntimeGovernanceKindV1::IdentityPenalty => PenaltyColumnV1::ALL
                .into_iter()
                .find(|c| c.name() == name)
                .map(Self::Penalty),
        }
        .ok_or(MemberRecordError::Invalid("governance scalar column"))
    }
    pub fn codec(self) -> MemberPayloadCodecV1 {
        use RuntimeGovernanceColumnV1 as C;
        match self {
            C::Epoch(EpochColumnV1::ValidatorSetHash | EpochColumnV1::StakeRoot) | C::Signer(_) => {
                MemberPayloadCodecV1::Text
            }
            C::Penalty(
                PenaltyColumnV1::TargetAgentId
                | PenaltyColumnV1::EvidenceHash
                | PenaltyColumnV1::Reason
                | PenaltyColumnV1::DetectionSource
                | PenaltyColumnV1::DetectionIncidentId
                | PenaltyColumnV1::EvidenceChainHash,
            ) => MemberPayloadCodecV1::Text,
            C::Penalty(PenaltyColumnV1::DetectionRiskScore) => MemberPayloadCodecV1::I64,
            C::OptionPresence(_) => MemberPayloadCodecV1::Bool,
            C::OptionValue(GovernanceOptionV1::ResolvedAtTick) => MemberPayloadCodecV1::U64,
            C::OptionValue(_) => MemberPayloadCodecV1::Text,
            _ => MemberPayloadCodecV1::U64,
        }
    }
    pub fn integer_max(self) -> u64 {
        match self {
            Self::Policy(
                PolicyColumnV1::EmergencyBrakeGuardianThreshold
                | PolicyColumnV1::EmergencyVetoGuardianThreshold,
            )
            | Self::Epoch(
                EpochColumnV1::ThresholdBps
                | EpochColumnV1::MinUniqueSigners
                | EpochColumnV1::Threshold,
            ) => u16::MAX as u64,
            Self::Penalty(PenaltyColumnV1::Status) => 3,
            Self::Penalty(PenaltyColumnV1::IdentityStatusBefore) => 2,
            _ => u64::MAX,
        }
    }
    pub(crate) fn nullable(self) -> bool {
        matches!(
            self,
            Self::Signer(_) | Self::Stake(_) | Self::OptionValue(_)
        )
    }
    pub(crate) fn payload(
        self,
        value: RuntimeGovernanceValueV1<'_>,
    ) -> Result<MemberPayloadRefV1<'_>, ValueTreeError> {
        use RuntimeGovernanceValueV1 as V;
        match (value, self.codec()) {
            (V::Unsigned(v), MemberPayloadCodecV1::U64) if v <= self.integer_max() => {
                Ok(MemberPayloadRefV1::U64(v))
            }
            (V::Signed(v), MemberPayloadCodecV1::I64) => Ok(MemberPayloadRefV1::I64(v)),
            (V::Bool(v), MemberPayloadCodecV1::Bool) => Ok(MemberPayloadRefV1::Bool(v)),
            (V::Text(v), MemberPayloadCodecV1::Text) => Ok(MemberPayloadRefV1::Text(v)),
            _ => Err(ValueTreeError::Invalid("governance column payload")),
        }
    }
}
