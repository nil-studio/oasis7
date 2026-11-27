use multiaddr::Multiaddr;

const MULTIADDR_EXAMPLE: &str = "/dns4/bootstrap1.oasis7.tech/tcp/5612";

fn parse_chain_replication_bootstrap_peer(raw: &str) -> Result<String, String> {
    let token = raw.trim();
    if token.is_empty() {
        return Err("chain replication bootstrap peer cannot be empty".to_string());
    }
    token.parse::<Multiaddr>().map_err(|err| {
        format!(
            "chain replication bootstrap peers must use libp2p multiaddr values like {MULTIADDR_EXAMPLE}; invalid `{token}`: {err}"
        )
    })?;
    Ok(token.to_string())
}

pub(super) fn parse_chain_replication_bootstrap_peers(raw: &str) -> Result<Vec<String>, String> {
    let mut peers = Vec::new();
    for token in raw.split([',', ';', ' ', '\n', '\r', '\t']) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        peers.push(parse_chain_replication_bootstrap_peer(token)?);
    }
    Ok(peers)
}

pub(super) fn next_value<'a, I>(
    iter: &mut std::iter::Peekable<I>,
    flag: &str,
) -> Result<String, String>
where
    I: Iterator<Item = &'a str>,
{
    iter.next()
        .map(str::to_string)
        .ok_or_else(|| format!("{flag} requires a value"))
}

pub(super) fn parse_port(raw: &str, label: &str) -> Result<u16, String> {
    let value = raw.trim();
    let port = value
        .parse::<u16>()
        .map_err(|_| format!("{label} must be integer in 1..=65535"))?;
    if port == 0 {
        return Err(format!("{label} must be in 1..=65535"));
    }
    Ok(port)
}

pub(super) fn parse_positive_u64(raw: &str, label: &str) -> Result<u64, String> {
    let value = raw.trim();
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{label} must be a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{label} must be a positive integer"));
    }
    Ok(parsed)
}

pub(super) fn parse_non_negative_u64(raw: &str, label: &str) -> Result<u64, String> {
    let value = raw.trim();
    value
        .parse::<u64>()
        .map_err(|_| format!("{label} must be a non-negative integer"))
}

pub(super) fn parse_optional_i64(raw: &str, label: &str) -> Result<Option<i64>, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<i64>()
        .map(Some)
        .map_err(|_| format!("{label} must be an integer"))
}

pub(super) fn parse_host_port(raw: &str, label: &str) -> Result<(String, u16), String> {
    let value = raw.trim();
    let (host_raw, port_raw) = if let Some(rest) = value.strip_prefix('[') {
        let (host, remainder) = rest
            .split_once(']')
            .ok_or_else(|| format!("{label} IPv6 host must be in [addr]:port format"))?;
        let port_raw = remainder
            .strip_prefix(':')
            .ok_or_else(|| format!("{label} must be in <host:port> format"))?;
        (host, port_raw)
    } else {
        let (host, port_raw) = value
            .rsplit_once(':')
            .ok_or_else(|| format!("{label} must be in <host:port> format"))?;
        if host.contains(':') {
            return Err(format!("{label} IPv6 host must be wrapped in []"));
        }
        (host, port_raw)
    };

    let host = host_raw.trim();
    if host.is_empty() {
        return Err(format!("{label} host cannot be empty"));
    }
    let port = parse_port(port_raw, label)?;
    Ok((host.to_string(), port))
}

pub(super) fn parse_chain_role(raw: &str) -> Result<String, String> {
    let role = raw.trim().to_ascii_lowercase();
    match role.as_str() {
        "sequencer" | "storage" | "observer" => Ok(role),
        _ => Err("chain role must be one of: sequencer|storage|observer".to_string()),
    }
}

pub(super) fn parse_chain_validators(raw: &str) -> Result<Vec<String>, String> {
    let mut validators = Vec::new();
    for token in raw.split([',', ';', ' ']) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let (validator_id, stake) = token
            .rsplit_once(':')
            .ok_or_else(|| "chain validators must be <validator_id:stake>".to_string())?;
        if validator_id.trim().is_empty() {
            return Err("chain validators cannot contain empty validator_id".to_string());
        }
        let stake = stake
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| "chain validator stake must be positive integer".to_string())?;
        validators.push(format!("{}:{}", validator_id.trim(), stake));
    }
    Ok(validators)
}
