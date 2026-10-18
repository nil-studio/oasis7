use std::path::Path;

pub(crate) fn audited_manifest_digest(path: Option<&Path>) -> Result<Option<String>, String> {
    path.map(|path| {
        std::fs::read(path)
            .map(|bytes| {
                oasis7_client_api::rollback_audit_evidence::strict_audit_manifest_digest(&bytes)
            })
            .map_err(|err| format!("read public manifest {} failed: {err}", path.display()))
    })
    .transpose()
}
