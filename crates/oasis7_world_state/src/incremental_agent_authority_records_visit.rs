use super::*;
use crate::incremental_intent_fields::{IntentScalarRefV1, visit_intent_values_v1};
pub(super) fn record<'a, E: From<MemberRecordError>>(
    record: AgentAuthorityRecordRefV1<'a>,
    emit: &mut impl FnMut(AgentAuthorityCellV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use AgentAuthorityColumnV1 as C;
    use MemberPayloadRefV1 as V;
    match record {
        AgentAuthorityRecordRefV1::IntentLedger(value) => {
            visit_intent_values_v1(value, |field, value| {
                emit(
                    AgentAuthorityCellV1::Column(C::Intent(field)),
                    match value {
                        IntentScalarRefV1::U32(v) => V::U64(u64::from(v)),
                        IntentScalarRefV1::U64(v) => V::U64(v),
                        IntentScalarRefV1::Text(v) => V::Text(v),
                        IntentScalarRefV1::OptionalText(v) => v.map_or(V::Unit, V::Text),
                        IntentScalarRefV1::OptionalU64(v) => v.map_or(V::Unit, V::U64),
                    },
                )
            })
        }
        AgentAuthorityRecordRefV1::AgentLocation(AgentLocationAuthorityV1 {
            agent_id,
            location_id,
            active,
            authority_revision,
            effective_at,
        }) => {
            for (column, value) in [
                (C::AgentId, V::Text(agent_id)),
                (C::LocationId, V::Text(location_id)),
                (C::Active, V::Bool(*active)),
                (C::AuthorityRevision, V::U64(*authority_revision)),
                (C::EffectiveAt, V::U64(*effective_at)),
            ] {
                emit(AgentAuthorityCellV1::Column(column), value)?;
            }
            Ok(())
        }
        AgentAuthorityRecordRefV1::LocationAnchor(LocationAnchorV1 {
            location_id,
            active,
            authority_revision,
            effective_at,
        }) => {
            for (column, value) in [
                (C::LocationId, V::Text(location_id)),
                (C::Active, V::Bool(*active)),
                (C::AuthorityRevision, V::U64(*authority_revision)),
                (C::EffectiveAt, V::U64(*effective_at)),
            ] {
                emit(AgentAuthorityCellV1::Column(column), value)?;
            }
            Ok(())
        }
    }
}
