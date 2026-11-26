//! A read-only exact-cut bridge between qualified-H successor material and
//! header read-contract-4. This is a *candidate*, never authenticated evidence.
//!
//! Callers must independently verify the original H/signing authority, certified
//! read4 prefix, projection, current FullReadOnly grant, and the complete archive
//! before publication. This module does not install a world, sign, serve HTTP,
//! allocate a transfer budget, or change the live Node/Chain paths.
use oasis7_client_api::world_service::{
    FormalBftHeaderReadHeadV4, FormalBftHeaderReadRequestV4,
};
use oasis7_distfs::protected_bft_v3::{
    BftSuccessorWindowCutV1, BftSuccessorWindowSourceV1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftRead4SuccessorCutCandidateV1 {
    /// Original qualified-H identity; possession does not prove its authority.
    pub source_origin_digest: String,
    /// Exact locally pinned closure metadata; not a verified read receipt.
    pub cut: BftSuccessorWindowCutV1,
}

impl BftRead4SuccessorCutCandidateV1 {
    /// Reject conflicting H identities, exact target or repeated read head
    /// commitments *before* any archive fetch or replay.
    ///
    /// A valid result carries no evidence that the cut was independently
    /// authenticated, that FullReadOnly was granted, or that the projection and
    /// archive closure were verified. Those checks remain mandatory consumers.
    pub fn bind_untrusted_exact_cut(
        original: &BftSuccessorWindowSourceV1,
        request: &FormalBftHeaderReadRequestV4,
        cut: &BftSuccessorWindowCutV1,
        head: &FormalBftHeaderReadHeadV4,
    ) -> Result<Self, String> {
        let source_origin_digest = original.origin_digest()?;
        request.validate().map_err(|e| e.to_string())?;
        cut.validate().map_err(|e| e.to_string())?;
        head.validate().map_err(|e| e.to_string())?;

        // These identifiers originate from an independently qualified H, not
        // from a responder-selected head or a mutable local latest pointer.
        if request.world.world_id != original.scope.world_id
            || request.world.genesis_digest != original.scope.genesis_hash
            || request.handoff_anchor.position != original.handoff_position
            || request.handoff_anchor.record_id != original.handoff_record_id
            || request.target_config_digest != original.target_digest
            || request.activation_digest != original.activation_digest
        {
            return Err("read4 successor source differs from original qualified H".into());
        }

        // read4 is bounded to a single H-anchored 64-height prefix, and the
        // exact (height, header_id) must be pinned before issuing the request.
        if cut.height != request.exact_target.height
            || cut.header_id != request.exact_target.header_id
            || head.height != cut.height
            || head.header_id != cut.header_id
            || head.execution_commitment_hash != cut.execution_commitment_digest
            || head.state_root != cut.semantic_state_root
            || head.post_manifest_hash != cut.post_manifest_hash
        {
            return Err("read4 successor cut differs from exact selected head".into());
        }

        Ok(Self {
            source_origin_digest,
            cut: cut.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_client_api::world_service::{
        FormalBftHeaderReadTargetV4, FormalReadAnchorRefV1, WorldIdentity,
    };
    use oasis7_distfs::protected_bft_v3::BftSuccessorWindowArchiveRefV1;
    use oasis7_proto::distributed_bft::v2::BftScopeV2;

    fn digest(n: u8) -> String {
        format!("{n:02x}").repeat(32)
    }

    fn original() -> BftSuccessorWindowSourceV1 {
        BftSuccessorWindowSourceV1 {
            scope: BftScopeV2 {
                world_id: "original-world".into(),
                chain_id: "original-chain".into(),
                genesis_hash: digest(1),
            },
            binding_digest: digest(2),
            handoff_position: 100,
            handoff_record_id: digest(3),
            handoff_snapshot_root: digest(4),
            target_digest: digest(5),
            activation_digest: digest(6),
            material_identity: digest(7),
        }
    }

    fn request(src: &BftSuccessorWindowSourceV1) -> FormalBftHeaderReadRequestV4 {
        FormalBftHeaderReadRequestV4 {
            schema_version: 4,
            contract_version: 4,
            world: WorldIdentity {
                world_id: src.scope.world_id.clone(),
                genesis_digest: src.scope.genesis_hash.clone(),
            },
            scope_id: "public".into(),
            observation_nonce: digest(8),
            handoff_anchor: FormalReadAnchorRefV1 {
                position: src.handoff_position,
                record_id: src.handoff_record_id.clone(),
                active_authority_id: digest(9),
            },
            target_config_digest: src.target_digest.clone(),
            activation_digest: src.activation_digest.clone(),
            exact_target: FormalBftHeaderReadTargetV4 {
                height: 101,
                header_id: digest(10),
            },
        }
    }

    fn cut() -> BftSuccessorWindowCutV1 {
        BftSuccessorWindowCutV1 {
            height: 101,
            header_id: digest(10),
            certificate_digest: digest(11),
            execution_commitment_digest: digest(12),
            snapshot_ref: BftSuccessorWindowArchiveRefV1 {
                digest: digest(13),
                bytes: 1,
            },
            journal_ref: BftSuccessorWindowArchiveRefV1 {
                digest: digest(14),
                bytes: 1,
            },
            semantic_state_root: digest(15),
            post_manifest_hash: digest(16),
            business_result_root: digest(17),
            driver_record_digest: digest(18),
            driver_install_digest: digest(19),
        }
    }

    fn head(cut: &BftSuccessorWindowCutV1) -> FormalBftHeaderReadHeadV4 {
        FormalBftHeaderReadHeadV4 {
            height: cut.height,
            header_id: cut.header_id.clone(),
            execution_commitment_hash: cut.execution_commitment_digest.clone(),
            state_root: cut.semantic_state_root.clone(),
            post_manifest_hash: cut.post_manifest_hash.clone(),
            projection_root: digest(20),
            logical_tick: 1,
        }
    }

    fn check(
        original: &BftSuccessorWindowSourceV1,
        request: &FormalBftHeaderReadRequestV4,
        cut: &BftSuccessorWindowCutV1,
        head: &FormalBftHeaderReadHeadV4,
    ) -> bool {
        BftRead4SuccessorCutCandidateV1::bind_untrusted_exact_cut(
            original, request, cut, head,
        )
        .is_ok()
    }

    #[test]
    fn exact_qualified_h_cut_can_be_observed_without_installing_or_signing() {
        let src = original();
        let request = request(&src);
        let cut = cut();
        let head = head(&cut);
        let candidate = BftRead4SuccessorCutCandidateV1::bind_untrusted_exact_cut(
            &src, &request, &cut, &head,
        )
        .unwrap();
        assert_eq!(candidate.source_origin_digest, src.origin_digest().unwrap());
        assert_eq!(candidate.cut, cut);
    }

    #[test]
    fn foreign_original_h_and_changed_read_request_cannot_select_cut() {
        let src = original();
        let cut = cut();
        let head = head(&cut);
        let mut foreign = src.clone();
        foreign.scope.world_id = "foreign-world".into();
        assert!(!check(&foreign, &request(&src), &cut, &head));
        foreign = src.clone();
        foreign.scope.genesis_hash = digest(21);
        assert!(!check(&foreign, &request(&src), &cut, &head));
        foreign = src.clone();
        foreign.handoff_record_id = digest(22);
        assert!(!check(&foreign, &request(&src), &cut, &head));

        let mut changed = request(&src);
        changed.handoff_anchor.position = 99;
        assert!(!check(&src, &changed, &cut, &head));
        changed = request(&src);
        changed.target_config_digest = digest(23);
        assert!(!check(&src, &changed, &cut, &head));
        changed = request(&src);
        changed.activation_digest = digest(24);
        assert!(!check(&src, &changed, &cut, &head));
        changed = request(&src);
        changed.exact_target.height = 165;
        assert!(!check(&src, &changed, &cut, &head));
    }

    #[test]
    fn response_head_must_match_the_exact_original_cut_commitments() {
        let src = original();
        let req = request(&src);
        let cut = cut();
        let head = head(&cut);
        let mut altered_cut = cut.clone();
        altered_cut.header_id = digest(25);
        assert!(!check(&src, &req, &altered_cut, &head));
        altered_cut = cut.clone();
        altered_cut.snapshot_ref.bytes = 0;
        assert!(!check(&src, &req, &altered_cut, &head));
        let mut altered_head = head.clone();
        altered_head.height = 102;
        assert!(!check(&src, &req, &cut, &altered_head));
        altered_head = head.clone();
        altered_head.execution_commitment_hash = digest(26);
        assert!(!check(&src, &req, &cut, &altered_head));
        altered_head = head.clone();
        altered_head.state_root = digest(27);
        assert!(!check(&src, &req, &cut, &altered_head));
        altered_head = head.clone();
        altered_head.post_manifest_hash = digest(28);
        assert!(!check(&src, &req, &cut, &altered_head));
    }
}
