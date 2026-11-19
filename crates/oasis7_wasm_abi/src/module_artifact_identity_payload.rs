//! The original module identity signing preimage, streamed through one writer.
use crate::ModuleArtifactIdentity;
impl ModuleArtifactIdentity {
    pub fn signing_payload_v1(
        wasm_hash: &str,
        source_hash: &str,
        build_manifest_hash: &str,
        signer_node_id: &str,
    ) -> Vec<u8> {
        let mut output = Vec::new();
        // Vec's Write implementation cannot report an IO error.
        Self::write_signing_payload_v1(
            wasm_hash,
            source_hash,
            build_manifest_hash,
            signer_node_id,
            &mut output,
        )
        .expect("writing a module identity payload to Vec");
        output
    }
    /// The original identity preimage, shared by ordinary and metered writers.
    /// A metered sink can count first, reserve its actual capacity, and debit
    /// every copy without constructing an intermediate formatted String.
    pub fn write_signing_payload_v1<W: std::io::Write + ?Sized>(
        wasm_hash: &str,
        source_hash: &str,
        build_manifest_hash: &str,
        signer_node_id: &str,
        output: &mut W,
    ) -> std::io::Result<()> {
        output.write_all(b"modsig:ed25519:v1|")?;
        for (index, field) in [wasm_hash, source_hash, build_manifest_hash, signer_node_id]
            .into_iter()
            .enumerate()
        {
            if index != 0 {
                output.write_all(b"|")?;
            }
            for chunk in field.as_bytes().chunks(8192) {
                output.write_all(chunk)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streamed_payload_preserves_original_unicode_and_chunk_boundary() {
        let hash = "a".repeat(8193);
        let expected = format!("modsig:ed25519:v1|{hash}|原件|build|node").into_bytes();
        let mut actual = Vec::new();
        ModuleArtifactIdentity::write_signing_payload_v1(
            &hash,
            "原件",
            "build",
            "node",
            &mut actual,
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            ModuleArtifactIdentity::signing_payload_v1(&hash, "原件", "build", "node"),
            expected
        );
    }
    #[test]
    fn streamed_payload_propagates_sink_failure_without_later_fields() {
        struct Failing {
            calls: usize,
        }
        impl std::io::Write for Failing {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.calls += 1;
                if self.calls == 2 {
                    Err(std::io::Error::other("stop"))
                } else {
                    Ok(bytes.len())
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut sink = Failing { calls: 0 };
        let error = ModuleArtifactIdentity::write_signing_payload_v1(
            "hash", "source", "build", "node", &mut sink,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "stop");
        assert_eq!(sink.calls, 2);
    }
}
