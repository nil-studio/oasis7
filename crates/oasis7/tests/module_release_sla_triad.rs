#![cfg(feature = "test_tier_full")]

mod common;

use oasis7::GeoPos;
use oasis7::runtime::*;
use oasis7_agent_api::world_action::{ModuleInstallTarget, ResourceKind};
use oasis7_wasm_abi::MaterialStack;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

const SLA_THRESHOLD_MS: u128 = 60_000;
const SAMPLE_COUNT: usize = 20;

fn register_agent(world: &mut World, agent_id: &str, pos: GeoPos) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos,
    });
    world.step().expect("register agent");
    world
        .set_agent_resource_balance(agent_id, ResourceKind::Electricity, 50_000)
        .expect("seed electricity");
    world
        .set_agent_resource_balance(agent_id, ResourceKind::Data, 50_000)
        .expect("seed data");
}

fn bind_release_roles(world: &mut World, operator: &str, target: &str, roles: &[&str]) {
    world.submit_action(Action::ModuleReleaseBindRoles {
        operator_agent_id: operator.to_string(),
        target_agent_id: target.to_string(),
        roles: roles.iter().map(|role| role.to_string()).collect(),
    });
    world.step().expect("bind release roles");
}

fn module_manifest(module_id: &str, version: &str, wasm_hash: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.to_string(),
        name: format!("module-{module_id}"),
        version: version.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.to_string(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits::unbounded(),
    }
}

fn profile_changes(idx: usize) -> ModuleProfileChanges {
    ModuleProfileChanges {
        product_profiles: vec![ProductProfileV1 {
            product_id: format!("product.sla.{idx}"),
            role_tag: "scale".to_string(),
            maintenance_sink: vec![MaterialStack::new("hardware_part", 1)],
            tradable: true,
            unlock_stage: "sla".to_string(),
        }],
        recipe_profiles: vec![RecipeProfileV1 {
            recipe_id: format!("recipe.sla.{idx}"),
            bottleneck_tags: vec!["control_chip".to_string()],
            stage_gate: "sla".to_string(),
            preferred_factory_tags: vec!["assembly".to_string()],
        }],
        factory_profiles: vec![FactoryProfileV1 {
            factory_id: format!("factory.sla.{idx}"),
            tier: 1,
            recipe_slots: 2,
            tags: vec!["assembly".to_string()],
        }],
    }
}

fn wasm_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn percentile(samples: &mut [u128], percentile: u32) -> u128 {
    samples.sort_unstable();
    let len = samples.len();
    let rank = ((len * percentile as usize) + 99) / 100;
    let index = rank.saturating_sub(1).min(len.saturating_sub(1));
    samples[index]
}

fn write_report(durations: &[u128], p50_ms: u128, p95_ms: u128, min_ms: u128, max_ms: u128) {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let report_dir = repo_root.join("output/world-runtime");
    fs::create_dir_all(&report_dir).expect("create report dir");
    let report_path = report_dir.join("module_release_sla_triad.json");

    let report = json!({
        "scenario": "triad_release_sla",
        "sample_count": durations.len(),
        "p50_ms": p50_ms,
        "p95_ms": p95_ms,
        "min_ms": min_ms,
        "max_ms": max_ms,
        "threshold_ms": SLA_THRESHOLD_MS,
        "status": if p95_ms <= SLA_THRESHOLD_MS { "pass" } else { "fail" },
        "samples_ms": durations,
    });

    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report).expect("serialize report"),
    )
    .expect("write report");
}

#[test]
fn triad_release_sla_submit_to_apply_p95_under_60s() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    register_agent(
        &mut world,
        "publisher",
        GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
    );
    register_agent(
        &mut world,
        "approver-security",
        GeoPos {
            x_cm: 10,
            y_cm: 0,
            z_cm: 0,
        },
    );
    register_agent(
        &mut world,
        "approver-economy",
        GeoPos {
            x_cm: 20,
            y_cm: 0,
            z_cm: 0,
        },
    );

    let wasm_bytes = b"module-release-sla-triad".to_vec();
    let wasm_hash = wasm_hash(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    bind_release_roles(
        &mut world,
        "approver-security",
        "approver-security",
        &["security"],
    );
    bind_release_roles(
        &mut world,
        "approver-security",
        "approver-economy",
        &["economy"],
    );
    bind_release_roles(&mut world, "approver-security", "publisher", &["runtime"]);

    let mut durations_ms: Vec<u128> = Vec::with_capacity(SAMPLE_COUNT);
    for idx in 0..SAMPLE_COUNT {
        let manifest = module_manifest(
            format!("m.release.sla.{idx}").as_str(),
            format!("0.1.{idx}").as_str(),
            wasm_hash.as_str(),
        );

        let start = Instant::now();
        let event_start = world.journal().events.len();
        world.submit_action(Action::ModuleReleaseSubmit {
            requester_agent_id: "publisher".to_string(),
            manifest,
            activate: true,
            install_target: ModuleInstallTarget::SelfAgent,
            required_roles: vec![
                "security".to_string(),
                "economy".to_string(),
                "runtime".to_string(),
            ],
            profile_changes: profile_changes(idx),
        });
        world.step().expect("submit module release");

        let request_id = world
            .journal()
            .events
            .get(event_start..)
            .unwrap_or_default()
            .iter()
            .find_map(|event| match &event.body {
                WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested {
                    request_id, ..
                }) => Some(*request_id),
                _ => None,
            })
            .expect("module release requested event");

        world.submit_action(Action::ModuleReleaseShadow {
            operator_agent_id: "approver-security".to_string(),
            request_id,
        });
        world.step().expect("shadow module release");

        world.submit_action(Action::ModuleReleaseApproveRole {
            approver_agent_id: "approver-security".to_string(),
            request_id,
            role: "security".to_string(),
        });
        world.step().expect("approve security role");

        world.submit_action(Action::ModuleReleaseApproveRole {
            approver_agent_id: "approver-economy".to_string(),
            request_id,
            role: "economy".to_string(),
        });
        world.step().expect("approve economy role");

        world.submit_action(Action::ModuleReleaseApproveRole {
            approver_agent_id: "publisher".to_string(),
            request_id,
            role: "runtime".to_string(),
        });
        world.step().expect("approve runtime role");

        world.submit_action(Action::ModuleReleaseApply {
            operator_agent_id: "approver-security".to_string(),
            request_id,
        });
        world.step().expect("apply module release");

        let duration_ms = start.elapsed().as_millis();
        durations_ms.push(duration_ms);
    }

    let min_ms = *durations_ms.iter().min().expect("min duration");
    let max_ms = *durations_ms.iter().max().expect("max duration");
    let mut p50_samples = durations_ms.clone();
    let mut p95_samples = durations_ms.clone();
    let p50_ms = percentile(&mut p50_samples, 50);
    let p95_ms = percentile(&mut p95_samples, 95);

    write_report(&durations_ms, p50_ms, p95_ms, min_ms, max_ms);

    assert!(
        p95_ms <= SLA_THRESHOLD_MS,
        "submit->apply p95 {p95_ms}ms exceeds SLA {SLA_THRESHOLD_MS}ms"
    );
}
