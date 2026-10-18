//! Runtime contract for the trusted module-command authorization lane; provider
//! responses remain candidates until all live authority checks pass.
use super::super::*;
use super::capability_grant_v2_catalog::budget_account_snapshot;
use super::pos;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{
    AgentCommandResponse, CapabilityCatalogSnapshot, CapabilityGrantV2, CapabilitySubject,
    ModuleCallFailure, ModuleCallRequest, ModuleCommandDeclaration, ModuleEffectIntent, ModuleEmit,
    ModuleOutput, ModuleSandbox, ModuleSchemaDeclarations, canonical_hash,
};
use oasis7_wasm_executor::WasmExecutor;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub(super) const MODULE_ID: &str = "module.weather";
pub(super) const MODULE_VERSION: &str = "1.0.0";
pub(super) const WORLD_ID: &str = "world.test";
pub(super) const BRANCH_ID: &str = "branch-1";
pub(super) const SUBJECT_ID: &str = "agent-7";
pub(super) const PRESENTER_ID: &str = "provider-1";
pub(super) const ISSUER_ID: &str = "governance-1";
pub(super) const FINALITY_SIGNER_2: &str = "governance-finality-2";
pub(super) const SCHEMA_HASH: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
#[derive(Default)]
pub(super) struct RecordingSandbox {
    pub(super) calls: usize,
}
impl ModuleSandbox for RecordingSandbox {
    fn call(&mut self, _request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.calls += 1;
        Ok(ModuleOutput {
            new_state: Some(vec![0x42]),
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        })
    }
}
pub(super) struct ConfiguredSandbox {
    pub(super) calls: usize,
    pub(super) output: ModuleOutput,
}
#[derive(Default)]
pub(super) struct ProvenanceSandbox {
    pub(super) requests: Vec<ModuleCallRequest>,
}
impl ModuleSandbox for ProvenanceSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.requests.push(request.clone());
        Ok(ModuleOutput {
            new_state: Some(vec![0x42]),
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        })
    }
}
impl ModuleSandbox for ConfiguredSandbox {
    fn call(&mut self, _request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.calls += 1;
        Ok(self.output.clone())
    }
}
pub(super) fn grant_json(overrides: Value) -> Value {
    let mut grant = json!({
        "grant_id": "grant-weather-1",
        "grant_version": 2,
        "subject": {
            "kind": "agent",
            "agent_id": SUBJECT_ID,
            "owner_binding": "owner-7",
            "generation": 1
        },
        "audience": {
            "world_id": WORLD_ID,
            "branch_id": BRANCH_ID,
            "finality_epoch": 4,
            "target_kind": "world",
            "target_id": null
        },
        "issuer": {
            "issuer_id": ISSUER_ID,
            "issuer_kind": "governance",
            "governance_epoch": 9,
            "finalized_receipt_id": "finality-9",
            "key_id": "governance-key-1",
            "issuer_key_epoch": 3,
            "authority_rotation_receipt_id": null,
            "signature": "ed25519:trusted-signature"
        },
        "scope": {
            "module_id": MODULE_ID,
            "module_version": MODULE_VERSION,
            "namespace": "weather",
            "object_kind": "command",
            "object_name": "observe",
            "operation": "execute",
            "entity_selector": null,
            "resource_selector": null,
            "max_payload_bytes": 128,
            "policy_class": "read-only"
        },
        "issued_at_tick": 0,
        "expires_at_tick": 100,
        "grant_nonce": "grant-nonce-1",
        "parent_grant_id": null,
        "delegation_depth": 0,
        "revocation_epoch": 2,
        "status": "verified",
        "canonical_body_hash": "body-hash-weather-1",
        "issuance_signature": "ed25519:trusted-signature"
    });
    merge_json(&mut grant, overrides);
    grant
}
pub(super) fn capability_issuer_signing_key() -> SigningKey {
    let seed = util::sha256_hex(b"oasis7-test-capability-issuer-v2");
    let seed_bytes = hex::decode(seed).expect("decode capability issuer signing seed");
    let private_key_bytes: [u8; 32] = seed_bytes
        .as_slice()
        .try_into()
        .expect("capability issuer signing seed is 32 bytes");
    SigningKey::from_bytes(&private_key_bytes)
}
pub(super) fn capability_finality_signing_key_2() -> SigningKey {
    let seed = util::sha256_hex(b"oasis7-test-capability-finality-signer-2-v1");
    let seed_bytes = hex::decode(seed).expect("decode capability finality signing seed");
    let private_key_bytes: [u8; 32] = seed_bytes
        .as_slice()
        .try_into()
        .expect("capability finality signing seed is 32 bytes");
    SigningKey::from_bytes(&private_key_bytes)
}
pub(super) fn signed_grant(value: Value) -> CapabilityGrantV2 {
    let mut grant: CapabilityGrantV2 = typed(value);
    let body_hash = grant
        .canonical_body_hash()
        .expect("compute canonical capability grant body hash");
    grant.canonical_body_hash = body_hash.clone();
    grant.grant_id = body_hash;
    if grant.issuer.issuer_id == ISSUER_ID
        && grant.issuer.signature == "ed25519:trusted-signature"
        && grant.issuance_signature == "ed25519:trusted-signature"
    {
        let signature = capability_issuer_signing_key().sign(
            grant
                .canonical_body_bytes()
                .expect("encode canonical capability grant body")
                .as_slice(),
        );
        let encoded = format!("ed25519:{}", hex::encode(signature.to_bytes()));
        grant.issuer.signature = encoded.clone();
        grant.issuance_signature = encoded;
    }
    grant
}
pub(super) fn signed_effect_grant() -> CapabilityGrantV2 {
    signed_grant(grant_json(json!({
        "grant_nonce": "effect-grant-nonce-1",
        "scope": {
            "object_kind": "effect",
            "object_name": "weather.publish",
            "operation": "invoke"
        }
    })))
}
pub(super) fn signed_effect_grant_with_selectors() -> CapabilityGrantV2 {
    signed_grant(grant_json(json!({
        "grant_nonce": "effect-grant-selector-nonce-1",
        "scope": {
            "object_kind": "effect",
            "object_name": "weather.publish",
            "operation": "invoke",
            "entity_selector": ["station-1"],
            "resource_selector": ["weather.read"]
        }
    })))
}
pub(super) fn catalog_json(overrides: Value) -> Value {
    let mut catalog = json!({
        "snapshot_id": "catalog-weather-1",
        "world_id": WORLD_ID,
        "world_head": 11,
        "branch_id": BRANCH_ID,
        "finality_epoch": 4,
        "logical_tick": 10,
        "module_registry_hash": "registry-hash-1",
        "policy_hash": "policy-hash-1",
        "revocation_epoch": 2,
        "subject": {
            "kind": "agent",
            "agent_id": SUBJECT_ID,
            "owner_binding": "owner-7",
            "generation": 1
        },
        "presenter": {
            "presenter_id": PRESENTER_ID,
            "presenter_kind": "provider",
            "session_id": "session-1",
            "attestation_ref": null
        },
        "audience": {
            "world_id": WORLD_ID,
            "branch_id": BRANCH_ID,
            "finality_epoch": 4,
            "target_kind": "world",
            "target_id": null
        },
        "entries": [{
            "module_id": MODULE_ID,
            "module_version": MODULE_VERSION,
            "namespace": "weather",
            "command": "observe",
            "schema_version": 1,
            "schema_hash": SCHEMA_HASH,
            "max_payload_bytes": 128,
            "eligible_grant_ids": ["grant-weather-1"]
        }],
        "valid_until_tick": 20
    });
    merge_json(&mut catalog, overrides);
    catalog
}
pub(super) fn response_json(overrides: Value) -> Value {
    let mut response = json!({
        "response_nonce": "response-1",
        "subject": {
            "kind": "agent",
            "agent_id": SUBJECT_ID,
            "owner_binding": "owner-7",
            "generation": 1
        },
        "presenter": {
            "presenter_id": PRESENTER_ID,
            "presenter_kind": "provider",
            "session_id": "session-1",
            "attestation_ref": null
        },
        "audience": {
            "world_id": WORLD_ID,
            "branch_id": BRANCH_ID,
            "finality_epoch": 4,
            "target_kind": "world",
            "target_id": null
        },
        "catalog_snapshot_id": "catalog-weather-1",
        "selected_entry": {
            "module_id": MODULE_ID,
            "module_version": MODULE_VERSION,
            "namespace": "weather",
            "command": "observe",
            "schema_version": 1,
            "schema_hash": SCHEMA_HASH,
            "max_payload_bytes": 128
        },
        "envelope": {
            "namespace": "weather",
            "name": "observe",
            "schema_version": 1,
            "schema_hash": SCHEMA_HASH,
            "payload": [123, 125]
        },
        "provider_id": PRESENTER_ID,
        "trace_id": "trace-weather-1"
    });
    merge_json(&mut response, overrides);
    response
}
fn merge_json(base: &mut Value, overrides: Value) {
    match (base, overrides) {
        (Value::Object(base), Value::Object(overrides)) => {
            for (key, value) in overrides {
                match base.get_mut(&key) {
                    Some(existing) => merge_json(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overrides) => *base = overrides,
    }
}
fn typed<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("decode authorization fixture")
}
pub(super) fn prepared_catalog(
    world: &World,
    grant: &CapabilityGrantV2,
    value: Value,
) -> CapabilityCatalogSnapshot {
    let mut catalog: CapabilityCatalogSnapshot = typed(value);
    if catalog.world_head == 11 {
        catalog.world_head = world
            .journal()
            .events
            .last()
            .map(|event| event.id)
            .unwrap_or(0);
    }
    if catalog.logical_tick == 10 {
        catalog.logical_tick = world.state().time;
    }
    if catalog.module_registry_hash == "registry-hash-1" {
        catalog.module_registry_hash =
            canonical_hash(world.module_registry()).expect("compute live module registry hash");
    }
    if catalog.policy_hash == "policy-hash-1" {
        catalog.policy_hash = canonical_hash(world.policies()).expect("compute live policy hash");
    }
    if catalog.revocation_epoch == 2 {
        catalog.revocation_epoch = world.capability_revocation_state().epoch;
    }
    for entry in &mut catalog.entries {
        if entry.eligible_grant_ids == ["grant-weather-1"] {
            entry.eligible_grant_ids = vec![grant.grant_id.clone()];
        }
    }
    if catalog.snapshot_id == "catalog-weather-1" {
        catalog.snapshot_id = catalog
            .canonical_hash()
            .expect("compute canonical catalog snapshot hash");
    }
    catalog
}
pub(super) fn prepared_response(
    value: Value,
    catalog: &CapabilityCatalogSnapshot,
) -> AgentCommandResponse {
    let mut response: AgentCommandResponse = typed(value);
    if response.catalog_snapshot_id == "catalog-weather-1" {
        response.catalog_snapshot_id = catalog.snapshot_id.clone();
    }
    response
}
fn activate_module_for_test(world: &mut World, manifest: ModuleManifest) {
    let changes = ModuleChangeSet {
        register: vec![manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: manifest.module_id.clone(),
            version: manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };
    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(changes).expect("serialize module activation"),
    );
    let proposal = world
        .propose_manifest_update(
            Manifest {
                version: world.manifest().version.saturating_add(1),
                content: Value::Object(content),
            },
            "tester",
        )
        .expect("propose module activation");
    world
        .shadow_proposal(proposal)
        .expect("shadow module activation");
    world
        .approve_proposal(proposal, "tester", ProposalDecision::Approve)
        .expect("approve module activation");
    world
        .apply_proposal(proposal)
        .expect("apply module activation");
}
fn install_test_capability_authority(world: &mut World, revoked_grant_ids: BTreeSet<String>) {
    install_test_capability_authority_with_metadata(
        world,
        revoked_grant_ids,
        "finality-9",
        BRANCH_ID,
        "block-hash-4",
        3,
        9,
        false,
    )
    .expect("register governed capability issuer");
}
#[expect(
    clippy::too_many_arguments,
    reason = "Capability authority test fixture keeps independently varied protocol fields explicit."
)]
pub(super) fn install_test_capability_authority_with_metadata(
    world: &mut World,
    revoked_grant_ids: BTreeSet<String>,
    finalized_receipt_id: &str,
    branch_id: &str,
    finality_block_hash: &str,
    issuer_key_epoch: u64,
    governance_epoch: u64,
    rotated_key: bool,
) -> Result<(), WorldError> {
    let issuer_key = capability_issuer_signing_key();
    let finality_key = capability_finality_signing_key_2();
    world
        .bind_node_identity(
            ISSUER_ID,
            &hex::encode(issuer_key.verifying_key().to_bytes()),
        )
        .expect("bind capability issuer finality identity");
    world
        .bind_node_identity(
            FINALITY_SIGNER_2,
            &hex::encode(finality_key.verifying_key().to_bytes()),
        )
        .expect("bind capability finality signer identity");
    world
        .set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
            epoch_id: 4,
            threshold: 2,
            min_unique_signers: 2,
            threshold_bps: 10_000,
            signer_node_ids: vec![ISSUER_ID.to_string(), FINALITY_SIGNER_2.to_string()],
            validator_stakes: BTreeMap::from([
                (ISSUER_ID.to_string(), 100),
                (FINALITY_SIGNER_2.to_string(), 100),
            ]),
            ..GovernanceFinalityEpochSnapshot::default()
        })
        .expect("configure capability finality signer set");
    let proposal = world
        .propose_manifest_update(
            Manifest {
                version: world.manifest().version.saturating_add(1),
                content: json!({"capability_authority_fixture": true}),
            },
            "tester",
        )
        .expect("propose capability authority fixture");
    world
        .shadow_proposal(proposal)
        .expect("shadow capability authority fixture");
    world
        .approve_proposal(proposal, "tester", ProposalDecision::Approve)
        .expect("approve capability authority fixture");
    let manifest_hash = match &world
        .proposals()
        .get(&proposal)
        .expect("capability authority proposal")
        .status
    {
        ProposalStatus::Approved { manifest_hash, .. } => manifest_hash.clone(),
        status => panic!("expected approved capability authority fixture, got {status:?}"),
    };
    let snapshot = world
        .governance_finality_epoch_snapshots()
        .get(&4)
        .cloned()
        .expect("capability finality epoch snapshot");
    let consensus_height = world.journal().events.len() as u64 + 1;
    let min_unique_signers = snapshot.effective_min_unique_signers();
    let mut signatures = BTreeMap::new();
    for (node_id, signing_key) in [
        (ISSUER_ID, issuer_key.clone()),
        (FINALITY_SIGNER_2, finality_key),
    ] {
        let payload = GovernanceFinalityCertificate::signing_payload_v1(
            proposal,
            manifest_hash.as_str(),
            consensus_height,
            snapshot.epoch_id,
            snapshot.validator_set_hash.as_str(),
            snapshot.stake_root.as_str(),
            snapshot.threshold_bps,
            min_unique_signers,
            node_id,
        );
        let signature = signing_key.sign(payload.as_slice());
        signatures.insert(
            node_id.to_string(),
            format!(
                "{}{}",
                GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(signature.to_bytes())
            ),
        );
    }
    let certificate = GovernanceFinalityCertificate {
        proposal_id: proposal,
        manifest_hash,
        consensus_height,
        epoch_id: snapshot.epoch_id,
        validator_set_hash: snapshot.validator_set_hash,
        stake_root: snapshot.stake_root,
        threshold_bps: snapshot.threshold_bps,
        min_unique_signers,
        threshold: min_unique_signers,
        signatures,
    };
    let record = CapabilityAuthorityRecord {
        issuer_id: ISSUER_ID.to_string(),
        issuer_kind: "governance".to_string(),
        key_id: if rotated_key {
            "governance-key-2"
        } else {
            "governance-key-1"
        }
        .to_string(),
        public_key_hex: if rotated_key {
            "11".repeat(32)
        } else {
            hex::encode(issuer_key.verifying_key().to_bytes())
        },
        issuer_key_epoch,
        governance_epoch,
        finalized_receipt_id: finalized_receipt_id.to_string(),
        authority_rotation_receipt_id: None,
        world_id: WORLD_ID.to_string(),
        branch_id: branch_id.to_string(),
        finality_epoch: 4,
        finality_block_hash: finality_block_hash.to_string(),
        finality_status: "finalized".to_string(),
        revocation_epoch: 2,
        revoked_grant_ids,
        superseded_by: BTreeMap::new(),
    };
    let mut proof_record = record.clone();
    proof_record.issuer_key_epoch = 3;
    proof_record.governance_epoch = 9;
    proof_record.key_id = "governance-key-1".to_string();
    proof_record.public_key_hex = hex::encode(issuer_key.verifying_key().to_bytes());
    proof_record.finalized_receipt_id = "finality-9".to_string();
    proof_record.branch_id = BRANCH_ID.to_string();
    proof_record.finality_block_hash = "block-hash-4".to_string();
    let binding = CapabilityAuthorityFinalityBinding::from_record(&proof_record)
        .expect("hash capability authority proof binding");
    let mut proof = CapabilityAuthorityFinalityProof {
        proof_version: CapabilityAuthorityFinalityProof::PROOF_VERSION_V1,
        certificate,
        binding,
        signatures: BTreeMap::new(),
    };
    for (node_id, signing_key) in [
        (ISSUER_ID, capability_issuer_signing_key()),
        (FINALITY_SIGNER_2, capability_finality_signing_key_2()),
    ] {
        let payload = proof
            .signing_payload_v1(node_id)
            .expect("encode capability authority proof payload");
        let signature = signing_key.sign(payload.as_slice());
        proof.signatures.insert(
            node_id.to_string(),
            format!(
                "{}{}",
                CapabilityAuthorityFinalityProof::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(signature.to_bytes())
            ),
        );
    }
    world.install_capability_authority_record_with_finality_proof(record, proof)
}
pub(super) fn fixture_world() -> World {
    fixture_world_with_revocations(BTreeSet::new())
}

pub(super) fn fixture_world_with_revocations(revoked_grant_ids: BTreeSet<String>) -> World {
    fixture_world_with_revocations_and_budget(revoked_grant_ids, 128)
}
pub(super) fn fixture_world_with_revocations_and_budget(
    revoked_grant_ids: BTreeSet<String>,
    budget_units: i64,
) -> World {
    fixture_world_with_revocations_and_budget_and_effect_grant(
        revoked_grant_ids,
        budget_units,
        signed_effect_grant(),
    )
}
pub(super) fn fixture_world_with_revocations_and_budget_and_effect_grant(
    revoked_grant_ids: BTreeSet<String>,
    budget_units: i64,
    effect_grant: CapabilityGrantV2,
) -> World {
    let mut world = World::new();
    world.set_policy(PolicySet::allow_all());
    world.submit_action(Action::RegisterAgent {
        agent_id: SUBJECT_ID.to_string(),
        pos: pos(0, 0),
    });
    world
        .step()
        .expect("register trusted capability subject agent");
    world
        .install_capability_agent_identity(SUBJECT_ID, "owner-7", 1)
        .expect("bind trusted capability subject identity");
    let wasm_bytes = b"capability-grant-v2-module";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register active test module artifact");
    install_test_capability_authority(&mut world, revoked_grant_ids);
    world
        .register_capability_grant_v2(effect_grant.clone())
        .expect("register durable signed effect grant");
    let manifest = ModuleManifest {
        module_id: MODULE_ID.to_string(),
        name: "CapabilityGrantV2 module".to_string(),
        version: MODULE_VERSION.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::AgentInternal,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: vec![effect_grant.grant_id.clone()],
        abi_contract: ModuleAbiContract {
            declarations: ModuleSchemaDeclarations {
                commands: vec![ModuleCommandDeclaration {
                    namespace: "weather".to_string(),
                    name: "observe".to_string(),
                    schema_version: 1,
                    schema_hash: SCHEMA_HASH.to_string(),
                    max_payload_bytes: 128,
                }],
            },
            ..ModuleAbiContract::default()
        },
        artifact_identity: Some(super::signed_test_artifact_identity(&wasm_hash)),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 100_000,
            max_call_rate: 100,
            max_output_bytes: 4 * 1024,
            max_effects: 4,
            max_emits: 4,
        },
    };
    activate_module_for_test(&mut world, manifest.clone());
    world
        .set_agent_resource_balance(SUBJECT_ID, ResourceKind::Electricity, 128)
        .expect("seed module installer electricity");
    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: SUBJECT_ID.to_string(),
        manifest: manifest.clone(),
        activate: true,
    });
    world.step().expect("install live trusted module instance");
    let grant = signed_grant(grant_json(json!({})));
    world
        .install_capability_budget_account(CapabilityBudgetAccount {
            subject: grant.subject,
            grant_id: grant.grant_id,
            remaining_units: budget_units,
            reserved_units: 0,
            spent_units: 0,
        })
        .expect("install durable capability budget");
    world
}
pub(super) fn install_invocation_context(
    world: &mut World,
    grant: &CapabilityGrantV2,
    catalog: &CapabilityCatalogSnapshot,
    response: &AgentCommandResponse,
) {
    world
        .install_capability_invocation_context(CapabilityInvocationContext {
            grant_id: grant.grant_id.clone(),
            subject: grant.subject.clone(),
            presenter: response.presenter.clone(),
            audience: grant.audience.clone(),
            catalog_snapshot_id: catalog.snapshot_id.clone(),
            module_id: response.selected_entry.module_id.clone(),
            module_version: response.selected_entry.module_version.clone(),
            response_nonce: response.response_nonce.clone(),
        })
        .expect("install trusted invocation context");
}
pub(super) fn prepared_invocation(
    world: &World,
    grant: &CapabilityGrantV2,
    catalog_value: Value,
    response_value: Value,
) -> (CapabilityCatalogSnapshot, AgentCommandResponse) {
    let catalog = prepared_catalog(world, grant, catalog_value);
    let response = prepared_response(response_value.clone(), &catalog);
    let mut probe = world.clone();
    install_invocation_context(&mut probe, grant, &catalog, &response);
    let expected_head = probe
        .journal()
        .events
        .last()
        .map(|event| event.id)
        .unwrap_or(0);
    let mut catalog = catalog;
    if catalog.world_head != expected_head {
        catalog.world_head = expected_head;
        catalog.snapshot_id = catalog
            .canonical_hash()
            .expect("compute context-bound catalog snapshot hash");
    }
    let response = prepared_response(response_value, &catalog);
    (catalog, response)
}
pub(super) fn install_budget_for_grant(world: &mut World, grant: &CapabilityGrantV2, units: i64) {
    world
        .install_capability_budget_account(CapabilityBudgetAccount {
            subject: grant.subject.clone(),
            grant_id: grant.grant_id.clone(),
            remaining_units: units,
            reserved_units: 0,
            spent_units: 0,
        })
        .expect("install grant budget fixture");
}
fn install_bound_invocation_context(world: &mut World) {
    let bound_grant = signed_grant(grant_json(json!({})));
    let bound_catalog = prepared_catalog(world, &bound_grant, catalog_json(json!({})));
    if !world
        .capability_invocation_contexts()
        .contains_key(&bound_grant.grant_id)
    {
        let bound_response = prepared_response(response_json(json!({})), &bound_catalog);
        install_invocation_context(world, &bound_grant, &bound_catalog, &bound_response);
    }
}
pub(super) fn execute_prepared_fixture(
    world: &mut World,
    grant: CapabilityGrantV2,
    catalog: CapabilityCatalogSnapshot,
    response: AgentCommandResponse,
    sandbox: &mut dyn ModuleSandbox,
) -> Result<CapabilityAuthorizationAuditReceipt, WorldError> {
    install_bound_invocation_context(world);
    execute_without_invocation_context(world, grant, catalog, response, sandbox)
}
pub(super) fn execute_without_invocation_context(
    world: &mut World,
    grant: CapabilityGrantV2,
    catalog: CapabilityCatalogSnapshot,
    response: AgentCommandResponse,
    sandbox: &mut dyn ModuleSandbox,
) -> Result<CapabilityAuthorizationAuditReceipt, WorldError> {
    world.execute_trusted_module_command(
        grant,
        catalog,
        response,
        &mut WasmExecutor::new(oasis7_wasm_executor::WasmExecutorConfig::default())
            .expect("test wasm executor"),
        sandbox,
    )
}

fn execute_fixture(
    world: &mut World,
    grant: Value,
    catalog: Value,
    response: Value,
    sandbox: &mut dyn ModuleSandbox,
) -> Result<CapabilityAuthorizationAuditReceipt, WorldError> {
    let grant = signed_grant(grant);
    let catalog = prepared_catalog(world, &grant, catalog);
    let response = prepared_response(response, &catalog);
    execute_prepared_fixture(world, grant, catalog, response, sandbox)
}
#[test]
fn trusted_executor_accepts_exact_grant_and_never_routes_to_core_action() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let trusted_command_start = world.journal().events.len();
    let mut sandbox = RecordingSandbox::default();
    let receipt =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect("exact v2 authorization should execute");
    assert_eq!(sandbox.calls, 1);
    assert_eq!(receipt.decision, "accepted");
    assert_eq!(receipt.module_id.as_deref(), Some(MODULE_ID));
    assert!(
        receipt.committed_effect_receipt_id.is_none(),
        "pure trusted v2 execution must not fabricate an effect receipt"
    );
    assert!(
        world.journal().events[trusted_command_start..]
            .iter()
            .all(|event| { !matches!(event.caused_by, Some(CausedBy::Action(_))) })
    );
}
#[test]
fn trusted_executor_rejects_forged_issuer_or_signature_before_sandbox() {
    for (label, issuer) in [
        (
            "forged issuer",
            json!({"issuer": {"issuer_id": "attacker"}}),
        ),
        (
            "forged signature",
            json!({"issuer": {"signature": "ed25519:forged"}}),
        ),
    ] {
        let mut world = fixture_world();
        let mut sandbox = RecordingSandbox::default();
        let error = execute_fixture(
            &mut world,
            grant_json(issuer),
            catalog_json(json!({})),
            response_json(json!({})),
            &mut sandbox,
        )
        .expect_err(label);
        assert!(matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. }
        ));
        assert_eq!(sandbox.calls, 0);
    }
}
#[test]
fn trusted_executor_rejects_subject_presenter_audience_and_widened_scope_mismatch() {
    let cases = [
        (
            "subject",
            json!({"subject": {"agent_id": "agent-8"}}),
            json!({}),
        ),
        (
            "presenter",
            json!({}),
            json!({"presenter": {"presenter_id": "provider-2"}}),
        ),
        (
            "audience",
            json!({"audience": {"world_id": "world-other"}}),
            json!({}),
        ),
        (
            "widened operation",
            json!({"scope": {"operation": "write"}}),
            json!({}),
        ),
        (
            "widened entity",
            json!({"scope": {"entity_selector": ["*"]}}),
            json!({}),
        ),
    ];
    for (label, grant_override, response_override) in cases {
        let mut world = fixture_world();
        let mut sandbox = RecordingSandbox::default();
        let error = execute_fixture(
            &mut world,
            grant_json(grant_override),
            catalog_json(json!({})),
            response_json(response_override),
            &mut sandbox,
        )
        .expect_err(label);
        assert!(matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. }
        ));
        assert_eq!(sandbox.calls, 0);
    }
}
#[test]
fn trusted_executor_rejects_expiry_revocation_inactive_module_and_stale_catalog() {
    let cases = [
        ("expired", json!({"expires_at_tick": 1}), json!({})),
        ("revoked", json!({}), json!({})),
        (
            "inactive module",
            json!({}),
            json!({"module_registry_hash": "stale"}),
        ),
        ("stale catalog", json!({}), json!({"world_head": 2})),
    ];
    for (label, grant_override, catalog_override) in cases {
        let revoked_grant_ids = if label == "revoked" {
            [signed_grant(grant_json(grant_override.clone())).grant_id]
                .into_iter()
                .collect()
        } else {
            BTreeSet::new()
        };
        let mut world = fixture_world_with_revocations(revoked_grant_ids);
        if label == "expired" {
            world.step().expect("advance expired grant fixture");
            world.step().expect("advance expired grant fixture");
        }
        if label == "inactive module" {
            let changes = ModuleChangeSet {
                deactivate: vec![ModuleDeactivation {
                    module_id: MODULE_ID.to_string(),
                    reason: "authorization RED fixture".to_string(),
                }],
                ..ModuleChangeSet::default()
            };
            let mut content = serde_json::Map::new();
            content.insert(
                "module_changes".to_string(),
                serde_json::to_value(changes).expect("serialize deactivation"),
            );
            let proposal = world
                .propose_manifest_update(
                    Manifest {
                        version: 2,
                        content: Value::Object(content),
                    },
                    "tester",
                )
                .expect("propose module deactivation");
            world
                .shadow_proposal(proposal)
                .expect("shadow deactivation");
            world
                .approve_proposal(proposal, "tester", ProposalDecision::Approve)
                .expect("approve deactivation");
            world.apply_proposal(proposal).expect("apply deactivation");
        }
        let mut sandbox = RecordingSandbox::default();
        let error = execute_fixture(
            &mut world,
            grant_json(grant_override),
            catalog_json(catalog_override),
            response_json(json!({})),
            &mut sandbox,
        )
        .expect_err(label);
        assert!(matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. }
        ));
        assert_eq!(sandbox.calls, 0);
    }
}
#[test]
fn trusted_executor_atomically_commits_state_effect_emit_budget_nonce_and_receipt() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let trusted_command_start = world.journal().events.len();
    let mut sandbox = ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: Some(vec![0x99]),
            effects: vec![ModuleEffectIntent {
                kind: "weather.publish".to_string(),
                params: json!({"station": "station-1"}),
                cap_ref: signed_effect_grant().grant_id,
                cap_slot: None,
            }],
            emits: vec![ModuleEmit {
                kind: "weather.observed".to_string(),
                payload: json!({"station": "station-1"}),
            }],
            tick_lifecycle: None,
            output_bytes: 64,
        },
    };
    let receipt =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect("valid typed output should commit atomically");
    assert_eq!(sandbox.calls, 1);
    assert_eq!(receipt.decision, "accepted");
    assert_eq!(receipt.budget_before, 128);
    assert_eq!(receipt.budget_after, Some(123));
    assert!(receipt.committed_effect_receipt_id.is_none());
    assert_eq!(world.pending_effects_len(), 1);
    let intent = world
        .take_next_effect()
        .expect("atomic command queues its typed effect");
    assert_eq!(world.capability_effect_receipt_links().len(), 1);
    assert!(
        world
            .capability_effect_receipt_links()
            .contains_key(&intent.intent_id)
    );
    world
        .ingest_receipt(EffectReceipt {
            birth_era: None,
            world: None,
            intent_payload_digest: None,
            intent_id: intent.intent_id.clone(),
            status: "ok".to_string(),
            payload: json!({"published": true}),
            cost_cents: Some(1),
            signature: None,
        })
        .expect("ingest external effect receipt");
    let committed_receipt = world
        .capability_authorization_receipts()
        .get(&receipt.receipt_id)
        .expect("authorization audit receipt remains durable");
    assert_eq!(
        committed_receipt.committed_effect_receipt_id.as_deref(),
        Some(intent.intent_id.as_str())
    );
    assert!(
        !world
            .capability_effect_receipt_links()
            .contains_key(&intent.intent_id)
    );
    assert_eq!(world.capability_nonce_records().len(), 1);
    assert_eq!(world.capability_authorization_receipts().len(), 1);
    assert!(world.journal().events.iter().any(|event| matches!(
        &event.body,
        WorldEventBody::ModuleStateUpdated(update) if update.module_id == MODULE_ID
    )));
    assert!(world.journal().events.iter().any(|event| matches!(
        &event.body,
        WorldEventBody::EffectQueued(intent) if intent.kind == "weather.publish"
    )));
    assert!(world.journal().events.iter().any(|event| matches!(
        &event.body,
        WorldEventBody::ModuleEmitted(event) if event.kind == "weather.observed"
    )));
    assert!(
        world.journal().events[trusted_command_start..]
            .iter()
            .all(|event| !matches!(event.caused_by, Some(CausedBy::Action(_))))
    );
}
#[test]
fn trusted_executor_rolls_back_all_state_effects_budget_nonce_and_receipt_on_effect_failure() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let budget_before = budget_account_snapshot(&world);
    let mut sandbox = ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: Some(vec![0xee]),
            effects: vec![ModuleEffectIntent {
                kind: "weather.persist".to_string(),
                params: json!({"station": "station-1"}),
                cap_ref: "cap.weather.missing".to_string(),
                cap_slot: None,
            }],
            emits: vec![ModuleEmit {
                kind: "weather.should-not-emit".to_string(),
                payload: json!({}),
            }],
            tick_lifecycle: None,
            output_bytes: 64,
        },
    };
    let error =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect_err("effect failure must abort the staged trusted command");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(sandbox.calls, 1);
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(budget_account_snapshot(&world), budget_before);
    assert!(world.capability_nonce_records().is_empty());
    assert!(world.capability_authorization_receipts().is_empty());
    assert_eq!(world.pending_effects_len(), 0);
}

#[test]
fn trusted_executor_debits_durable_budget_once_and_retries_idempotently_after_snapshot_restore() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut first_sandbox = RecordingSandbox::default();
    let first = execute_without_invocation_context(
        &mut world,
        grant.clone(),
        catalog.clone(),
        response.clone(),
        &mut first_sandbox,
    )
    .expect("first budgeted command");
    let account_after_first = budget_account_snapshot(&world);
    assert_eq!(first.budget_before, 128);
    assert_eq!(first.budget_after, Some(127));
    assert_eq!(account_after_first.remaining_units, 127);
    assert_eq!(account_after_first.reserved_units, 0);
    assert_eq!(account_after_first.spent_units, 1);
    let restored_snapshot = world.snapshot();
    let restored_journal = world.journal().clone();
    let mut restored = World::from_snapshot(restored_snapshot, restored_journal)
        .expect("restore committed authorization state");
    let mut retry_sandbox = RecordingSandbox::default();
    let retry = execute_without_invocation_context(
        &mut restored,
        grant,
        catalog,
        response,
        &mut retry_sandbox,
    )
    .expect("exact request survives snapshot restore as idempotent");
    assert_eq!(retry.decision, "idempotent");
    assert_eq!(retry.receipt_id, first.receipt_id);
    assert_eq!(retry.budget_before, first.budget_before);
    assert_eq!(retry.budget_after, first.budget_after);
    assert_eq!(retry_sandbox.calls, 0);
    assert_eq!(restored.capability_nonce_records().len(), 1);
    assert_eq!(restored.capability_authorization_receipts().len(), 1);
    assert_eq!(budget_account_snapshot(&restored), account_after_first);
}

#[test]
fn trusted_executor_rejects_insufficient_budget_before_sandbox() {
    let mut world = fixture_world_with_revocations_and_budget(BTreeSet::new(), 0);
    let grant = signed_grant(grant_json(json!({})));
    let catalog = prepared_catalog(&world, &grant, catalog_json(json!({})));
    let response = prepared_response(response_json(json!({})), &catalog);
    let mut sandbox = RecordingSandbox::default();

    let error = execute_prepared_fixture(&mut world, grant, catalog, response, &mut sandbox)
        .expect_err("insufficient durable grant budget must deny before sandbox");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(sandbox.calls, 0);
    assert_eq!(budget_account_snapshot(&world).remaining_units, 0);
    assert!(world.capability_nonce_records().is_empty());
    assert!(world.capability_authorization_receipts().is_empty());
}

#[test]
fn trusted_executor_rejects_missing_forged_and_stale_invocation_context_before_sandbox() {
    let cases = ["missing", "forged subject", "stale catalog binding"];
    for case in cases {
        let mut world = fixture_world();
        let grant = signed_grant(grant_json(json!({})));
        let catalog = prepared_catalog(&world, &grant, catalog_json(json!({})));
        let response = prepared_response(response_json(json!({})), &catalog);
        if case == "forged subject" {
            world
                .install_capability_invocation_context(CapabilityInvocationContext {
                    grant_id: grant.grant_id.clone(),
                    subject: CapabilitySubject::Agent {
                        agent_id: "agent-forged".to_string(),
                        owner_binding: "owner-7".to_string(),
                        generation: 1,
                    },
                    presenter: response.presenter.clone(),
                    audience: response.audience.clone(),
                    catalog_snapshot_id: catalog.snapshot_id.clone(),
                    module_id: MODULE_ID.to_string(),
                    module_version: MODULE_VERSION.to_string(),
                    response_nonce: response.response_nonce.clone(),
                })
                .expect("install forged context fixture");
        } else if case == "stale catalog binding" {
            world
                .install_capability_invocation_context(CapabilityInvocationContext {
                    grant_id: grant.grant_id.clone(),
                    subject: grant.subject.clone(),
                    presenter: response.presenter.clone(),
                    audience: response.audience.clone(),
                    catalog_snapshot_id: "catalog-stale".to_string(),
                    module_id: MODULE_ID.to_string(),
                    module_version: MODULE_VERSION.to_string(),
                    response_nonce: response.response_nonce.clone(),
                })
                .expect("install stale context fixture");
        }
        let mut sandbox = RecordingSandbox::default();
        let error =
            execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
                .expect_err(case);
        assert!(matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. }
        ));
        assert_eq!(sandbox.calls, 0, "{case} must fail before sandbox");
    }
}
