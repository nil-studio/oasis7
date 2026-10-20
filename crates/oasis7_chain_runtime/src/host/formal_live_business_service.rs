//! Signed lookup pages retain the exact original qualified outcome identity.
use super::*;
use oasis7::world_service::wire::FormalAuthenticatedLookupV1;
use oasis7_client_api::world_service::FormalBusinessOutcomePageV1;

impl FormalLiveService {
    pub(super) fn handle_formal_lookup(&self, request: FormalAuthenticatedLookupV1) -> HttpReply {
        let digest = authority::request_digest(LOOKUP_PATH, &request);
        let result = (|| {
            request
                .lookup
                .request
                .validate()
                .map_err(|error| error.to_string())?;
            let correlation =
                correlation::derive_correlation(self.world.clone(), &request.lookup.original)?;
            if request.schema_version != 1
                || correlation.key != request.lookup.request.key
                || request.read.request.min_commit.is_some()
            {
                return Err("formal lookup identity/request binding".into());
            }
            // The signed view authorizes the current Agent audience. Original
            // request correlation is verified separately, without reapplying
            // mutable admission/nonce fences to an already committed result.
            let (view, target_publication) = self.read_view_with_publication(&request.read)?;
            let from = request
                .read
                .request
                .formal_anchor
                .clone()
                .ok_or("formal lookup anchor missing")?;
            let continuation_target = anchor_ref(target_publication.anchor())?;
            let result = self.history.business_result_for_publication(
                target_publication,
                &correlation,
                &request.read.request.scope_id,
                &request.read.subject_public_key,
            );
            let (result, historical_cbor_hex, continuation_anchor) = classify_lookup_result(
                result,
                view.version.commit.position > from.position,
                || Ok(continuation_target),
            )?;
            let page = FormalBusinessOutcomePageV1 {
                schema_version: 1,
                requested_correlation: correlation,
                from_anchor: from,
                target_request: request.read.request.clone(),
                target_view: view,
                result,
                historical_cbor_hex,
                continuation_anchor,
            };
            page.validate().map_err(|error| error.to_string())?;
            Ok::<_, String>(page)
        })();
        match result {
            Ok(page) => signed_reply_with_digest(200, LOOKUP_PATH, digest, &page, &self.signer),
            Err(error) => signed_error_with_digest(
                503,
                LOOKUP_PATH,
                digest,
                "formal_lookup_unavailable",
                &error,
                &self.signer,
            ),
        }
    }
}

type ClassifiedLookup<T> = (Option<T>, Option<String>, Option<FormalReadAnchorRefV1>);

pub(super) fn classify_lookup_result<T>(
    result: Result<Option<(T, Option<String>)>, String>,
    can_continue: bool,
    next_anchor: impl FnOnce() -> Result<FormalReadAnchorRefV1, String>,
) -> Result<ClassifiedLookup<T>, String> {
    match result {
        Ok(Some((proof, history))) => Ok((Some(proof), history, None)),
        Ok(None) if can_continue => Ok((None, None, Some(next_anchor()?))),
        Ok(None) => Err("HistoryUnavailable: no result at the exact qualified lookup cut".into()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::classify_lookup_result;
    use oasis7_client_api::world_service::FormalReadAnchorRefV1;
    use std::cell::Cell;

    fn anchor(position: u64) -> FormalReadAnchorRefV1 {
        FormalReadAnchorRefV1 {
            position,
            record_id: "a".repeat(64),
            active_authority_id: "b".repeat(64),
        }
    }

    #[test]
    fn only_genuine_absence_uses_forward_continuation() {
        let called = Cell::new(false);
        let failed =
            classify_lookup_result::<u8>(Err("formal CAS proof read failed".into()), true, || {
                called.set(true);
                Ok(anchor(9))
            })
            .unwrap_err();
        assert_eq!(failed, "formal CAS proof read failed");
        assert!(
            !called.get(),
            "a storage/proof error must not become a page"
        );

        let next = anchor(9);
        let (proof, history, continuation) =
            classify_lookup_result::<u8>(Ok(None), true, || Ok(next.clone())).unwrap();
        assert!(proof.is_none());
        assert!(history.is_none());
        assert_eq!(continuation, Some(next));
        assert!(classify_lookup_result::<u8>(Ok(None), false, || Ok(anchor(9))).is_err());
    }
}
