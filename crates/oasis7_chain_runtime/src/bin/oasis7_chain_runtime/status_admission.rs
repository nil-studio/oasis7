//! Bounded HTTP safety admission, independent of World authorization.
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::{IpAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) const ACTIVE_LIMIT: usize = 32;
pub(super) const PEER_ACTIVE_LIMIT: usize = 4;
const PEER_LIMIT: usize = 1024;
const BURST: f64 = 32.0;
pub(super) const IO_DEADLINE: Duration = Duration::from_secs(2);

struct Peer {
    active: usize,
    tokens: f64,
    updated: Instant,
}
#[derive(Default)]
struct State {
    active: usize,
    peers: BTreeMap<IpAddr, Peer>,
}
#[derive(Clone, Default)]
pub(super) struct Admission(Arc<Mutex<State>>);
pub(super) struct Permit {
    admission: Admission,
    peer: IpAddr,
}
impl Drop for Permit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.admission.0.lock() {
            state.active = state.active.saturating_sub(1);
            if let Some(peer) = state.peers.get_mut(&self.peer) {
                peer.active = peer.active.saturating_sub(1);
            }
            pressure_witness(&state);
        }
    }
}
impl Admission {
    pub(super) fn acquire(&self, address: IpAddr) -> Result<Permit, u16> {
        self.acquire_at(address, Instant::now())
    }
    fn acquire_at(&self, address: IpAddr, now: Instant) -> Result<Permit, u16> {
        let mut state = self.0.lock().map_err(|_| 503u16)?;
        state.peers.retain(|_, peer| {
            peer.active > 0 || now.duration_since(peer.updated) < Duration::from_secs(60)
        });
        if state.active >= ACTIVE_LIMIT {
            return Err(503);
        }
        if !state.peers.contains_key(&address) && state.peers.len() >= PEER_LIMIT {
            return Err(429);
        }
        let peer = state.peers.entry(address).or_insert(Peer {
            active: 0,
            tokens: BURST,
            updated: now,
        });
        peer.tokens =
            (peer.tokens + now.duration_since(peer.updated).as_secs_f64() * BURST).min(BURST);
        peer.updated = now;
        if peer.active >= PEER_ACTIVE_LIMIT || peer.tokens < 1.0 {
            return Err(429);
        }
        peer.tokens -= 1.0;
        peer.active += 1;
        state.active += 1;
        pressure_witness(&state);
        Ok(Permit {
            admission: self.clone(),
            peer: address,
        })
    }
}

// Opt-in operator evidence from actual permits; never changes admission decisions.
fn pressure_witness(state: &State) {
    #[cfg(test)]
    if let Some(path) = std::env::var_os("PRE2_PRESSURE_WITNESS") {
        let path = std::path::PathBuf::from(path);
        let peers: BTreeMap<String, usize> = state
            .peers
            .iter()
            .filter(|(_, peer)| peer.active > 0)
            .map(|(ip, peer)| (ip.to_string(), peer.active))
            .collect();
        let bytes =
            serde_json::to_vec(&serde_json::json!({"active":state.active,"peers":peers})).unwrap();
        let temporary = path.with_extension("pending");
        std::fs::write(&temporary, bytes).expect("actual admission witness write");
        std::fs::rename(temporary, path).expect("actual admission witness publish");
    }
    #[cfg(not(test))]
    let _ = state;
}

pub(super) fn write_bounded(
    stream: &mut TcpStream,
    bytes: &[u8],
    deadline: Instant,
) -> io::Result<()> {
    let mut remaining = bytes;
    while !remaining.is_empty() {
        let timeout = deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "HTTP write deadline exceeded")
            })?;
        stream
            .set_write_timeout(Some(timeout))
            .inspect_err(|error| {
                trace_write_error(
                    "set_write_timeout",
                    stream,
                    deadline,
                    remaining.len(),
                    error,
                );
            })?;
        let written = stream.write(remaining).inspect_err(|error| {
            trace_write_error("write", stream, deadline, remaining.len(), error);
        })?;
        if written == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "HTTP connection closed",
            ));
        }
        remaining = &remaining[written..];
    }
    Ok(())
}

fn trace_write_error(
    stage: &'static str,
    stream: &TcpStream,
    deadline: Instant,
    bytes_remaining: usize,
    error: &io::Error,
) {
    #[cfg(any(test, feature = "test_tier_required"))]
    if std::env::var_os("PRE2_WORLD_RESPONSE_WRITE_TRACE").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        let remaining_ns = deadline
            .checked_duration_since(Instant::now())
            .map(|remaining| remaining.as_nanos())
            .unwrap_or(0);
        let peer_port = stream.peer_addr().ok().map(|peer| peer.port());
        eprintln!(
            "PRE2_WORLD_RESPONSE_WRITE_TRACE stage={stage} raw_os_error={:?} remaining_ns={remaining_ns} bytes_remaining={bytes_remaining} public_peer_port={peer_port:?}",
            error.raw_os_error()
        );
    }
    #[cfg(not(any(test, feature = "test_tier_required")))]
    let _ = (stage, stream, deadline, bytes_remaining, error);
}
pub(super) fn overload(mut stream: TcpStream, status: u16) {
    let _ = stream.set_nonblocking(false);
    let response = format!(
        "HTTP/1.1 {status} {}\r\nContent-Length: 0\r\nConnection: close\r\nRetry-After: 1\r\n\r\n",
        if status == 429 {
            "Too Many Requests"
        } else {
            "Service Unavailable"
        }
    );
    // Overload cannot pin the accept loop for the ordinary write deadline.
    let _ = write_bounded(
        &mut stream,
        response.as_bytes(),
        Instant::now() + Duration::from_millis(10),
    );
    let _ = stream.shutdown(std::net::Shutdown::Write);
}

/// Complete a rejected ingress response before boundedly discarding unread
/// request bytes. This is handler-only: the accept loop never performs a drain.
pub(super) fn reject_ingress(stream: &mut TcpStream, status: u16) -> io::Result<()> {
    let text = if status == 413 {
        "Payload Too Large"
    } else {
        "Request Timeout"
    };
    let response =
        format!("HTTP/1.1 {status} {text}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    write_bounded(stream, response.as_bytes(), Instant::now() + IO_DEADLINE)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let deadline = Instant::now() + Duration::from_millis(50);
    let mut discarded = 0;
    let mut bytes = [0u8; 4096];
    while discarded < 65_536 {
        let Some(remaining) = deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
        else {
            break;
        };
        stream.set_read_timeout(Some(remaining))?;
        let limit = bytes.len().min(65_536 - discarded);
        match stream.read(&mut bytes[..limit]) {
            Ok(0) | Err(_) => break,
            Ok(count) => discarded += count,
        }
    }
    Ok(())
}

/// Assemble real production HTTP ingress around a supplied genuine NodeRuntime.
/// Disabled reward telemetry is configuration, not a replacement execution hook.
#[cfg(test)]
pub(crate) fn start_test_server(
    root: &std::path::Path,
    endpoint: &str,
    runtime: Arc<Mutex<oasis7_node::NodeRuntime>>,
    signer: super::feedback_submit_api::FeedbackSubmitSigner,
) -> Result<super::status_server_support::ChainStatusServer, String> {
    let options = super::CliOptions::default();
    let reward_config = super::reward_runtime_worker::RewardRuntimeWorkerConfig {
        enabled: false,
        poll_interval: Duration::from_secs(1),
        world_id: "w1".into(),
        local_node_id: "node-a".into(),
        report_dir: root.join("reward-reports"),
        state_path: root.join("reward-state.json"),
        distfs_probe_state_path: root.join("reward-probe.json"),
        storage_root: root.join("store"),
        signer_node_id: "node-a".into(),
        signer_private_key_hex: signer.private_key_hex.clone(),
        reward_runtime_epoch_duration_secs: None,
        reward_runtime_auto_redeem: false,
        reward_asset_config: oasis7_runtime::RewardAssetConfig::default(),
        reward_initial_reserve_power_units: 0,
        reward_runtime_node_identity_bindings: BTreeMap::new(),
        reward_distfs_probe_config: super::distfs_probe_runtime::DistfsProbeRuntimeConfig::default(
        ),
    };
    let address: std::net::SocketAddr = endpoint
        .strip_prefix("http://")
        .unwrap_or(endpoint)
        .parse()
        .map_err(|error| format!("test status endpoint: {error}"))?;
    let policy = super::build_node_network_policy(&options);
    let storage_metrics =
        super::storage_metrics::init_shared_storage_metrics(options.storage_profile);
    super::status_server_support::start_chain_status_server(
        &address.ip().to_string(),
        address.port(),
        runtime,
        Arc::new(oasis7_node::Libp2pReplicationNetwork::new(
            oasis7_node::Libp2pReplicationNetworkConfig::default(),
        )),
        options,
        "node-a".into(),
        "w1".into(),
        root.join("world"),
        root.join("records"),
        root.join("store"),
        None,
        oasis7_runtime::ReleaseSecurityPolicy::default(),
        policy,
        super::reward_runtime_worker::init_shared_metrics(&reward_config),
        storage_metrics,
        signer,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_bounds_slow_peers_and_releases_slots() {
        let admission = Admission::default();
        let a = "127.0.0.1".parse().unwrap();
        let b = "127.0.0.2".parse().unwrap();
        let held: Vec<_> = (0..PEER_ACTIVE_LIMIT)
            .map(|_| admission.acquire(a).unwrap())
            .collect();
        assert!(matches!(admission.acquire(a), Err(429)));
        assert!(admission.acquire(b).is_ok());
        drop(held);
        assert!(admission.acquire(a).is_ok());
    }
    #[test]
    fn admission_rate_and_peer_storage_are_bounded() {
        let admission = Admission::default();
        let now = Instant::now();
        let a = "127.0.0.1".parse().unwrap();
        for _ in 0..32 {
            drop(admission.acquire_at(a, now).unwrap());
        }
        assert!(matches!(admission.acquire_at(a, now), Err(429)));
        assert!(
            admission
                .acquire_at(a, now + Duration::from_secs(1))
                .is_ok()
        );
        for index in 1..PEER_LIMIT as u32 {
            drop(
                admission
                    .acquire_at(IpAddr::V4(std::net::Ipv4Addr::from(index)), now)
                    .unwrap(),
            );
        }
        assert!(admission.0.lock().unwrap().peers.len() <= PEER_LIMIT);
        assert!(matches!(
            admission.acquire_at("192.0.2.1".parse().unwrap(), now),
            Err(429)
        ));
        assert!(
            admission
                .acquire_at("192.0.2.1".parse().unwrap(), now + Duration::from_secs(61))
                .is_ok()
        );
    }

    #[test]
    fn global_saturation_is_bounded_and_recovers_after_drop() {
        let admission = Admission::default();
        let held: Vec<_> = (1..=ACTIVE_LIMIT as u32)
            .map(|index| {
                admission
                    .acquire(IpAddr::V4(std::net::Ipv4Addr::from(index)))
                    .unwrap()
            })
            .collect();
        assert!(matches!(
            admission.acquire("192.0.2.2".parse().unwrap()),
            Err(503)
        ));
        drop(held);
        assert!(admission.acquire("192.0.2.2".parse().unwrap()).is_ok());
    }
}
