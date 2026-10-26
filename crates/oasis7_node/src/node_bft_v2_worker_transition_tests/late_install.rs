use super::*;

#[test]
fn late_validator_installs_from_historical_confirmation_at_current_tip() {
    const FOLLOW_HEIGHT: u64 = 8;

    let root = PrivateRoot::new();
    let identities = identities();
    let addresses = reserve_udp_addresses(identities.len());

    // Run only the genesis-set participants and the already-installed new
    // validator. Validator 16 remains offline past the h+1 confirmation.
    let mut initial_nodes = identities
        .iter()
        .take(5)
        .enumerate()
        .map(|(index, identity)| make_node(identity, index, &addresses, root.path()))
        .collect::<Vec<_>>();
    for node in &mut initial_nodes {
        node.start().expect("start pre-transition BFT V2 worker");
    }
    wait_all_height(
        &initial_nodes,
        &identities,
        FOLLOW_HEIGHT,
        "history beyond the transition confirmation",
    );
    stop_all(&mut initial_nodes);

    // Select a real, complete chain-runtime test-hook history and its exact
    // matching signer history; do not synthesize a QC or driver head.
    let (source_index, source_config, history) = identities
        .iter()
        .take(5)
        .enumerate()
        .filter_map(|(index, identity)| {
            let driver = identity.driver.lock().ok()?;
            let config = driver.config.clone()?;
            let signer_history = identity.provider.backend.snapshot().state.history;
            (driver.certificates == signer_history
                && signer_history.len() >= FOLLOW_HEIGHT as usize)
                .then_some((index, config, signer_history))
        })
        .max_by_key(|(_, _, history)| history.len())
        .expect("a stopped worker has matching driver and protected signer histories");
    let tip_height = u64::try_from(history.len()).expect("bounded BFT V2 test history");
    assert!(tip_height > SCHEDULED_HEIGHT + 1);
    verify_certificates(&identities[source_index].binding, &history, tip_height);

    let late_index = 5;
    {
        let mut driver = identities[late_index]
            .driver
            .lock()
            .expect("late validator test driver mutex");
        driver.config = Some(source_config);
        driver.certificates = history.clone();
        driver.replay_cursor = 0;
    }

    // The remote provider receives every validated QC through its real TCP
    // lease API. No install receipt is created by history catch-up itself.
    let appended = identities[late_index]
        .provider
        .append_history(&history)
        .expect("append complete verified history through the remote signer");
    assert_eq!(appended.state.history, history);
    assert!(appended.state.install_receipts.is_empty());
    assert!(appended.state.pending_install.is_none());

    // Startup replays the driver history, finds the original h+1
    // confirmation, and installs at the later current tip T.
    let mut late_node = make_node(&identities[late_index], late_index, &addresses, root.path());
    late_node
        .start()
        .expect("late validator replays and installs from historical confirmation");
    wait_node_height(&late_node, tip_height, "late validator history replay");
    let installed_state = identities[late_index].provider.backend.snapshot().state;
    assert_eq!(installed_state.history, history);
    let receipt = installed_state
        .install_receipts
        .iter()
        .find(|receipt| receipt.body.activation_height == ACTIVATION_HEIGHT)
        .expect("current-tip transition install receipt");
    assert_eq!(
        receipt.body.confirmation_height,
        SCHEDULED_HEIGHT + 1,
        "the original h+1 confirmation remains the authority for installation"
    );
    assert_eq!(
        receipt.body.installed_head_height, tip_height,
        "late installation binds the actual verified current tip, not the old confirmation"
    );
    assert_eq!(
        receipt.body.active_validator_set_hash,
        BftValidatorSetV2::new(scope(), validators(&[11, 12, 13, 15, 16]))
            .expect("scheduled active set")
            .set_hash()
    );

    // Run exactly four validators of the five-member active set. Every one
    // must sign the next QC, proving the late node is now admitted to signing.
    let mut active_nodes = vec![late_node];
    for index in [0, 1, 4] {
        let mut node = make_node(&identities[index], index, &addresses, root.path());
        node.start().expect("restart active BFT V2 validator");
        active_nodes.push(node);
    }
    let next_height = tip_height.checked_add(1).expect("bounded test height");
    wait_all_height(
        &active_nodes,
        &identities,
        next_height,
        "post-install active-set QC",
    );
    stop_all(&mut active_nodes);

    let next_certificate = identities[late_index]
        .provider
        .backend
        .snapshot()
        .state
        .history
        .get(usize::try_from(next_height - 1).expect("test height fits usize"))
        .cloned()
        .expect("late signer persists the post-install QC");
    let complete_history = history
        .iter()
        .cloned()
        .chain(std::iter::once(next_certificate.clone()))
        .collect::<Vec<_>>();
    verify_certificates(
        &identities[late_index].binding,
        &complete_history,
        next_height,
    );
    let signers = next_certificate
        .precommits
        .iter()
        .map(|vote| vote.validator_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for seed in [11, 12, 15, 16] {
        assert!(
            signers.contains(validator_id(seed).as_str()),
            "all four online active validators must sign height {next_height}"
        );
    }
    assert!(!signers.contains(validator_id(14).as_str()));
}
