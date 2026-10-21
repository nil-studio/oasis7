//! Trusted node-only local administration. No client-selected authority.
use super::*;
use crate::execution_config::{
    DevLocalParticipantConfig, release_security_policy_for_storage_profile,
};

pub(crate) fn validate(o: &CliOptions) -> Result<(), String> {
    let enabled = o.devlocal_participant_operator_public_key.is_some();
    if (o.devlocal_participant_policy_file.is_some()
        || o.devlocal_participant_origin_root.is_some())
        && !enabled
    {
        return Err(
            "local participant policy requires an independently pinned operator key".into(),
        );
    }
    if o.devlocal_participant_origin_root.is_some() && o.devlocal_participant_policy_file.is_none()
    {
        return Err("origin binding requires an explicit signed startup policy".into());
    }
    if !enabled {
        return Ok(());
    }
    if !o.development_runtime
        || o.storage_profile != StorageProfile::DevLocal
        || o.guarded_initial_config.is_some()
        || o.formal_live_config.is_some()
        || o.network_tier_manifest_path.is_some()
        || o.loaded_network_tier_manifest.is_some()
        || o.genesis_validator_registry_path.is_some()
        || o.deployment_inventory_path.is_some()
        || o.capture_schedule_recipe_history
        || !o.provider_backed_bootstrap_authority_paths.is_empty()
        || o.local_test_provider_authority_path.is_some()
        || o.p2p_deployment_mode != PeerDeploymentMode::Private
        || !o.node_gossip_peers.is_empty()
        || o.node_gossip_bind.is_some()
        || !o.replication_network_listen_addrs.is_empty()
        || !o.replication_network_bootstrap_peers.is_empty()
        || !o.replication_fetch_requester_public_keys.is_empty()
        || !super::super::execution_role::node_role_materializes_execution_state(o.node_role)
    {
        return Err("local participant authority requires isolated DevLocal execution without network/formal/guarded/deployment authority".into());
    }
    Ok(())
}

pub(crate) fn devlocal_participant_config(
    o: &CliOptions,
) -> Result<DevLocalParticipantConfig, String> {
    validate(o)?;
    let Some(pin) = &o.devlocal_participant_operator_public_key else {
        return Ok(DevLocalParticipantConfig::default());
    };
    let release = release_security_policy_for_storage_profile(o.storage_profile);
    let authority =
        oasis7_runtime::DevLocalParticipantAuthority::new(o.storage_profile, &release, pin.clone())
            .map_err(|e| format!("local participant authority: {e:?}"))?;
    let startup = if let Some(path) = &o.devlocal_participant_policy_file {
        if !path.is_absolute()
            || path
                .canonicalize()
                .map_err(|_| "local policy unavailable")?
                != *path
        {
            return Err("local policy path must be canonical and absolute".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
            let file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)
                .map_err(|_| "local policy unavailable")?;
            let m = file
                .metadata()
                .map_err(|_| "local policy metadata unavailable")?;
            unsafe extern "C" {
                fn geteuid() -> u32;
            }
            if !m.is_file()
                || m.uid() != unsafe { geteuid() }
                || m.mode() & 0o777 != 0o600
                || m.nlink() != 1
            {
                return Err("local policy must be an operator-owned private regular file".into());
            }
            let mut bytes = Vec::new();
            file.take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "local policy read failed")?;
            if bytes.len() > 1024 * 1024 {
                return Err("local policy exceeds byte bound".into());
            }
            Some(
                oasis7_agent_api::game::parse_bounded_json(&bytes, 1024 * 1024)
                    .map_err(|_| "invalid signed local participant policy")?,
            )
        }
        #[cfg(not(unix))]
        {
            return Err("local participant policy requires Unix private storage".into());
        }
    } else {
        None
    };
    Ok(DevLocalParticipantConfig {
        authority: Some(authority),
        startup,
        ..DevLocalParticipantConfig::default()
    })
}

pub(crate) fn prepare_participant_world(
    options: &CliOptions,
    paths: &crate::RuntimePaths,
    keypair: &crate::node_keypair_config::NodeKeypairConfig,
    control: &mut DevLocalParticipantConfig,
) -> Result<(), String> {
    if let Some(root) = &options.devlocal_participant_origin_root {
        if root
            .join("world")
            .canonicalize()
            .map_err(|_| "origin world directory unavailable")?
            != paths
                .execution_world_dir
                .canonicalize()
                .map_err(|_| "execution world directory unavailable")?
        {
            return Err(
                "participant origin must identify the actual existing execution world directory"
                    .into(),
            );
        }
        let service = crate::build_feedback_submit_signer(&options.node_id, keypair)?;
        let identity = crate::preview_world_init::verify_existing_origin(
            root,
            &options.world_id,
            &options.node_id,
            &service.public_key_hex,
        )?;
        control.set_verified_origin(identity)?;
    }
    if control.authority.is_none() {
        crate::governance_registry::ensure_world_governance_validator_registry(
            &paths.execution_world_dir,
            options.genesis_validator_registry_path.as_deref(),
            options.loaded_network_tier_manifest.as_ref(),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn local() -> CliOptions {
        CliOptions {
            development_runtime: true,
            devlocal_participant_operator_public_key: Some(hex::encode(
                ed25519_dalek::SigningKey::from_bytes(&[91; 32])
                    .verifying_key()
                    .to_bytes(),
            )),
            ..CliOptions::default()
        }
    }
    #[test]
    fn authority_requires_effective_isolated_devlocal_and_independent_pin() {
        let options = local();
        assert!(
            devlocal_participant_config(&options)
                .unwrap()
                .authority
                .is_some()
        );
        for mutate in [
            |o: &mut CliOptions| o.development_runtime = false,
            |o: &mut CliOptions| o.storage_profile = StorageProfile::ReleaseDefault,
            |o: &mut CliOptions| o.storage_profile = StorageProfile::SoakForensics,
            |o: &mut CliOptions| o.formal_live_config = Some("/private/formal".into()),
            |o: &mut CliOptions| o.guarded_initial_config = Some("/private/guarded".into()),
            |o: &mut CliOptions| o.network_tier_manifest_path = Some("/private/tier".into()),
            |o: &mut CliOptions| o.deployment_inventory_path = Some("/private/inventory".into()),
            |o: &mut CliOptions| {
                o.genesis_validator_registry_path = Some("/private/registry".into())
            },
            |o: &mut CliOptions| o.node_gossip_bind = Some("127.0.0.1:1".parse().unwrap()),
            |o: &mut CliOptions| {
                o.replication_network_listen_addrs
                    .push("/ip4/127.0.0.1/tcp/1".into())
            },
            |o: &mut CliOptions| {
                o.local_test_provider_authority_path = Some("/private/provider".into())
            },
        ] {
            let mut denied = local();
            mutate(&mut denied);
            assert!(devlocal_participant_config(&denied).is_err());
        }
        let no_pin = CliOptions {
            devlocal_participant_policy_file: Some("/private/policy".into()),
            ..CliOptions::default()
        };
        assert!(validate(&no_pin).is_err());
        let mut invalid = local();
        invalid.devlocal_participant_operator_public_key = Some("00".repeat(32));
        assert!(devlocal_participant_config(&invalid).is_err());
        assert!(
            devlocal_participant_config(&CliOptions::default())
                .unwrap()
                .authority
                .is_none()
        );
    }
    #[test]
    fn local_cli_options_do_not_change_ordinary_defaults() {
        let parsed = parse_options(
            [
                "--devlocal-participant-operator-public-key",
                local()
                    .devlocal_participant_operator_public_key
                    .as_deref()
                    .unwrap(),
            ]
            .into_iter(),
        )
        .unwrap();
        assert!(
            devlocal_participant_config(&parsed)
                .unwrap()
                .authority
                .is_some()
        );
    }
}
