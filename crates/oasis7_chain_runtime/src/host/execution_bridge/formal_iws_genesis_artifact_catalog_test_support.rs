//! Independent fixture oracle and actual per-namespace Catalog cold reads.
//! These checks use no in-memory page source and confer no acceptance authority.
use super::*;
use oasis7_formal_protocol::{
    incremental_artifacts::{
        ARTIFACT_MAX_PAGE_BYTES, ArtifactCatalogV1, ArtifactEntryV1, ArtifactKindV1,
    },
    incremental_execution::ExecutionError,
    incremental_state::MerkleNodeReader,
};

pub(super) struct ExpectedArtifact {
    pub(super) key: &'static str,
    pub(super) bytes: &'static [u8],
    pub(super) reference: OriginalArtifactRefV1,
}

// Independent frozen-format oracle for the one/two original fixtures. This
// bounded reference tree never invokes production Catalog/Codec or serves IO.
fn expected_wasm_catalog_original(
    originals: &[ExpectedArtifact],
) -> (OriginalArtifactRefV1, Vec<u8>) {
    assert!((1..=2).contains(&originals.len()));
    fn page(payload: &[u8]) -> [u8; 32] {
        let mut wire = b"oasis7.iws.artifact-page.v1\0".to_vec();
        wire.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        wire.extend_from_slice(payload);
        *blake3::hash(&wire).as_bytes()
    }
    fn branch(depth: u16, left: [u8; 32], right: [u8; 32]) -> [u8; 32] {
        let mut payload = vec![1];
        payload.extend_from_slice(&depth.to_be_bytes());
        payload.extend_from_slice(&left);
        payload.extend_from_slice(&right);
        page(&payload)
    }
    let mut nodes = originals
        .iter()
        .map(|artifact| {
            let original = artifact.reference;
            let mut leaf = vec![0, 1];
            leaf.extend_from_slice(&original.id);
            leaf.extend_from_slice(&original.byte_len.to_be_bytes());
            (original.id, page(&leaf))
        })
        .collect::<Vec<_>>();
    let mut empty = page(&[2]);
    for depth in (0..256).rev() {
        let mut pairs: Vec<([u8; 32], [Option<[u8; 32]>; 2])> = Vec::with_capacity(2);
        for (mut path, id) in nodes {
            let mask = 0x80 >> (depth % 8);
            let right = usize::from(path[depth / 8] & mask != 0);
            path[depth / 8] &= !mask;
            if let Some((_, children)) = pairs.iter_mut().find(|(prefix, _)| *prefix == path) {
                assert!(children[right].replace(id).is_none());
            } else {
                let mut children = [None; 2];
                children[right] = Some(id);
                pairs.push((path, children));
            }
        }
        nodes = pairs
            .into_iter()
            .map(|(path, children)| {
                (
                    path,
                    branch(
                        depth as u16,
                        children[0].unwrap_or(empty),
                        children[1].unwrap_or(empty),
                    ),
                )
            })
            .collect();
        empty = branch(depth as u16, empty, empty);
    }
    assert_eq!(nodes.len(), 1);
    let mut wire = b"oasis7.iws.artifact-root.v1\0".to_vec();
    wire.extend_from_slice(&41u32.to_be_bytes());
    wire.push(1);
    wire.extend_from_slice(&(originals.len() as u64).to_be_bytes());
    wire.extend_from_slice(&nodes[0].1);
    (
        OriginalArtifactRefV1 {
            id: *blake3::hash(&wire).as_bytes(),
            byte_len: wire.len() as u64,
        },
        wire,
    )
}

struct ColdCatalogPage(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for ColdCatalogPage {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
struct ColdCatalogNodes<'a, 'store, 'io> {
    objects: &'a IncrementalObjectStore<'store>,
    memory: &'a MemoryBudget,
    io: RefCell<ObjectIo<'io>>,
    reads: Cell<usize>,
}
impl MerkleNodeReader for ColdCatalogNodes<'_, '_, '_> {
    type Node = ColdCatalogPage;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn read_node(&self, digest: &str, max: usize) -> Result<Option<Self::Node>, String> {
        assert_eq!(max, ARTIFACT_MAX_PAGE_BYTES);
        let mut id = [0; 32];
        hex::decode_to_slice(digest, &mut id).map_err(|error| error.to_string())?;
        let bytes = self
            .objects
            .read_object_owned(&id, max, &mut self.io.borrow_mut())
            .map_err(|error| error.to_string())?;
        assert!(bytes.uses_budget(self.memory));
        assert_eq!(blake3::hash(&bytes).as_bytes(), &id);
        self.reads.set(self.reads.get() + 1);
        Ok(Some(ColdCatalogPage(bytes)))
    }
}

pub(super) fn check_cold_wasm_catalog(
    store: &IncrementalPageStore,
    material: Option<&business_originals::ProvisionalProjectedGenesisBusinessV1>,
    expected: &[ExpectedArtifact],
    inspection: &mut Inspection,
    i: usize,
) {
    let baseline = store.memory_budget().used();
    {
        let _scratch = store
            .memory_budget()
            .reserve(32 << 10, MemoryClass::Recovery)
            .unwrap();
        let (expected_ref, expected_wire) = expected_wasm_catalog_original(expected);
        let objects = IncrementalObjectStore::open(
            store,
            MemoryClass::Recovery,
            &mut inspection.physical[i],
            &inspection.stop,
        )
        .unwrap();
        let mut io = ObjectIo {
            physical: &mut inspection.physical[i],
            logical: &mut inspection.logical[i],
            class: MemoryClass::Recovery,
            cancelled: &inspection.stop,
        };
        let descriptor = objects.read_object_owned(&expected_ref.id, 128, &mut io);
        if let Some(material) = material {
            let (reference, wire) = material.wasm_catalog_original();
            assert_eq!(reference, expected_ref);
            assert_eq!(wire.as_slice(), expected_wire);
            let descriptor = descriptor.unwrap();
            assert!(descriptor.uses_budget(store.memory_budget()));
            assert_eq!(descriptor.len() as u64, reference.byte_len);
            assert_eq!(descriptor.as_slice(), wire.as_slice());
            assert_eq!(*blake3::hash(&descriptor).as_bytes(), reference.id);
            // An independent finite inspection account. No production Core or
            // IO lane is reset, reused or increased for these cold checks.
            let mut steps = 1_000_000u64;
            let mut bytes = 8u64 << 20;
            let mut work = |s, b| {
                steps = steps
                    .checked_sub(s)
                    .ok_or(ExecutionError::ResourceLimited("cold catalog steps"))?;
                bytes = bytes
                    .checked_sub(b)
                    .ok_or(ExecutionError::ResourceLimited("cold catalog bytes"))?;
                Ok(())
            };
            let catalog =
                ArtifactCatalogV1::from_canonical_bytes_with_work(&descriptor, &mut work).unwrap();
            assert_eq!(&catalog, material.wasm_catalog());
            assert_eq!(catalog.kind, ArtifactKindV1::Wasm);
            assert_eq!(catalog.count, expected.len() as u64);
            let nodes = ColdCatalogNodes {
                objects: &objects,
                memory: store.memory_budget(),
                io: RefCell::new(io),
                reads: Cell::new(0),
            };
            for artifact in expected {
                let before_reads = nodes.reads.get();
                let entry = catalog
                    .get_with_work(&artifact.reference.id, &nodes, &mut work)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    entry,
                    ArtifactEntryV1 {
                        kind: ArtifactKindV1::Wasm,
                        original: artifact.reference
                    }
                );
                // Every path object comes from this reopened namespace. Each
                // lookup checks 256 stored branch depths and its terminal leaf.
                assert_eq!(nodes.reads.get() - before_reads, 257);
                let original = objects
                    .read_object_owned(
                        &entry.original.id,
                        entry.original.byte_len as usize,
                        &mut nodes.io.borrow_mut(),
                    )
                    .unwrap();
                assert_eq!(original.as_slice(), artifact.bytes);
                assert_eq!(original.len() as u64, entry.original.byte_len);
                assert_eq!(*blake3::hash(&original).as_bytes(), entry.original.id);
            }
            assert_eq!(nodes.reads.get(), expected.len() * 257);
            if expected.len() == 2 {
                let (prefix_ref, prefix_wire) = expected_wasm_catalog_original(&expected[..1]);
                let first_root: [u8; 32] =
                    prefix_wire[prefix_wire.len() - 32..].try_into().unwrap();
                assert_ne!(catalog.node_root, hex::encode(first_root));
                assert_ne!(reference.id, prefix_ref.id);
                // The first callback really persisted its root page. Only the
                // complete producer publishes the final two-entry descriptor.
                let first_page = objects
                    .read_object_owned(
                        &first_root,
                        ARTIFACT_MAX_PAGE_BYTES,
                        &mut nodes.io.borrow_mut(),
                    )
                    .unwrap();
                assert_eq!(*blake3::hash(&first_page).as_bytes(), first_root);
                assert!(matches!(
                    objects.read_object_owned(&prefix_ref.id, 128, &mut nodes.io.borrow_mut()),
                    Err(PageStoreError::Missing(id)) if id == prefix_ref.id
                ));
            }
            assert!(steps < 1_000_000 && steps > 0);
            assert!(bytes < (8 << 20) && bytes > 0);
        } else {
            assert!(
                matches!(descriptor, Err(PageStoreError::Missing(id)) if id == expected_ref.id)
            );
        }
    }
    assert_eq!(inspection.physical[i].writes_remaining, 0);
    assert_eq!(inspection.physical[i].write_bytes_remaining, 0);
    assert_eq!(store.memory_budget().used(), baseline);
}
