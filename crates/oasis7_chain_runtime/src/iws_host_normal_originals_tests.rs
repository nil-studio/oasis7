use super::*;
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::incremental_value::ValueIoBudget;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-normal-originals-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 100000,
        read_bytes_remaining: 128 << 20,
        writes_remaining: 100000,
        write_bytes_remaining: 128 << 20,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100000,
        read_bytes_remaining: 128 << 20,
        write_bytes_remaining: 128 << 20,
    }
}
struct Json<'a> {
    memory: &'a MemoryBudget,
    bytes: &'a [u8],
}
impl ObservationJsonOriginalStreamV2 for Json<'_> {
    fn memory(&self) -> &MemoryBudget {
        self.memory
    }
    fn original_id(&self) -> [u8; 32] {
        *blake3::hash(self.bytes).as_bytes()
    }
    fn original_bytes(&self) -> u64 {
        self.bytes.len() as u64
    }
    fn write_to(
        &mut self,
        output: &mut dyn Write,
    ) -> Result<(), ObservationOriginalStreamFailedV2> {
        output
            .write_all(self.bytes)
            .map_err(|_| ObservationOriginalStreamFailedV2)
    }
}
struct Event<'a>(&'a [u8]);
impl AgentEventOriginalStreamV1 for Event<'_> {
    fn original_id(&self) -> [u8; 32] {
        *blake3::hash(self.0).as_bytes()
    }
    fn original_bytes(&self) -> u64 {
        self.0.len() as u64
    }
    fn write_to(&mut self, output: &mut dyn Write) -> Result<(), BodyMutationError> {
        output
            .write_all(self.0)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))
    }
}
#[test]
fn shared_original_consumers_use_real_store_and_one_continuous_loan() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let stop = AtomicBool::new(false);
    let pages = IncrementalPageStore::create(
        &fixture.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let mut p = physical();
    let objects = IncrementalObjectStore::create(&pages, &mut p, &stop).unwrap();
    let mut l = logical();
    let p_before = p.writes_remaining;
    let l_before = l.write_bytes_remaining;
    {
        let owner = HostNormalOriginalsV1::new(
            &objects,
            ObjectIo {
                physical: &mut p,
                logical: &mut l,
                class: MemoryClass::Ordinary,
                cancelled: &stop,
            },
            &memory,
        )
        .unwrap();
        let mut reader = owner.reader();
        let bytes = b"actual runtime artifact bytes";
        let id = *blake3::hash(bytes).as_bytes();
        assert_eq!(
            OriginalObjectStreamWriter::write_original(
                &mut reader,
                &mut &bytes[..],
                bytes.len() as u64,
                id,
                &memory
            )
            .unwrap(),
            id
        );
        assert_eq!(
            reader.read_original(&id, 4096, &memory).unwrap().as_slice(),
            bytes
        );
        let mut json = Json {
            memory: &memory,
            bytes: b"{\"actual\":\"Current observation\"}",
        };
        let json_id = json.original_id();
        assert_eq!(
            owner
                .observation_sink()
                .finish_json_original(&mut json, &mut || false)
                .unwrap(),
            json_id
        );
        assert_eq!(
            reader
                .read_original(&json_id, 4096, &memory)
                .unwrap()
                .as_slice(),
            json.bytes
        );
        let mut event = Event(b"actual producer stream bytes");
        let event_id = event.original_id();
        HostNormalAgentSinkV1(&owner)
            .finish_original(&mut event, &mut || false)
            .unwrap();
        assert_eq!(
            reader
                .read_original(&event_id, 4096, &memory)
                .unwrap()
                .as_slice(),
            event.0
        );
    }
    assert!(p.writes_remaining < p_before);
    assert!(l.write_bytes_remaining < l_before);
}
#[test]
fn shared_original_reentry_is_typed_and_does_not_spend_io() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let stop = AtomicBool::new(false);
    let pages = IncrementalPageStore::create(
        &fixture.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let mut p = physical();
    let objects = IncrementalObjectStore::create(&pages, &mut p, &stop).unwrap();
    let mut l = logical();
    let owner = HostNormalOriginalsV1::new(
        &objects,
        ObjectIo {
            physical: &mut p,
            logical: &mut l,
            class: MemoryClass::Ordinary,
            cancelled: &stop,
        },
        &memory,
    )
    .unwrap();
    let held = owner.io.borrow_mut();
    let p_before = held.physical.reads_remaining;
    let w_before = held.physical.writes_remaining;
    let l_before = held.logical.nodes_remaining;
    assert!(matches!(
        owner.reader().read_original(&[0; 32], 4096, &memory),
        Err(PageStoreError::Corrupt("normal original IO reentry"))
    ));
    assert!(matches!(
        owner.observation_sink().finish_json_original(
            &mut Json {
                memory: &memory,
                bytes: b"{}"
            },
            &mut || false
        ),
        Err(ObjectStreamWriteError::Store(PageStoreError::Corrupt(
            "normal original IO reentry"
        )))
    ));
    assert!(
        matches!(HostNormalAgentSinkV1(&owner).finish_original(&mut Event(b"event"),&mut ||false),Err(BodyMutationError::Original(error)) if matches!(error.downcast_ref::<PageStoreError>(),Some(PageStoreError::Corrupt("normal original IO reentry"))))
    );
    assert_eq!(held.physical.reads_remaining, p_before);
    assert_eq!(held.physical.writes_remaining, w_before);
    assert_eq!(held.logical.nodes_remaining, l_before);
}

#[test]
fn fixed_pair_replays_actual_stored_original_for_one_shot_input() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let stop = AtomicBool::new(false);
    let primary = IncrementalPageStore::create(
        &fixture.0.join("primary"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let replica = IncrementalPageStore::create(
        &fixture.0.join("replica"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let (mut pp, mut rp) = (physical(), physical());
    let p = IncrementalObjectStore::create(&primary, &mut pp, &stop).unwrap();
    let r = IncrementalObjectStore::create(&replica, &mut rp, &stop).unwrap();
    let (mut pl, mut rl) = (logical(), logical());
    let bytes = b"nonrewindable original consumed once and verified twice";
    let id = *blake3::hash(bytes).as_bytes();
    {
        let owner = HostNormalOriginalsV1::new_pair(
            &p,
            ObjectIo {
                physical: &mut pp,
                logical: &mut pl,
                class: MemoryClass::Ordinary,
                cancelled: &stop,
            },
            &r,
            ObjectIo {
                physical: &mut rp,
                logical: &mut rl,
                class: MemoryClass::Ordinary,
                cancelled: &stop,
            },
            &memory,
        )
        .unwrap();
        let mut once = &bytes[..];
        let mut writer = owner.reader();
        assert_eq!(
            writer
                .write_original(&mut once, bytes.len() as u64, id, &memory)
                .unwrap(),
            id
        );
        assert!(once.is_empty());
    }
    let mut io = ObjectIo {
        physical: &mut rp,
        logical: &mut rl,
        class: MemoryClass::Ordinary,
        cancelled: &stop,
    };
    let actual = r
        .provisional_read_write_session(&mut io)
        .unwrap()
        .read_original(&id, 4096, &memory)
        .unwrap();
    assert_eq!(actual.as_slice(), bytes);
    assert!(pp.reads_remaining < physical().reads_remaining);
    assert!(rp.writes_remaining < physical().writes_remaining);
}
