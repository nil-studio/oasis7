//! Actual protected P/R + Wasmtime qualified outbox + lost-reply SIGKILL.
//! The loopback custody/receiver are controlled fixtures, not deployment proof.
use super::*;
use crate::formal_effect_delivery_store::{DeliveryPhaseV1, FormalEffectDeliveryStore};
use crate::formal_effect_outbox::QualifiedEffectOutboxV1;
use crate::formal_effect_transport::test_support::LocalEffectReceiverFixture;
use std::{
    os::unix::process::ExitStatusExt,
    process::{Child, Command, Stdio},
};

pub(super) struct EffectCaseContext<'a> {
    pub(super) root: &'a Path,
    pub(super) config_path: &'a Path,
    pub(super) writer_directory: &'a Path,
    pub(super) owner_request: &'a SubmitIntentRequest<WorldServicePayloadV1>,
    pub(super) public_config: WorldServiceClientConfig,
    pub(super) owner_config: WorldServiceClientConfig,
    pub(super) initial_anchor: &'a VerifiedFormalReadAnchorV1,
}
pub(super) struct EffectScenario {
    receiver: LocalEffectReceiverFixture,
    blocked_without_contract: bool,
    new_sender_after_store_loss: bool,
}
impl EffectScenario {
    pub(super) fn new(blocked_without_contract: bool) -> Result<Self, String> {
        Ok(Self {
            receiver: LocalEffectReceiverFixture::start(
                vec![fixtures::public(90)],
                !blocked_without_contract,
                !blocked_without_contract,
            )?,
            blocked_without_contract,
            new_sender_after_store_loss: false,
        })
    }
    fn new_store_loss() -> Result<Self, String> {
        Ok(Self {
            receiver: LocalEffectReceiverFixture::start(
                vec![fixtures::public(90), fixtures::public(92)],
                true,
                true,
            )?,
            blocked_without_contract: false,
            new_sender_after_store_loss: true,
        })
    }
    pub(super) fn prepare_world(&self, world: &mut oasis7_runtime::World) -> Result<(), String> {
        let contract = self.receiver.contract();
        let output = oasis7_wasm_abi::ModuleOutput {
            new_state: Some((1..=16).collect()),
            effects: vec![oasis7_wasm_abi::ModuleEffectIntent {
                kind: "http.request".into(),
                cap_ref: "c13-effects".into(),
                cap_slot: None,
                params: serde_json::json!({"schema_version":1,"target_id":contract.target_id,
                    "receiver_contract_digest":contract.canonical_digest()?,"payload":{"message":"original qualified C13"}}),
            }],
            emits: vec![oasis7_wasm_abi::ModuleEmit {
                kind: "wasm.test".into(),
                payload: serde_json::json!({"ok":true}),
            }],
            tick_lifecycle: None,
            output_bytes: 0,
        };
        let empty = oasis7_wasm_abi::ModuleOutput {
            new_state: None,
            effects: vec![],
            emits: vec![],
            tick_lifecycle: None,
            output_bytes: 0,
        };
        let wasm = one_shot_reducer(
            &serde_cbor::to_vec(&output).map_err(|e| e.to_string())?,
            &serde_cbor::to_vec(&empty).map_err(|e| e.to_string())?,
        );
        let hash = hex::encode(Sha256::digest(&wasm));
        world
            .register_module_artifact(hash.clone(), &wasm)
            .map_err(|e| format!("C13 artifact: {e:?}"))?;
        world.add_capability(oasis7_runtime::CapabilityGrant::allow_all("c13-effects"));
        let module = ModuleManifest {
            module_id: "m.formal-c13-one-shot".into(),
            name: "C13 persistent reducer".into(),
            version: "0.1.0".into(),
            kind: ModuleKind::Reducer,
            role: ModuleRole::Domain,
            wasm_hash: hash.clone(),
            interface_version: "wasm-1".into(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["reduce".into()],
            subscriptions: vec![ModuleSubscription {
                event_kinds: vec![],
                action_kinds: vec!["action.register_agent".into()],
                stage: Some(ModuleSubscriptionStage::PostAction),
                filters: None,
            }],
            required_caps: vec!["c13-effects".into()],
            artifact_identity: Some(common::signed_test_artifact_identity(&hash)),
            limits: ModuleLimits {
                max_mem_bytes: 64 * 1024,
                max_gas: 10_000_000,
                max_call_rate: 1,
                max_output_bytes: 4096,
                max_effects: 1,
                max_emits: 1,
            },
        };
        let changes = ModuleChangeSet {
            register: vec![module.clone()],
            activate: vec![ModuleActivation {
                module_id: module.module_id,
                version: module.version,
            }],
            ..Default::default()
        };
        let proposal = world
            .propose_manifest_update(
                Manifest {
                    version: 2,
                    content: serde_json::json!({"module_changes":changes}),
                },
                "alice",
            )
            .map_err(|e| format!("C13 proposal: {e:?}"))?;
        world
            .shadow_proposal(proposal)
            .map_err(|e| format!("C13 shadow: {e:?}"))?;
        world
            .approve_proposal(proposal, "bob", ProposalDecision::Approve)
            .map_err(|e| format!("C13 approval: {e:?}"))?;
        world
            .apply_proposal(proposal)
            .map(|_| ())
            .map_err(|e| format!("C13 activation: {e:?}"))
    }
    pub(super) fn decorate_config(&self, value: &mut Value) -> Result<(), String> {
        value["effect_delivery"] = serde_json::json!({"routes":[self.receiver.contract()]});
        Ok(())
    }
    pub(super) fn exercise(&self, ctx: EffectCaseContext<'_>) -> Result<(), String> {
        let mut child = start_child(&ctx, "first")?;
        let client =
            RemoteWorldServiceClient::new(ctx.public_config.clone()).map_err(|e| e.to_string())?;
        let owner =
            RemoteWorldServiceClient::new(ctx.owner_config.clone()).map_err(|e| e.to_string())?;
        let world = &ctx.public_config.expected_world;
        let _ = read_current(&client, world)?;
        assert_volatile_admission(
            client
                .submit(ctx.owner_request.clone())
                .map_err(|e| e.to_string())?,
            &ctx.owner_request.correlation,
        )?;
        let deadline = Instant::now() + Duration::from_secs(60);
        if !self.blocked_without_contract && !self.receiver.wait_for_execution(deadline) {
            return Err(format!(
                "C13 receiver execution deadline: {:?}",
                self.receiver.last_error()
            ));
        }
        if self.blocked_without_contract {
            // No POST is allowed; wait for the ordinary externally verified
            // original result before killing the process.
            let mut qualified = false;
            while Instant::now() < deadline {
                if lookup_original_commit(&owner, ctx.owner_request).is_ok() {
                    qualified = true;
                    break;
                }
                ensure_alive(&mut child)?;
                thread::sleep(Duration::from_millis(20));
            }
            if !qualified {
                return Err("C13 no-contract original gameplay was not qualified".into());
            }
        }
        if !self.blocked_without_contract && self.receiver.executions() != 1 {
            return Err("C13 first receiver effect count differs".into());
        }
        kill(&mut child)?;
        let restored = FormalLiveRuntime::open_with_native_history(ctx.config_path)?;
        let before = restored.published.current()?;
        if before.anchor.initial_certificate() != ctx.initial_anchor.initial_certificate() {
            return Err("C13 reopen changed independent initial origin".into());
        }
        let snapshot = serde_cbor::to_vec(&before.world.snapshot()).map_err(|e| e.to_string())?;
        let historical = restored.history.full_sync_material(&before.anchor)?;
        let outbox = QualifiedEffectOutboxV1::from_qualified_material(&before.anchor, &historical)?;
        if outbox.obligations.len() != 1 {
            return Err(format!(
                "C13 actual Wasmtime outbox expected one effect: {}",
                outbox.obligations.len()
            ));
        }
        let obligation = &outbox.obligations[0];
        if obligation.identity.birth_era != 0
            || obligation.committed_at.position <= ctx.initial_anchor.head().record().position
        {
            return Err("C13 effect lacks genuine formal birth/qualification".into());
        }
        let original_binding = obligation.identity.clone();
        let original_payload = obligation.payload_digest.clone();
        let first_source;
        let frozen_head = before.anchor.checkpoint();
        drop(restored);
        // Opening the sidecar store does not authorize a POST. Inspect only
        // while the runtime worker is stopped; close it before start.
        {
            let store =
                FormalEffectDeliveryStore::open(&ctx.writer_directory.join("effect-delivery-v1"))?;
            let record = store
                .read(&original_binding)?
                .ok_or("C13 first qualified delivery record missing")?;
            if record.obligation.payload_digest != original_payload {
                return Err("C13 durable obligation binding changed".into());
            }
            if !self.blocked_without_contract
                && matches!(record.phase, DeliveryPhaseV1::Delivered { .. })
            {
                return Err("C13 kill missed the lost-reply Unknown window".into());
            }
            first_source = record.obligation.committed_at.clone();
        }
        let mut restored = FormalLiveRuntime::open_with_native_history(ctx.config_path)?;
        if restored.published.current()?.anchor.checkpoint() != frozen_head {
            return Err("C13 stopped reopen changed qualified cut".into());
        }
        let source = restored.history.origin_publication(
            &oasis7_client_api::world_service::FormalReadAnchorRefV1 {
                position: first_source.position,
                record_id: first_source.record_id.clone(),
                active_authority_id: first_source.active_authority_id.clone(),
            },
        )?;
        let source_material = restored.history.full_sync_material(source.anchor())?;
        let source_outbox =
            QualifiedEffectOutboxV1::from_qualified_material(source.anchor(), &source_material)?;
        if !source_outbox.obligations.iter().any(|effect| {
            effect.identity == original_binding
                && effect.payload_digest == original_payload
                && effect.committed_at == first_source
        }) {
            return Err("C13 sidecar provenance was not independently qualified".into());
        }
        if serde_cbor::to_vec(&restored.published.current()?.world.snapshot())
            .map_err(|e| e.to_string())?
            != snapshot
        {
            return Err("C13 sidecar inspection mutated committed World".into());
        }
        if self.new_sender_after_store_loss {
            // Preserve the lost store as test evidence. Neither its contents
            // nor the new store can authorize delivery: source_outbox above
            // was independently restored from the fixed external C0.
            drop(restored);
            let active_store = ctx.writer_directory.join("effect-delivery-v1");
            let lost_store = ctx.writer_directory.join("effect-delivery-lost-fixture");
            std::fs::rename(&active_store, &lost_store)
                .map_err(|e| format!("C13 simulate sidecar loss: {e}"))?;
            let queries_before = self.receiver.queries();
            let posts_before = self.receiver.posts();
            let executions_before = self.receiver.executions();
            {
                let store = std::sync::Arc::new(FormalEffectDeliveryStore::open(&active_store)?);
                if store.read(&original_binding)?.is_some() {
                    return Err("C13 replacement store was not empty".into());
                }
                let transport = crate::formal_effect_transport::FormalEffectDeliveryTransport::new(
                    vec![self.receiver.contract().clone()],
                    fixtures::key(92),
                    store.clone(),
                )?;
                crate::formal_effect_outbox::validate_qualified_effect_outbox(&source_outbox)?;
                let results: Vec<_> = source_outbox
                    .obligations
                    .iter()
                    .map(|obligation| transport.drive_qualified_obligation(obligation))
                    .collect();
                let record = store
                    .read(&original_binding)?
                    .ok_or("C13 replacement store did not retain original receipt")?;
                if !matches!(record.phase, DeliveryPhaseV1::Delivered { .. })
                    || record.obligation.identity != original_binding
                    || record.obligation.payload_digest != original_payload
                    || record.obligation.committed_at != first_source
                {
                    return Err(format!(
                        "C13 replacement sender failed original signed-receipt query: {results:?}"
                    ));
                }
            }
            if self.receiver.posts() != posts_before
                || self.receiver.executions() != executions_before
                || executions_before != 1
                || self.receiver.queries() <= queries_before
            {
                return Err("C13 replacement store/sender repeated POST or effect".into());
            }
            let observations = self.receiver.observations();
            if !observations.iter().any(|r| {
                r.sender_public_key_hex == fixtures::public(92)
                    && r.operation == "query"
                    && r.identity == original_binding
            }) || observations.iter().any(|r| {
                r.sender_public_key_hex == fixtures::public(92) && r.operation == "deliver"
            }) {
                return Err(
                    "C13 replacement sender did not exclusively query original identity".into(),
                );
            }
            restored = FormalLiveRuntime::open_with_native_history(ctx.config_path)?;
            if restored.published.current()?.anchor.checkpoint() != frozen_head
                || serde_cbor::to_vec(&restored.published.current()?.world.snapshot())
                    .map_err(|e| e.to_string())?
                    != snapshot
            {
                return Err("C13 delivery-store loss changed committed World/cut".into());
            }
        }
        restored.start()?;
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let status = restored
                .state
                .effect_delivery
                .lock()
                .map_err(|_| "C13 delivery status lock poisoned")?
                .clone();
            if self.blocked_without_contract {
                if status.as_ref().is_some_and(|s| s.blocked >= 1) {
                    break;
                }
            } else if status.as_ref().is_some_and(|s| s.terminal >= 1) {
                break;
            }
            if Instant::now() >= until {
                return Err("C13 restarted delivery did not reach expected status".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
        let owner =
            RemoteWorldServiceClient::new(ctx.owner_config.clone()).map_err(|e| e.to_string())?;
        let original = lookup_original_commit(&owner, ctx.owner_request)?;
        if original.position != first_source.position {
            return Err(
                "C13 effect did not originate in the original gameplay qualified cut".into(),
            );
        }
        if lookup_original_commit(&owner, ctx.owner_request)? != original {
            return Err("C13 retry changed original CommitRef".into());
        }
        assert_volatile_admission(
            client
                .submit(ctx.owner_request.clone())
                .map_err(|e| e.to_string())?,
            &ctx.owner_request.correlation,
        )?;
        restored.stop(Duration::from_secs(30))?;
        drop(restored);
        let store =
            FormalEffectDeliveryStore::open(&ctx.writer_directory.join("effect-delivery-v1"))?;
        let record = store
            .read(&original_binding)?
            .ok_or("C13 recovered delivery record missing")?;
        if record.obligation.committed_at != first_source
            || record.obligation.payload_digest != original_payload
        {
            return Err("C13 reopen replaced original effect evidence".into());
        }
        if self.blocked_without_contract {
            if self.receiver.executions() != 0
                || self.receiver.posts() != 0
                || self.receiver.queries() != 0
            {
                return Err("C13 receiver without retry contract was POSTed".into());
            }
        } else {
            if !matches!(record.phase, DeliveryPhaseV1::Delivered { .. })
                || self.receiver.executions() != 1
                || self.receiver.queries() == 0
                || self.receiver.posts() != 1
            {
                return Err(
                    "C13 original effect was not resolved by signed query exactly once".into(),
                );
            }
            let observations = self.receiver.observations();
            let stable = observations
                .first()
                .ok_or("C13 signed receiver observations missing")?
                .stable_effect_digest
                .clone();
            if observations.iter().any(|r| {
                r.identity != original_binding
                    || r.stable_effect_digest != stable
                    || (r.sender_public_key_hex != fixtures::public(90)
                        && !(self.new_sender_after_store_loss
                            && r.sender_public_key_hex == fixtures::public(92)))
            }) {
                return Err("C13 changed external effect identity".into());
            }
        }
        Ok(())
    }
}

struct RuntimeChild(Child);
impl Drop for RuntimeChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn ensure_alive(child: &mut RuntimeChild) -> Result<(), String> {
    if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
        Err(format!("C13 runtime child exited {status}"))
    } else {
        Ok(())
    }
}
fn kill(child: &mut RuntimeChild) -> Result<(), String> {
    child.0.kill().map_err(|e| e.to_string())?;
    let status = child.0.wait().map_err(|e| e.to_string())?;
    if status.signal() != Some(9) {
        return Err(format!("C13 expected SIGKILL: {status}"));
    }
    Ok(())
}
fn start_child(ctx: &EffectCaseContext<'_>, tag: &str) -> Result<RuntimeChild, String> {
    let ready = ctx.root.join(format!("c13-{tag}-ready"));
    let mut child = RuntimeChild(
        Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args([
                "protected_runtime_crash_child",
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("OASIS7_FORMAL_TEST_CHILD_CONFIG", ctx.config_path)
            .env("OASIS7_FORMAL_TEST_CHILD_READY", &ready)
            .env_remove("OASIS7_FORMAL_TEST_CRASH_PHASE")
            .env_remove("OASIS7_FORMAL_TEST_CRASH_REQUEST")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ready.exists() {
        ensure_alive(&mut child)?;
        if Instant::now() >= deadline {
            return Err("C13 runtime ready deadline".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    if std::fs::read(ready).map_err(|e| e.to_string())? != b"ready" {
        return Err("C13 child marker malformed".into());
    }
    Ok(child)
}

/// Minimal real reducer: first invocation stores a persistent CBOR state;
/// subsequent inputs containing that state return no effects. No host mock.
fn one_shot_reducer(first: &[u8], again: &[u8]) -> Vec<u8> {
    fn u(mut n: u64) -> Vec<u8> {
        let mut b = vec![];
        loop {
            let x = (n & 127) as u8;
            n >>= 7;
            b.push(x | if n != 0 { 128 } else { 0 });
            if n == 0 {
                return b;
            }
        }
    }
    fn s(mut n: i64) -> Vec<u8> {
        let mut b = vec![];
        loop {
            let x = (n & 127) as u8;
            n >>= 7;
            let done = (n == 0 && x & 64 == 0) || (n == -1 && x & 64 != 0);
            b.push(x | if done { 0 } else { 128 });
            if done {
                return b;
            }
        }
    }
    fn section(w: &mut Vec<u8>, id: u8, b: Vec<u8>) {
        w.push(id);
        w.extend(u(b.len() as u64));
        w.extend(b);
    }
    fn c(n: i64) -> Vec<u8> {
        let mut b = vec![0x41];
        b.extend(s(n));
        b
    }
    let mut w = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut w,
        1,
        vec![
            2, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 2, 0x7f, 0x7f, 2, 0x7f, 0x7f,
        ],
    );
    section(&mut w, 3, vec![2, 0, 1]);
    section(&mut w, 5, vec![1, 0, 1]);
    let mut exports = vec![3];
    for (name, kind, index) in [("memory", 2, 0), ("alloc", 0, 0), ("reduce", 0, 1)] {
        exports.extend(u(name.len() as u64));
        exports.extend(name.as_bytes());
        exports.extend([kind, index]);
    }
    section(&mut w, 7, exports);
    let mut alloc = vec![0];
    alloc.extend(c(4096));
    alloc.push(0x0b);
    let mut call = vec![
        1, 1, 0x7f, 0x20, 0, 0x21, 2, 0x02, 0x40, 0x03, 0x40, 0x20, 2, 0x20, 0, 0x20, 1, 0x6a,
    ];
    call.extend(c(8));
    call.extend([0x6b, 0x4b, 0x0d, 1, 0x20, 2, 0x29, 0, 0, 0x42]);
    call.extend(s(0x0807060504030201));
    call.extend([0x51, 0x04, 0x40]);
    call.extend(c(2048));
    call.extend(c(again.len() as i64));
    call.extend([0x0f, 0x0b, 0x20, 2]);
    call.extend(c(1));
    call.extend([0x6a, 0x21, 2, 0x0c, 0, 0x0b, 0x0b]);
    call.extend(c(0));
    call.extend(c(first.len() as i64));
    call.push(0x0b);
    let mut code = vec![2];
    for body in [alloc, call] {
        code.extend(u(body.len() as u64));
        code.extend(body);
    }
    section(&mut w, 10, code);
    let mut data = vec![2];
    for (offset, bytes) in [(0, first), (2048, again)] {
        data.push(0);
        data.extend(c(offset));
        data.push(0x0b);
        data.extend(u(bytes.len() as u64));
        data.extend(bytes);
    }
    section(&mut w, 11, data);
    w
}
#[test]
#[ignore = "requires protected endpoint binary; actual receiver lost reply and runtime SIGKILL"]
fn protected_endpoint_children_effect_lost_reply_sigkill_resolves_original_once() {
    run_effect(false).unwrap();
}
#[test]
#[ignore = "requires protected endpoint binary; actual no-contract receiver refusal"]
fn protected_endpoint_children_effect_without_retry_contract_never_posts_after_restart() {
    run_effect(true).unwrap();
}
fn run_effect(blocked: bool) -> Result<(), String> {
    thread::Builder::new()
        .name("formal-c13-runtime".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let scenario = EffectScenario::new(blocked)?;
            run_protected_runtime_effect_case(&scenario)
        })
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "C13 fixture panic".to_string())?
}

#[test]
#[ignore = "requires protected endpoint binary; actual qualified effect and fresh sender/store"]
fn protected_endpoint_children_effect_lost_store_new_authorized_sender_queries_original_receipt() {
    thread::Builder::new()
        .name("formal-c13-store-loss".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            let scenario = EffectScenario::new_store_loss()?;
            run_protected_runtime_effect_case(&scenario)
        })
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
}
