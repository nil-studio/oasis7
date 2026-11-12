//! Read captured native originals, never mint an accepted source from metadata.
use oasis7_formal_protocol::native_environment::{
    MAX_NATIVE_CHUNK_BYTES, MAX_NATIVE_DESCRIPTOR_BYTES, NativeExecutionEnvironmentV1,
};
use std::{collections::BTreeMap, io::Read, path::Path};

fn read(path: &Path, cap: usize) -> Vec<u8> {
    let file = std::fs::File::open(path).expect("native original missing");
    let length = usize::try_from(file.metadata().unwrap().len()).unwrap();
    assert!(length <= cap, "native original exceeds fixture input bound");
    let mut bytes = Vec::with_capacity(length);
    file.take((cap + 1) as u64).read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes.len(), length, "native original changed while reading");
    bytes
}
pub(super) fn load(root: &Path) -> (NativeExecutionEnvironmentV1, Vec<Vec<u8>>) {
    let bytes = read(&root.join("descriptor.cbor"), MAX_NATIVE_DESCRIPTOR_BYTES);
    let native = NativeExecutionEnvironmentV1::decode(&bytes).expect("original native descriptor");
    let mut objects = BTreeMap::new();
    let mut total = 0usize;
    for component in &native.components {
        for chunk in &component.chunks {
            if objects.contains_key(&chunk.digest) {
                continue;
            }
            total = total.checked_add(chunk.byte_len as usize).unwrap();
            // This is a fresh test input limit, not a new production grammar.
            assert!(total <= 256 << 20, "native fixture archive resource bound");
            let bytes = read(&root.join(&chunk.digest), MAX_NATIVE_CHUNK_BYTES);
            assert_eq!(bytes.len(), chunk.byte_len as usize);
            assert_eq!(blake3::hash(&bytes).to_hex().as_str(), chunk.digest);
            objects.insert(chunk.digest.clone(), bytes);
        }
    }
    // The original verifier checks the full ordered component hash as well as
    // each chunk. Its temporary read copies are not acceptance capabilities.
    native
        .verify(|id| {
            objects
                .get(id)
                .cloned()
                .ok_or_else(|| "native chunk missing".into())
        })
        .unwrap();
    (native, objects.into_values().collect())
}
