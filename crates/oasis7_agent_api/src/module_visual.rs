//! Pure decoding of module visual emit contracts. State ownership and anchor admission stay in Runtime.
use crate::world_entity::{ModuleVisualAnchor, ModuleVisualEntity};
use oasis7_wasm_abi::ModuleEmitEvent;
use serde::Deserialize;
const MAX_IDENTIFIER_BYTES: usize = 128;
const MAX_EXPECTED_VALUE_BYTES: usize = 512;
const MODULE_VISUAL_ENTITY_UPSERTED_KIND: &str = "module_visual_entity_upserted";
const MODULE_VISUAL_ENTITY_REMOVED_KIND: &str = "module_visual_entity_removed";
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleVisualMutation {
    Upsert(ModuleVisualEntity),
    Remove(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModuleVisualEntityUpsertPayload {
    entity: StrictModuleVisualEntity,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictModuleVisualEntity {
    entity_id: String,
    module_id: String,
    kind: String,
    #[serde(default)]
    label: Option<String>,
    anchor: ModuleVisualAnchor,
}

impl StrictModuleVisualEntity {
    fn into_entity(self) -> ModuleVisualEntity {
        ModuleVisualEntity {
            entity_id: self.entity_id,
            module_id: self.module_id,
            kind: self.kind,
            label: self.label,
            anchor: self.anchor,
        }
        .sanitized()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModuleVisualEntityRemovedPayload {
    entity_id: String,
}

pub fn parse_module_visual_emit(
    event: &ModuleEmitEvent,
) -> Result<Option<ModuleVisualMutation>, String> {
    let kind = match event.kind.as_str() {
        MODULE_VISUAL_ENTITY_UPSERTED_KIND | "ModuleVisualEntityUpserted" => {
            MODULE_VISUAL_ENTITY_UPSERTED_KIND
        }
        MODULE_VISUAL_ENTITY_REMOVED_KIND | "ModuleVisualEntityRemoved" => {
            MODULE_VISUAL_ENTITY_REMOVED_KIND
        }
        _ => return Ok(None),
    };
    validate_bounded_non_empty(
        event.module_id.as_str(),
        "module_id",
        MAX_IDENTIFIER_BYTES,
        kind,
    )?;
    let module_id = event.module_id.trim();
    let mutation = if kind == MODULE_VISUAL_ENTITY_UPSERTED_KIND {
        let payload =
            serde_json::from_value::<ModuleVisualEntityUpsertPayload>(event.payload.clone())
                .map_err(|error| invalid_payload(kind, error))?;
        validate_bounded_non_empty(
            payload.entity.entity_id.as_str(),
            "entity_id",
            MAX_IDENTIFIER_BYTES,
            kind,
        )?;
        validate_bounded_non_empty(
            payload.entity.module_id.as_str(),
            "module_id",
            MAX_IDENTIFIER_BYTES,
            kind,
        )?;
        validate_bounded_text(
            payload.entity.kind.as_str(),
            "kind",
            MAX_IDENTIFIER_BYTES,
            kind,
        )?;
        if let Some(label) = payload.entity.label.as_deref() {
            validate_bounded_text(label, "label", MAX_EXPECTED_VALUE_BYTES, kind)?;
        }
        let entity = payload.entity.into_entity();
        if entity.module_id != module_id {
            return Err(invalid_payload(
                kind,
                format!(
                    "entity module_id {} does not match emitter module_id {}",
                    entity.module_id, module_id
                ),
            ));
        }
        ModuleVisualMutation::Upsert(entity)
    } else {
        let payload =
            serde_json::from_value::<ModuleVisualEntityRemovedPayload>(event.payload.clone())
                .map_err(|error| invalid_payload(kind, error))?;
        ModuleVisualMutation::Remove(non_empty(payload.entity_id.as_str(), "entity_id")?)
    };
    Ok(Some(mutation))
}

fn non_empty(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid_payload(field, format!("{field} is required")));
    }
    Ok(value.to_string())
}

fn validate_bounded_non_empty(
    value: &str,
    field: &str,
    max_bytes: usize,
    kind: &str,
) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(invalid_payload(kind, format!("{field} is required")));
    }
    validate_bounded_text(value, field, max_bytes, kind)
}

fn validate_bounded_text(
    value: &str,
    field: &str,
    max_bytes: usize,
    kind: &str,
) -> Result<(), String> {
    if value.len() > max_bytes {
        return Err(invalid_payload(
            kind,
            format!("{field} exceeds maximum of {max_bytes} bytes"),
        ));
    }
    Ok(())
}

fn invalid_payload(kind: &str, detail: impl std::fmt::Display) -> String {
    format!("invalid {kind} module visual payload: {detail}")
}
