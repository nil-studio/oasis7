use super::WorldState;
use super::module_admission_freeze::projection_has_freeze_field;
use super::module_instance_transition::PreparedModuleInstance;
use serde::ser::SerializeStruct;

pub(super) fn base_field_count(
    base: usize,
    state: &WorldState,
    module_instance_overlay: Option<&PreparedModuleInstance>,
) -> usize {
    base + usize::from(state.retained_history_root.is_some())
        + usize::from(projection_has_freeze_field(state, module_instance_overlay))
}

pub(super) fn serialize_prefix<S>(output: &mut S, state: &WorldState) -> Result<(), S::Error>
where
    S: SerializeStruct,
{
    output.serialize_field("time", &state.time)?;
    if let Some(root) = state.retained_history_root.as_ref() {
        output.serialize_field("retained_history_root", root)?;
    }
    Ok(())
}
