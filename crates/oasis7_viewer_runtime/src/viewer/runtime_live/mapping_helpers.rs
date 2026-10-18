use crate::runtime::WorldEventBody as RuntimeWorldEventBody;

pub(in crate::viewer::runtime_live) fn runtime_event_kind_label(
    body: &RuntimeWorldEventBody,
) -> (String, Option<String>) {
    let label = match body {
        RuntimeWorldEventBody::Domain(_) => "domain",
        RuntimeWorldEventBody::EffectQueued(_) => "effect_queued",
        RuntimeWorldEventBody::ReceiptAppended(_) => "receipt_appended",
        RuntimeWorldEventBody::CapabilityAuthorization(_) => "capability_authorization",
        RuntimeWorldEventBody::PolicyDecisionRecorded(_) => "policy_decision_recorded",
        RuntimeWorldEventBody::RuleDecisionRecorded(_) => "rule_decision_recorded",
        RuntimeWorldEventBody::ActionOverridden(_) => "action_overridden",
        RuntimeWorldEventBody::Governance(_) => "governance",
        RuntimeWorldEventBody::ModuleEvent(_) => "module_event",
        RuntimeWorldEventBody::ModuleCallFailed(_) => "module_call_failed",
        RuntimeWorldEventBody::ModuleEmitted(_) => "module_emitted",
        RuntimeWorldEventBody::ModuleStateUpdated(_) => "module_state_updated",
        RuntimeWorldEventBody::ModuleRuntimeCharged(_) => "module_runtime_charged",
        RuntimeWorldEventBody::ProductValidationDeliveryCursorUpdated(_) => {
            "product_validation_delivery_cursor_updated"
        }
        RuntimeWorldEventBody::SnapshotCreated(_) => "snapshot_created",
        RuntimeWorldEventBody::ManifestUpdated(_) => "manifest_updated",
        RuntimeWorldEventBody::RollbackApplied(_) => "rollback_applied",
    };
    (label.to_string(), None)
}
