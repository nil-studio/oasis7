//! Workers implementation of dev-test-accounts-v1. World authority is remote.
#![forbid(unsafe_code)]
mod config;
mod platform;
mod protocol;
mod sessions;
mod state;
mod store;

mod cache;
mod native;

mod operations;

#[cfg(test)]
mod config_tests;
#[cfg(test)]
mod protocol_tests;

/// Finalize only empty resource pins in a trusted operator policy, then validate
/// the complete closed configuration. Contains Secret references, never values.
/// This pure Rust helper is not a Worker/MCP export.
pub fn prepare_operator_policy(raw: &str) -> Result<String, &'static str> {
    let mut value: serde_json::Value =
        oasis7_agent_api::game::parse_bounded_json(raw.as_bytes(), 60 * 1024)
            .map_err(|_| "configuration_unavailable")?;
    let resources = value["resources"]
        .as_object()
        .ok_or("configuration_unavailable")?;
    let mut pins = std::collections::BTreeMap::new();
    for (name, resource) in resources {
        let typed: config::Resource =
            serde_json::from_value(resource.clone()).map_err(|_| "configuration_unavailable")?;
        pins.insert(
            name.clone(),
            blake3::hash(&serde_json::to_vec(&typed).map_err(|_| "configuration_unavailable")?)
                .to_hex()
                .to_string(),
        );
    }
    for grant in value["grants"]
        .as_array_mut()
        .ok_or("configuration_unavailable")?
    {
        let resource = grant["binding"]["resource"]
            .as_str()
            .ok_or("configuration_unavailable")?;
        let pin = pins.get(resource).ok_or("configuration_unavailable")?;
        let old = grant["binding"]["executor_config_digest"]
            .as_str()
            .ok_or("configuration_unavailable")?;
        if !old.is_empty() && old != pin {
            return Err("resource_configuration_changed_new_session_required");
        }
        grant["binding"]["executor_config_digest"] = serde_json::json!(pin);
    }
    let normalized = serde_json::to_string(&value).map_err(|_| "configuration_unavailable")?;
    config::Config::parse(&normalized)?;
    Ok(normalized)
}
