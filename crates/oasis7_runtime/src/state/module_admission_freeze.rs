use super::*;
use serde::ser::SerializeStruct;

/// Immutable rollback disposition for new admissions of one exact artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleAdmissionFreeze {
    pub module_id: String,
    pub module_version: String,
    pub wasm_hash: String,
    pub rollback_proposal_id: ProposalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_release_request_id: Option<u64>,
    pub reason: String,
}

impl ModuleAdmissionFreeze {
    pub(crate) fn key(module_id: &str, version: &str, wasm_hash: &str) -> String {
        serde_json::to_string(&(module_id, version, wasm_hash))
            .expect("string tuple serialization is infallible")
    }
}

pub(super) fn projection_has_freeze_field(
    state: &WorldState,
    module_instance_overlay: Option<&module_instance_transition::PreparedModuleInstance>,
) -> bool {
    !state.module_admission_freezes.is_empty()
        || module_instance_overlay.is_some_and(|overlay| overlay.has_admission_freeze())
}

pub(super) fn serialize_module_instance_projection<S: SerializeStruct>(
    state: &WorldState,
    module_instance_overlay: Option<&module_instance_transition::PreparedModuleInstance>,
    output: &mut S,
) -> Result<(), S::Error> {
    if let Some(overlay) = module_instance_overlay {
        overlay.serialize_fields(state, output)?;
    } else {
        output.serialize_field("module_instances", &state.module_instances)?;
        if !state.module_admission_freezes.is_empty() {
            output.serialize_field("module_admission_freezes", &state.module_admission_freezes)?;
        }
    }
    Ok(())
}
