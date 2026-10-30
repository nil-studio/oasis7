//! Split the Runtime's JSON envelope by actual ownership. Unknown members are
//! rejected, never silently serialized into a generic business-state value.
use super::*;
use serde_json::Value;
const REQUIRED: &[&str] = &[
    "schema_version",
    "cognition_journal",
    "responses",
    "commit_records",
    "receipt_registry",
    "receipt_lineage_registry",
    "feedback_outbox",
    "idempotency_index",
    "staged_actions",
    "scheduler_state",
    "continuations",
    "recovery",
];
const OPTIONAL: &[&str] = &[
    "runtime_binding",
    "retention_state",
    "continuation_contexts",
];

pub(super) fn visit<S: GenesisFieldVisitor>(
    value: &Value,
    sink: &mut S,
) -> Result<(), ProjectionError<S::Error>> {
    let object = value
        .as_object()
        .ok_or(ProjectionError::Invalid("cognition object"))?;
    if object
        .keys()
        .any(|key| !REQUIRED.contains(&key.as_str()) && !OPTIONAL.contains(&key.as_str()))
        || REQUIRED.iter().any(|key| !object.contains_key(*key))
    {
        return Err(ProjectionError::Invalid(
            "unclassified or missing cognition field",
        ));
    }
    if object["schema_version"].as_str() != Some("cognition-persistence.v1") {
        return Err(ProjectionError::Invalid("cognition schema"));
    }
    state_value(
        sink,
        "schema_version",
        Namespace::Artifacts,
        &object["schema_version"],
    )?;
    let journal = object["cognition_journal"]
        .as_object()
        .ok_or(ProjectionError::Invalid("cognition journal object"))?;
    let journal_keys = ["schema_version", "head_seq", "head_digest", "events"];
    if journal.len() != journal_keys.len()
        || journal_keys.iter().any(|key| !journal.contains_key(*key))
        || journal["schema_version"].as_str() != Some("cognition-journal.v1")
        || journal["head_seq"].as_u64().is_none()
        || journal["head_digest"].as_str().is_none()
    {
        return Err(ProjectionError::Invalid(
            "unclassified or invalid cognition journal field",
        ));
    }
    state_value(
        sink,
        "journal.schema_version",
        Namespace::Artifacts,
        &journal["schema_version"],
    )?;
    state_value(
        sink,
        "journal.head_seq",
        Namespace::Allocator,
        &journal["head_seq"],
    )?;
    state_value(
        sink,
        "journal.head_digest",
        Namespace::Allocator,
        &journal["head_digest"],
    )?;
    let events = journal["events"]
        .as_array()
        .ok_or(ProjectionError::Invalid("cognition events shape"))?;
    sink.side(
        descriptor(
            "journal.events",
            FieldDestination::History,
            FieldShape::History,
        ),
        events,
        Some(events.len() as u64),
    )
    .map_err(ProjectionError::Sink)?;
    for (key, destination) in [
        ("responses", FieldDestination::History),
        ("commit_records", FieldDestination::ProofLedger),
        ("receipt_registry", FieldDestination::ProofLedger),
        ("receipt_lineage_registry", FieldDestination::ProofLedger),
    ] {
        let values = object[key]
            .as_array()
            .ok_or(ProjectionError::Invalid("cognition side array shape"))?;
        sink.side(
            descriptor(key, destination, FieldShape::History),
            values,
            Some(values.len() as u64),
        )
        .map_err(ProjectionError::Sink)?;
    }
    for (key, namespace) in [
        ("feedback_outbox", Namespace::Effects),
        ("idempotency_index", Namespace::Authorization),
        ("staged_actions", Namespace::Effects),
    ] {
        state_map(sink, key, namespace, &object[key])?;
    }
    state_value(
        sink,
        "scheduler_state",
        Namespace::Scheduler,
        &object["scheduler_state"],
    )?;
    let continuations = object["continuations"]
        .as_array()
        .ok_or(ProjectionError::Invalid("continuations shape"))?;
    let field = descriptor(
        "continuations",
        FieldDestination::State(Namespace::Effects),
        FieldShape::Ordered,
    );
    oasis7_world_state::visit_iws_field!(sink, field, continuations, ordered)?;
    // These contain current-root/receipt projections. Keep them in the
    // independent proof owner instead of introducing a RootSet self-cycle.
    for key in ["recovery", "runtime_binding"] {
        if let Some(value) = object.get(key) {
            sink.side(
                descriptor(key, FieldDestination::ProofLedger, FieldShape::Value),
                value,
                None,
            )
            .map_err(ProjectionError::Sink)?;
        }
    }
    if let Some(value) = object.get("continuation_contexts") {
        state_map(sink, "continuation_contexts", Namespace::Effects, value)?;
    }
    if let Some(value) = object.get("retention_state") {
        // Terminal entries contain commit_record_id and receipt digests.
        // They belong to the separately protected retention owner, not to
        // the RootSet whose own commit is only formed afterwards.
        sink.side(
            descriptor(
                "retention_state",
                FieldDestination::RetentionLedger,
                FieldShape::Value,
            ),
            value,
            None,
        )
        .map_err(ProjectionError::Sink)?;
    }
    Ok(())
}
fn descriptor(
    name: &'static str,
    destination: FieldDestination,
    shape: FieldShape,
) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Cognition,
        name,
        destination,
        shape,
    }
}
fn state_value<S: GenesisFieldVisitor>(
    sink: &mut S,
    name: &'static str,
    namespace: Namespace,
    value: &Value,
) -> Result<(), ProjectionError<S::Error>> {
    scalar(
        sink,
        descriptor(name, FieldDestination::State(namespace), FieldShape::Value),
        value,
    )
}
fn state_map<S: GenesisFieldVisitor>(
    sink: &mut S,
    name: &'static str,
    namespace: Namespace,
    value: &Value,
) -> Result<(), ProjectionError<S::Error>> {
    let value = value
        .as_object()
        .ok_or(ProjectionError::Invalid("cognition state map shape"))?;
    let field = descriptor(name, FieldDestination::State(namespace), FieldShape::Map);
    oasis7_world_state::visit_iws_field!(sink, field, value, map)
}
