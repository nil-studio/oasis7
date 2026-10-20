//! Bounded Changes from a current-authorized qualified projection.
use super::*;
use oasis7::world_service::{
    formal_result::build_formal_changes_page, wire::FormalAuthenticatedChangesV1,
};

impl FormalLiveService {
    pub(super) fn handle_formal_changes(&self, request: FormalAuthenticatedChangesV1) -> HttpReply {
        let digest = authority::request_digest(CHANGES_PATH, &request);
        let result = (|| {
            request.cursor.validate().map_err(|e| e.to_string())?;
            if request.schema_version != 1
                || request.max_items == 0
                || request.max_items > 1024
                || request.max_bytes == 0
                || request.max_bytes > 4 * 1024 * 1024
                || request.read.request.min_commit.is_some()
                || request.read.request.fixed_commit.is_some()
            {
                return Err("formal Changes schema/bounds/request binding".into());
            }
            let view = self.read_view(&request.read)?;
            let from = request
                .read
                .request
                .formal_anchor
                .clone()
                .ok_or("formal Changes anchor missing")?;
            build_formal_changes_page(
                from,
                request.cursor,
                request.read.request,
                view,
                request.max_items,
                request.max_bytes,
            )
        })();
        match result {
            Ok(page) => signed_reply_with_digest(200, CHANGES_PATH, digest, &page, &self.signer),
            Err(error) => signed_error_with_digest(
                503,
                CHANGES_PATH,
                digest,
                "formal_changes_unavailable",
                &error,
                &self.signer,
            ),
        }
    }
}
