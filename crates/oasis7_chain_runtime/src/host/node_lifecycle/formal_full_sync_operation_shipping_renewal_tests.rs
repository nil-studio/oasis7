//! Real shipping CLI stopped-only signed grant renewal, original paid-ledger
//! preservation and subsequent completed World cycle. The registered ignored
//! Formal case invokes this module; ordinary libtest does not imply success.
use super::*;

/// Formal suite only: execute the shipping node command, not a mock worker.
/// A valid signed grant renewal must leave the old paid operation immutable
/// until the next legal session, and a signed same-epoch replacement must fail.
pub(super) fn run() {
    eprintln!("formal_budgeted_cli case=signed_renewal_cold_next_cycle phase=started");
    let binary =
        PathBuf::from(std::env::var_os("OASIS7_FORMAL_NODE_TEST_BIN").expect("shipping binary"));
    let mut fixture = BudgetFixture::new(
        false,
        |wire| {
            wire["poll_interval_ms"] = 700.into();
        },
        Some(binary),
    );
    fixture.cli.as_mut().unwrap().start().unwrap();
    fixture
        .cli
        .as_ref()
        .unwrap()
        .wait_ready(&fixture.expected)
        .unwrap();
    fixture.cli.as_mut().unwrap().stop().unwrap();
    let previous = fixture.snapshot();
    assert!(previous.installation.as_ref().unwrap().completed);
    let old_terminal = previous.terminal_index.as_ref().unwrap();
    assert!(old_terminal.completed_operations >= 1);

    let original = fixture
        .node
        .identity
        .chain
        .budgeted_formal_full_sync()
        .unwrap()
        .grant
        .as_ref()
        .unwrap()
        .clone();
    // A freshly signed but non-advancing grant is not accepted as a renewal.
    let mut same_epoch = original.body.clone();
    same_epoch.expires_unix_ms += 1000;
    let stale = FormalNodeReadGrantV1::sign(
        same_epoch,
        &hex::encode(test_key(9).to_bytes()),
    )
    .unwrap();
    assert_ne!(stale, original);
    let stale_path = fixture._root.0.join("same-epoch-issuer-grant.json");
    local_io::write(&stale_path, &stale).unwrap();
    let cli = fixture.cli.as_ref().unwrap();
    assert!(cli
        .command(&[
            "full-sync-grant",
            "--data-dir",
            path(&fixture.node.identity.data_dir).unwrap(),
            "--grant",
            path(&stale_path).unwrap(),
        ])
        .is_err());
    assert_eq!(fixture.snapshot(), previous);
    assert_eq!(
        model::NodeConfig::load(&fixture.node.identity.data_dir.join(model::CONFIG_FILE)).unwrap(),
        fixture.node
    );

    let next_grant = signed_renewal(&fixture);
    assert_eq!(next_grant.body.grant_epoch, original.body.grant_epoch + 1);
    let next_path = fixture._root.0.join("next-issuer-grant.json");
    local_io::write(&next_path, &next_grant).unwrap();
    let installed = cli
        .command(&[
            "full-sync-grant",
            "--data-dir",
            path(&fixture.node.identity.data_dir).unwrap(),
            "--grant",
            path(&next_path).unwrap(),
        ])
        .unwrap();
    assert_eq!(installed["changed"], true);
    assert_eq!(fixture.snapshot(), previous, "renewal must not rewrite original paid ledger");
    let adopted =
        model::NodeConfig::load(&fixture.node.identity.data_dir.join(model::CONFIG_FILE)).unwrap();
    assert_eq!(
        adopted
            .identity
            .chain
            .budgeted_formal_full_sync()
            .unwrap()
            .grant
            .as_ref()
            .unwrap(),
        &next_grant
    );

    fixture.cli.as_mut().unwrap().start().unwrap();
    fixture
        .cli
        .as_ref()
        .unwrap()
        .wait_ready(&fixture.expected)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = fixture.cli.as_ref().unwrap().status().unwrap();
        let completed = status["formal_full_state_sync"]["operation"]["terminal"]
            ["completed_operations"]
            .as_u64()
            .unwrap_or(0);
        if completed > old_terminal.completed_operations {
            break;
        }
        assert!(Instant::now() < deadline, "renewed signed grant did not allow next cycle: {status}");
        thread::sleep(Duration::from_millis(50));
    }
    fixture.cli.as_mut().unwrap().stop().unwrap();
    let after = fixture.snapshot();
    let terminal = after.terminal_index.as_ref().unwrap();
    assert!(terminal.completed_operations > old_terminal.completed_operations);
    assert!(terminal.encoded_tx_actual >= old_terminal.encoded_tx_actual);
    assert!(terminal.encoded_rx_actual >= old_terminal.encoded_rx_actual);
    assert_ne!(after.identity.operation_id, previous.identity.operation_id);
    eprintln!(
        "formal_budgeted_cli case=signed_renewal_cold_next_cycle phase=passed previous_operations={} current_operations={}",
        old_terminal.completed_operations,
        terminal.completed_operations
    );
}
