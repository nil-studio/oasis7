use super::*;
#[test]
fn native_component_streams_bounded_original_chunks_and_propagates_failure() {
    let bytes = vec![73; MAX_NATIVE_CHUNK_BYTES * 2 + 17];
    let mut stored = std::collections::BTreeMap::new();
    let result = component(
        NativeEnvironmentRole::Executable,
        "test-original",
        &mut &bytes[..],
        &mut |digest, value| {
            stored.insert(digest.to_string(), value.to_vec());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(result.byte_len, bytes.len() as u64);
    assert_eq!(
        result.content_digest,
        blake3::hash(&bytes).to_hex().to_string()
    );
    assert_eq!(
        result
            .chunks
            .iter()
            .map(|chunk| chunk.byte_len)
            .collect::<Vec<_>>(),
        vec![
            MAX_NATIVE_CHUNK_BYTES as u32,
            MAX_NATIVE_CHUNK_BYTES as u32,
            17
        ]
    );
    for chunk in result.chunks {
        assert_eq!(stored[&chunk.digest].len(), chunk.byte_len as usize);
    }
    assert!(
        component(
            NativeEnvironmentRole::Executable,
            "failure",
            &mut &bytes[..],
            &mut |_, _| Err("sink rejected".into())
        )
        .unwrap_err()
        .contains("sink rejected")
    );
}
#[test]
#[cfg(not(target_os = "linux"))]
fn native_environment_never_claims_incomplete_platform_closure() {
    assert!(capture(|_, _| Ok(())).unwrap_err().contains("Unavailable"));
}
#[test]
#[cfg(target_os = "linux")]
fn native_environment_original_loaded_bytes_verify() {
    let descriptor = capture_and_verify_original();
    assert!(
        descriptor
            .components
            .iter()
            .any(|entry| entry.name == "build/Cargo.lock")
    );
    assert!(
        descriptor
            .components
            .iter()
            .any(|entry| entry.role == NativeEnvironmentRole::LoadedLibrary)
    );
}

#[test]
#[cfg(all(target_os = "linux", feature = "wasmtime"))]
fn native_environment_accepts_actual_deterministic_wasm_instantiation() {
    use oasis7_wasm_abi::{ModuleCallRequest, ModuleLimits, ModuleSandbox};
    use sha2::{Digest, Sha256};
    // (module (memory (export "memory") 2) (data (i32.const 0) "x"))
    // A data segment exercises Wasmtime's heap-image construction. Missing
    // alloc is detected AFTER actual instantiation, not during compilation.
    let wasm = b"\0asm\x01\0\0\0\x05\x03\x01\0\x02\x07\x0a\x01\x06memory\x02\0\x0b\x07\x01\0\x41\0\x0b\x01x";
    let mut executor = crate::controlled_execution_policy::GuardedExecutionPolicy::current()
        .executor()
        .unwrap();
    let failure = executor
        .call(&ModuleCallRequest {
            module_id: "original-environment-proof".into(),
            wasm_hash: hex::encode(Sha256::digest(wasm)),
            trace_id: "original-environment-proof".into(),
            entrypoint: "call".into(),
            input: vec![],
            limits: ModuleLimits {
                max_mem_bytes: 2 * 64 * 1024,
                ..Default::default()
            },
            wasm_bytes: std::sync::Arc::from(wasm.as_slice()),
        })
        .unwrap_err();
    assert!(
        failure.detail.contains("missing alloc export"),
        "{failure:?}"
    );
    capture_and_verify_original();
    // Keep the compiled module cache alive through capture, so a retained CoW
    // backing file could not disappear merely because the request returned.
    drop(executor);
}

#[cfg(target_os = "linux")]
fn capture_and_verify_original() -> NativeExecutionEnvironmentV1 {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).unwrap();
    let root = std::env::temp_dir().join(format!("oasis-native-capture-{}", hex::encode(random)));
    std::fs::create_dir(&root).unwrap();
    // Debug executable bytes can be large. The test preserves the production
    // streaming memory bound instead of retaining the whole binary in a map.
    let result = (|| {
        let descriptor = capture(|digest, bytes| {
            std::fs::write(root.join(digest), bytes).map_err(|error| error.to_string())
        })?;
        descriptor
            .verify(|digest| std::fs::read(root.join(digest)).map_err(|error| error.to_string()))?;
        Ok::<_, String>(descriptor)
    })();
    std::fs::remove_dir_all(root).unwrap();
    result.unwrap()
}
