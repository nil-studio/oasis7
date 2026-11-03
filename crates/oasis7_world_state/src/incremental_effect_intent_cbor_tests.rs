use super::*;
use serde_json::json;
use std::cell::Cell;
fn intent(params: Value) -> EffectIntent {
    EffectIntent {
        birth_era: None,
        intent_id: " raw\0界 ".into(),
        kind: "".into(),
        params,
        cap_ref: "cap".into(),
        origin: EffectOrigin::System,
    }
}
fn allowance() -> EffectIntentCborBudgetV1 {
    EffectIntentCborBudgetV1 {
        nodes_remaining: 100000,
        bytes_remaining: 1 << 20,
        max_json_depth: 128,
    }
}
#[test]
fn exact_original_tuple_wire_all_origins_and_json_types() {
    let memory = MemoryBudget::new(512 << 10, 128 << 10).unwrap();
    let params = json!({"":null,"a":[true,false,-0.0,1.5,1e300],"i":i64::MIN,"u":u64::MAX,"n":{"long-key":"escaped\n\0界"},"length":(0..25).collect::<Vec<_>>()});
    for era in [None, Some(0), Some(u64::MAX)] {
        for origin in [
            EffectOrigin::System,
            EffectOrigin::Reducer { name: "".into() },
            EffectOrigin::Plan {
                name: " raw plan ".into(),
            },
            EffectOrigin::Module {
                module_id: "module\0界".into(),
            },
        ] {
            let mut source = intent(params.clone());
            source.birth_era = era;
            source.origin = origin;
            let expected =
                serde_cbor::to_vec(&("oasis7.effect.intent-payload.v2", &source)).unwrap();
            let mut ctx =
                EffectIntentCborContextV1::new(&memory, MemoryClass::Ordinary, allowance(), || {
                    false
                });
            let mut actual = Vec::new();
            assert_eq!(
                stream_effect_intent_payload_v1(&source, &mut ctx, &mut actual).unwrap(),
                expected.len() as u64
            );
            assert_eq!(actual, expected);
            let digest = hash_effect_intent_payload_v1(&source, &mut ctx).unwrap();
            assert_eq!(digest.digest, *blake3::hash(&expected).as_bytes());
            assert_eq!(
                blake3::Hash::from(digest.digest).to_hex().as_str(),
                source.payload_digest().unwrap()
            );
        }
    }
}
struct Oracle {
    hash: blake3::Hasher,
    bytes: u64,
    max: usize,
}
impl Write for Oracle {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.hash.update(bytes);
        self.bytes += bytes.len() as u64;
        self.max = self.max.max(bytes.len());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn borrowed_32m_and_64m_text_hash_with_small_operation_budget() {
    for length in [32 << 20, 64 << 20] {
        // This original source allocation belongs to the caller, not C.
        let source = intent(Value::String("t".repeat(length)));
        let memory = MemoryBudget::new(512 << 10, 128 << 10).unwrap();
        let limits = EffectIntentCborBudgetV1 {
            bytes_remaining: length as u64 + 4096,
            ..allowance()
        };
        let mut context =
            EffectIntentCborContextV1::new(&memory, MemoryClass::Ordinary, limits, || false);
        let digest = hash_effect_intent_payload_v1(&source, &mut context).unwrap();
        // Independent original derived Serialize path streamed to an oracle;
        // no large to_vec oracle or entire Value model is created.
        let mut expected = Oracle {
            hash: blake3::Hasher::new(),
            bytes: 0,
            max: 0,
        };
        serde_cbor::to_writer(&mut expected, &("oasis7.effect.intent-payload.v2", &source))
            .unwrap();
        assert_eq!(digest.digest, *expected.hash.finalize().as_bytes());
        assert_eq!(digest.encoded_bytes, expected.bytes);
        let mut sink = Oracle {
            hash: blake3::Hasher::new(),
            bytes: 0,
            max: 0,
        };
        let mut context =
            EffectIntentCborContextV1::new(&memory, MemoryClass::Ordinary, limits, || false);
        stream_effect_intent_payload_v1(&source, &mut context, &mut sink).unwrap();
        assert!(sink.max <= 16 << 10);
        assert_eq!(sink.bytes, expected.bytes);
    }
}
struct Short {
    bytes: Vec<u8>,
    interrupt: bool,
}
impl Write for Short {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.interrupt {
            self.interrupt = false;
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        let n = bytes.len().min(3);
        self.bytes.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn partial_interrupted_and_final_cancel_are_precise_and_poisoned() {
    let source = intent(json!({"x":["payload",null]}));
    let expected = serde_cbor::to_vec(&("oasis7.effect.intent-payload.v2", &source)).unwrap();
    let memory = MemoryBudget::new(512 << 10, 128 << 10).unwrap();
    let mut context =
        EffectIntentCborContextV1::new(&memory, MemoryClass::Ordinary, allowance(), || false);
    let mut short = Short {
        bytes: Vec::new(),
        interrupt: true,
    };
    stream_effect_intent_payload_v1(&source, &mut context, &mut short).unwrap();
    assert_eq!(short.bytes, expected);
    struct Last<'a> {
        remaining: usize,
        stopped: &'a Cell<bool>,
    }
    impl Write for Last<'_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.remaining -= b.len();
            if self.remaining == 0 {
                self.stopped.set(true);
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let stopped = Cell::new(false);
    let mut context =
        EffectIntentCborContextV1::new(&memory, MemoryClass::Ordinary, allowance(), || {
            stopped.get()
        });
    assert!(matches!(
        stream_effect_intent_payload_v1(
            &source,
            &mut context,
            &mut Last {
                remaining: expected.len(),
                stopped: &stopped
            }
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert!(context.is_poisoned());
    stopped.set(false);
    assert!(matches!(
        hash_effect_intent_payload_v1(&source, &mut context),
        Err(ValueTreeError::Invalid(_))
    ));
}
#[test]
fn finite_node_byte_depth_and_memory_admission_poison() {
    let source = intent(json!([[["x"]]]));
    let memory = MemoryBudget::new(512 << 10, 128 << 10).unwrap();
    for mode in 0..3 {
        let mut limits = allowance();
        match mode {
            0 => limits.nodes_remaining = 0,
            1 => limits.bytes_remaining = 0,
            _ => limits.max_json_depth = 2,
        };
        let mut context =
            EffectIntentCborContextV1::new(&memory, MemoryClass::Ordinary, limits, || false);
        assert!(matches!(
            hash_effect_intent_payload_v1(&source, &mut context),
            Err(ValueTreeError::ResourceLimited(_))
        ));
        assert!(context.is_poisoned());
    }
    let memory = MemoryBudget::new(64 << 10, 64 << 10).unwrap();
    let mut context =
        EffectIntentCborContextV1::new(&memory, MemoryClass::Ordinary, allowance(), || false);
    assert!(matches!(
        hash_effect_intent_payload_v1(&source, &mut context),
        Err(ValueTreeError::Memory(_))
    ));
    assert!(context.is_poisoned());
    let mut recovery =
        EffectIntentCborContextV1::new(&memory, MemoryClass::Recovery, allowance(), || false);
    hash_effect_intent_payload_v1(&source, &mut recovery).unwrap();
}

#[test]
fn sink_zero_overreport_and_io_preserve_exact_error_and_context_poison() {
    struct Bad(u8);
    impl Write for Bad {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            match self.0 {
                0 => Ok(0),
                1 => Ok(b.len() + 1),
                _ => Err(std::io::Error::other("actual sink failed")),
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let source = intent(json!({"nested":["原值",false]}));
    let memory = MemoryBudget::new(512 << 10, 128 << 10).unwrap();
    for mode in 0..3 {
        let mut context =
            EffectIntentCborContextV1::new(&memory, MemoryClass::Ordinary, allowance(), || false);
        // A preceding real complete hash is possible in the same work scope;
        // later failure prevents continuing that scope to an acceptance token.
        hash_effect_intent_payload_v1(&source, &mut context).unwrap();
        let result = stream_effect_intent_payload_v1(&source, &mut context, &mut Bad(mode));
        match mode {
            0 => assert!(
                matches!(result,Err(ValueTreeError::Io(ref e)) if e.kind()==std::io::ErrorKind::WriteZero)
            ),
            1 => assert!(matches!(
                result,
                Err(ValueTreeError::Invalid("effect CBOR sink count"))
            )),
            _ => assert!(
                matches!(result,Err(ValueTreeError::Io(ref e)) if e.to_string()=="actual sink failed")
            ),
        }
        assert!(context.is_poisoned());
        assert!(matches!(
            hash_effect_intent_payload_v1(&source, &mut context),
            Err(ValueTreeError::Invalid("effect CBOR context poisoned"))
        ));
    }
}

#[test]
fn interrupted_sink_attempts_exhaust_finite_work_before_hard_io() {
    struct Retry {
        calls: usize,
    }
    impl Write for Retry {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            if self.calls <= 100 {
                Err(std::io::ErrorKind::Interrupted.into())
            } else {
                Err(std::io::Error::other("retry hard stop"))
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let source = intent(Value::Null);
    let memory = MemoryBudget::new(512 << 10, 128 << 10).unwrap();
    let mut context = EffectIntentCborContextV1::new(
        &memory,
        MemoryClass::Ordinary,
        EffectIntentCborBudgetV1 {
            nodes_remaining: 50,
            ..allowance()
        },
        || false,
    );
    let mut sink = Retry { calls: 0 };
    let result = stream_effect_intent_payload_v1(&source, &mut context, &mut sink);
    assert!(
        matches!(result, Err(ValueTreeError::ResourceLimited(_))),
        "calls={} result={result:?}",
        sink.calls
    );
    assert!(sink.calls < 100);
    assert!(context.is_poisoned());
}
