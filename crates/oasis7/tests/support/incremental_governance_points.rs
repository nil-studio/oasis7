use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
fn state_error(e: oasis7_distfs::incremental_page_store::PageStoreError) -> ValueTreeError {
    use oasis7_distfs::incremental_page_store::PageStoreError as E;
    match e {
        E::Memory(error) => ValueTreeError::Memory(error),
        E::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
        E::Cancelled => ValueTreeError::Cancelled,
        E::Io(error) => ValueTreeError::Io(error),
        E::Missing(_) => ValueTreeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "agent authority physical page missing",
        )),
        E::Corrupt(reason) => ValueTreeError::Invalid(reason),
        E::UnsupportedPlatform => ValueTreeError::Invalid("agent authority physical platform"),
    }
}
#[test]
fn ordinary_governance_cells_members_and_lifecycles_have_complete_raw_after() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("point-pages"),
            memory.clone(),
            compression,
        )
        .unwrap();
        let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
        let original = source();
        let before = seed(&store, &memory, &original);
        let parent = ExecutionParent {
            roots: before.clone(),
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical_scope());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let top = before
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Authorization)
            .unwrap();
        let mut ctx = WorldFieldReadContextV1 {
            root: top,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let family = qualify_governance_family_v1(
            &before,
            &mut ctx,
            &mut work().scan,
            &mut work().scan,
            &mut 8192,
            165,
        )
        .unwrap();
        let mut proofs = Vec::new();
        for key in [
            GovernanceRecordKeyV1 {
                kind: GovernanceRecordKindV1::Identity,
                id: " outer identity ",
            },
            GovernanceRecordKeyV1 {
                kind: GovernanceRecordKindV1::Proposal,
                id: " outer proposal ",
            },
            GovernanceRecordKeyV1 {
                kind: GovernanceRecordKindV1::Vote,
                id: "",
            },
        ] {
            ctx.root = before
                .state
                .iter()
                .find(|r| r.namespace == key.kind.namespace())
                .unwrap();
            proofs.push(
                open_qualified_governance_record_v1(key, &before, &family, &mut ctx)
                    .unwrap()
                    .unwrap(),
            );
        }
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 32,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut overlay_io = logical();
        let mut overlay_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut overlay_io,
            },
            key_work: &mut overlay_keys,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut writer = store.value_session(physical_scope());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 4096,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        };

        use GovernanceBallotColumnV1 as B;
        use GovernanceCellV1 as C;
        use GovernanceColumnV1 as F;
        use GovernanceSnapshotColumnV1 as S;
        use GovernanceValueRefV1 as V;
        let identity = GovernanceRecordKeyV1 {
            kind: GovernanceRecordKindV1::Identity,
            id: " outer identity ",
        };
        let proposal = GovernanceRecordKeyV1 {
            kind: GovernanceRecordKindV1::Proposal,
            id: " outer proposal ",
        };
        let vote = GovernanceRecordKeyV1 {
            kind: GovernanceRecordKindV1::Vote,
            id: "",
        };
        for (i, key, cell, expected, after) in [
            (
                0,
                identity,
                C::Column(F::IdentityStatus),
                V::U64(2),
                V::U64(0),
            ),
            (
                0,
                identity,
                C::Column(F::SlashCount),
                V::U64(u32::MAX as u64),
                V::U64(0),
            ),
            (
                1,
                proposal,
                C::Column(F::PassThresholdBps),
                V::U64(u16::MAX as u64),
                V::U64(0),
            ),
            (1, proposal, C::Column(F::FinalizedAt), V::Unit, V::U64(0)),
            (
                1,
                proposal,
                C::Column(F::FinalizedAt),
                V::U64(0),
                V::U64(u64::MAX),
            ),
            (
                1,
                proposal,
                C::Column(F::FinalizedAt),
                V::U64(u64::MAX),
                V::Unit,
            ),
            (
                1,
                proposal,
                C::Column(F::WinningOption),
                V::Text(""),
                V::Text(" raw winning "),
            ),
            (
                1,
                proposal,
                C::Column(F::WinningOption),
                V::Text(" raw winning "),
                V::Unit,
            ),
            (
                1,
                proposal,
                C::Column(F::WinningOption),
                V::Unit,
                V::Text(""),
            ),
            (
                1,
                proposal,
                C::Snapshot(" raw snapshot key ", S::ReputationScore),
                V::I64(i64::MIN),
                V::I64(i64::MAX),
            ),
            (
                1,
                proposal,
                C::Option(1),
                V::Text(""),
                V::Text(" original mid "),
            ),
            (
                2,
                vote,
                C::Ballot(" raw ballot key ", B::Weight),
                V::U64(u32::MAX as u64),
                V::U64(0),
            ),
            (2, vote, C::Tally(""), V::U64(u64::MAX), V::U64(0)),
        ] {
            let guard = overlay.transaction.guard();
            proofs[i] = change_qualified_governance_cell_v1(
                GovernanceCellChangeV1 {
                    key,
                    cell,
                    qualification: &proofs[i],
                    expected: Some(expected),
                    after: Some(after),
                    max_current_payload_bytes: 1 << 20,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
        }
        use GovernanceMemberEditV1 as E;
        use GovernanceMemberKeyV1 as K;
        use GovernanceMemberValueV1 as N;
        let mut budget = work();
        for (i, key, n, edit) in [
            (1, proposal, 3, E::Insert(N::Option("z"))),
            (1, proposal, 4, E::Remove(K::Option(3))),
            (
                1,
                proposal,
                1,
                E::Insert(N::Snapshot {
                    id: " new snapshot key ",
                    value: &original.governance_proposals[" outer proposal "].vote_weight_snapshot
                        [" raw snapshot key "],
                }),
            ),
            (1, proposal, 2, E::Remove(K::Snapshot(" raw snapshot key "))),
            (
                2,
                vote,
                1,
                E::Insert(N::Ballot {
                    id: " new ballot key ",
                    value: &original.governance_votes[""].votes_by_agent[" raw ballot key "],
                }),
            ),
            (2, vote, 2, E::Remove(K::Ballot(" raw ballot key "))),
            (
                2,
                vote,
                2,
                E::Insert(N::Tally {
                    id: " new tally key ",
                    value: u64::MAX,
                }),
            ),
            (2, vote, 3, E::Remove(K::Tally(""))),
        ] {
            let guard = overlay.transaction.guard();
            proofs[i] = change_qualified_governance_member_v1(
                GovernanceMemberChangeV1 {
                    key,
                    qualification: &proofs[i],
                    expected_count: n,
                    edit,
                },
                &mut overlay,
                &mut write,
                &mut budget,
                &guard,
                &live,
            )
            .unwrap();
        }
        let guard = overlay.transaction.guard();
        let missing = read_qualified_governance_cell_overlay_v1(
            GovernanceCellRequestV1 {
                key: proposal,
                cell: C::Snapshot(" raw snapshot key ", S::AgentId),
                qualification: &proofs[1],
            },
            &mut overlay,
            1 << 20,
            &mut std::io::sink(),
            &guard,
            &live,
        )
        .unwrap();
        assert_eq!(missing, None);
        let created_key = GovernanceRecordKeyV1 {
            kind: GovernanceRecordKindV1::Identity,
            id: " created outer identity ",
        };
        let guard = overlay.transaction.guard();
        let created = create_qualified_governance_record_v1(
            GovernanceRecordCreateV1 {
                key: created_key,
                source: GovernanceRecordRefV1::Identity(
                    &original.governance_identity_profiles[" outer identity "],
                ),
                family: &family,
            },
            &mut overlay,
            &mut write,
            &mut budget,
            &guard,
            &live,
        )
        .unwrap();
        let _created = created;
        let guard = overlay.transaction.guard();
        delete_qualified_governance_record_v1(
            GovernanceRecordDeleteV1 {
                key: identity,
                qualification: &proofs[0],
            },
            &mut overlay,
            &guard,
            &live,
        )
        .unwrap();
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.writes.len(), 4);
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let verified = verifier.verify(&sealed.record).unwrap();
        let receiver_state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let receiver_values = store.value_session(physical_scope());
        let mut receiver = WorldFieldReadContextV1 {
            root: &sealed.record.after.state[0],
            state: &receiver_state,
            values: ValueReadContext {
                source: &receiver_values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || receiver_state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let accepted = accept_governance_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut receiver,
            &mut work().scan,
            &mut 100_000,
            1 << 20,
        )
        .unwrap();
        assert_eq!(accepted.record_counts(), [1, 1, 1]);
        accepted
            .validate_binding(&sealed.record.after.digest().unwrap())
            .unwrap();

        for node in &verified.new_nodes {
            assert_eq!(node.digest, blake3::hash(&node.bytes).to_hex().to_string());
            write
                .sink
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    memory
                        .reserve(node.bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        let family_after = qualify(&store, &memory, &sealed.record.after, MemoryClass::Ordinary);
        assert_eq!(family_after.record_counts(), [1, 1, 1]);
        let final_state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let final_values = store.value_session(physical_scope());
        let mut final_ctx = WorldFieldReadContextV1 {
            root: sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .unwrap(),
            state: &final_state,
            values: ValueReadContext {
                source: &final_values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || final_state.take_read_error().map(state_error),
            cancelled: || false,
        };
        assert!(
            open_qualified_governance_record_v1(
                identity,
                &sealed.record.after,
                &family_after,
                &mut final_ctx
            )
            .unwrap()
            .is_none()
        );
        for (key, cell, expected, text) in [
            (
                created_key,
                C::Column(F::IdentityStatus),
                GovernanceValueV1::Unsigned(2),
                "",
            ),
            (
                created_key,
                C::Column(F::AgentId),
                GovernanceValueV1::Text {
                    utf8_bytes: " different inner agent ".len() as u64,
                },
                " different inner agent ",
            ),
            (
                proposal,
                C::Column(F::FinalizedAt),
                GovernanceValueV1::Unit,
                "",
            ),
            (
                proposal,
                C::Column(F::WinningOption),
                GovernanceValueV1::Text { utf8_bytes: 0 },
                "",
            ),
            (
                proposal,
                C::Option(1),
                GovernanceValueV1::Text {
                    utf8_bytes: " original mid ".len() as u64,
                },
                " original mid ",
            ),
            (
                proposal,
                C::Snapshot(" new snapshot key ", S::ReputationScore),
                GovernanceValueV1::Signed(i64::MIN),
                "",
            ),
            (
                vote,
                C::Ballot(" new ballot key ", B::Weight),
                GovernanceValueV1::Unsigned(u32::MAX as u64),
                "",
            ),
            (
                vote,
                C::Tally(" new tally key "),
                GovernanceValueV1::Unsigned(u64::MAX),
                "",
            ),
        ] {
            let proof = open_qualified_governance_record_v1(
                key,
                &sealed.record.after,
                &family_after,
                &mut final_ctx,
            )
            .unwrap()
            .unwrap();
            let _out = memory
                .reserve(text.len() + 1024, MemoryClass::Ordinary)
                .unwrap();
            let mut output = Vec::with_capacity(text.len());
            let actual = read_qualified_governance_cell_v1(
                GovernanceCellRequestV1 {
                    key,
                    cell,
                    qualification: &proof,
                },
                &mut final_ctx,
                1 << 20,
                &mut output,
            )
            .unwrap();
            assert_eq!(actual, expected);
            assert_eq!(output, text.as_bytes());
        }
    }
}
