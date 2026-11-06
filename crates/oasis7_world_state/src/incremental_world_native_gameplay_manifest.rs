//! Exhaustive original DTOs; each nested collection keeps its native order/key.
use super::*;
dto!(oasis7_wasm_abi::ModuleManifest, {
    module_id,
    name,
    version,
    kind,
    role,
    wasm_hash,
    interface_version,
    exports,
    subscriptions,
    required_caps,
    abi_contract,
    artifact_identity,
    limits,
});
dto!(oasis7_wasm_abi::ModuleLimits, {
    max_mem_bytes,
    max_gas,
    max_call_rate,
    max_output_bytes,
    max_effects,
    max_emits,
});
dto!(oasis7_wasm_abi::ModuleArtifactIdentity, {
    source_hash,
    build_manifest_hash,
    signer_node_id,
    signature_scheme,
    artifact_signature,
});
dto!(oasis7_wasm_abi::ModuleSubscription, {
    event_kinds,
    action_kinds,
    stage,
    filters,
});
dto!(oasis7_wasm_abi::ModuleAbiContract, {
    abi_version,
    input_schema,
    output_schema,
    cap_slots,
    policy_hooks,
    gameplay,
    declarations,
});
dto!(oasis7_wasm_abi::GameplayContract, {
    kind,
    game_modes,
    min_players,
    max_players,
});
dto!(oasis7_wasm_abi::ModuleSchemaDeclarations, {
    commands,
});
dto!(oasis7_wasm_abi::ModuleCommandDeclaration, {
    namespace,
    name,
    schema_version,
    schema_hash,
    max_payload_bytes,
});
