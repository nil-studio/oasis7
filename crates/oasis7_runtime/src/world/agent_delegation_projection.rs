//! Borrowed authorization records and separately owned causal receipts.
use super::Ledger;
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
use oasis7_world_state::incremental_projection::{
    FieldDescriptor, FieldDestination, FieldOwner, FieldShape, GenesisFieldVisitor, ProjectionError,
};
use serde_json::Value;
const KEYS: &[&str] = &["grants", "spent", "decisions", "receipts", "controls"];
const _: fn(&Ledger) = |ledger| {
    let Ledger {
        grants: _,
        spent: _,
        decisions: _,
        receipts: _,
        controls: _,
    } = ledger;
};
fn descriptor(name: &'static str, destination: FieldDestination) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Cognition,
        name,
        destination,
        shape: FieldShape::Map,
    }
}

pub(in crate::world) fn visit<S: GenesisFieldVisitor>(
    value: &Value,
    sink: &mut S,
) -> Result<(), ProjectionError<S::Error>> {
    let object = value
        .as_object()
        .ok_or(ProjectionError::Invalid("delegation object"))?;
    if object.len() != KEYS.len() || object.keys().any(|key| !KEYS.contains(&key.as_str())) {
        return Err(ProjectionError::Invalid(
            "unclassified or missing delegation field",
        ));
    }
    for (key, name) in [
        ("grants", "delegation.grants"),
        ("spent", "delegation.spent"),
        ("decisions", "delegation.decisions"),
        ("controls", "delegation.controls"),
    ] {
        let values = object[key]
            .as_object()
            .ok_or(ProjectionError::Invalid("delegation map shape"))?;
        let field = descriptor(
            name,
            FieldDestination::State(StateNamespaceV1::Authorization),
        );
        sink.begin(field, Some(values.len()))
            .map_err(ProjectionError::Sink)?;
        for (id, value) in values {
            if (key == "spent" && value.as_u64().is_none())
                || (key != "spent" && !value.is_object())
            {
                return Err(ProjectionError::Invalid("delegation row shape"));
            }
            // Preserve original marker/receipt and world-event identities. They
            // are not silently rewritten to an IWS CommitBody content hash.
            sink.entry(field, id, value)
                .map_err(ProjectionError::Sink)?;
        }
        sink.end(field).map_err(ProjectionError::Sink)?;
    }
    let receipts = object["receipts"]
        .as_object()
        .ok_or(ProjectionError::Invalid("delegation receipt map shape"))?;
    sink.side(
        descriptor("delegation.receipts", FieldDestination::ProofLedger),
        receipts,
        Some(receipts.len() as u64),
    )
    .map_err(ProjectionError::Sink)
}
