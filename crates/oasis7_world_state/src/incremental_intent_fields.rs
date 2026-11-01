//! Original intent fields shared by AgentCell and the durable World ledger.
//! This borrow-only projection has no owner key, wire layout or permission policy.
use crate::agent_cell::AgentIntentV2;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentFieldV1 {
    SchemaVersion,
    AgentId,
    IntentId,
    Kind,
    Summary,
    TargetId,
    EffectIntentId,
    IntentTick,
    WorldId,
    ReorgEpoch,
    AuthorityScope,
    Status,
    Source,
    LogicalTime,
    EventSeq,
    UpdatedAt,
    ReceiptRef,
    ReasonCode,
    ReasonSummary,
    ReplacedBy,
    ActorId,
    RequestDigest,
}
impl IntentFieldV1 {
    pub const ALL: [Self; 22] = [
        Self::SchemaVersion,
        Self::AgentId,
        Self::IntentId,
        Self::Kind,
        Self::Summary,
        Self::TargetId,
        Self::EffectIntentId,
        Self::IntentTick,
        Self::WorldId,
        Self::ReorgEpoch,
        Self::AuthorityScope,
        Self::Status,
        Self::Source,
        Self::LogicalTime,
        Self::EventSeq,
        Self::UpdatedAt,
        Self::ReceiptRef,
        Self::ReasonCode,
        Self::ReasonSummary,
        Self::ReplacedBy,
        Self::ActorId,
        Self::RequestDigest,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::SchemaVersion => "schema_version",
            Self::AgentId => "agent_id",
            Self::IntentId => "intent_id",
            Self::Kind => "kind",
            Self::Summary => "summary",
            Self::TargetId => "target_id",
            Self::EffectIntentId => "effect_intent_id",
            Self::IntentTick => "intent_tick",
            Self::WorldId => "world_id",
            Self::ReorgEpoch => "reorg_epoch",
            Self::AuthorityScope => "authority_scope",
            Self::Status => "status",
            Self::Source => "source",
            Self::LogicalTime => "logical_time",
            Self::EventSeq => "event_seq",
            Self::UpdatedAt => "updated_at",
            Self::ReceiptRef => "receipt_ref",
            Self::ReasonCode => "reason_code",
            Self::ReasonSummary => "reason_summary",
            Self::ReplacedBy => "replaced_by",
            Self::ActorId => "actor_id",
            Self::RequestDigest => "request_digest",
        }
    }
    pub fn is_optional(self) -> bool {
        matches!(
            self,
            Self::TargetId
                | Self::EffectIntentId
                | Self::IntentTick
                | Self::WorldId
                | Self::ReorgEpoch
                | Self::AuthorityScope
                | Self::ReceiptRef
                | Self::ReasonCode
                | Self::ReasonSummary
                | Self::ReplacedBy
        )
    }
}
#[derive(Clone, Copy)]
pub(crate) enum IntentScalarRefV1<'a> {
    U32(u32),
    U64(u64),
    Text(&'a str),
    OptionalText(Option<&'a str>),
    OptionalU64(Option<u64>),
}
pub(crate) fn visit_intent_values_v1<'a, E>(
    intent: &'a AgentIntentV2,
    mut emit: impl FnMut(IntentFieldV1, IntentScalarRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let AgentIntentV2 {
        schema_version,
        agent_id,
        intent_id,
        kind,
        summary,
        target_id,
        effect_intent_id,
        intent_tick,
        world_id,
        reorg_epoch,
        authority_scope,
        status,
        source,
        logical_time,
        event_seq,
        updated_at,
        receipt_ref,
        reason_code,
        reason_summary,
        replaced_by,
        actor_id,
        request_digest,
    } = intent;
    emit(
        IntentFieldV1::SchemaVersion,
        IntentScalarRefV1::U32(*schema_version),
    )?;
    emit(IntentFieldV1::AgentId, IntentScalarRefV1::Text(agent_id))?;
    emit(IntentFieldV1::IntentId, IntentScalarRefV1::Text(intent_id))?;
    emit(IntentFieldV1::Kind, IntentScalarRefV1::Text(kind))?;
    emit(IntentFieldV1::Summary, IntentScalarRefV1::Text(summary))?;
    emit(
        IntentFieldV1::TargetId,
        IntentScalarRefV1::OptionalText(target_id.as_deref()),
    )?;
    emit(
        IntentFieldV1::EffectIntentId,
        IntentScalarRefV1::OptionalText(effect_intent_id.as_deref()),
    )?;
    emit(
        IntentFieldV1::IntentTick,
        IntentScalarRefV1::OptionalU64(*intent_tick),
    )?;
    emit(
        IntentFieldV1::WorldId,
        IntentScalarRefV1::OptionalText(world_id.as_deref()),
    )?;
    emit(
        IntentFieldV1::ReorgEpoch,
        IntentScalarRefV1::OptionalU64(*reorg_epoch),
    )?;
    emit(
        IntentFieldV1::AuthorityScope,
        IntentScalarRefV1::OptionalText(authority_scope.as_deref()),
    )?;
    emit(IntentFieldV1::Status, IntentScalarRefV1::Text(status))?;
    emit(IntentFieldV1::Source, IntentScalarRefV1::Text(source))?;
    emit(
        IntentFieldV1::LogicalTime,
        IntentScalarRefV1::U64(*logical_time),
    )?;
    emit(IntentFieldV1::EventSeq, IntentScalarRefV1::U64(*event_seq))?;
    emit(
        IntentFieldV1::UpdatedAt,
        IntentScalarRefV1::U64(*updated_at),
    )?;
    emit(
        IntentFieldV1::ReceiptRef,
        IntentScalarRefV1::OptionalText(receipt_ref.as_deref()),
    )?;
    emit(
        IntentFieldV1::ReasonCode,
        IntentScalarRefV1::OptionalText(reason_code.as_deref()),
    )?;
    emit(
        IntentFieldV1::ReasonSummary,
        IntentScalarRefV1::OptionalText(reason_summary.as_deref()),
    )?;
    emit(
        IntentFieldV1::ReplacedBy,
        IntentScalarRefV1::OptionalText(replaced_by.as_deref()),
    )?;
    emit(IntentFieldV1::ActorId, IntentScalarRefV1::Text(actor_id))?;
    emit(
        IntentFieldV1::RequestDigest,
        IntentScalarRefV1::Text(request_digest),
    )?;
    Ok(())
}
