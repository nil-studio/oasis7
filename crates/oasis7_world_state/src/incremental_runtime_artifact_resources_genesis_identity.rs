//! Fixed fields for the nested builtin module artifact identity record.
use super::*;

pub(super) fn record_module_identity<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &oasis7_wasm_abi::ModuleArtifactIdentity,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    let kind = RuntimeArtifactRecordV1::ModuleArtifactIdentity;
    metadata(&mut root, kind, context, work)?;
    for (field, value) in [
        ("source_hash", source.source_hash.as_str()),
        ("build_manifest_hash", source.build_manifest_hash.as_str()),
        ("signer_node_id", source.signer_node_id.as_str()),
        ("signature_scheme", source.signature_scheme.as_str()),
        ("artifact_signature", source.artifact_signature.as_str()),
    ] {
        text(&mut root, kind, field, value, context, work)?;
    }
    Ok(root)
}
