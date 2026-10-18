#![cfg(feature = "wasmtime")]

use super::super::*;
use super::modules::activate_module_manifest;
use super::signed_test_artifact_identity;
use oasis7_wasm_abi::{ModuleEmit, ModuleLimits, ModuleManifest, ModuleOutput};
use oasis7_wasm_executor::WasmExecutor;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MODULE_ID: &str = "m.visual.wasm-recovery";
const MODULE_VERSION: &str = "0.1.0";
const ENTITY_ID: &str = "wasm-recovery-relay";
const UPSERT_DATA_OFFSET: u32 = 32;
const REMOVE_DATA_OFFSET: u32 = 512;

fn temp_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-module-visual-wasm-recovery-{unique}"))
}

fn unsigned_leb(mut value: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            return bytes;
        }
    }
}

fn signed_leb(mut value: i32) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
            return bytes;
        }
    }
}

fn push_section(wasm: &mut Vec<u8>, id: u8, payload: Vec<u8>) {
    wasm.push(id);
    wasm.extend(unsigned_leb(payload.len() as u32));
    wasm.extend(payload);
}

fn output_bytes(kind: &str, payload: serde_json::Value) -> Vec<u8> {
    serde_cbor::to_vec(&ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: kind.to_string(),
            payload,
        }],
        tick_lifecycle: None,
        output_bytes: 0,
    })
    .expect("encode visual module output")
}

fn visual_wasm_artifact() -> Vec<u8> {
    let upsert = output_bytes(
        "module_visual_entity_upserted",
        json!({
            "entity": {
                "entity_id": ENTITY_ID,
                "module_id": MODULE_ID,
                "kind": "relay",
                "label": "WASM recovery relay",
                "anchor": {
                    "type": "absolute",
                    "data": { "pos": { "x_cm": 100, "y_cm": 200, "z_cm": 0 } }
                }
            }
        }),
    );
    let remove = output_bytes(
        "module_visual_entity_removed",
        json!({ "entity_id": ENTITY_ID }),
    );

    // This fixture is a real wasm32 module.  alloc returns address zero, and
    // reduce selects the serialized output by the first input byte.  It uses
    // the same multi-value (ptr, len) ABI that WasmExecutor admits.
    let mut wasm = vec![0x00, b'a', b's', b'm', 0x01, 0x00, 0x00, 0x00];

    let mut types = vec![3, 0x60, 1, 0x7f, 1, 0x7f];
    types.extend([0x60, 2, 0x7f, 0x7f, 2, 0x7f, 0x7f]);
    types.extend([0x60, 0, 2, 0x7f, 0x7f]);
    push_section(&mut wasm, 1, types);

    push_section(&mut wasm, 3, vec![2, 0, 1]);
    push_section(&mut wasm, 5, vec![1, 0, 1]);

    let mut exports = vec![3];
    exports.extend([6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 0]);
    exports.extend([5, b'a', b'l', b'l', b'o', b'c', 0, 0]);
    exports.extend([6, b'r', b'e', b'd', b'u', b'c', b'e', 0, 1]);
    push_section(&mut wasm, 7, exports);

    let mut alloc_body = vec![0, 0x41, 0, 0x0b];
    let mut reduce_body = vec![
        0, // local declaration count
        0x20, 0x00, // local.get input pointer
        0x2d, 0x00, 0x00, // i32.load8_u, align=0, offset=0
        0x41, 0x01, // i32.const 1
        0x46, // i32.eq
        0x04, 0x02, // if (type index 2: () -> (i32, i32))
        0x41,
    ];
    reduce_body.extend(signed_leb(REMOVE_DATA_OFFSET as i32));
    reduce_body.extend([0x41]);
    reduce_body.extend(signed_leb(remove.len() as i32));
    reduce_body.push(0x05); // else
    reduce_body.push(0x41);
    reduce_body.extend(signed_leb(UPSERT_DATA_OFFSET as i32));
    reduce_body.push(0x41);
    reduce_body.extend(signed_leb(upsert.len() as i32));
    reduce_body.push(0x0b); // end if
    reduce_body.push(0x0b); // end function

    let mut code = vec![2];
    code.extend(unsigned_leb(alloc_body.len() as u32));
    code.append(&mut alloc_body);
    code.extend(unsigned_leb(reduce_body.len() as u32));
    code.extend(reduce_body);
    push_section(&mut wasm, 10, code);

    let mut data = vec![2, 0, 0x41];
    data.extend(signed_leb(UPSERT_DATA_OFFSET as i32));
    data.push(0x0b);
    data.extend(unsigned_leb(upsert.len() as u32));
    data.extend(upsert);
    data.extend([0, 0x41]);
    data.extend(signed_leb(REMOVE_DATA_OFFSET as i32));
    data.push(0x0b);
    data.extend(unsigned_leb(remove.len() as u32));
    data.extend(remove);
    push_section(&mut wasm, 11, data);

    wasm
}

fn active_manifest(wasm_hash: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: MODULE_ID.to_string(),
        name: "WASM visual recovery fixture".to_string(),
        version: MODULE_VERSION.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.to_string(),
        interface_version: "wasm-1".to_string(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        abi_contract: ModuleAbiContract::default(),
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 100_000,
            max_call_rate: 8,
            max_output_bytes: 64 * 1024,
            max_effects: 0,
            max_emits: 1,
        },
    }
}

fn world_with_visual_module(wasm_bytes: &[u8]) -> (World, String) {
    let wasm_hash = util::sha256_hex(wasm_bytes);
    let mut world = World::new();
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register real visual wasm artifact");
    activate_module_manifest(&mut world, active_manifest(&wasm_hash));
    (world, wasm_hash)
}

fn emitted_visual_events(world: &World) -> Vec<&oasis7_wasm_abi::ModuleEmitEvent> {
    world
        .journal()
        .events
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::ModuleEmitted(emitted) => Some(emitted),
            _ => None,
        })
        .collect()
}

#[test]
fn real_wasm_visual_upsert_remove_survives_on_disk_recovery() {
    let wasm_bytes = visual_wasm_artifact();
    assert_eq!(&wasm_bytes[..4], b"\0asm", "fixture must be real wasm");
    let (mut world, wasm_hash) = world_with_visual_module(&wasm_bytes);
    let mut executor = WasmExecutor::new(super::test_wasm_executor_config())
        .expect("initialize real wasm executor");

    let upsert = world
        .execute_module_call(
            MODULE_ID,
            "trace-wasm-visual-upsert",
            vec![0],
            &mut executor,
        )
        .expect("real wasm visual upsert");
    assert_eq!(upsert.emits.len(), 1);
    let entity = world
        .state()
        .module_visual_entities
        .get(ENTITY_ID)
        .expect("real wasm upsert reaches authoritative state");
    assert_eq!(entity.module_id, MODULE_ID);
    assert_eq!(entity.label.as_deref(), Some("WASM recovery relay"));
    assert_eq!(emitted_visual_events(&world).len(), 1);
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .expect("visual upsert consensus record")
            .block
            .header
            .state_root,
        world
            .current_state_root_hash()
            .expect("visual upsert state root")
    );

    let dir = temp_dir();
    world
        .save_to_dir(&dir)
        .expect("persist visual world and module store");
    let artifact_path = dir.join("modules").join(format!("{wasm_hash}.wasm"));
    let persisted_bytes = fs::read(&artifact_path).expect("read persisted visual artifact");
    assert_eq!(persisted_bytes, wasm_bytes);
    assert_eq!(util::sha256_hex(&persisted_bytes), wasm_hash);
    assert!(dir.join("module_registry.json").exists());
    assert!(dir.join("snapshot.json").exists());

    // The snapshot owns the canonical bytes. A mutable external cache may be
    // stale or tampered with without changing the bytes selected for recovery.
    fs::write(&artifact_path, b"tampered-visual-wasm").expect("tamper persisted artifact");
    let mut recovered_with_tampered_cache =
        World::load_from_dir(&dir).expect("canonical snapshot bytes bypass tampered cache");
    assert_eq!(
        recovered_with_tampered_cache
            .load_module(&wasm_hash)
            .expect("canonical recovered artifact is executable")
            .bytes
            .as_ref(),
        wasm_bytes.as_slice()
    );
    assert_eq!(
        recovered_with_tampered_cache
            .state()
            .module_visual_entities
            .get(ENTITY_ID)
            .expect("visual entity survives sidecar recovery")
            .label
            .as_deref(),
        Some("WASM recovery relay")
    );

    // Remove the sidecar so the ordinary JSON snapshot/journal recovery path
    // is also proven to use the canonical inline bytes.
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON recovery");
    let mut restored = World::load_from_dir(&dir).expect("recover visual world from disk");
    assert_eq!(
        restored.module_registry().active.get(MODULE_ID),
        Some(&MODULE_VERSION.to_string())
    );
    let restored_record = restored
        .module_registry()
        .records
        .get(&ModuleRegistry::record_key(MODULE_ID, MODULE_VERSION))
        .expect("recovered module manifest");
    assert_eq!(restored_record.manifest.wasm_hash, wasm_hash);
    assert!(
        restored_record
            .manifest
            .artifact_identity
            .as_ref()
            .is_some_and(|identity| identity.is_complete())
    );
    let restored_entity = restored
        .state()
        .module_visual_entities
        .get(ENTITY_ID)
        .expect("visual entity survives disk recovery");
    assert_eq!(restored_entity.module_id, MODULE_ID);
    assert_eq!(
        restored
            .load_module(&wasm_hash)
            .expect("recovered artifact is executable")
            .bytes
            .as_ref(),
        wasm_bytes.as_slice()
    );

    let mut restored_executor = WasmExecutor::new(super::test_wasm_executor_config())
        .expect("initialize recovery wasm executor");
    restored
        .execute_module_call(
            MODULE_ID,
            "trace-wasm-visual-remove-after-recovery",
            vec![1],
            &mut restored_executor,
        )
        .expect("real recovered wasm visual remove");
    assert!(
        !restored
            .state()
            .module_visual_entities
            .contains_key(ENTITY_ID)
    );
    assert_eq!(emitted_visual_events(&restored).len(), 2);
    assert_eq!(
        restored
            .tick_consensus_records()
            .last()
            .expect("visual removal consensus record")
            .block
            .header
            .state_root,
        restored
            .current_state_root_hash()
            .expect("visual removal state root")
    );

    fs::remove_dir_all(dir).expect("remove visual recovery fixture");
}
