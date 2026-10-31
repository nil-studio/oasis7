#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{
        ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError, write_value,
    },
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_field_member::*, incremental_field_read::*,
    incremental_world_rule_effects::*, world_rules::WorldRulesStateV1,
};
use std::{
    cell::Cell,
    io::{self, Write},
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
#[path = "support/incremental_world_rules_fixture.rs"]
mod rules_fixture;
use rules_fixture::*;

fn semantic(key: Option<&str>) -> Vec<u8> {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    if let Some(key) = key {
        bytes.push(5); // Closed pair, then two independent Text frames.
        for text in ["governance_effects", key] {
            bytes.push(1);
            bytes.extend_from_slice(&(text.len() as u64).to_be_bytes());
            bytes.extend_from_slice(text.as_bytes());
        }
    } else {
        bytes.push(0);
    }
    bytes
}
fn key(original: Option<&str>) -> WorldFieldKeyV1 {
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic(original)).as_bytes()),
    )
    .unwrap()
}
fn metadata() -> WorldFieldKeyV1 {
    WorldFieldKeyV1::new(descriptor(), WorldFieldRecordKindV1::Metadata, None).unwrap()
}
fn source() -> WorldState {
    WorldState {
        world_rules: Some(WorldRulesStateV1 {
            governance_effects: [
                ("".into(), "".into()),
                (" P\0é ".into(), " R\0甲 ".into()),
                ("unique-noop".into(), "keep".into()),
            ]
            .into(),
            ..WorldRulesStateV1::default()
        }),
        ..WorldState::default()
    }
}
struct Reader<'a> {
    store: &'a IncrementalPageStore,
    memory: &'a MemoryBudget,
    class: MemoryClass,
    state_io: PageIoBudget,
}
impl Reader<'_> {
    fn read(
        &self,
        root: &StateRootV1,
        key: &str,
        max: u64,
        output: &mut impl Write,
        cancelled: impl FnMut() -> bool,
    ) -> Result<WorldRuleEffectValueV1, ValueTreeError> {
        let state = self
            .store
            .state_session(self.state_io.clone(), self.class)
            .unwrap();
        let values = self.store.value_session(physical());
        read_world_rule_effect_v1(
            key,
            &mut WorldFieldReadContextV1 {
                root,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: self.memory,
                    class: self.class,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled,
            },
            max,
            output,
        )
    }
}
struct CancelOutput<'a> {
    cancelled: &'a Cell<bool>,
    interrupted: bool,
    calls: usize,
}
impl Write for CancelOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        self.cancelled.set(true);
        if self.interrupted {
            Err(io::ErrorKind::Interrupted.into())
        } else {
            Ok(1.min(bytes.len()))
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn rule_option_and_original_effects_have_separate_typed_cold_paths() {
    use WorldRuleEffectValueV1 as V;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let producer = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _source = producer.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
        let roots = {
            let store = IncrementalPageStore::create(&path, producer.clone(), compression).unwrap();
            let _batch = producer.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
            let good = produce(&store, &producer, &source());
            // Independent descriptor, pair framing, literal tags and original bytes.
            for (original, codec, payload) in [
                (None, 4, &[1][..]),
                (Some(""), 5, &b""[..]),
                (Some(" P\0é "), 5, " R\0甲 ".as_bytes()),
            ] {
                let key = key(original);
                let semantic = semantic(original);
                let id = good.iter().find(|(k, _)| k == key.as_bytes()).unwrap().1;
                let mut expected = b"oasis7.iws.field-member.v1\0".to_vec();
                expected.extend_from_slice(&(key.as_bytes().len() as u16).to_be_bytes());
                expected.push(codec);
                expected.extend_from_slice(&(semantic.len() as u64).to_be_bytes());
                expected.extend_from_slice(&(payload.len() as u64).to_be_bytes());
                expected.extend_from_slice(key.as_bytes());
                expected.extend(semantic);
                expected.extend(payload);
                assert_eq!(raw(&store, &producer, &id), expected);
            }
            let absent = produce(&store, &producer, &WorldState::default());
            let empty = produce(
                &store,
                &producer,
                &WorldState {
                    world_rules: Some(WorldRulesStateV1::default()),
                    ..WorldState::default()
                },
            );
            let mut variants = vec![good.clone(), absent, empty];
            let mut missing_meta = good.clone();
            missing_meta.retain(|(k, _)| k != metadata().as_bytes());
            variants.push(missing_meta);
            let mut missing_parent = good.clone();
            missing_parent.retain(|(k, _)| k != key(None).as_bytes());
            variants.push(missing_parent);
            variants.push(replace(&store, &producer, &good, &key(None), |b| {
                *b.last_mut().unwrap() = 0
            }));
            variants.push(replace(&store, &producer, &good, &key(None), |b| {
                *b.last_mut().unwrap() = 2
            }));
            variants.push(replace(&store, &producer, &good, &key(None), |b| {
                b[b"oasis7.iws.field-member.v1\0".len() + 2] = 5
            }));
            variants.push(replace(
                &store,
                &producer,
                &good,
                &key(Some(" P\0é ")),
                |b| *b.last_mut().unwrap() = 0xff,
            ));
            variants.push(replace(
                &store,
                &producer,
                &good,
                &key(Some(" P\0é ")),
                |b| b[b"oasis7.iws.field-member.v1\0".len() + 2] = 7,
            ));
            variants
                .iter()
                .map(|rows| build(&store, &producer, rows).canonical_bytes().unwrap())
                .collect::<Vec<_>>()
        };
        drop(_source);
        drop(producer);
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(
                4 << 20,
                if class == MemoryClass::Recovery {
                    3 << 20
                } else {
                    1 << 20
                },
            )
            .unwrap();
            let _full = (class == MemoryClass::Recovery)
                .then(|| memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap());
            if _full.is_some() {
                assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
            }
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let _output = memory.reserve(16 << 10, class).unwrap();
            let reader = Reader {
                store: &store,
                memory: &memory,
                class,
                state_io: physical(),
            };
            let good = StateRootV1::from_canonical_bytes(&roots[0]).unwrap();
            for (index, key, expected, text) in [
                (0, "", V::Value(0), ""),
                (0, " P\0é ", V::Value(" R\0甲 ".len() as u64), " R\0甲 "),
                (0, "missing", V::EffectAbsent, ""),
                (1, " P\0é ", V::RulesAbsent, ""),
                (2, " P\0é ", V::EffectAbsent, ""),
            ] {
                let root = StateRootV1::from_canonical_bytes(&roots[index]).unwrap();
                let mut output = Vec::with_capacity(128);
                assert_eq!(
                    reader.read(&root, key, 128, &mut output, || false).unwrap(),
                    expected
                );
                assert_eq!(output, text.as_bytes());
            }
            for bytes in &roots[3..] {
                assert!(matches!(
                    reader.read(
                        &StateRootV1::from_canonical_bytes(bytes).unwrap(),
                        " P\0é ",
                        128,
                        &mut io::sink(),
                        || false
                    ),
                    Err(ValueTreeError::Invalid(_))
                ));
            }
            for interrupted in [false, true] {
                let cancelled = Cell::new(false);
                let mut out = CancelOutput {
                    cancelled: &cancelled,
                    interrupted,
                    calls: 0,
                };
                assert!(matches!(
                    reader.read(&good, " P\0é ", 128, &mut out, || cancelled.get()),
                    Err(ValueTreeError::Cancelled)
                ));
                assert_eq!(out.calls, 1);
            }
            assert!(matches!(
                reader.read(&good, " P\0é ", 0, &mut io::sink(), || false),
                Err(ValueTreeError::ResourceLimited(_))
            ));
            let no_io = Reader {
                state_io: PageIoBudget {
                    reads_remaining: 0,
                    ..physical()
                },
                ..reader
            };
            assert!(matches!(
                no_io.read(&good, "missing", 128, &mut io::sink(), || false),
                Err(ValueTreeError::ResourceLimited(_))
            ));
            assert!(matches!(
                no_io.read(
                    &StateRootV1::empty(Ns::Business).unwrap(),
                    "missing",
                    128,
                    &mut io::sink(),
                    || false
                ),
                Err(ValueTreeError::Invalid("field namespace"))
            ));
        }
        overlay::exercise(&path, compression, &roots);
    }
}

#[test]
fn rule_genesis_failures_never_return_a_completed_prefix() {
    let dir = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let _source = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let source = source();
    for case in 0..11 {
        let mut values = store.value_session(physical());
        let cancel = Cell::new(case == 3);
        let mut io = logical();
        if case == 8 {
            io.write_bytes_remaining = 0;
        }
        let mut records = match case {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 5,
        };
        let mut count = 0;
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: if case == 7 { 0 } else { 8192 },
        };
        let mut bound = limits();
        if case == 6 {
            bound.payload_bytes = 0;
        }
        if case == 9 {
            bound.record_bytes = 0;
        }
        if case == 10 {
            bound.semantic_bytes = 0;
        }
        let result = prepare_world_rule_effects_genesis_v1(
            &source,
            &mut MemberRecordWriteContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io,
                key_work: &mut key_work,
                limits: bound,
                cancelled: || cancel.get(),
            },
            &mut records,
            &mut |_row| {
                count += 1;
                if case == 4 {
                    cancel.set(true);
                }
                if case == 5 {
                    return Err(ValueTreeError::Invalid("injected rule sink"));
                }
                Ok(())
            },
        );
        assert!(result.is_err(), "case {case}");
        if case == 0 || case == 3 {
            assert_eq!(count, 0);
        }
        // The sink drops every charged record. No root is constructed on error.
    }
}

#[path = "support/incremental_world_rule_effects_overlay.rs"]
mod overlay;
