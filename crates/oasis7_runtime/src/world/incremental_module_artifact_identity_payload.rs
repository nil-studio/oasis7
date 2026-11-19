//! Paid original module identity preimage. This is data, not admission evidence.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::io::{self, Write};

struct PayloadWriter<'a, 'b, 'ctx> {
    common: &'a mut FormalBoundedContextV1<'ctx>,
    output: Option<&'b mut Vec<u8>>,
    count: usize,
    limit: usize,
    first: Option<BodyMutationError>,
}
impl Write for PayloadWriter<'_, '_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let attempt = (|| {
            let next = self
                .count
                .checked_add(bytes.len())
                .filter(|n| *n <= self.limit)
                .ok_or(BodyMutationError::Invalid("module identity preimage limit"))?;
            self.common
                .charge(
                    1,
                    u64::try_from(bytes.len()).map_err(|_| {
                        BodyMutationError::Invalid("module identity preimage width")
                    })?,
                )
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            if let Some(output) = self.output.as_deref_mut() {
                // The measured capacity was paid before constructing this Vec.
                if next > output.capacity() {
                    return Err(BodyMutationError::Invalid(
                        "module identity preimage capacity",
                    ));
                }
                output.extend_from_slice(bytes);
            }
            self.count = next;
            Ok(())
        })();
        match attempt {
            Ok(()) => Ok(bytes.len()),
            Err(error) => {
                if self.first.is_none() {
                    self.first = Some(error);
                }
                Err(io::Error::other("module identity preimage work"))
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode(
    identity: &crate::ModuleArtifactIdentity,
    wasm_hash: &str,
    writer: &mut PayloadWriter<'_, '_, '_>,
) -> Result<(), BodyMutationError> {
    let result = crate::ModuleArtifactIdentity::write_signing_payload_v1(
        wasm_hash,
        &identity.source_hash,
        &identity.build_manifest_hash,
        &identity.signer_node_id,
        writer,
    );
    if let Some(first) = writer.first.take() {
        return Err(first);
    }
    result.map_err(|e| BodyMutationError::Original(Box::new(e)))
}

/// Invoked at the original signature-verification point, after the original
/// identity/key checks. Both passes use the same writer and caller work Core.
pub(in crate::world) fn prepare_identity_signature_payload_v1(
    wasm_hash: &str,
    identity: &crate::ModuleArtifactIdentity,
    max_payload_bytes: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
    let mut measure = PayloadWriter {
        common,
        output: None,
        count: 0,
        limit: max_payload_bytes,
        first: None,
    };
    encode(identity, wasm_hash, &mut measure)?;
    let count = measure.count;
    let layout =
        count
            .checked_add(std::mem::size_of::<Vec<u8>>())
            .ok_or(BodyMutationError::Invalid(
                "module identity preimage layout",
            ))?;
    let fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    common
        .consume_steps(1)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let mut output = Vec::with_capacity(count);
    let mut writer = PayloadWriter {
        common,
        output: Some(&mut output),
        count: 0,
        limit: count,
        first: None,
    };
    encode(identity, wasm_hash, &mut writer)?;
    if writer.count != count {
        return Err(BodyMutationError::Invalid(
            "module identity preimage changed",
        ));
    }
    Ok(fee.retain(output))
}
