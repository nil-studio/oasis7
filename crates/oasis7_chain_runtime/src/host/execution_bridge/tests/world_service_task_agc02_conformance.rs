//! AGC02: independent processes and the Rust SDK exercise the same native contract.
//! Scripted fixed decisions only; timings are diagnostics, never benchmarks.
use super::*;
use oasis7_game_client::{device_client::Client, operation_store::ClientStateKind};
use oasis7_runtime::{Action, MaterialLedgerId, STARTER_SMELTER_FACTORY_ID};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};
const OPERATION: &str = "agc02-fixed-build";
const SITE: &str = "starter-site";
const FACTORY: &str = STARTER_SMELTER_FACTORY_ID;

struct StopFixture(PathBuf);
impl Drop for StopFixture {
    fn drop(&mut self) {
        let _ = fs::write(self.0.join("service-stop"), b"stop");
    }
}
fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn cli(binary: &Path, config: &Path, root: &Path, phase: &str, arguments: &[&str]) -> bool {
    // Files live inside the fixture's private directory, not in diagnostic output.
    // Redirecting also avoids a pipe-buffer deadlock on a full context response.
    let output = root.join(format!("agc02-{phase}.stdout"));
    let error = root.join(format!("agc02-{phase}.stderr"));
    let file = |path: &Path| {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .unwrap()
    };
    let started = Instant::now();
    let mut child = Command::new(binary)
        .arg("--config")
        .arg(config)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(file(&output))
        .stderr(file(&error))
        .spawn()
        .unwrap();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(40) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("AGC02 CLI phase deadline: {phase}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    eprintln!(
        "AGC02 lane=cli phase={phase} elapsed_ms={} success={}",
        started.elapsed().as_millis(),
        status.success()
    );
    if status.success() {
        let _: Value = read_json(&output);
    }
    status.success()
}
fn results(world: &RuntimeWorld) -> Vec<CanonicalIntentResultV1> {
    world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|v| serde_json::from_value::<CanonicalIntentResultV1>(v.clone()).ok())
        .filter(|r| {
            matches!(
                &r.request.signed_payload,
                WorldServicePayloadV1::Cognition(_)
            )
        })
        .collect()
}
fn material_balances(world: &RuntimeWorld) -> Value {
    let entries = [
        "structural_frame",
        "heat_coil",
        "refractory_brick",
        "iron_ore",
        "carbon_fuel",
    ]
    .map(|kind| {
        (
            kind,
            world.ledger_material_balance(&MaterialLedgerId::agent("agent-a"), kind),
        )
    });
    json!(entries)
}
fn trial(binary: &Path, external_cli: bool) -> (Value, Value) {
    let lane = if external_cli { "cli" } else { "sdk" };
    let ready = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-agc02-{lane}-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let deadline = Instant::now() + Duration::from_secs(180);
    std::thread::scope(|scope| {
        let service = scope.spawn(|| {
            run_prepared_world_service_kind(
                ready.clone(),
                Duration::from_secs(5),
                false,
                None,
                false,
                Duration::from_secs(30),
                PreparedWorldKind::StarterChain,
                Duration::ZERO,
            )
        });
        while !ready.exists() {
            assert!(Instant::now() < deadline, "AGC02 readiness deadline");
            std::thread::sleep(Duration::from_millis(20));
        }
        let metadata = read_json(&ready);
        let root = PathBuf::from(metadata["root"].as_str().unwrap());
        let stop = StopFixture(root.clone());
        let config = PathBuf::from(metadata["config"].as_str().unwrap());
        let client = Client::open(&config).unwrap();
        let pin = || {
            super::super::super::super::world_service_read::pin(
                &root.join("records"),
                &root.join("store"),
                &client.world,
                None,
            )
            .unwrap()
        };
        let before = pin();
        assert!(results(&before.world).is_empty());
        assert!(before.world.state().factories.is_empty());
        let view = client.public_view().unwrap();
        let projection = view.projection();
        // Private goal salt, commit roots and capture IDs are independently checked
        // within each lane. They intentionally differ between fresh fixtures.
        let mut control = serde_json::to_value(&projection.game_task_control).unwrap();
        control
            .as_object_mut()
            .unwrap()
            .remove("active_goal_reference");
        let initial = json!({"world":client.world,"agent":client.agent,"signer":client.public_key,
            "control":control,"grants":projection.native_delegation_authorizations,
            "catalog_entries":projection.agent_context.as_ref().unwrap().capability_catalog.entries,
            "materials":material_balances(&before.world),
            "electricity":before.world.state().agents["agent-a"].state.resources.get(oasis7_agent_api::world_action::ResourceKind::Electricity)});
        let started = Instant::now();
        if external_cli {
            assert!(cli(
                binary,
                &config,
                &root,
                "context",
                &["context", "--operation", OPERATION]
            ));
        } else {
            let prefix = client.prefix(OPERATION).unwrap();
            assert!(matches!(
                client.submit_and_poll(&prefix).unwrap().outcome,
                IntentOutcome::Committed { .. }
            ));
            eprintln!(
                "AGC02 lane=sdk phase=context elapsed_ms={}",
                started.elapsed().as_millis()
            );
        }
        let captured = client.captured(OPERATION).unwrap();
        assert_eq!(
            serde_json::to_value(
                &captured
                    .prepared
                    .snapshot
                    .active_goal
                    .owner_proof
                    .request
                    .content
            )
            .unwrap(),
            serde_json::to_value(starter_chain_playtest::goal()).unwrap()
        );
        assert!(
            client
                .cached_intent(ClientStateKind::Action, OPERATION)
                .unwrap()
                .is_none()
        );
        if external_cli {
            let printed = read_json(&root.join("agc02-context.stdout"));
            assert_eq!(
                printed["goal"]["content"],
                serde_json::to_value(starter_chain_playtest::goal()).unwrap()
            );
            assert_eq!(printed["prepare_operation_id"], OPERATION);
            assert!(!printed["actor_observation"].is_null());
            assert!(!printed["capabilities"].is_null());
        }
        if external_cli {
            assert!(!cli(
                binary,
                &config,
                &root,
                "invalid",
                &[
                    "build",
                    "--context",
                    OPERATION,
                    "--site",
                    SITE,
                    "--factory",
                    FACTORY,
                    "--kind",
                    "factory.invalid"
                ]
            ));
        } else {
            assert!(
                client
                    .build(OPERATION, SITE, FACTORY, "factory.invalid")
                    .is_err()
            );
        }
        assert!(
            client
                .cached_intent(ClientStateKind::Action, OPERATION)
                .unwrap()
                .is_none()
        );
        assert!(results(&pin().world).is_empty());
        let started = Instant::now();
        if external_cli {
            assert!(cli(
                binary,
                &config,
                &root,
                "build",
                &[
                    "build",
                    "--context",
                    OPERATION,
                    "--site",
                    SITE,
                    "--factory",
                    FACTORY,
                    "--kind",
                    "factory.smelter.mk1"
                ]
            ));
        } else {
            assert!(matches!(
                client
                    .build(OPERATION, SITE, FACTORY, "factory.smelter.mk1")
                    .unwrap()
                    .outcome,
                IntentOutcome::Committed { .. }
            ));
            eprintln!(
                "AGC02 lane=sdk phase=build elapsed_ms={}",
                started.elapsed().as_millis()
            );
        }
        let original = client
            .cached_intent(ClientStateKind::Action, OPERATION)
            .unwrap()
            .unwrap();
        assert!(matches!(
            client.lookup(&original).unwrap().outcome,
            IntentOutcome::Committed { .. }
        ));
        let after = loop {
            let current = pin();
            if current.world.state().factories.contains_key(FACTORY)
                && current.world.state().pending_factory_builds.is_empty()
            {
                break current;
            }
            assert!(
                Instant::now() < deadline,
                "AGC02 factory completion deadline"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
        let canonical = results(&after.world);
        assert_eq!(canonical.len(), 1);
        assert!(canonical[0].rejected.is_none());
        assert_eq!(
            serde_json::to_value(&canonical[0].request.correlation).unwrap(),
            serde_json::to_value(&original.correlation).unwrap()
        );
        let WorldServicePayloadV1::Cognition(signed) = &canonical[0].request.signed_payload else {
            unreachable!()
        };
        assert!(matches!(
            &signed.request.action,
            Action::BuildFactory { .. }
        ));
        let applied = after
            .world
            .agent_causal_receipts("agent-a")
            .unwrap()
            .into_iter()
            .filter(|r| r.disposition == "applied" && r.action_kind == "BuildFactory")
            .collect::<Vec<_>>();
        assert_eq!(applied.len(), 1);
        // The consensus envelope ID differs from the domain action ID.
        // Canonical cognition lineage is the authenticated bridge between them.
        assert_eq!(
            canonical[0].receipt["lineage"]["action_id"],
            format!("action:{}", applied[0].action_id)
        );
        assert_eq!(
            canonical[0].receipt["lineage"]["receipt_id"],
            applied[0].receipt_id
        );
        assert_eq!(applied[0].agent_id, "agent-a");
        let authorization = applied[0].authorization.as_ref().unwrap();
        assert_eq!(authorization.grant.grant_id, "starter-build");
        assert_eq!(authorization.cost_units, 6);
        let mut factory = serde_json::to_value(&after.world.state().factories[FACTORY]).unwrap();
        // External world time is independently owned, so compare completed factory
        // semantics after verifying completion, not the wall-clock-dependent tick.
        factory.as_object_mut().unwrap().remove("built_at");
        let final_state = json!({"admitted":true,"rejected_invalid_selection":true,"action":signed.request.action,
            "materials":material_balances(&after.world), "grant":authorization.grant.grant_id,"cost":authorization.cost_units,
            "electricity":after.world.state().agents["agent-a"].state.resources.get(oasis7_agent_api::world_action::ResourceKind::Electricity),
            "factory":factory});
        // Recovery via a separate process must query the immutable original.
        if external_cli {
            assert!(cli(
                binary,
                &config,
                &root,
                "restart",
                &[
                    "build",
                    "--context",
                    OPERATION,
                    "--site",
                    SITE,
                    "--factory",
                    FACTORY,
                    "--kind",
                    "factory.smelter.mk1"
                ]
            ));
        } else {
            assert!(
                Client::open(&config)
                    .unwrap()
                    .build(OPERATION, SITE, FACTORY, "factory.smelter.mk1")
                    .is_ok()
            );
        }
        assert_eq!(results(&pin().world).len(), 1);
        assert_eq!(
            client
                .cached_intent(ClientStateKind::Action, OPERATION)
                .unwrap()
                .unwrap(),
            original
        );
        drop(stop);
        assert_eq!(service.join().unwrap(), root);
        fs::remove_file(&ready).unwrap();
        fs::remove_dir_all(root).unwrap();
        (initial, final_state)
    })
}

#[test]
#[ignore = "requires explicitly prebuilt OASIS7_AGC02_CLIENT_BIN; real local node, no inference"]
fn cli_and_sdk_http_fixed_build_admission_effect_receipt_conformance() {
    let _slot = prepared_world_probe_slot();
    let binary = PathBuf::from(
        std::env::var_os("OASIS7_AGC02_CLIENT_BIN").expect("set absolute prebuilt CLI path"),
    )
    .canonicalize()
    .unwrap();
    assert!(binary.is_file());
    let sdk = trial(&binary, false);
    let cli = trial(&binary, true);
    assert_eq!(
        sdk.0, cli.0,
        "AGC02 initial authorized business state differs"
    );
    assert_eq!(
        sdk.1, cli.1,
        "AGC02 normalized admission/effect/causality differs"
    );
}
