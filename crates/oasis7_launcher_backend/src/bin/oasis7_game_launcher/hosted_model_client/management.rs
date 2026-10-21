//! Bounded, coherent management reads. Cursors carry no credentials or object data.
use super::*;
use sha2::{Digest, Sha256};

impl HostedModelClient {
    pub(super) fn connection_page(
        &self,
        p: &Principal,
        cursor: Option<&str>,
    ) -> Result<Value, IdentityReply> {
        const PAGE_LIMIT: usize = 64;
        let mut connections = self.manager.list(p).map_err(connection_error)?;
        connections.sort_by(|a, b| a.id.cmp(&b.id));
        let values: Vec<_> = connections
            .iter()
            .map(|c| self.execution_snapshot(c))
            .collect();
        // Bind every continuation to the authenticated account, session, and full
        // sanitized read cut. Any concurrent state change requires a fresh read;
        // callers must never splice two different cuts into a complete list.
        let cut_input = json!([p.account_id, p.session_id, p.auth_ref, values]);
        let cut = hex::encode(Sha256::digest(cut_input.to_string().as_bytes()));
        let offset = if let Some(cursor) = cursor {
            let (original_cut, offset) = cursor
                .split_once('_')
                .ok_or_else(|| error(400, "invalid_request"))?;
            let offset = offset
                .parse::<usize>()
                .map_err(|_| error(400, "invalid_request"))?;
            if original_cut != cut
                || offset == 0
                || offset % PAGE_LIMIT != 0
                || offset >= values.len()
            {
                return Err(error(409, "list_changed"));
            }
            offset
        } else {
            0
        };
        let end = offset.saturating_add(PAGE_LIMIT).min(values.len());
        let next_cursor = (end < values.len()).then(|| format!("{cut}_{end}"));
        Ok(
            json!({"ok":true,"connections":&values[offset..end],"page_limit":PAGE_LIMIT,"management_limits":{"active_credentials_per_principal":8,"retained_records_per_process":4096,"history_retention":"process_lifetime"},"list_cut":cut,"next_cursor":next_cursor}),
        )
    }
}
