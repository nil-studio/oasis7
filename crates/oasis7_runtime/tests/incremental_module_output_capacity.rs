#![cfg(all(feature = "wasmtime", not(target_arch = "wasm32")))]
//! Payload capacity only: this does not certify compiler/JIT process memory.
use oasis7_proto::{
    cbor_decode_preflight::CborPreflightWorkV1,
    memory_budget::{MemoryBudget, MemoryClass},
};
use oasis7_runtime::{RootModuleOutputWorkV1, call_admitted_root_module_output_v1};
use oasis7_wasm_abi::{ModuleCallRequest, ModuleLimits, ModuleOutput};
use oasis7_wasm_executor::WasmExecutor;
use sha2::{Digest, Sha256};
use std::sync::Arc;
fn leb(mut n: usize) -> Vec<u8> {
    let mut b = Vec::new();
    loop {
        let v = (n & 127) as u8;
        n >>= 7;
        b.push(v | if n != 0 { 128 } else { 0 });
        if n == 0 {
            return b;
        }
    }
}
fn signed_positive(n: usize) -> Vec<u8> {
    let mut b = leb(n);
    if b.last().unwrap() & 0x40 != 0 {
        *b.last_mut().unwrap() |= 0x80;
        b.push(0);
    }
    b
}
fn section(wasm: &mut Vec<u8>, id: u8, data: &[u8]) {
    wasm.push(id);
    wasm.extend(leb(data.len()));
    wasm.extend(data);
}
fn request(raw: &[u8]) -> ModuleCallRequest {
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut wasm,
        1,
        &[
            2, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 2, 0x7f, 0x7f, 2, 0x7f, 0x7f,
        ],
    );
    section(&mut wasm, 3, &[2, 0, 1]);
    let pages = raw.len().max(4097).div_ceil(65536);
    let mut memory = vec![1, 0];
    memory.extend(leb(pages));
    section(&mut wasm, 5, &memory);
    section(
        &mut wasm,
        7,
        &[
            3, 6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 0, 5, b'a', b'l', b'l', b'o', b'c', 0, 0,
            4, b'c', b'a', b'l', b'l', 0, 1,
        ],
    );
    let alloc = [0, 0x41, 0x80, 0x20, 0x0b];
    let mut call = vec![0, 0x41, 0, 0x41];
    call.extend(signed_positive(raw.len()));
    call.push(0x0b);
    let mut code = vec![2];
    code.extend(leb(alloc.len()));
    code.extend(alloc);
    code.extend(leb(call.len()));
    code.extend(call);
    section(&mut wasm, 10, &code);
    let mut data = vec![1, 0, 0x41, 0, 0x0b];
    data.extend(leb(raw.len()));
    data.extend(raw);
    section(&mut wasm, 11, &data);
    ModuleCallRequest {
        module_id: "actual-capacity-module".into(),
        trace_id: "actual-capacity-trace".into(),
        entrypoint: "call".into(),
        input: vec![],
        wasm_hash: hex::encode(Sha256::digest(&wasm)),
        wasm_bytes: Arc::from(wasm),
        limits: ModuleLimits {
            max_mem_bytes: (pages * 65536) as u64,
            max_gas: 10_000_000,
            max_call_rate: 0,
            max_output_bytes: 4 << 20,
            max_effects: 2,
            max_emits: 2,
        },
    }
}
fn roundtrip(bytes: usize) {
    let memory = MemoryBudget::new(512 << 20, 64 << 20).unwrap();
    let _fixture = memory
        .reserve(
            bytes
                .checked_mul(128)
                .unwrap()
                .checked_add(16 << 20)
                .unwrap(),
            MemoryClass::Ordinary,
        )
        .unwrap();
    let output = ModuleOutput {
        new_state: Some((0..bytes).map(|i| (i % 256) as u8).collect()),
        effects: vec![],
        emits: vec![],
        tick_lifecycle: None,
        output_bytes: 0,
    };
    let raw = serde_cbor::to_vec(&output).unwrap();
    assert!(raw.len() < 4 << 20);
    let request = request(&raw);
    let mut sandbox = WasmExecutor::new_for_deterministic_replay(Default::default()).unwrap();
    let mut work = RootModuleOutputWorkV1 {
        calls_remaining: 1,
        structure: CborPreflightWorkV1 {
            bytes_remaining: 32 << 20,
            values_remaining: 16 << 20,
            max_depth: 32,
        },
        max_identity_bytes: 128,
        max_original_output_bytes: 4 << 20,
        max_output_heap_bytes: 256 << 20,
    };
    let before = memory.used();
    let actual=call_admitted_root_module_output_v1(&mut sandbox,&request,&memory,&mut work,||false)
        .unwrap_or_else(|e|panic!("actual Wasm new_state bytes={bytes}, encoded={}, 512MiB-pool/256MiB-heap decode failed: {e:?}",raw.len()));
    assert_eq!(actual.new_state, output.new_state);
    assert_eq!(actual.output_bytes, raw.len() as u64);
    assert!(actual.reserved_bytes() <= 256 << 20);
    assert_eq!(work.calls_remaining, 0);
    drop(actual);
    assert_eq!(memory.used(), before);
}
#[test]
fn actual_wasm_output_state_64kib_fits_512mib_pool() {
    roundtrip(64 << 10);
}
#[test]
fn actual_wasm_output_state_1mib_fits_512mib_pool() {
    roundtrip(1 << 20);
}

mod visual_decode_tests {
    use oasis7_agent_api::{
        module_visual::ModuleVisualMutation,
        world_action::GeoPos,
        world_entity::{ModuleVisualAnchor, ModuleVisualEntity},
    };
    use oasis7_formal_protocol::incremental_value::ValueTreeError;
    use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
    use oasis7_runtime::incremental_module_execution_visual_decode::{
        RootModuleVisualDecodeErrorV1, RootModuleVisualDecodeWorkV1,
        decode_root_module_visual_emit_v1,
    };
    use oasis7_wasm_abi::ModuleEmitEvent;
    use oasis7_world_state::{ResourceDelta, RuleDecision, RuleVerdict, WorldError};
    use serde_json::{Value, json};

    const VISUAL_NODES: u64 = 512;
    const VISUAL_TEXT_BYTES: u64 = 16 << 10;

    fn work() -> RootModuleVisualDecodeWorkV1 {
        RootModuleVisualDecodeWorkV1 {
            nodes_remaining: VISUAL_NODES,
            text_bytes_remaining: VISUAL_TEXT_BYTES,
            max_depth: 8,
            max_heap_bytes: 4 << 20,
        }
    }

    fn event(kind: &str, payload: Value) -> ModuleEmitEvent {
        ModuleEmitEvent {
            module_id: " visual.module ".into(),
            trace_id: "visual-decode-inventory".into(),
            kind: kind.into(),
            payload,
        }
    }

    fn upsert_payload(anchor: Value) -> Value {
        // Deliberately construct the original JSON wire independently of the
        // entity serializer. The strict parser supplies only the missing label.
        json!({
            "entity": {
                "entity_id": " visual-7 ",
                "module_id": " visual.module ",
                "kind": " ",
                "anchor": anchor
            }
        })
    }

    fn assert_work_spent(work: &RootModuleVisualDecodeWorkV1) {
        assert!(work.nodes_remaining < VISUAL_NODES);
        assert!(work.text_bytes_remaining < VISUAL_TEXT_BYTES);
        assert_eq!(work.max_depth, 8);
    }

    #[test]
    fn module_visual_decode_nonvisual_rule_decision_keeps_paid_none_until_drop() {
        let memory = MemoryBudget::new(16 << 20, 2 << 20).unwrap();
        let fixture = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        let emit = event(
            "rule.decision",
            serde_json::to_value(RuleDecision {
                action_id: 7,
                verdict: RuleVerdict::Allow,
                override_action: None,
                cost: ResourceDelta::default(),
                notes: vec!["nonvisual emit remains nonvisual".into()],
            })
            .unwrap(),
        );
        let before = memory.used();
        let mut work = work();
        let actual = decode_root_module_visual_emit_v1(&emit, &memory, &mut work, &mut || false)
            .expect("a real nonvisual rule emit must pass both source inventories");
        assert!(actual.is_none());
        assert!(actual.uses_budget(&memory));
        assert!(actual.reserved_bytes() > 0);
        assert_eq!(memory.used(), before + actual.reserved_bytes());
        assert_work_spent(&work);
        drop(actual);
        assert_eq!(memory.used(), before);
        drop(emit);
        drop(fixture);
        assert_eq!(memory.used(), 0);
    }

    #[test]
    fn module_visual_decode_upsert_preserves_original_schema_sanitizer_and_anchors() {
        let memory = MemoryBudget::new(16 << 20, 2 << 20).unwrap();
        let fixture = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        // Expected DTOs are independent values, not a second call to the
        // parser or sanitizer under test. No Current/permission is certified.
        let cases = [
            (
                json!({"type": "agent", "data": {"agent_id": "agent-7"}}),
                ModuleVisualAnchor::Agent {
                    agent_id: "agent-7".into(),
                },
                Some("  Beacon  "),
                Some("Beacon"),
            ),
            (
                json!({"type": "location", "data": {"location_id": "site-9"}}),
                ModuleVisualAnchor::Location {
                    location_id: "site-9".into(),
                },
                None,
                None,
            ),
            (
                json!({
                    "type": "absolute",
                    "data": {"pos": {"x_cm": -17, "y_cm": 29, "z_cm": 3}}
                }),
                ModuleVisualAnchor::Absolute {
                    pos: GeoPos::new(-17, 29, 3),
                },
                Some("  "),
                None,
            ),
        ];
        let before = memory.used();
        for (anchor_wire, anchor, label_wire, expected_label) in cases {
            let mut payload = upsert_payload(anchor_wire);
            if let Some(label) = label_wire {
                payload["entity"]["label"] = json!(label);
            }
            let emit = event("module_visual_entity_upserted", payload);
            let mut work = work();
            let actual =
                decode_root_module_visual_emit_v1(&emit, &memory, &mut work, &mut || false)
                    .expect("the original tagged visual schema must decode");
            let expected = ModuleVisualMutation::Upsert(ModuleVisualEntity {
                entity_id: "visual-7".into(),
                module_id: "visual.module".into(),
                kind: "artifact".into(),
                label: expected_label.map(str::to_owned),
                anchor,
            });
            assert_eq!(&*actual, &Some(expected));
            assert!(actual.uses_budget(&memory));
            assert!(actual.reserved_bytes() > 0);
            assert_eq!(memory.used(), before + actual.reserved_bytes());
            assert_work_spent(&work);
            drop(actual);
            assert_eq!(memory.used(), before);
            drop(emit);
        }
        drop(fixture);
        assert_eq!(memory.used(), 0);
    }

    #[test]
    fn module_visual_decode_unknown_field_keeps_business_error_paid_until_drop() {
        let memory = MemoryBudget::new(16 << 20, 2 << 20).unwrap();
        let fixture = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        let mut payload = upsert_payload(json!({"type": "agent", "data": {"agent_id": "agent-7"}}));
        payload["entity"]["unexpected"] = json!(true);
        let emit = event("module_visual_entity_upserted", payload);
        let before = memory.used();
        let mut work = work();
        let error = decode_root_module_visual_emit_v1(&emit, &memory, &mut work, &mut || false)
            .expect_err("the original strict entity payload must reject unknown fields");
        let RootModuleVisualDecodeErrorV1::Business(error) = error else {
            panic!("unknown fields must remain a charged parser business error");
        };
        assert!(matches!(&*error, WorldError::Serde(detail)
                if detail.contains("unknown field") && detail.contains("unexpected")));
        assert!(error.uses_budget(&memory));
        assert!(error.reserved_bytes() > 0);
        assert_eq!(memory.used(), before + error.reserved_bytes());
        assert_work_spent(&work);
        drop(error);
        assert_eq!(memory.used(), before);
        drop(emit);
        drop(fixture);
        assert_eq!(memory.used(), 0);
    }

    #[test]
    fn module_visual_decode_small_heap_refuses_without_leaking_or_refunding_work() {
        let memory = MemoryBudget::new(16 << 20, 2 << 20).unwrap();
        let fixture = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        let emit = event(
            "module_visual_entity_upserted",
            upsert_payload(json!({"type": "agent", "data": {"agent_id": "agent-7"}})),
        );
        let before = memory.used();
        let mut work = work();
        work.max_heap_bytes = 1;
        let error = decode_root_module_visual_emit_v1(&emit, &memory, &mut work, &mut || false)
            .expect_err("the original heap bound must not be enlarged");
        assert!(matches!(
            error,
            RootModuleVisualDecodeErrorV1::Value(ValueTreeError::ResourceLimited(
                "module visual payload heap"
            ))
        ));
        assert_eq!(memory.used(), before);
        assert_work_spent(&work);
        assert_eq!(work.max_heap_bytes, 1);
        drop(emit);
        drop(fixture);
        assert_eq!(memory.used(), 0);
    }

    #[test]
    fn module_visual_decode_cancel_before_and_after_admission_releases_original_pool() {
        let memory = MemoryBudget::new(16 << 20, 2 << 20).unwrap();
        let fixture = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        let emit = event(
            "module_visual_entity_upserted",
            upsert_payload(json!({"type": "agent", "data": {"agent_id": "agent-7"}})),
        );
        let before = memory.used();
        let mut early = work();
        let error = decode_root_module_visual_emit_v1(&emit, &memory, &mut early, &mut || true)
            .expect_err("pre-cancelled payload must not decode");
        assert!(matches!(
            error,
            RootModuleVisualDecodeErrorV1::Value(ValueTreeError::Cancelled)
        ));
        assert_eq!(early.nodes_remaining, VISUAL_NODES);
        assert_eq!(early.text_bytes_remaining, VISUAL_TEXT_BYTES);
        assert_eq!(memory.used(), before);

        // Observe the actual pool charge, not an assumed traversal/callback
        // count. The first paid probe is before parsing, the second after it.
        for stop_at_paid_probe in [1, 2] {
            let mut paid_probes = 0;
            let mut work = work();
            let error = {
                let mut cancelled = || {
                    if memory.used() > before {
                        paid_probes += 1;
                        paid_probes == stop_at_paid_probe
                    } else {
                        false
                    }
                };
                decode_root_module_visual_emit_v1(&emit, &memory, &mut work, &mut cancelled)
                    .expect_err("cancellation must release even an admitted payload")
            };
            assert!(matches!(
                error,
                RootModuleVisualDecodeErrorV1::Value(ValueTreeError::Cancelled)
            ));
            assert_eq!(paid_probes, stop_at_paid_probe);
            assert_work_spent(&work);
            assert_eq!(memory.used(), before);
        }
        drop(emit);
        drop(fixture);
        assert_eq!(memory.used(), 0);
    }
}
