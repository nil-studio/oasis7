//! Original projection encoding and an independent, narrow observation schema.
//!
//! HTTP turns numeric map keys into strings. Restore only the two numeric-keyed
//! maps in the published state allowlist, never numeric-looking Agent/site IDs.
//! No authoritative Runtime state is constructed during decoding or verification.
use crate::formal_read::FormalReadProjection;
use oasis7_client_api::{
    world_read_observation::WorldReadObservationV1,
    world_service::{ReadWorldViewRequest, ReadWorldViewResponse},
    world_service_read::WorldServiceProjectionMetadataV1,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_cbor::Value;

/// Unverified projection bytes plus read values. Authority is supplied only by
/// `VerifiedWorldView` and its independently checked transport/proof provenance.
#[derive(Debug, Clone)]
pub struct WorldReadProjectionV1 {
    original: Value,
    observation: WorldReadObservationV1,
}
impl WorldReadProjectionV1 {
    pub fn from_serializable(projection: &impl Serialize) -> Result<Self, String> {
        Self::from_original(serde_cbor::value::to_value(projection).map_err(|e| e.to_string())?)
    }
    fn from_original(original: Value) -> Result<Self, String> {
        let observation = serde_cbor::value::from_value(original.clone())
            .map_err(|error| format!("World read observation: {error}"))?;
        Ok(Self {
            original,
            observation,
        })
    }
    pub fn observation(&self) -> &WorldReadObservationV1 {
        &self.observation
    }
}
impl Serialize for WorldReadProjectionV1 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // The parsed observation is NEVER substituted into signed bytes or a
        // projection root. Its intentionally narrower shape cannot replace P.
        self.original.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for WorldReadProjectionV1 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let human_readable = deserializer.is_human_readable();
        let mut original = Value::deserialize(deserializer)?;
        if human_readable {
            restore_published_numeric_map_keys(&mut original).map_err(serde::de::Error::custom)?;
        }
        Self::from_original(original).map_err(serde::de::Error::custom)
    }
}
impl FormalReadProjection for WorldReadProjectionV1 {
    fn read_metadata(&self) -> Result<WorldServiceProjectionMetadataV1, String> {
        Ok(self.observation.metadata())
    }
    fn validate_contents(
        response: &ReadWorldViewResponse<Self>,
        request: &ReadWorldViewRequest,
    ) -> Result<(), String> {
        response
            .validate(request)
            .map_err(|error| error.to_string())?;
        response.view.observation.validate_bound_contents(
            &response.version,
            response.logical_tick,
            request,
        )
    }
}

fn restore_published_numeric_map_keys(projection: &mut Value) -> Result<(), String> {
    let Value::Map(projection) = projection else {
        return Err("World read projection must be a map".into());
    };
    let Some(Value::Map(state)) = projection.get_mut(&Value::Text("state".into())) else {
        return Err("World read state must be a map".into());
    };
    for name in ["pending_factory_builds", "pending_material_transits"] {
        let Some(value) = state.get_mut(&Value::Text(name.into())) else {
            continue;
        };
        let Value::Map(entries) = value else {
            return Err(format!("World read {name} must be a map"));
        };
        let mut restored = std::collections::BTreeMap::new();
        for (key, value) in std::mem::take(entries) {
            let id = match key {
                Value::Text(key) => key
                    .parse::<u64>()
                    .map_err(|_| format!("World read {name} action key is invalid"))?,
                Value::Integer(key) => u64::try_from(key)
                    .map_err(|_| format!("World read {name} action key is invalid"))?,
                _ => return Err(format!("World read {name} action key is invalid")),
            };
            if restored.insert(Value::Integer(id.into()), value).is_some() {
                return Err(format!("World read {name} has ambiguous action keys"));
            }
        }
        *entries = restored;
    }
    Ok(())
}

/// Concrete backend-independent reader of the established projection encoding.
pub type WorldReadClient =
    crate::remote_world_service::RemoteWorldServiceReader<WorldReadProjectionV1>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_declared_action_maps_restore_numeric_keys() {
        let mut projection = serde_cbor::value::to_value(serde_json::json!({"state":{
            "pending_factory_builds":{"7":{"job_id":7}},
            "pending_material_transits":{"18446744073709551615":{"job_id":7}},
            "governance_votes":{"7":{"votes_by_agent":{"8":{}}}},
            "agents":{"7":{}},"unknown":{"7":{}}
        }}))
        .unwrap();
        restore_published_numeric_map_keys(&mut projection).unwrap();
        let Value::Map(root) = projection else {
            panic!("map")
        };
        let Value::Map(state) = &root[&Value::Text("state".into())] else {
            panic!("state")
        };
        for field in ["governance_votes", "agents", "unknown"] {
            let Value::Map(entries) = &state[&Value::Text(field.into())] else {
                panic!("field")
            };
            assert!(entries.contains_key(&Value::Text("7".into())));
        }
        let Value::Map(builds) = &state[&Value::Text("pending_factory_builds".into())] else {
            panic!("builds")
        };
        assert!(builds.contains_key(&Value::Integer(7)));
        let Value::Map(transits) = &state[&Value::Text("pending_material_transits".into())] else {
            panic!("transits")
        };
        assert!(transits.contains_key(&Value::Integer(u64::MAX.into())));
    }
    #[test]
    fn ambiguous_numeric_aliases_are_not_silently_dropped() {
        let mut projection = serde_cbor::value::to_value(serde_json::json!({"state":{
            "pending_factory_builds":{"7":{},"07":{}}
        }}))
        .unwrap();
        assert!(restore_published_numeric_map_keys(&mut projection).is_err());
    }
}
