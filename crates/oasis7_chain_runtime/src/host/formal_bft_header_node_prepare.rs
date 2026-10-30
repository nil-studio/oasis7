//! Explicit original provisioning and read-only normal-operation preparation.
use super::*;
pub(super) fn prepare(config: &Config, provision_original: bool) -> Result<Prepared, String> {
    // All private inputs and existing directories are checked before creating Node state.
    let cas = require_existing_directory(&config.cas_directory, "private Node CAS")?;
    crate::controlled_live_config::require_private_owned(&cas, true)?;
    for path in [&config.records_directory, &config.replication_directory] {
        let path = require_existing_directory(path, "private Node state")?;
        crate::controlled_live_config::require_private_owned(&path, true)?;
    }
    let directories = [
        &cas,
        &config.records_directory,
        &config.replication_directory,
    ]
    .into_iter()
    .map(std::fs::canonicalize)
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| "header Node state directory resolution rejected")?;
    for (index, left) in directories.iter().enumerate() {
        if directories
            .iter()
            .skip(index + 1)
            .any(|right| left.starts_with(right) || right.starts_with(left))
        {
            return Err(
                "header Node CAS, driver and replication directories must be disjoint".into(),
            );
        }
    }
    if !config.requester_signing_key_path.is_absolute() {
        return Err("header Node requester key path must be absolute".into());
    }
    let requester = crate::controlled_live_config::read_existing_signing_key(
        &config.requester_signing_key_path,
    )
    .map_err(|_| "header Node requester key rejected")?;
    let trust = load_handoff_trust_config(&config.trust_config_path)
        .map_err(|_| "header Node qualified H trust rejected")?;
    let anchor = trust
        .verified_anchor()
        .map_err(|_| "header Node qualified H trust rejected")?;
    if !config.handoff_material_path.is_absolute() {
        return Err("header Node H material path must be absolute".into());
    }
    crate::controlled_live_config::require_private_owned(&config.handoff_material_path, false)?;
    let material = read_qualified_handoff_material(&config.handoff_material_path)
        .map_err(|_| "header Node qualified H material rejected")?;
    let store = LocalCasStore::new(cas);
    let source = Arc::new(CasBusinessIndexReader {
        store: store.clone(),
    });
    #[cfg(feature = "node-libp2p")]
    let peer_network = config.network.is_some();
    #[cfg(not(feature = "node-libp2p"))]
    let peer_network = false;
    let (driver, handoff, initial_schedule, material_identity) = if provision_original {
        // --print-binding remains the explicit original-H provisioning command.
        let driver = if peer_network {
            FormalBftHeaderNodeDriverV3::open_from_handoff_with_peer_network(
                &config.records_directory,
                store,
                anchor,
                config.target.clone(),
                material,
                source,
            )
        } else {
            FormalBftHeaderNodeDriverV3::open_from_handoff(
                &config.records_directory,
                store,
                anchor,
                config.target.clone(),
                material,
                source,
            )
        }
        .map_err(|_| "header Node original provisioning rejected")?;
        let handoff = driver.handoff().clone();
        let initial = driver.initial_schedule().clone();
        let identity = driver.material_identity().to_owned();
        (
            Box::new(driver) as Box<dyn oasis7_node::NodeExecutionHook>,
            handoff,
            initial,
            identity,
        )
    } else {
        let archive = config
            .successor_private_archive_directory
            .as_ref()
            .map(|path| {
                let path = require_existing_directory(path, "private successor archive")?;
                crate::controlled_live_config::require_private_owned(&path, true)?;
                oasis7_distfs::protected_bft_v3::LocalBftSuccessorArchiveStoreV1::new(
                    LocalCasStore::new(path),
                )
                .map(Arc::new)
                .map_err(|_| "header Node fixed private archive rejected".to_owned())
            })
            .transpose()?;
        let provider_pin = config
            .provider_trust
            .as_ref()
            .ok_or("header Node requires pinned provider trust")?
            .provider_public_key
            .clone();
        FormalBftHeaderNodeDriverV3::deferred_existing(
            &config.records_directory,
            store,
            anchor,
            config.target.clone(),
            material,
            source,
            archive,
            provider_pin,
            peer_network,
        )
        .map_err(|_| "header Node read-only original identity rejected")?
    };
    let binding = BftSignerBindingV3::new(
        handoff,
        initial_schedule,
        material_identity,
        config.validator_id.clone(),
        config.provisioned_signing_keys.clone(),
        config.timeouts,
        config.operator_instance_id.clone(),
        hex::encode(requester.verifying_key().to_bytes()),
    )
    .map_err(|_| "header Node trusted signer binding rejected")?;
    Ok(Prepared {
        driver,
        binding,
        requester,
        #[cfg(feature = "node-libp2p")]
        initial_trust: trust.initial_trust,
    })
}
