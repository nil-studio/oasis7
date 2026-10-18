use std::env;
use std::path::{Path, PathBuf};

use oasis7::geometry::GeoPos;
use oasis7::simulator::WorldKernel;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_runtime::{
    Action as RuntimeAction, ChainResourceDerivationContext, Journal, World, WorldError, blake3_hex,
};
use oasis7_simulator::{WorldConfig, WorldModel};

struct CliOptions {
    source_world_dir: Option<PathBuf>,
    generated_world_dir: Option<PathBuf>,
    output_world_dir: PathBuf,
    world_id: Option<String>,
    chain_id: Option<String>,
    resource_commit_height: Option<u64>,
    resource_commit_hash: Option<String>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("oasis7_world_repair_rebuild failed: {err:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), WorldError> {
    let options = parse_args()?;
    let (rebuilt, source_label, location_count, generated_source) =
        if let Some(generated_world_dir) = options.generated_world_dir.as_ref() {
            let (world, _config, seed_model) =
                bootstrap_generated_sidecar_runtime_world(generated_world_dir.as_path())
                    .map_err(WorldError::Io)?;
            (
                world,
                format!("generated_world_dir={}", generated_world_dir.display()),
                seed_model.locations.len(),
                true,
            )
        } else {
            let source_world_dir = options
                .source_world_dir
                .as_ref()
                .ok_or_else(|| WorldError::Io("missing --source-world-dir".to_string()))?;
            let journal_path = source_world_dir.join("journal.json");
            let journal = Journal::load_json(journal_path.as_path())?;

            let seed_world = oasis7::runtime_host::world::new();
            let seed_snapshot = seed_world.snapshot();
            (
                oasis7::runtime_host::world::from_snapshot(seed_snapshot, journal)?,
                format!("source_world_dir={}", source_world_dir.display()),
                0,
                false,
            )
        };

    let rebuilt = if generated_source && options.world_id.is_some() {
        align_generated_bootstrap_world_to_commit_height(
            rebuilt,
            options.resource_commit_height.unwrap_or(0),
        )?
    } else {
        rebuilt
    };

    std::fs::create_dir_all(options.output_world_dir.as_path())?;
    if let Some(world_id) = options.world_id.as_deref() {
        let chain_id = options.chain_id.as_deref().unwrap_or(world_id);
        let commit_height = options.resource_commit_height.unwrap_or(0);
        let commit_hash = options
            .resource_commit_hash
            .clone()
            .unwrap_or_else(|| resource_commit_hash(world_id, commit_height));
        let resource_context_hash = resource_context_hash(world_id);
        rebuilt.save_to_dir_with_chain_resource_context(
            options.output_world_dir.as_path(),
            ChainResourceDerivationContext {
                world_id,
                chain_id,
                genesis_ref: None,
                created_at_height: commit_height,
                manifest_height: commit_height,
                commit_block_hash: Some(commit_hash.as_str()),
                tick: commit_height,
            },
            resource_context_hash.as_str(),
            resource_context_hash.as_str(),
        )?;
    } else {
        rebuilt.save_to_dir(options.output_world_dir.as_path())?;
    }
    let verified = oasis7::runtime_host::world::load_from_dir(options.output_world_dir.as_path())?;

    println!("{source_label}");
    println!("output_world_dir={}", options.output_world_dir.display());
    println!("journal_events={}", rebuilt.journal().len());
    println!(
        "tick_consensus_records={}",
        verified.tick_consensus_record_count()
    );
    println!("world_time={}", verified.state().time);
    println!("agent_count={}", verified.state().agents.len());
    println!("location_count={location_count}");
    Ok(())
}

fn align_generated_bootstrap_world_to_commit_height(
    world: World,
    commit_height: u64,
) -> Result<World, WorldError> {
    if world.state().time == commit_height
        && world.journal().is_empty()
        && world.tick_consensus_records().is_empty()
    {
        return Ok(world);
    }

    let mut snapshot = world.snapshot();
    snapshot.state.time = commit_height;
    snapshot.journal_len = 0;
    snapshot.journal_commitment = Journal::new().commitment()?;
    snapshot.last_event_id = 0;
    snapshot.event_id_era = 0;
    snapshot.tick_consensus_records.clear();
    snapshot.tick_consensus_total_record_count = 0;
    snapshot.tick_consensus_archived_record_count = 0;
    snapshot.tick_consensus_hot_from_tick = None;
    snapshot.tick_consensus_hot_to_tick = None;
    snapshot.tick_consensus_rejection_audit_events.clear();
    oasis7::runtime_host::world::from_snapshot(snapshot, Journal::new())
}

fn resource_context_hash(world_id: &str) -> String {
    blake3_hex(format!("repair_rebuild_resource_context_v1:{world_id}").as_bytes())
}

fn resource_commit_hash(world_id: &str, height: u64) -> String {
    blake3_hex(format!("repair_rebuild_resource_commit_v1:{world_id}:{height}").as_bytes())
}

fn bootstrap_generated_sidecar_runtime_world(
    generated_world_dir: &Path,
) -> Result<(World, WorldConfig, WorldModel), String> {
    let sidecar_dir = generated_world_dir.join("generated-scenario-world");
    let provenance_path = generated_world_dir.join("world-generation-provenance.json");
    if !provenance_path.is_file() {
        return Err(format!(
            "generated world provenance missing: {}",
            provenance_path.display()
        ));
    }
    let kernel = WorldKernel::load_from_dir(&sidecar_dir).map_err(|err| {
        format!(
            "generated sidecar load failed dir={} err={err:?}",
            sidecar_dir.display()
        )
    })?;
    let snapshot = kernel.snapshot();
    let config = snapshot.config;
    let model = snapshot.model;
    let (world, config) =
        bootstrap_runtime_world_from_model(config, &model, "repair rebuild generated sidecar")?;
    Ok((world, config, model))
}

fn bootstrap_runtime_world_from_model(
    config: WorldConfig,
    model: &WorldModel,
    label: &str,
) -> Result<(World, WorldConfig), String> {
    let mut world = oasis7::runtime_host::world::new_production_hardened();
    world.set_resource_balance(ResourceKind::Electricity, 400);
    for (material, amount) in [
        ("structural_frame", 40),
        ("circuit_board", 4),
        ("servo_motor", 2),
        ("heat_coil", 6),
        ("refractory_brick", 8),
        ("iron_ore", 60),
        ("carbon_fuel", 20),
        ("copper_ore", 60),
        ("silicate_ore", 20),
        ("hardware_part", 40),
    ] {
        world
            .set_material_balance(material, amount)
            .map_err(|err| {
                format!("{label} set material balance failed material={material} err={err:?}")
            })?;
    }

    let mut seed_agents: Vec<(String, GeoPos, i64, i64)> = model
        .agents
        .iter()
        .map(|(agent_id, agent)| {
            (
                agent_id.clone(),
                agent.pos,
                agent.resources.get(ResourceKind::Electricity),
                agent.resources.get(ResourceKind::Data),
            )
        })
        .collect();
    seed_agents.sort_by(|left, right| left.0.cmp(&right.0));

    if seed_agents.is_empty() {
        seed_agents.push(("runtime-agent-0".to_string(), GeoPos::new(0, 0, 0), 32, 8));
        seed_agents.push(("runtime-agent-1".to_string(), GeoPos::new(0, 0, 0), 32, 8));
    }

    for (agent_id, pos, _, _) in &seed_agents {
        world.submit_action(RuntimeAction::RegisterAgent {
            agent_id: agent_id.clone(),
            pos: *pos,
        });
    }

    if world.pending_actions_len() > 0 {
        world
            .step()
            .map_err(|err| format!("{label} register step failed: {err:?}"))?;
    }

    let agent_resource_seeds = world
        .state()
        .agents
        .keys()
        .cloned()
        .map(|agent_id| {
            let maybe_seed = seed_agents
                .iter()
                .find(|entry| entry.0 == agent_id)
                .cloned();
            match maybe_seed {
                Some((_, _, electricity, data)) => (agent_id, electricity.max(32), data.max(8)),
                None => (agent_id, 32, 8),
            }
        })
        .collect::<Vec<_>>();

    for (agent_id, electricity, data) in agent_resource_seeds {
        world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, electricity)
            .map_err(|err| {
                format!("{label} set electricity failed agent={agent_id} err={err:?}")
            })?;
        world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, data)
            .map_err(|err| format!("{label} set data failed agent={agent_id} err={err:?}"))?;
    }
    world
        .step()
        .map_err(|err| format!("{label} resource seed consensus step failed: {err:?}"))?;

    Ok((world, config))
}

fn parse_args() -> Result<CliOptions, WorldError> {
    let mut source_world_dir = None;
    let mut generated_world_dir = None;
    let mut output_world_dir = None;
    let mut world_id = None;
    let mut chain_id = None;
    let mut resource_commit_height = None;
    let mut resource_commit_hash = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source-world-dir" => {
                source_world_dir = Some(parse_path_arg(args.next(), "--source-world-dir")?);
            }
            "--generated-world-dir" => {
                generated_world_dir = Some(parse_path_arg(args.next(), "--generated-world-dir")?);
            }
            "--output-world-dir" => {
                output_world_dir = Some(parse_path_arg(args.next(), "--output-world-dir")?);
            }
            "--world-id" => {
                let value = parse_required_value(args.next(), "--world-id")?;
                if value.trim().is_empty() {
                    return Err(WorldError::Io("--world-id cannot be empty".to_string()));
                }
                world_id = Some(value);
            }
            "--chain-id" => {
                let value = parse_required_value(args.next(), "--chain-id")?;
                if value.trim().is_empty() {
                    return Err(WorldError::Io("--chain-id cannot be empty".to_string()));
                }
                chain_id = Some(value);
            }
            "--resource-commit-height" => {
                let value = parse_required_value(args.next(), "--resource-commit-height")?;
                resource_commit_height = Some(value.parse::<u64>().map_err(|_| {
                    WorldError::Io(format!(
                        "--resource-commit-height must be a non-negative integer, got {value}"
                    ))
                })?);
            }
            "--resource-commit-hash" => {
                let value = parse_required_value(args.next(), "--resource-commit-hash")?;
                if value.trim().is_empty() {
                    return Err(WorldError::Io(
                        "--resource-commit-hash cannot be empty".to_string(),
                    ));
                }
                resource_commit_hash = Some(value);
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => {
                return Err(WorldError::Io(format!("unknown argument: {arg}")));
            }
        }
    }
    if source_world_dir.is_some() == generated_world_dir.is_some() {
        return Err(WorldError::Io(
            "provide exactly one of --source-world-dir or --generated-world-dir".to_string(),
        ));
    }

    Ok(CliOptions {
        source_world_dir,
        generated_world_dir,
        output_world_dir: output_world_dir
            .ok_or_else(|| WorldError::Io("missing --output-world-dir".to_string()))?,
        world_id,
        chain_id,
        resource_commit_height,
        resource_commit_hash,
    })
}

fn parse_required_value(value: Option<String>, flag: &str) -> Result<String, WorldError> {
    value.ok_or_else(|| WorldError::Io(format!("missing value for {flag}")))
}

fn parse_path_arg(value: Option<String>, flag: &str) -> Result<PathBuf, WorldError> {
    let raw = parse_required_value(value, flag)?;
    let path = Path::new(raw.as_str());
    if path.as_os_str().is_empty() {
        return Err(WorldError::Io(format!(
            "empty path is not allowed for {flag}"
        )));
    }
    Ok(path.to_path_buf())
}

fn print_usage() {
    println!(
        "Usage: oasis7_world_repair_rebuild (--source-world-dir <path> | --generated-world-dir <path>) --output-world-dir <path> [--world-id <id> [--chain-id <id>] [--resource-commit-height <n>] [--resource-commit-hash <hash>]]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_bootstrap_alignment_keeps_seed_state_at_chain_commit_height() {
        let (world, _) = bootstrap_runtime_world_from_model(
            WorldConfig::default(),
            &WorldModel::default(),
            "test generated bootstrap",
        )
        .expect("bootstrap generated runtime world");
        assert_eq!(world.state().time, 2);
        assert!(!world.journal().is_empty());
        assert!(!world.tick_consensus_records().is_empty());

        let aligned =
            align_generated_bootstrap_world_to_commit_height(world, 0).expect("align world");

        assert_eq!(aligned.state().time, 0);
        assert!(aligned.journal().is_empty());
        assert!(aligned.tick_consensus_records().is_empty());
        assert_eq!(aligned.state().agents.len(), 2);
        assert!(aligned.state().agents.contains_key("runtime-agent-0"));
        assert!(aligned.state().agents.contains_key("runtime-agent-1"));

        let aligned_snapshot = aligned.snapshot();
        assert_eq!(
            aligned_snapshot.journal_commitment,
            Journal::new()
                .commitment()
                .expect("empty journal commitment")
        );
        let mut tampered_snapshot = aligned_snapshot;
        tampered_snapshot.journal_commitment = "tampered".to_string();
        assert_eq!(
            oasis7::runtime_host::world::from_snapshot(tampered_snapshot, Journal::new())
                .expect_err("tampered empty journal commitment must fail"),
            WorldError::JournalMismatch
        );
    }
}
