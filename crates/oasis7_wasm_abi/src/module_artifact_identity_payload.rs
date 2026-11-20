//! The original module identity signing preimage, streamed through one writer.
use crate::ModuleArtifactIdentity;
impl ModuleArtifactIdentity {
    /// Encode the v1 signing preimage using the same encoder as streaming consumers.
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
    /// Field bytes are written in chunks of at most 8192 bytes, including chunks
    /// that split a UTF-8 character. On failure, the sink may contain a prefix;
    /// the first non-interrupted write error is returned without later writes.
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
#[path = "module_artifact_identity_payload_tests.rs"]
mod tests;
