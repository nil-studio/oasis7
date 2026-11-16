use super::WorldState;
use super::module_admission_freeze::projection_has_freeze_field;
use super::module_instance_transition::PreparedModuleInstance;
use serde::ser::SerializeStruct;

pub fn base_field_count(
    base: usize,
    state: &WorldState,
    module_instance_overlay: Option<&PreparedModuleInstance>,
) -> usize {
    base + usize::from(state.retained_history_root.is_some())
        + usize::from(projection_has_freeze_field(state, module_instance_overlay))
}

pub fn serialize_prefix<S>(output: &mut S, state: &WorldState) -> Result<(), S::Error>
where
    S: SerializeStruct,
{
    output.serialize_field(ORIGINAL_RETENTION_FIELD_SLOTS_V1[0].name, &state.time)?;
    if let Some(root) = state.retained_history_root.as_ref() {
        output.serialize_field(ORIGINAL_RETENTION_FIELD_SLOTS_V1[1].name, root)?;
    }
    Ok(())
}

pub(crate) const ORIGINAL_RETENTION_FIELD_SLOTS_V1:[crate::state::projection::OriginalStateFieldSlotV1;2]=[
 crate::state::projection::OriginalStateFieldSlotV1{name:"time",emission:crate::state::projection::OriginalStateFieldEmissionV1::Always},
 crate::state::projection::OriginalStateFieldSlotV1{name:"retained_history_root",emission:crate::state::projection::OriginalStateFieldEmissionV1::Some},
];
