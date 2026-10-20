use std::fs;
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, TcpListener, UdpSocket};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::RuntimePaths;

#[derive(Debug, Clone, Deserialize)]
struct ExecutionRecordLatest {
    world_id: String,
    height: u64,
    #[serde(default)]
    node_block_hash: Option<String>,
    execution_block_hash: String,
    execution_state_root: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct NodePosStateSnapshot {
    next_height: u64,
    next_slot: u64,
    #[serde(default)]
    last_observed_slot: u64,
    #[serde(default)]
    missed_slot_count: u64,
    #[serde(default)]
    last_observed_tick: u64,
    #[serde(default)]
    missed_tick_count: u64,
    committed_height: u64,
    network_committed_height: u64,
    last_broadcast_proposal_height: u64,
    last_broadcast_local_attestation_height: u64,
    last_broadcast_committed_height: u64,
    #[serde(default)]
    last_committed_block_hash: Option<String>,
    #[serde(default)]
    last_execution_height: u64,
    #[serde(default)]
    last_execution_block_hash: Option<String>,
    #[serde(default)]
    last_execution_state_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StartupReconcileReport {
    pub(super) previous_committed_height: u64,
    pub(super) reconciled_height: u64,
}

pub(super) fn reserve_replication_listen_addr(
    listen_addr: &str,
    tcp_listeners: &mut Vec<TcpListener>,
    udp_sockets: &mut Vec<UdpSocket>,
) -> Result<(), String> {
    let segments = listen_addr
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let Some(ip_index) = segments
        .iter()
        .position(|segment| *segment == "ip4" || *segment == "ip6")
    else {
        return Err(format!(
            "startup reconcile preflight failed: unsupported replication listen address {listen_addr}"
        ));
    };
    let Some(host) = segments.get(ip_index + 1) else {
        return Err(format!(
            "startup reconcile preflight failed: unsupported replication listen address {listen_addr}"
        ));
    };
    let ip = host.parse::<IpAddr>().map_err(|err| {
        format!(
            "startup reconcile preflight failed: unsupported replication listen address {listen_addr}: {err}"
        )
    })?;
    if let Some(tcp_index) = segments.iter().position(|segment| *segment == "tcp") {
        let Some(port) = segments
            .get(tcp_index + 1)
            .and_then(|raw| raw.parse::<u16>().ok())
        else {
            return Err(format!(
                "startup reconcile preflight failed: unsupported replication tcp listen {listen_addr}"
            ));
        };
        tcp_listeners
            .push(TcpListener::bind(SocketAddr::new(ip, port)).map_err(|err| {
                format!(
                    "startup reconcile preflight failed: replication tcp listen {listen_addr} unavailable: {err}"
                )
            })?);
        return Ok(());
    }
    if let Some(udp_index) = segments.iter().position(|segment| *segment == "udp") {
        let Some(port) = segments
            .get(udp_index + 1)
            .and_then(|raw| raw.parse::<u16>().ok())
        else {
            return Err(format!(
                "startup reconcile preflight failed: unsupported replication udp listen {listen_addr}"
            ));
        };
        udp_sockets.push(UdpSocket::bind(SocketAddr::new(ip, port)).map_err(|err| {
            format!(
                "startup reconcile preflight failed: replication udp listen {listen_addr} unavailable: {err}"
            )
        })?);
        return Ok(());
    }
    Err(format!(
        "startup reconcile preflight failed: unsupported replication listen address {listen_addr}"
    ))
}

pub(super) fn reconcile_startup_state_from_execution_latest(
    paths: &RuntimePaths,
    world_id: &str,
) -> Result<Option<StartupReconcileReport>, String> {
    reconcile_startup_state_from_execution_latest_paths(
        paths.execution_records_dir.as_path(),
        paths.replication_root.as_path(),
        world_id,
    )
}

fn reconcile_startup_state_from_execution_latest_paths(
    execution_records_dir: &Path,
    replication_root: &Path,
    world_id: &str,
) -> Result<Option<StartupReconcileReport>, String> {
    let latest_path = execution_records_dir.join("latest.json");
    let latest_bytes = match fs::read(latest_path.as_path()) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(format!(
                "read execution latest {} failed: {err}",
                latest_path.display()
            ));
        }
    };
    let latest: ExecutionRecordLatest =
        serde_json::from_slice(latest_bytes.as_slice()).map_err(|err| {
            format!(
                "parse execution latest {} failed: {err}",
                latest_path.display()
            )
        })?;
    if latest.world_id != world_id || latest.height == 0 {
        return Ok(None);
    }
    let Some(node_block_hash) = latest
        .node_block_hash
        .as_deref()
        .map(str::trim)
        .filter(|hash| !hash.is_empty())
    else {
        return Ok(None);
    };
    if latest.execution_block_hash.trim().is_empty()
        || latest.execution_state_root.trim().is_empty()
    {
        return Err(format!(
            "execution latest {} cannot reconcile pos state: missing block/execution binding at height {}",
            latest_path.display(),
            latest.height
        ));
    }

    let state_path = replication_root.join("node_pos_state.json");
    let existing = match fs::read(state_path.as_path()) {
        Ok(bytes) => Some(
            serde_json::from_slice::<NodePosStateSnapshot>(bytes.as_slice()).map_err(|err| {
                format!(
                    "parse node pos state {} failed: {err}",
                    state_path.display()
                )
            })?,
        ),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => {
            return Err(format!(
                "read node pos state {} failed: {err}",
                state_path.display()
            ));
        }
    };
    let previous_committed_height = existing
        .as_ref()
        .map(|snapshot| snapshot.committed_height)
        .unwrap_or(0);
    if previous_committed_height >= latest.height {
        return Ok(None);
    }

    let next_height = latest
        .height
        .checked_add(1)
        .ok_or_else(|| format!("execution latest height {} has no successor", latest.height))?;
    let reconciled = NodePosStateSnapshot {
        next_height,
        next_slot: existing
            .as_ref()
            .map(|snapshot| snapshot.next_slot)
            .unwrap_or(0),
        last_observed_slot: existing
            .as_ref()
            .map(|snapshot| snapshot.last_observed_slot)
            .unwrap_or(0),
        missed_slot_count: existing
            .as_ref()
            .map(|snapshot| snapshot.missed_slot_count)
            .unwrap_or(0),
        last_observed_tick: existing
            .as_ref()
            .map(|snapshot| snapshot.last_observed_tick)
            .unwrap_or(0),
        missed_tick_count: existing
            .as_ref()
            .map(|snapshot| snapshot.missed_tick_count)
            .unwrap_or(0),
        committed_height: latest.height,
        network_committed_height: existing
            .as_ref()
            .map(|snapshot| snapshot.network_committed_height.max(latest.height))
            .unwrap_or(latest.height),
        last_broadcast_proposal_height: latest.height,
        last_broadcast_local_attestation_height: latest.height,
        last_broadcast_committed_height: latest.height,
        last_committed_block_hash: Some(node_block_hash.to_string()),
        last_execution_height: latest.height,
        last_execution_block_hash: Some(latest.execution_block_hash),
        last_execution_state_root: Some(latest.execution_state_root),
    };
    let bytes = serde_json::to_vec_pretty(&reconciled)
        .map_err(|err| format!("serialize reconciled node pos state failed: {err}"))?;
    super::write_bytes_atomic(state_path.as_path(), bytes.as_slice())?;
    Ok(Some(StartupReconcileReport {
        previous_committed_height,
        reconciled_height: latest.height,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let unique = format!(
            "oasis7-startup-reconcile-{name}-{}-{nanos}",
            std::process::id()
        );
        std::env::temp_dir().join(unique)
    }

    #[test]
    fn startup_reconcile_advances_stale_pos_state_from_execution_latest() {
        let root = test_dir("advances-stale-pos");
        let records = root.join("records");
        let replication = root.join("replication");
        fs::create_dir_all(records.as_path()).expect("create records");
        fs::create_dir_all(replication.as_path()).expect("create replication");
        fs::write(
            records.join("latest.json"),
            br#"{"world_id":"world-a","height":42,"node_block_hash":"block-42","execution_block_hash":"exec-42","execution_state_root":"state-42"}"#,
        )
        .expect("write execution latest");
        fs::write(
            replication.join("node_pos_state.json"),
            br#"{"next_height":3,"next_slot":9,"last_observed_slot":8,"missed_slot_count":7,"last_observed_tick":80,"missed_tick_count":6,"committed_height":2,"network_committed_height":2,"last_broadcast_proposal_height":2,"last_broadcast_local_attestation_height":2,"last_broadcast_committed_height":2,"last_committed_block_hash":"old","last_execution_height":2,"last_execution_block_hash":"old-exec","last_execution_state_root":"old-state"}"#,
        )
        .expect("write stale pos state");

        let report = reconcile_startup_state_from_execution_latest_paths(
            records.as_path(),
            replication.as_path(),
            "world-a",
        )
        .expect("reconcile")
        .expect("report");
        assert_eq!(report.previous_committed_height, 2);
        assert_eq!(report.reconciled_height, 42);
        let state: NodePosStateSnapshot = serde_json::from_slice(
            &fs::read(replication.join("node_pos_state.json")).expect("read state"),
        )
        .expect("parse state");
        assert_eq!(state.next_height, 43);
        assert_eq!(state.committed_height, 42);
        assert_eq!(state.network_committed_height, 42);
        assert_eq!(state.last_committed_block_hash.as_deref(), Some("block-42"));
        assert_eq!(state.last_execution_height, 42);
        assert_eq!(state.last_execution_block_hash.as_deref(), Some("exec-42"));
        assert_eq!(state.last_execution_state_root.as_deref(), Some("state-42"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_reconcile_skips_legacy_latest_without_node_block_hash() {
        for (name, latest_json) in [
            (
                "missing-node-block-hash",
                br#"{"world_id":"world-a","height":42,"execution_block_hash":"exec-42","execution_state_root":"state-42"}"#.as_slice(),
            ),
            (
                "null-node-block-hash",
                br#"{"world_id":"world-a","height":42,"node_block_hash":null,"execution_block_hash":"exec-42","execution_state_root":"state-42"}"#.as_slice(),
            ),
        ] {
            let root = test_dir(name);
            let records = root.join("records");
            let replication = root.join("replication");
            fs::create_dir_all(records.as_path()).expect("create records");
            fs::create_dir_all(replication.as_path()).expect("create replication");
            fs::write(records.join("latest.json"), latest_json).expect("write execution latest");

            let report = reconcile_startup_state_from_execution_latest_paths(
                records.as_path(),
                replication.as_path(),
                "world-a",
            )
            .expect("reconcile");
            assert_eq!(report, None);
            assert!(!replication.join("node_pos_state.json").exists());

            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn startup_reconcile_ignores_foreign_world_latest() {
        let root = test_dir("foreign-world");
        let records = root.join("records");
        let replication = root.join("replication");
        fs::create_dir_all(records.as_path()).expect("create records");
        fs::write(
            records.join("latest.json"),
            br#"{"world_id":"other-world","height":42,"node_block_hash":"block-42","execution_block_hash":"exec-42","execution_state_root":"state-42"}"#,
        )
        .expect("write execution latest");
        let report = reconcile_startup_state_from_execution_latest_paths(
            records.as_path(),
            replication.as_path(),
            "world-a",
        )
        .expect("reconcile");
        assert_eq!(report, None);
        assert!(!replication.join("node_pos_state.json").exists());

        let _ = fs::remove_dir_all(root);
    }
}
