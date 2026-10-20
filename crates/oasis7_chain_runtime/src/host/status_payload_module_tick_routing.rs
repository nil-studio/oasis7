use std::path::Path;

use serde::Serialize;

#[cfg(not(test))]
use super::super::execution_bridge::load_execution_world;
use super::super::execution_bridge::snapshot_execution_bridge_module_tick_routing_metrics;

#[derive(Debug, Serialize)]
pub(crate) struct ChainModuleTickRoutingStatus {
    pub(crate) available: bool,
    pub(crate) source: String,
    pub(crate) load_error: Option<String>,
    pub(crate) metrics: Option<serde_json::Value>,
    pub(crate) live_metrics: Option<serde_json::Value>,
}

#[cfg(not(test))]
pub(super) fn build_module_tick_routing_status(
    execution_world_dir: &Path,
) -> ChainModuleTickRoutingStatus {
    match load_execution_world(execution_world_dir) {
        Ok(world) => ChainModuleTickRoutingStatus {
            available: true,
            source: "execution_world".to_string(),
            load_error: None,
            metrics: serde_json::to_value(world.module_tick_routing_metrics_snapshot()).ok(),
            live_metrics: snapshot_execution_bridge_module_tick_routing_metrics()
                .and_then(|metrics| serde_json::to_value(metrics).ok()),
        },
        Err(err) => ChainModuleTickRoutingStatus {
            available: false,
            source: "execution_world".to_string(),
            load_error: Some(err),
            metrics: None,
            live_metrics: snapshot_execution_bridge_module_tick_routing_metrics()
                .and_then(|metrics| serde_json::to_value(metrics).ok()),
        },
    }
}

#[cfg(test)]
pub(super) fn build_module_tick_routing_status(
    execution_world_dir: &Path,
) -> ChainModuleTickRoutingStatus {
    let snapshot_path = execution_world_dir.join("snapshot.json");
    match std::fs::read_to_string(snapshot_path.as_path())
        .map_err(|err| err.to_string())
        .and_then(|raw| {
            serde_json::from_str::<serde_json::Value>(&raw).map_err(|err| err.to_string())
        }) {
        Ok(snapshot) => ChainModuleTickRoutingStatus {
            available: true,
            source: "execution_world".to_string(),
            load_error: None,
            metrics: snapshot.get("module_tick_routing_metrics").cloned(),
            live_metrics: snapshot_execution_bridge_module_tick_routing_metrics()
                .and_then(|metrics| serde_json::to_value(metrics).ok()),
        },
        Err(err) => ChainModuleTickRoutingStatus {
            available: false,
            source: "execution_world".to_string(),
            load_error: Some(err),
            metrics: None,
            live_metrics: snapshot_execution_bridge_module_tick_routing_metrics()
                .and_then(|metrics| serde_json::to_value(metrics).ok()),
        },
    }
}
