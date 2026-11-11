//! Data-only immutable-store consumer proof; this record is never installed as
//! a Qualified checkpoint reference or used as execution/namespace authority.
use super::*;
#[test]
fn bounded_local_closure_store_preserves_wire_and_checkpoint() {
    use super::super::incremental::local_closure::{
        LOCAL_DATA_CLOSURE_WIRE_BYTES, LocalDataClosureRecordV1,
    };
    use oasis7_proto::memory_budget::MemoryBudget;
    let fixture = Fixture::new();
    let primary = fixture.endpoint(EndpointRole::Primary);
    let checkpoint = primary.store.body.clone();
    let domain = b"oasis7.iws.local-data-closure.v1\0";
    let mut payload = vec![0u8; LOCAL_DATA_CLOSURE_WIRE_BYTES - 3];
    payload[..domain.len()].copy_from_slice(domain);
    payload[domain.len()..domain.len() + 4].copy_from_slice(&1u32.to_be_bytes());
    let original = serde_cbor::to_vec(&serde_cbor::Value::Bytes(payload)).unwrap();
    assert_eq!(original.len(), LOCAL_DATA_CLOSURE_WIRE_BYTES);
    let record: LocalDataClosureRecordV1 = serde_cbor::from_slice(&original).unwrap();
    assert_eq!(serde_cbor::to_vec(&record).unwrap(), original);
    let expected = blake3::hash(&original).to_hex().to_string();
    let object = fixture.pdir.join("formal-objects-v1").join(&expected);
    assert!(!object.exists());
    let memory = MemoryBudget::new(256 << 20, 0).unwrap();
    let before = memory.used();
    assert!(
        primary
            .store
            .put_local_closure_record(&record, &memory, &mut || true)
            .is_err()
    );
    assert!(!object.exists());
    let id = primary
        .store
        .put_local_closure_record(&record, &memory, &mut || false)
        .unwrap();
    assert_eq!(id, expected);
    assert_eq!(fs::read(&object).unwrap(), original);
    assert_eq!(
        primary
            .store
            .put_local_closure_record(&record, &memory, &mut || false)
            .unwrap(),
        expected
    );
    assert_eq!(primary.store.body, checkpoint);
    assert_eq!(memory.used(), before);
}

#[cfg(unix)]
#[test]
fn bounded_local_closure_store_reads_real_physical_selector_before_retry() {
    use super::super::incremental::local_closure::{
        LOCAL_DATA_CLOSURE_WIRE_BYTES, LocalDataClosureRecordV1,
    };
    use crate::controlled_authority::formal::physical;
    use oasis7_proto::memory_budget::MemoryBudget;
    let fixture = Fixture::new();
    let primary = fixture.endpoint(EndpointRole::Primary);
    let checkpoint = primary.store.body.clone();
    let domain = b"oasis7.iws.local-data-closure.v1\0";
    let mut payload = vec![0u8; LOCAL_DATA_CLOSURE_WIRE_BYTES - 3];
    payload[..domain.len()].copy_from_slice(domain);
    payload[domain.len()..domain.len() + 4].copy_from_slice(&1u32.to_be_bytes());
    let original = serde_cbor::to_vec(&serde_cbor::Value::Bytes(payload)).unwrap();
    let record: LocalDataClosureRecordV1 = serde_cbor::from_slice(&original).unwrap();
    let id = blake3::hash(&original).to_hex().to_string();
    let objects = fixture.pdir.join("formal-objects-v1");
    let padding = vec![13u8; 16_384];
    let mut physical_objects = vec![
        (id.clone(), original.clone()),
        (blake3::hash(&padding).to_hex().to_string(), padding),
    ];
    physical_objects.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(physical::persist(&objects, &physical_objects).unwrap());
    assert!(objects.join(".representation-v1").join(&id).is_file());
    assert!(!objects.join(&id).exists());
    let memory = MemoryBudget::new(256 << 20, 0).unwrap();
    assert_eq!(
        primary
            .store
            .put_local_closure_record(&record, &memory, &mut || false)
            .unwrap(),
        id
    );
    assert!(
        !objects.join(&id).exists(),
        "retry must reuse physical bytes without making a raw alias"
    );
    assert_eq!(
        physical::read_object(&objects.join(&id), &id, true)
            .unwrap()
            .unwrap(),
        original
    );
    assert_eq!(primary.store.body, checkpoint);
    assert_eq!(memory.used(), 0);
}

#[test]
fn bounded_local_closure_store_reads_small_member_from_real_legacy_pack() {
    use super::super::incremental::local_closure::{
        LOCAL_DATA_CLOSURE_WIRE_BYTES, LocalDataClosureRecordV1,
    };
    use crate::controlled_authority::formal::{
        business_index::BusinessIndexNodeWriteV1, business_pack,
    };
    use oasis7_proto::memory_budget::MemoryBudget;
    let fixture = Fixture::new();
    let primary = fixture.endpoint(EndpointRole::Primary);
    let checkpoint = primary.store.body.clone();
    let domain = b"oasis7.iws.local-data-closure.v1\0";
    let mut payload = vec![0u8; LOCAL_DATA_CLOSURE_WIRE_BYTES - 3];
    payload[..domain.len()].copy_from_slice(domain);
    payload[domain.len()..domain.len() + 4].copy_from_slice(&1u32.to_be_bytes());
    let original = serde_cbor::to_vec(&serde_cbor::Value::Bytes(payload)).unwrap();
    let record: LocalDataClosureRecordV1 = serde_cbor::from_slice(&original).unwrap();
    let id = blake3::hash(&original).to_hex().to_string();
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
    let objects = fixture.pdir.join("formal-objects-v1");
    business_pack::persist(&objects, &writes).unwrap();
    let packed = fs::read(objects.join(&id)).unwrap();
    assert!(packed.starts_with(b"oasis7.formal.business-cas.pack.v1\0"));
    assert!(packed.len() > original.len());
    let memory = MemoryBudget::new(256 << 20, 0).unwrap();
    assert_eq!(
        primary
            .store
            .put_local_closure_record(&record, &memory, &mut || false)
            .unwrap(),
        id
    );
    assert_eq!(
        fs::read(objects.join(&id)).unwrap(),
        packed,
        "retry must not replace the shared legacy pack with a raw member"
    );
    assert_eq!(
        business_pack::read_object_durable(&objects.join(&id), &id)
            .unwrap()
            .unwrap(),
        original
    );
    assert_eq!(primary.store.body, checkpoint);
    assert_eq!(memory.used(), 0);
}

#[cfg(unix)]
#[test]
fn bounded_local_closure_store_rejects_missing_selected_with_stale_overlay() {
    use super::super::incremental::local_closure::{
        LOCAL_DATA_CLOSURE_WIRE_BYTES, LocalDataClosureRecordV1,
    };
    use crate::controlled_authority::formal::physical;
    use oasis7_proto::memory_budget::MemoryBudget;
    let fixture = Fixture::new();
    let primary = fixture.endpoint(EndpointRole::Primary);
    let checkpoint = primary.store.body.clone();
    let domain = b"oasis7.iws.local-data-closure.v1\0";
    let mut payload = vec![0u8; LOCAL_DATA_CLOSURE_WIRE_BYTES - 3];
    payload[..domain.len()].copy_from_slice(domain);
    payload[domain.len()..domain.len() + 4].copy_from_slice(&1u32.to_be_bytes());
    let original = serde_cbor::to_vec(&serde_cbor::Value::Bytes(payload)).unwrap();
    let record: LocalDataClosureRecordV1 = serde_cbor::from_slice(&original).unwrap();
    let id = blake3::hash(&original).to_hex().to_string();
    let objects = fixture.pdir.join("formal-objects-v1");
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
    let generation = selected.expect("real generation completed");
    let dir = objects.join(".generations-v1").join(generation);
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
    let memory = MemoryBudget::new(256 << 20, 0).unwrap();
    let result = primary
        .store
        .put_local_closure_record(&record, &memory, &mut || false);
    assert!(
        result.is_err(),
        "catalog-selected object is missing; stale overlay must not satisfy retry"
    );
    assert_eq!(primary.store.body, checkpoint);
    assert_eq!(memory.used(), 0);
}

#[test]
fn bounded_archive_store_caller_keeps_paid_original_and_checkpoint() {
    use super::super::FormalArchiveReadWorkV1;
    use oasis7_proto::memory_budget::MemoryBudget;
    let fixture = Fixture::new();
    let primary = fixture.endpoint(EndpointRole::Primary);
    let checkpoint = primary.store.body.clone();
    let payload = b"data-only Store archive original";
    let id = blake3::hash(payload).to_hex().to_string();
    let path = fixture.pdir.join("formal-objects-v1").join(&id);
    fs::write(&path, payload).unwrap();
    let memory = MemoryBudget::new(256 << 20, 0).unwrap();
    let fresh = || FormalArchiveReadWorkV1::new(10000, 128 << 20).unwrap();
    let mut work = fresh();
    let owner = primary
        .store
        .raw_object_bounded(&id, payload.len(), &memory, &mut work, &mut || false)
        .unwrap()
        .unwrap();
    assert_eq!(owner.as_slice(), payload);
    assert!(owner.uses_budget(&memory));
    assert_eq!(memory.used(), owner.reserved_bytes());
    assert!(work.remaining_steps() < 10000);
    drop(owner);
    assert_eq!(memory.used(), 0);
    assert!(
        primary
            .store
            .raw_object_bounded(&id, payload.len() - 1, &memory, &mut fresh(), &mut || false)
            .is_err()
    );
    assert!(
        primary
            .store
            .raw_object_bounded(&id, payload.len(), &memory, &mut fresh(), &mut || true)
            .is_err()
    );
    fs::write(&path, b"corrupt").unwrap();
    assert!(
        primary
            .store
            .raw_object_bounded(&id, payload.len(), &memory, &mut fresh(), &mut || false)
            .is_err()
    );
    fs::remove_file(path).unwrap();
    assert!(
        primary
            .store
            .raw_object_bounded(&id, payload.len(), &memory, &mut fresh(), &mut || false)
            .unwrap()
            .is_none()
    );
    assert_eq!(primary.store.body, checkpoint);
    assert_eq!(memory.used(), 0);
}
