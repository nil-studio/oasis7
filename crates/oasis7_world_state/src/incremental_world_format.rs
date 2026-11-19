//! Explicit new-world format selection. This is configuration scope only:
//! no accepted-head, source attestation, durability or execution authority.
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, RootSetV1},
    incremental_history::GenesisConfigV1,
};
const PROFILE: &[u8] = b"oasis7.iws.complete-world-format.v1\0native97\0runtime46\0raw-owners-v1\0cognition-history-v3\0runtime-history-v1\0";
pub fn complete_world_format_profile_v1() -> [u8; 32] {
    *blake3::hash(PROFILE).as_bytes()
}
/// A tag for the actual content-addressed GenesisConfig, reusable at later
/// heights of this world. It cannot make an arbitrary signed head complete.
#[derive(Debug, Clone, Copy)]
pub struct CompleteWorldFormatScopeV1 {
    genesis: [u8; 32],
    world: [u8; 32],
}
impl CompleteWorldFormatScopeV1 {
    pub fn validate(&self, roots: &RootSetV1) -> Result<(), ExecutionError> {
        roots.validate()?;
        if roots.genesis_config != self.genesis
            || blake3::hash(roots.world_id.as_bytes()).as_bytes() != &self.world
        {
            return Err(ExecutionError::Invalid("complete world format scope"));
        }
        Ok(())
    }
}
pub fn select_complete_world_format_v1(
    config: &GenesisConfigV1,
    roots: &RootSetV1,
) -> Result<CompleteWorldFormatScopeV1, ExecutionError> {
    roots.validate()?;
    if config.format_profile != complete_world_format_profile_v1()
        || config.world_id != roots.world_id
    {
        return Err(ExecutionError::Invalid(
            "complete world format profile/world",
        ));
    }
    let genesis = config.digest()?;
    if genesis != roots.genesis_config {
        return Err(ExecutionError::Invalid(
            "complete world format original config",
        ));
    }
    Ok(CompleteWorldFormatScopeV1 {
        genesis,
        world: *blake3::hash(config.world_id.as_bytes()).as_bytes(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_format_is_bound_to_original_configuration_and_never_implicit_on_old_profiles() {
        let config = GenesisConfigV1 {
            world_id: "explicit-new-world".into(),
            format_profile: complete_world_format_profile_v1(),
            initial_content: [2; 32],
            activation_config: [3; 32],
            execution_environment: [4; 32],
        };
        let roots = RootSetV1::empty(config.world_id.clone(), config.digest().unwrap()).unwrap();
        let tag = select_complete_world_format_v1(&config, &roots).unwrap();
        let mut later = roots.clone();
        later.height = 7;
        later.simulation_tick = 23;
        tag.validate(&later).unwrap();
        let mut other = roots.clone();
        other.world_id = "other-world".into();
        assert!(tag.validate(&other).is_err());
        let mut altered = config.clone();
        altered.execution_environment = [5; 32];
        assert!(select_complete_world_format_v1(&altered, &roots).is_err());
        altered = config.clone();
        altered.format_profile = [1; 32];
        let old = RootSetV1::empty(altered.world_id.clone(), altered.digest().unwrap()).unwrap();
        assert!(select_complete_world_format_v1(&altered, &old).is_err());
        assert!(tag.validate(&old).is_err());
    }
}
