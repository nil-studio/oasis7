use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Group {
    Base,
    Resource,
    Slot,
    Cargo,
    Activity,
    Intent,
}
pub(super) fn group(field: AgentFieldV1) -> Group {
    use AgentFieldV1::*;
    match field {
        Identity | PositionX | PositionY | PositionZ | BodyKind | BodyHeight | FrameKind
        | SlotCapacity | ExpansionLevel | SlotCount | SlotNext | CargoCount | CargoNext
        | ActivityPresent | IntentPresent => Group::Base,
        ResourceAmount => Group::Resource,
        SlotId | SlotType | SlotInstalled | SlotLocked => Group::Slot,
        CargoId | CargoKind | CargoQuantity | CargoUnitSize => Group::Cargo,
        ActivityStatus
        | ActivityUpdated
        | ActivityOperationKind
        | ActivityOperationId
        | ActivityTarget
        | ActivityReasonCode
        | ActivityReasonSummary => Group::Activity,
        IntentSchema | IntentAgent | IntentId | IntentKind | IntentSummary | IntentTarget
        | IntentEffect | IntentTick | IntentWorld | IntentReorg | IntentAuthorityScope
        | IntentStatus | IntentSource | IntentLogicalTime | IntentEventSeq | IntentUpdated
        | IntentReceipt | IntentReasonCode | IntentReasonSummary | IntentReplacedBy
        | IntentActor | IntentRequestDigest => Group::Intent,
    }
}
pub(super) fn bit(field: AgentFieldV1) -> u64 {
    1u64 << ((field as u16) - 1)
}
pub(super) fn required(activity: bool, intent: bool) -> u64 {
    AgentFieldV1::ALL
        .iter()
        .copied()
        .filter(|f| {
            matches!(group(*f), Group::Base)
                || activity && group(*f) == Group::Activity
                || intent && group(*f) == Group::Intent
        })
        .fold(0, |mask, f| mask | bit(f))
}
