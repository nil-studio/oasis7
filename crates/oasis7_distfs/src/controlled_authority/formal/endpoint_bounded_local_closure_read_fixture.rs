//! Fresh original immutable writer representations, data only; no qualification.
use super::super::incremental::local_closure::{
    LOCAL_DATA_CLOSURE_WIRE_BYTES, LocalDataClosureRecordV1,
};
use super::*;
use crate::controlled_authority::formal::{
    business_index::BusinessIndexNodeWriteV1, business_pack, physical,
};
use oasis7_proto::memory_budget::MemoryBudget;
#[derive(Clone, Copy, Debug)]
pub(in super::super) enum LocalClosureReadFixtureV1 {
    Raw,
    Physical,
    LegacyPack,
    Generation,
    GenerationMissingSelected,
}
pub(in super::super) fn with_local_closure_read_fixture(
    mode: LocalClosureReadFixtureV1,
    run: impl FnOnce(&Store, &str, &LocalDataClosureRecordV1, &Path),
) {
    let fixture = Fixture::new();
    let primary = fixture.endpoint(EndpointRole::Primary);
    let before = primary.store.checkpoint_bytes().unwrap();
    let domain = b"oasis7.iws.local-data-closure.v1\0";
    let mut payload = vec![0u8; LOCAL_DATA_CLOSURE_WIRE_BYTES - 3];
    payload[..domain.len()].copy_from_slice(domain);
    payload[domain.len()..domain.len() + 4].copy_from_slice(&1u32.to_be_bytes());
    let original = serde_cbor::to_vec(&serde_cbor::Value::Bytes(payload)).unwrap();
    let record: LocalDataClosureRecordV1 = serde_cbor::from_slice(&original).unwrap();
    let id = blake3::hash(&original).to_hex().to_string();
    let objects = fixture.pdir.join("formal-objects-v1");
    match mode {
        LocalClosureReadFixtureV1::Raw => {}
        LocalClosureReadFixtureV1::Physical => {
            let padding = vec![13u8; 16384];
            let mut writes = vec![
                (id.clone(), original.clone()),
                (blake3::hash(&padding).to_hex().to_string(), padding),
            ];
            writes.sort_by(|a, b| a.0.cmp(&b.0));
            assert!(physical::persist(&objects, &writes).unwrap());
            assert!(objects.join(".representation-v1").join(&id).is_file());
            assert!(!objects.join(&id).exists());
        }
        LocalClosureReadFixtureV1::LegacyPack => {
            let padding = vec![7u8; 2000];
            let mut writes = vec![
                BusinessIndexNodeWriteV1 {
                    digest: id.clone(),
                    bytes: original.clone(),
                },
                BusinessIndexNodeWriteV1 {
                    digest: blake3::hash(&padding).to_hex().to_string(),
                    bytes: padding,
                },
            ];
            writes.sort_by(|a, b| a.digest.cmp(&b.digest));
            business_pack::persist(&objects, &writes).unwrap();
            assert!(
                fs::read(objects.join(&id))
                    .unwrap()
                    .starts_with(b"oasis7.formal.business-cas.pack.v1\0")
            );
        }
        LocalClosureReadFixtureV1::Generation
        | LocalClosureReadFixtureV1::GenerationMissingSelected => {
            fs::write(objects.join(&id), &original).unwrap();
            physical::roots::retain(&objects, std::slice::from_ref(&id)).unwrap();
            let mut session = physical::GenerationSession::default();
            let mut selected = None;
            for _ in 0..100 {
                let progress = physical::advance_with_session(&objects, &mut session).unwrap();
                if !progress.building {
                    selected = progress.selected;
                    break;
                }
            }
            let dir = objects
                .join(".generations-v1")
                .join(selected.expect("real generation completed"));
            assert!(dir.join(&id).is_file() || dir.join(".representation-v1").join(&id).is_file());
            if matches!(mode, LocalClosureReadFixtureV1::GenerationMissingSelected) {
                for path in [
                    dir.join(&id),
                    dir.join(".representation-v1").join(&id),
                    objects.join(&id),
                ] {
                    match fs::remove_file(path) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => panic!("{e}"),
                    }
                }
                fs::write(objects.join(&id), &original).unwrap();
            }
        }
    }
    let memory = MemoryBudget::new(256 << 20, 0).unwrap();
    if matches!(mode, LocalClosureReadFixtureV1::GenerationMissingSelected) {
        assert!(
            primary
                .store
                .put_local_closure_record(&record, &memory, &mut || false)
                .is_err()
        );
    } else {
        assert_eq!(
            primary
                .store
                .put_local_closure_record(&record, &memory, &mut || false)
                .unwrap(),
            id
        );
        let old: LocalDataClosureRecordV1 = primary.store.get(&id).unwrap();
        assert_eq!(serde_cbor::to_vec(&old).unwrap(), original);
    }
    assert_eq!(memory.used(), 0);
    run(&primary.store, &id, &record, &objects);
    assert_eq!(primary.store.checkpoint_bytes().unwrap(), before);
}
