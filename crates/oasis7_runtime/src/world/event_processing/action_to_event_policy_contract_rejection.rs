use super::*;

pub(super) fn economic_contract_rule_denied(
    action_id: ActionId,
    note: impl Into<String>,
) -> WorldEventBody {
    WorldEventBody::Domain(DomainEvent::ActionRejected {
        action_id,
        reason: RejectReason::RuleDenied {
            notes: vec![note.into()],
        },
    })
}
