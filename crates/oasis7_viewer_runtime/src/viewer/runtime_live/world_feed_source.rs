//! Verified public history source selection and source-scoped cursor fencing.
use super::*;

impl ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn public_world_feed(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> protocol::WorldFeedEnvelope {
        // This route is shared with anonymous readers. Operator metadata
        // describing a restricted audience is not per-reader authorization.
        let visibility = match self.config.major_world_event_visibility {
            crate::runtime::MajorWorldEventVisibilityPermission::Public => {
                crate::runtime::MajorWorldEventVisibilityPermission::Public
            }
            crate::runtime::MajorWorldEventVisibilityPermission::Denied => {
                crate::runtime::MajorWorldEventVisibilityPermission::Denied
            }
            _ => crate::runtime::MajorWorldEventVisibilityPermission::Unknown,
        };
        let unavailable = || {
            envelope(
                &self.config.world_id,
                self.reorg_epoch,
                encode_cursor(&self.config.world_id, self.reorg_epoch, 0),
                Vec::new(),
                protocol::WorldFeedStatus::Unavailable,
                None,
                Some(protocol::WorldFeedUnavailableReason::SourceUnavailable),
            )
        };
        let Some(config) = &self.config.world_service else {
            return build_world_feed(
                &self.config.world_id,
                self.reorg_epoch,
                self.world.journal(),
                &self.world.state().crises,
                cursor,
                limit,
                visibility,
            );
        };
        let Some(view) = &self.verified_world_view else {
            return unavailable();
        };
        let Ok(reader) = config.read_authority() else {
            return unavailable();
        };
        if config.scope_id != "public"
            || view.version().visibility_scope != "public"
            || view.version().commit.world != config.expected_world
            || self.config.world_id != config.expected_world.world_id
            || !view.belongs_to(&reader)
            || view.evidence_policy()
                != crate::world_service::verified_view::ReadEvidencePolicy::Ordinary
            || view.execution_evidence_scope().is_some()
            || view.logical_tick() != self.world.state().time
            || (view.projection().events.is_empty() && view.continuation().sequence != 0)
        {
            return unavailable();
        }
        // The disposable mapping World has no journal. Use only the installed,
        // verified public source window, never its private execution journal.
        let source = serde_json::json!({"world":config.expected_world,
            "binding":view.version().commit.binding,"scope":view.version().visibility_scope,
            "reader":reader.cache_partition_key()});
        let source_hash = blake3::hash(source.to_string().as_bytes())
            .to_hex()
            .to_string();
        let prefix = format!("wfs1.{source_hash}.");
        let inner_cursor = match cursor {
            Some(cursor) => match cursor.strip_prefix(&prefix) {
                Some(inner) => Some(inner),
                None => {
                    return envelope(
                        &self.config.world_id,
                        self.reorg_epoch,
                        format!(
                            "{prefix}{}",
                            encode_cursor(&self.config.world_id, self.reorg_epoch, 0)
                        ),
                        Vec::new(),
                        protocol::WorldFeedStatus::Gap,
                        Some(protocol::WorldFeedGapReason::CursorInvalid),
                        None,
                    );
                }
            },
            None => None,
        };
        let journal = RuntimeJournal {
            events: view.projection().events.clone(),
        };
        let mut feed = build_world_feed(
            &self.config.world_id,
            self.reorg_epoch,
            &journal,
            &view.projection().state.crises,
            inner_cursor,
            limit,
            visibility,
        );
        feed.cursor = format!("{prefix}{}", feed.cursor);
        feed
    }
}
