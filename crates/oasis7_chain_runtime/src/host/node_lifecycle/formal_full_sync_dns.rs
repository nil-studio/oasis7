//! DNS under the admitted exchange's original deadline. No getaddrinfo worker
//! or detached blocking thread survives a timeout; the private current-thread
//! runtime owns and drops all resolver futures before returning.
use super::metered_io::remaining;
use hickory_resolver::{
    TokioResolver,
    config::{LookupIpStrategy, ResolveHosts, ResolverConfig},
};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(super) enum Target {
    Socket(SocketAddr),
    Name {
        host: String,
        port: u16,
        config: Arc<ResolverConfig>,
    },
}
impl Target {
    pub fn resolve(
        &self,
        deadline: Instant,
        request_timeout: Duration,
    ) -> Result<SocketAddr, String> {
        match self {
            Self::Socket(address) => Ok(*address),
            Self::Name { host, port, config } => resolve_configured(
                host,
                *port,
                deadline,
                request_timeout,
                config.as_ref().clone(),
            ),
        }
    }
}
impl From<SocketAddr> for Target {
    fn from(address: SocketAddr) -> Self {
        Self::Socket(address)
    }
}

/// Local configuration preparation, not a bounded network attempt. The factory
/// must perform this before admitting network work; no system config file is
/// read by the resolver while an exchange is active.
pub(super) fn capture_system_config() -> Result<Arc<ResolverConfig>, String> {
    let (config, _) =
        hickory_resolver::system_conf::read_system_conf().map_err(|e| e.to_string())?;
    if config.name_servers.is_empty()
        || config.name_servers.len() > 8
        || config
            .name_servers
            .iter()
            .any(|server| server.connections.is_empty() || server.connections.len() > 2)
    {
        return Err("system DNS configuration exceeds supported bound".into());
    }
    Ok(Arc::new(ResolverConfig::from_name_servers(
        config.name_servers,
    )))
}

fn resolve_configured(
    host: &str,
    port: u16,
    deadline: Instant,
    request_timeout: Duration,
    config: ResolverConfig,
) -> Result<SocketAddr, String> {
    if host.is_empty() || host.len() > 253 || !host.is_ascii() || port == 0 {
        return Err("invalid controlled DNS name".into());
    }
    remaining(deadline, request_timeout)?;
    let dns_deadline = deadline.min(
        Instant::now()
            .checked_add(request_timeout)
            .ok_or("DNS deadline overflow")?,
    );
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err("controlled synchronous DNS requires its dedicated worker thread".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let result = runtime.block_on(async {
        let mut builder = TokioResolver::builder_with_config(config, Default::default());
        let options = builder.options_mut();
        options.timeout = remaining(dns_deadline, request_timeout)?;
        options.attempts = 1;
        options.ip_strategy = LookupIpStrategy::Ipv4thenIpv6;
        options.num_concurrent_reqs = 1;
        options.max_active_requests = 1;
        options.cache_size = 0;
        options.use_hosts_file = ResolveHosts::Never;
        options.try_tcp_on_error = false;
        let resolver = builder.build().map_err(|e| e.to_string())?;
        // Fully-qualified lookup prevents search-domain expansion. TLS keeps
        // verifying the original configured hostname, never this answer's IP.
        let fqdn = format!("{}.", host.trim_end_matches('.'));
        let answer = tokio::time::timeout_at(
            tokio::time::Instant::from_std(dns_deadline),
            resolver.lookup_ip(fqdn),
        )
        .await
        .map_err(|_| "controlled DNS deadline expired".to_string())?
        .map_err(|e| format!("controlled DNS lookup failed: {e}"))?;
        let mut addresses = answer.iter();
        let first = addresses
            .next()
            .ok_or("controlled DNS returned no address")?;
        if addresses.take(16).count() >= 16 {
            return Err("controlled DNS address set exceeds bound".into());
        }
        remaining(dns_deadline, request_timeout)?;
        Ok(SocketAddr::new(first, port))
    });
    drop(runtime);
    remaining(dns_deadline, request_timeout)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_resolver::config::{ConnectionConfig, NameServerConfig};
    use std::net::UdpSocket;

    // Use an ordinary DNS name with the local-only resolver below. The .invalid
    // special-use suffix is rejected locally by Hickory without sending a query.
    fn local_config(address: SocketAddr) -> ResolverConfig {
        let mut connection = ConnectionConfig::udp();
        connection.port = address.port();
        ResolverConfig::from_name_servers(vec![NameServerConfig::new(
            address.ip(),
            true,
            vec![connection],
        )])
    }

    #[test]
    fn silent_dns_stops_at_original_deadline_without_late_queries() {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let config = local_config(socket.local_addr().unwrap());
        let start = Instant::now();
        assert!(
            resolve_configured(
                "budget.example.com",
                443,
                start + Duration::from_millis(100),
                Duration::from_secs(2),
                config
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        socket.set_nonblocking(true).unwrap();
        let mut packet = [0; 4096];
        let mut count = 0;
        while socket.recv_from(&mut packet).is_ok() {
            count += 1;
        }
        assert!(count > 0);
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(150)))
            .unwrap();
        assert!(socket.recv_from(&mut packet).is_err());
    }

    #[test]
    fn real_dns_answer_preserves_configured_service_port() {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let config = local_config(socket.local_addr().unwrap());
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let worker = std::thread::spawn(move || {
            let mut packet = [0; 4096];
            let (n, peer) = socket.recv_from(&mut packet).unwrap();
            assert!(n >= 17);
            let mut end = 12;
            while packet[end] != 0 {
                end += 1 + packet[end] as usize;
                assert!(end < n);
            }
            end += 5;
            assert!(end <= n);
            let kind = u16::from_be_bytes([packet[end - 4], packet[end - 3]]);
            assert!(matches!(kind, 1 | 28));
            let mut response = packet[..end].to_vec();
            response[2..4].copy_from_slice(&[0x81, 0x80]);
            response[4..12].copy_from_slice(&[0, 1, 0, 1, 0, 0, 0, 0]);
            response.extend_from_slice(&[0xc0, 0x0c]);
            response.extend_from_slice(&kind.to_be_bytes());
            response.extend_from_slice(&[0, 1, 0, 0, 0, 1]);
            if kind == 1 {
                response.extend_from_slice(&[0, 4, 127, 0, 0, 1]);
            } else {
                response.extend_from_slice(&[0, 16]);
                response.extend_from_slice(&std::net::Ipv6Addr::LOCALHOST.octets());
            }
            socket.send_to(&response, peer).unwrap();
        });
        let result = resolve_configured(
            "budget.example.com",
            8443,
            Instant::now() + Duration::from_secs(2),
            Duration::from_secs(1),
            config,
        )
        .unwrap();
        assert!(result.ip().is_loopback());
        assert_eq!(result.port(), 8443);
        worker.join().unwrap();
    }
}
