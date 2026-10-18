use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use oasis7::viewer::viewer_bootstrap_formal_release_runtime_world;
use oasis7_runtime::{
    ChainResourceDerivationContext, LOCAL_TEST_PROVIDER_GRANT_RENEWAL_THRESHOLD_TICKS,
    LocalTestProviderAuthorityConfig, LocalTestProviderGrantStatus,
    LocalTestProviderModuleArtifact, LocalTestProviderSessionMode,
    ProviderBackedBootstrapAuthorityV1, ReleaseSecurityPolicy, World as RuntimeWorld,
};
use serde::Deserialize;

use super::cli::CliOptions;

#[derive(Debug, Deserialize)]
struct LocalTestProviderBuildMetadata {
    source_hash: String,
    build_manifest_hash: String,
}

/// Prepare the explicit DevLocal provider authority before the execution
/// driver opens the world. The writer owns this once-only transition; the
/// viewer remains an observer and consumes the resulting JSON bundle.
pub(super) fn ensure_local_test_provider_authority(
    options: &CliOptions,
    world_dir: &Path,
) -> Result<(), String> {
    let authority_path = options
        .local_test_provider_authority_path
        .as_deref()
        .ok_or_else(|| "local test authority output path is missing".to_string())?;
    let wasm_path = options
        .local_test_provider_wasm_path
        .as_deref()
        .ok_or_else(|| "local test WASM artifact path is missing".to_string())?;
    let metadata_path = options
        .local_test_provider_metadata_path
        .as_deref()
        .ok_or_else(|| "local test build metadata path is missing".to_string())?;
    let finality_block_hash = options
        .local_test_provider_finality_block_hash
        .as_deref()
        .ok_or_else(|| "local test finality marker is missing".to_string())?;

    let wasm_bytes = fs::read(wasm_path).map_err(|error| {
        format!(
            "read local test WASM artifact {} failed: {error}",
            wasm_path.display()
        )
    })?;
    let metadata_bytes = fs::read(metadata_path).map_err(|error| {
        format!(
            "read local test build metadata {} failed: {error}",
            metadata_path.display()
        )
    })?;
    let metadata =
        serde_json::from_slice::<LocalTestProviderBuildMetadata>(metadata_bytes.as_slice())
            .map_err(|error| {
                format!(
                    "decode local test build metadata {} failed: {error}",
                    metadata_path.display()
                )
            })?;
    let mut config = LocalTestProviderAuthorityConfig::hosted_public_join(
        options.local_test_provider_agent_id.clone(),
        options.local_test_provider_owner_binding.clone(),
        finality_block_hash.to_string(),
    );
    if options.local_test_provider_session_mode == "loopback" {
        config.session_mode = LocalTestProviderSessionMode::Loopback;
        config.presenter.session_id =
            Some(format!("loopback:{}", options.local_test_provider_agent_id));
    }
    let artifact = LocalTestProviderModuleArtifact {
        wasm_bytes,
        source_hash: metadata.source_hash,
        build_manifest_hash: metadata.build_manifest_hash,
    };

    let snapshot_path = world_dir.join("snapshot.json");
    let journal_path = world_dir.join("journal.json");
    let snapshot_exists = snapshot_path.is_file();
    let journal_exists = journal_path.is_file();
    if snapshot_exists != journal_exists {
        return Err(format!(
            "local test authority setup refuses partial execution world persistence: {} and {}",
            snapshot_path.display(),
            journal_path.display()
        ));
    }

    if snapshot_exists {
        let mut world = oasis7::runtime_host::world::load_from_dir(world_dir).map_err(|error| {
            format!(
                "load existing execution world for local test authority setup failed: {error:?}"
            )
        })?;
        let authority_bytes = fs::read(authority_path).map_err(|error| {
            format!(
                "existing execution world requires local test authority bundle {}: {error}",
                authority_path.display()
            )
        })?;
        let authority = serde_json::from_slice::<ProviderBackedBootstrapAuthorityV1>(
            authority_bytes.as_slice(),
        )
        .map_err(|error| {
            format!(
                "decode existing local test authority bundle {} failed: {error}",
                authority_path.display()
            )
        })?;
        let binding = world.current_cognition_runtime_binding().map_err(|error| {
            format!(
                "existing execution world is missing the local Runtime binding; choose a fresh runtime root: {error:?}"
            )
        })?;
        if authority.world_id != binding.world_id
            || authority.branch_id != binding.branch_id
            || authority.reorg_epoch != binding.reorg_epoch
            || authority.agent_id != options.local_test_provider_agent_id
            || authority.owner_binding != options.local_test_provider_owner_binding
            || authority.authority_record.finality_block_hash != finality_block_hash
        {
            return Err(format!(
                "existing local test authority bundle {} does not match the requested world/session binding",
                authority_path.display()
            ));
        }
        world
            .validate_local_test_main_token_funding(
                options.local_test_provider_agent_id.as_str(),
            )
            .map_err(|error| {
                format!(
                    "existing local execution world is missing canonical W3 starter funding; choose a fresh runtime root: {error:?}"
                )
            })?;
        let status = world
            .local_test_provider_grant_status(&config, &artifact, &authority)
            .map_err(|error| {
                format!("validate existing local test provider grant failed: {error:?}")
            })?;
        let promoted = world
            .promote_cognition_runtime_finality()
            .map_err(|error| {
                format!("promote existing local Runtime finality failed: {error:?}")
            })?;
        if !should_reissue_local_test_provider_grant(status) {
            if promoted {
                persist_local_test_provider_world(
                    options,
                    world_dir,
                    authority_path,
                    &world,
                    &authority,
                )?;
            }
            return Ok(());
        }
        let provisioning = world
            .initialize_local_test_provider_authority(config, artifact)
            .map_err(|error| {
                format!("renew local test authority through Runtime admission failed: {error:?}")
            })?;
        persist_local_test_provider_world(
            options,
            world_dir,
            authority_path,
            &world,
            &provisioning.bootstrap_authority,
        )?;
        return Ok(());
    }

    let (mut world, _) = viewer_bootstrap_formal_release_runtime_world()
        .map_err(|error| format!("formal starter world bootstrap failed: {error}"))?;
    world = world.with_release_security_policy(ReleaseSecurityPolicy::default());

    // The module publication must precede the immutable cognition binding.
    world
        .install_local_test_provider_module(&config, &artifact)
        .map_err(|error| format!("install local test module failed: {error:?}"))?;
    let finality_epoch = world.state().time
        / world
            .governance_execution_policy()
            .epoch_length_ticks
            .max(1);
    world
        .bind_cognition_runtime(
            options.world_id.clone(),
            "main",
            finality_epoch,
            Some(finality_block_hash.to_string()),
            "pending",
            0,
        )
        .map_err(|error| format!("bind local Runtime cognition authority failed: {error:?}"))?;
    let funding_config = config.clone();
    let provisioning = world
        .initialize_local_test_provider_authority(config, artifact)
        .map_err(|error| {
            format!("initialize local test authority through Runtime admission failed: {error:?}")
        })?;
    world
        .initialize_local_test_main_token_funding(&funding_config)
        .map_err(|error| format!("initialize local W3 starter funding failed: {error:?}"))?;

    persist_local_test_provider_world(
        options,
        world_dir,
        authority_path,
        &world,
        &provisioning.bootstrap_authority,
    )?;
    Ok(())
}

fn should_reissue_local_test_provider_grant(status: Option<LocalTestProviderGrantStatus>) -> bool {
    status.is_none_or(|status| {
        status.remaining_ticks < LOCAL_TEST_PROVIDER_GRANT_RENEWAL_THRESHOLD_TICKS
    })
}

fn persist_local_test_provider_world(
    options: &CliOptions,
    world_dir: &Path,
    authority_path: &Path,
    world: &RuntimeWorld,
    authority: &ProviderBackedBootstrapAuthorityV1,
) -> Result<(), String> {
    let world_config_hash = world.current_manifest_hash().map_err(|error| {
        format!("derive local test world manifest hash before persistence failed: {error:?}")
    })?;
    world
        .save_to_dir_with_chain_resource_context(
            world_dir,
            ChainResourceDerivationContext {
                world_id: options.world_id.as_str(),
                chain_id: options.world_id.as_str(),
                genesis_ref: None,
                created_at_height: 0,
                manifest_height: 0,
                commit_block_hash: None,
                tick: world.state().time,
            },
            world_config_hash,
            "local-test-provider-runtime-v1",
        )
        .map_err(|error| {
            format!(
                "persist local test execution world {} failed: {error:?}",
                world_dir.display()
            )
        })?;
    write_authority_atomically(authority_path, authority)?;
    Ok(())
}

fn write_authority_atomically(
    path: &Path,
    authority: &ProviderBackedBootstrapAuthorityV1,
) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "create local test authority output directory {} failed: {error}",
            parent.display()
        )
    })?;
    let bytes = serde_json::to_vec_pretty(authority)
        .map_err(|error| format!("encode local test authority failed: {error}"))?;
    let tmp_path = temporary_authority_path(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(tmp_path.as_path())
        .map_err(|error| {
            format!(
                "create temporary local test authority {} failed: {error}",
                tmp_path.display()
            )
        })?;
    if let Err(error) = file
        .write_all(bytes.as_slice())
        .and_then(|_| file.sync_all())
    {
        let _ = fs::remove_file(tmp_path.as_path());
        return Err(format!(
            "write temporary local test authority {} failed: {error}",
            tmp_path.display()
        ));
    }
    drop(file);
    if let Err(error) = fs::rename(tmp_path.as_path(), path) {
        let _ = fs::remove_file(tmp_path.as_path());
        return Err(format!(
            "publish local test authority {} failed: {error}",
            path.display()
        ));
    }
    Ok(())
}

fn temporary_authority_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("provider-authority.json");
    path.with_file_name(format!(".{file_name}.tmp-{}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::CliOptions;
    use oasis7_runtime::{Action, DomainEvent, WorldEventBody};

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn local_setup_persists_real_runtime_order_and_reuses_durable_bundle() {
        let root = std::env::temp_dir().join(format!(
            "oasis7-local-provider-setup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let world_dir = root.join("world");
        let authority_path = root.join("authority.json");
        let wasm_path = root.join("provider.wasm");
        let metadata_path = root.join("provider.metadata.json");
        fs::create_dir_all(&root).expect("create setup root");
        fs::write(&wasm_path, b"real-wasm-artifact").expect("write artifact");
        fs::write(
            &metadata_path,
            serde_json::json!({
                "source_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "build_manifest_hash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            })
            .to_string(),
        )
        .expect("write metadata");
        let mut options = CliOptions::default();
        options.world_id = "local-provider-test-world".to_string();
        options.local_test_provider_authority_path = Some(authority_path.clone());
        options.local_test_provider_wasm_path = Some(wasm_path);
        options.local_test_provider_metadata_path = Some(metadata_path);
        options.local_test_provider_finality_block_hash =
            Some(format!("blake3:{}", "0".repeat(64)));
        ensure_local_test_provider_authority(&options, world_dir.as_path()).expect("setup");
        let authority_bytes = fs::read(&authority_path).expect("authority output");
        let authority = serde_json::from_slice::<ProviderBackedBootstrapAuthorityV1>(
            authority_bytes.as_slice(),
        )
        .expect("decode authority output");
        let mut world = oasis7::runtime_host::world::load_from_dir(&world_dir).expect("load world");
        assert!(world.current_cognition_runtime_binding().is_ok());
        assert!(
            world
                .module_registry()
                .active
                .contains_key("module.runtime.local-test-provider")
        );
        assert_eq!(authority.agent_id, options.local_test_provider_agent_id);
        assert_eq!(
            world.state().time,
            2,
            "funding must preserve baseline tick 2"
        );
        assert_eq!(world.main_token_config().initial_supply, 325);
        assert_eq!(world.main_token_supply().total_supply, 325);
        assert_eq!(world.main_token_supply().circulating_supply, 325);
        assert_eq!(world.main_token_liquid_balance("starter-agent-0"), 325);
        assert_eq!(
            world.main_token_last_claim_nonce("starter-agent-0"),
            Some(1)
        );
        let funding_events = world
            .journal()
            .events
            .iter()
            .filter_map(|event| match &event.body {
                WorldEventBody::Domain(
                    event @ (DomainEvent::MainTokenGenesisInitialized { .. }
                    | DomainEvent::MainTokenVestingClaimed { .. }),
                ) => Some(event),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(matches!(
            funding_events.as_slice(),
            [DomainEvent::MainTokenGenesisInitialized { total_supply: 325, allocations },
                DomainEvent::MainTokenVestingClaimed {
                    bucket_id,
                    beneficiary,
                    amount: 325,
                    nonce: 1,
                }]
            if allocations.len() == 1
                && allocations[0].bucket_id == "dev_local_w3_starter_vesting"
                && allocations[0].ratio_bps == 10_000
                && allocations[0].recipient == "starter-agent-0"
                && allocations[0].cliff_epochs == 0
                && allocations[0].linear_unlock_epochs == 0
                && allocations[0].start_epoch == 0
                && bucket_id == "dev_local_w3_starter_vesting"
                && beneficiary == "starter-agent-0"
        ));
        assert_eq!(
            world
                .cognition_economy()
                .expect("cognition economy")
                .available_balance(
                    options.local_test_provider_owner_binding.as_str(),
                    "cognition_units"
                ),
            128,
            "Builtin cognition allowance is independent from OC"
        );
        world.submit_action(Action::TransferMainToken {
            from_account_id: "starter-agent-0".to_string(),
            to_account_id: "local-test-recipient".to_string(),
            amount: 325,
            nonce: 1,
            asset_id: Some("main_token".to_string()),
            memo: Some("w3-local-post-spend-restart".to_string()),
            chain_id: None,
            network_id: None,
            tx_version: None,
            tx_type: None,
            valid_until_unix_ms: None,
            max_fee: None,
            fee_asset_id: None,
            application_payload_hash: None,
            client_request_id: None,
        });
        world.step().expect("normal post-funding spend");
        world
            .save_to_dir(&world_dir)
            .expect("persist post-spend world");
        let restarted = oasis7::runtime_host::world::load_from_dir(&world_dir)
            .expect("restart post-spend world");
        assert_eq!(restarted.main_token_liquid_balance("starter-agent-0"), 0);
        assert_eq!(
            restarted.main_token_liquid_balance("local-test-recipient"),
            325
        );
        let journal_len_after_spend = restarted.journal().events.len();
        ensure_local_test_provider_authority(&options, world_dir.as_path())
            .expect("durable setup should be idempotent");
        let reused =
            oasis7::runtime_host::world::load_from_dir(&world_dir).expect("reload reused world");
        assert_eq!(reused.journal().events.len(), journal_len_after_spend);
        assert_eq!(reused.main_token_liquid_balance("starter-agent-0"), 0);
        assert_eq!(
            reused.main_token_liquid_balance("local-test-recipient"),
            325
        );
        let mut mismatched_options = options.clone();
        mismatched_options.local_test_provider_finality_block_hash =
            Some(format!("blake3:{}", "1".repeat(64)));
        assert!(
            ensure_local_test_provider_authority(&mismatched_options, world_dir.as_path())
                .expect_err("changed finality must not reuse durable authority")
                .contains("does not match")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn local_setup_renews_expired_grant_without_refill_and_preserves_history() {
        let root = std::env::temp_dir().join(format!(
            "oasis7-local-provider-lifetime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let world_dir = root.join("world");
        let authority_path = root.join("authority.json");
        let wasm_path = root.join("provider.wasm");
        let metadata_path = root.join("provider.metadata.json");
        fs::create_dir_all(&root).expect("create setup root");
        fs::write(&wasm_path, b"real-wasm-artifact").expect("write artifact");
        fs::write(
            &metadata_path,
            serde_json::json!({
                "source_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "build_manifest_hash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            })
            .to_string(),
        )
        .expect("write metadata");
        let mut options = CliOptions::default();
        options.world_id = "local-provider-lifetime-world".to_string();
        options.local_test_provider_authority_path = Some(authority_path.clone());
        options.local_test_provider_wasm_path = Some(wasm_path);
        options.local_test_provider_metadata_path = Some(metadata_path);
        options.local_test_provider_finality_block_hash =
            Some(format!("blake3:{}", "0".repeat(64)));

        ensure_local_test_provider_authority(&options, world_dir.as_path()).expect("initial setup");
        let old_authority = serde_json::from_slice::<ProviderBackedBootstrapAuthorityV1>(
            fs::read(&authority_path)
                .expect("initial authority output")
                .as_slice(),
        )
        .expect("decode initial authority");
        let old_grant_json =
            serde_json::to_value(&old_authority.grant).expect("encode initial grant");
        let old_expiry = old_authority
            .grant
            .expires_at_tick
            .expect("initial grant expiry");
        let mut world =
            oasis7::runtime_host::world::load_from_dir(&world_dir).expect("load initial world");
        while world.state().time <= old_expiry {
            world.step().expect("advance expired-grant fixture");
        }
        let expired_tick = world.state().time;
        let economy_before = world
            .cognition_economy()
            .expect("read economy before renewal");
        world
            .save_to_dir(&world_dir)
            .expect("persist expired world");

        ensure_local_test_provider_authority(&options, world_dir.as_path())
            .expect("renew expired local grant");
        let renewed_authority = serde_json::from_slice::<ProviderBackedBootstrapAuthorityV1>(
            fs::read(&authority_path)
                .expect("renewed authority output")
                .as_slice(),
        )
        .expect("decode renewed authority");
        assert_ne!(
            renewed_authority.grant.grant_id,
            old_authority.grant.grant_id
        );
        assert_eq!(renewed_authority.grant.issued_at_tick, expired_tick);
        assert_eq!(
            renewed_authority.grant.expires_at_tick,
            Some(expired_tick.saturating_add(300))
        );
        let renewed_world =
            oasis7::runtime_host::world::load_from_dir(&world_dir).expect("load renewed world");
        assert_eq!(
            renewed_world
                .capability_grants_v2()
                .get(old_authority.grant.grant_id.as_str()),
            Some(&old_grant_json)
        );
        assert_eq!(
            renewed_world
                .cognition_economy()
                .expect("read economy after renewal"),
            economy_before
        );
        let journal_len_after_renewal = renewed_world.journal().events.len();

        ensure_local_test_provider_authority(&options, world_dir.as_path())
            .expect("reuse renewed local grant");
        let restarted =
            oasis7::runtime_host::world::load_from_dir(&world_dir).expect("restart renewed world");
        assert_eq!(restarted.journal().events.len(), journal_len_after_renewal);
        assert_eq!(
            restarted
                .capability_grants_v2()
                .get(renewed_authority.grant.grant_id.as_str()),
            Some(&serde_json::to_value(&renewed_authority.grant).expect("encode renewed grant"))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_setup_renewal_boundary_is_strictly_below_margin() {
        assert!(should_reissue_local_test_provider_grant(None));
        assert!(should_reissue_local_test_provider_grant(Some(
            LocalTestProviderGrantStatus { remaining_ticks: 0 }
        )));
        assert!(should_reissue_local_test_provider_grant(Some(
            LocalTestProviderGrantStatus {
                remaining_ticks: 239
            }
        )));
        assert!(!should_reissue_local_test_provider_grant(Some(
            LocalTestProviderGrantStatus {
                remaining_ticks: 240
            }
        )));
        assert!(!should_reissue_local_test_provider_grant(Some(
            LocalTestProviderGrantStatus {
                remaining_ticks: 300
            }
        )));
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn local_setup_reuses_at_240_but_reissues_at_239() {
        let root = std::env::temp_dir().join(format!(
            "oasis7-local-provider-boundary-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let world_dir = root.join("world");
        let authority_path = root.join("authority.json");
        let wasm_path = root.join("provider.wasm");
        let metadata_path = root.join("provider.metadata.json");
        fs::create_dir_all(&root).expect("create setup root");
        fs::write(&wasm_path, b"real-wasm-artifact").expect("write artifact");
        fs::write(
            &metadata_path,
            serde_json::json!({
                "source_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "build_manifest_hash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            })
            .to_string(),
        )
        .expect("write metadata");
        let mut options = CliOptions::default();
        options.world_id = "local-provider-boundary-world".to_string();
        options.local_test_provider_authority_path = Some(authority_path.clone());
        options.local_test_provider_wasm_path = Some(wasm_path);
        options.local_test_provider_metadata_path = Some(metadata_path);
        options.local_test_provider_finality_block_hash =
            Some(format!("blake3:{}", "0".repeat(64)));

        ensure_local_test_provider_authority(&options, world_dir.as_path()).expect("initial setup");
        let initial_authority = serde_json::from_slice::<ProviderBackedBootstrapAuthorityV1>(
            fs::read(&authority_path)
                .expect("initial authority output")
                .as_slice(),
        )
        .expect("decode initial authority");
        let initial_grant_json =
            serde_json::to_value(&initial_authority.grant).expect("encode initial grant");
        let expiry = initial_authority
            .grant
            .expires_at_tick
            .expect("initial grant expiry");
        let mut world =
            oasis7::runtime_host::world::load_from_dir(&world_dir).expect("load initial world");
        let at_margin_tick = expiry.saturating_sub(240);
        while world.state().time < at_margin_tick {
            world.step().expect("advance to renewal margin");
        }
        assert_eq!(expiry.saturating_sub(world.state().time), 240);
        world.save_to_dir(&world_dir).expect("persist margin world");
        let journal_at_margin = world.journal().events.len();
        ensure_local_test_provider_authority(&options, world_dir.as_path())
            .expect("reuse grant at exact margin");
        let reused_at_margin = serde_json::from_slice::<ProviderBackedBootstrapAuthorityV1>(
            fs::read(&authority_path)
                .expect("margin authority output")
                .as_slice(),
        )
        .expect("decode margin authority");
        assert_eq!(
            reused_at_margin.grant.grant_id,
            initial_authority.grant.grant_id
        );
        assert_eq!(
            oasis7::runtime_host::world::load_from_dir(&world_dir)
                .expect("reload margin world")
                .journal()
                .events
                .len(),
            journal_at_margin
        );

        let mut below_margin =
            oasis7::runtime_host::world::load_from_dir(&world_dir).expect("reload margin world");
        below_margin.step().expect("advance below renewal margin");
        assert_eq!(expiry.saturating_sub(below_margin.state().time), 239);
        let below_margin_tick = below_margin.state().time;
        below_margin
            .save_to_dir(&world_dir)
            .expect("persist below-margin world");
        ensure_local_test_provider_authority(&options, world_dir.as_path())
            .expect("reissue grant below margin");
        let renewed = serde_json::from_slice::<ProviderBackedBootstrapAuthorityV1>(
            fs::read(&authority_path)
                .expect("renewed authority output")
                .as_slice(),
        )
        .expect("decode renewed authority");
        assert_ne!(renewed.grant.grant_id, initial_authority.grant.grant_id);
        assert_eq!(renewed.grant.issued_at_tick, below_margin_tick);
        assert_eq!(
            renewed.grant.expires_at_tick,
            Some(below_margin_tick.saturating_add(300))
        );
        let renewed_world =
            oasis7::runtime_host::world::load_from_dir(&world_dir).expect("reload renewed world");
        assert_eq!(
            renewed_world
                .capability_grants_v2()
                .get(initial_authority.grant.grant_id.as_str()),
            Some(&initial_grant_json)
        );
        let journal_after_renewal = renewed_world.journal().events.len();
        ensure_local_test_provider_authority(&options, world_dir.as_path())
            .expect("reuse reissued grant after restart");
        assert_eq!(
            oasis7::runtime_host::world::load_from_dir(&world_dir)
                .expect("reload reissued world")
                .journal()
                .events
                .len(),
            journal_after_renewal
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn local_setup_recovers_after_authority_publication_interruption() {
        let root = std::env::temp_dir().join(format!(
            "oasis7-local-provider-recovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let world_dir = root.join("world");
        let authority_path = root.join("authority.json");
        let wasm_path = root.join("provider.wasm");
        let metadata_path = root.join("provider.metadata.json");
        fs::create_dir_all(&root).expect("create setup root");
        fs::write(&wasm_path, b"real-wasm-artifact").expect("write artifact");
        fs::write(
            &metadata_path,
            serde_json::json!({
                "source_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "build_manifest_hash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            })
            .to_string(),
        )
        .expect("write metadata");
        let mut options = CliOptions::default();
        options.world_id = "local-provider-recovery-world".to_string();
        options.local_test_provider_authority_path = Some(authority_path.clone());
        options.local_test_provider_wasm_path = Some(wasm_path);
        options.local_test_provider_metadata_path = Some(metadata_path);
        options.local_test_provider_finality_block_hash =
            Some(format!("blake3:{}", "0".repeat(64)));

        ensure_local_test_provider_authority(&options, world_dir.as_path()).expect("initial setup");
        let old_authority = serde_json::from_slice::<ProviderBackedBootstrapAuthorityV1>(
            fs::read(&authority_path)
                .expect("initial authority output")
                .as_slice(),
        )
        .expect("decode initial authority");
        let old_authority_bytes = fs::read(&authority_path).expect("read old authority bytes");
        let old_grant_json =
            serde_json::to_value(&old_authority.grant).expect("encode initial grant");
        let old_expiry = old_authority
            .grant
            .expires_at_tick
            .expect("initial grant expiry");
        let mut expired_world =
            oasis7::runtime_host::world::load_from_dir(&world_dir).expect("load initial world");
        while expired_world.state().time <= old_expiry {
            expired_world.step().expect("advance expired world");
        }
        let economy_before = expired_world
            .cognition_economy()
            .expect("read economy before interrupted renewal");
        expired_world
            .save_to_dir(&world_dir)
            .expect("persist expired world");

        let temporary_path = temporary_authority_path(&authority_path);
        fs::write(&temporary_path, b"occupied publication slot")
            .expect("occupy authority temporary path");
        let interrupted = ensure_local_test_provider_authority(&options, world_dir.as_path())
            .expect_err("authority publication interruption must surface after world persistence");
        assert!(interrupted.contains("temporary local test authority"));
        assert_eq!(
            fs::read(&authority_path).expect("authority remains old after interruption"),
            old_authority_bytes
        );
        let partially_persisted = oasis7::runtime_host::world::load_from_dir(&world_dir)
            .expect("world remains replayable after publication interruption");
        assert!(partially_persisted.journal().events.len() > expired_world.journal().events.len());
        let journal_after_interruption = partially_persisted.journal().events.len();
        assert_eq!(
            partially_persisted
                .cognition_economy()
                .expect("read economy after interruption"),
            economy_before
        );
        fs::remove_file(&temporary_path).expect("release authority temporary path");

        ensure_local_test_provider_authority(&options, world_dir.as_path())
            .expect("normal startup must reconcile interrupted publication");
        let recovered_authority = serde_json::from_slice::<ProviderBackedBootstrapAuthorityV1>(
            fs::read(&authority_path)
                .expect("recovered authority output")
                .as_slice(),
        )
        .expect("decode recovered authority");
        assert_ne!(
            recovered_authority.grant.grant_id,
            old_authority.grant.grant_id
        );
        let recovered_world = oasis7::runtime_host::world::load_from_dir(&world_dir)
            .expect("reload reconciled world after interruption");
        assert_eq!(
            recovered_world.journal().events.len(),
            journal_after_interruption,
            "same-tick retry must not duplicate grant/context/provisioning events"
        );
        assert_eq!(
            recovered_world
                .capability_grants_v2()
                .get(old_authority.grant.grant_id.as_str()),
            Some(&old_grant_json)
        );
        assert_eq!(
            recovered_world
                .cognition_economy()
                .expect("read recovered economy"),
            economy_before
        );
        let _ = fs::remove_dir_all(root);
    }
}
