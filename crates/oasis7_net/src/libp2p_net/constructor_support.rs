use super::*;

pub(super) fn schedule_periodic_republish(command_tx: mpsc::Sender<Command>, interval_ms: i64) {
    if interval_ms <= 0 {
        return;
    }
    let mut command_tx = command_tx;
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(interval_ms as u64));
            match command_tx.try_send(Command::RepublishProviders) {
                Ok(()) => {}
                Err(err) if err.is_full() => {}
                Err(_) => break,
            }
        }
    });
}

pub(super) fn schedule_periodic_discovery_refresh(
    command_tx: mpsc::Sender<Command>,
    interval_ms: i64,
) {
    if interval_ms <= 0 {
        return;
    }
    let mut command_tx = command_tx;
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(interval_ms as u64));
            match command_tx.try_send(Command::RefreshPeerDiscovery) {
                Ok(()) => {}
                Err(err) if err.is_full() => {}
                Err(_) => break,
            }
        }
    });
}

pub(super) fn schedule_bootstrap_redial(
    command_tx: mpsc::Sender<Command>,
    peers: Vec<Multiaddr>,
    interval_ms: i64,
) {
    if interval_ms <= 0 || peers.is_empty() {
        return;
    }
    let mut command_tx = command_tx;
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(interval_ms as u64));
            for addr in &peers {
                match command_tx.try_send(Command::Dial(addr.clone())) {
                    Ok(()) => {}
                    Err(err) if err.is_full() => break,
                    Err(_) => return,
                }
            }
        }
    });
}

pub(super) fn enqueue_initial_bootstrap_dials(
    command_tx: mpsc::Sender<Command>,
    peers: Vec<Multiaddr>,
) {
    let mut command_tx = command_tx;
    for addr in peers {
        // Best effort: if the background task exits, dial requests can be dropped.
        if command_tx.try_send(Command::Dial(addr)).is_err() {
            break;
        }
    }
}

pub(super) fn listen_on_configured_addresses(
    swarm: &mut libp2p::Swarm<Behaviour>,
    addresses: Vec<Multiaddr>,
    errors: &Arc<Mutex<Vec<String>>>,
    max_errors: usize,
) {
    for addr in addresses {
        if let Err(err) = swarm.listen_on(addr) {
            push_bounded_clone(
                errors,
                format!("libp2p listen failed: {err}"),
                max_errors,
                "lock errors",
            );
        }
    }
}
