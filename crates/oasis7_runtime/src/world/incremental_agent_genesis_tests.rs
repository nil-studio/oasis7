use super::*;
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueReadContext};
use oasis7_proto::memory_budget::MemoryBudget;
use oasis7_world_state::{AgentCell, AgentState};
use std::collections::BTreeMap;

fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "oasis7-agent-fields-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn actual_agent_genesis_keeps_original_mailbox_occurrences_and_provisional_failures() {
    let dir = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let mut world = World::new();
    let mut cell = AgentCell::new(
        AgentState::new(
            "alice",
            oasis7_agent_api::world_action::GeoPos::new(-1, 2, 3),
        ),
        7,
    );
    let event = DomainEvent::BodyAttributesUpdated {
        agent_id: "alice".into(),
        view: cell.state.body_view.clone(),
        reason: "same occurrence content".into(),
    };
    cell.mailbox.extend([event.clone(), event]);
    world.state.agents.insert("alice".into(), cell);
    let before = world.state.clone();
    for failure in 0..5 {
        let mut sink = store.value_session(physical());
        let mut io = logical();
        let mut fields = BTreeMap::new();
        let mut occurrences = Vec::new();
        let result = world.write_iws_agent_genesis_fields(
            "alice",
            &mut AgentGenesisContext {
                mutation: BodyMutationContext {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut io,
                    cancelled: || failure == 1,
                },
                emit: |ns, key: &[u8], id| {
                    if failure == 2 {
                        return Err(BodyMutationError::Invalid("injected emit failure"));
                    }
                    assert!(fields.insert((ns as u8, key.to_vec()), id).is_none());
                    Ok(())
                },
                mailbox_original: |ordinal, original: &DomainEvent| {
                    if failure == 3 {
                        return Err(BodyMutationError::Invalid("missing original provenance"));
                    }
                    assert_eq!(
                        original,
                        &world.state.agents["alice"].mailbox[ordinal as usize]
                    );
                    occurrences.push(ordinal);
                    Ok([ordinal as u8 + 1; 32])
                },
                max_scalar_bytes: if failure == 4 { 1 } else { 4096 },
            },
        );
        assert_eq!(world.state, before);
        if failure != 0 {
            assert!(result.is_err());
            continue;
        }
        assert_eq!(result.unwrap() as usize, fields.len());
        assert_eq!(occurrences, vec![0, 1]);
        let values = store.value_session(physical());
        let mut quota = logical();
        let mut reader = ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut quota,
        };
        for ordinal in 0..2 {
            let key = MailboxKeyV1::new("alice", Some(ordinal)).unwrap();
            let id = fields[&(Ns::Scheduler as u8, key.as_bytes().to_vec())];
            let root = reader.load_root(&id).unwrap();
            let mut bytes = Vec::new();
            reader
                .read_range(&root, 0, root.0.logical_len, &mut bytes, || false)
                .unwrap();
            assert_eq!(
                oasis7_world_state::incremental_event_fields::mailbox_event_id_v1(&bytes).unwrap(),
                [ordinal as u8 + 1; 32]
            );
        }
    }
}

#[test]
fn large_summary_streams_and_small_status_update_reopens_independently() {
    use std::io::Write;
    let dir = Directory::new();
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let length = (64 << 20) + 123;
    // The source string itself is admitted; the writer must not make another
    // summary-sized encoding buffer within this pool.
    let source = memory
        .reserve(length, MemoryClass::Ordinary)
        .unwrap()
        .retain("x".repeat(length));
    let root_path = dir.0.join("pages");
    let store =
        IncrementalPageStore::create(&root_path, memory.clone(), PageCompression::Raw).unwrap();
    let mut sink = store.value_session(physical());
    let mut io = logical();
    let mut context = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut io,
        cancelled: || false,
    };
    let summary = write_agent_scalar_value(
        AgentFieldV1::IntentSummary,
        AgentScalarRefV1::Text(source.as_str()),
        &mut context,
        128 << 20,
    )
    .unwrap();
    let before = context.io.nodes_remaining;
    let status = write_agent_scalar_value(
        AgentFieldV1::IntentStatus,
        AgentScalarRefV1::Text("committed"),
        &mut context,
        4096,
    )
    .unwrap();
    assert_eq!(before - context.io.nodes_remaining, 2);
    drop(sink);
    drop(store);
    drop(source);
    let recovery = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
    let store = IncrementalPageStore::open(
        &root_path,
        recovery.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut reader = ValueReadContext {
        source: &values,
        memory: &recovery,
        class: MemoryClass::Recovery,
        io: &mut io,
    };
    let root = reader.load_root(&summary).unwrap();
    let mut expected = blake3::Hasher::new();
    struct HashWriter<'a>(&'a mut blake3::Hasher);
    impl Write for HashWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // Derive the oracle from explicit wire bytes, not the scalar writer.
    expected.update(b"oasis7.iws.agent-scalar.v1\0");
    expected.update(&[7]);
    expected.update(&(length as u64).to_be_bytes());
    for _ in 0..(length / 1024) {
        expected.update(&[b'x'; 1024]);
    }
    expected.update(&vec![b'x'; length % 1024]);
    let mut actual = blake3::Hasher::new();
    reader
        .read_range(
            &root,
            0,
            root.0.logical_len,
            &mut HashWriter(&mut actual),
            || false,
        )
        .unwrap();
    assert_eq!(actual.finalize(), expected.finalize());
    let root = reader.load_root(&status).unwrap();
    let mut bytes = Vec::new();
    reader
        .read_range(&root, 0, root.0.logical_len, &mut bytes, || false)
        .unwrap();
    assert_eq!(
        AgentScalarRefV1::from_canonical_bytes(&bytes, AgentFieldV1::IntentStatus).unwrap(),
        AgentScalarRefV1::Text("committed")
    );
}
