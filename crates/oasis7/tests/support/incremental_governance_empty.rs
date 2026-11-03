use super::*;
#[test]
fn empty_collections_and_opposite_options_cold_recover() {
    let directory = Directory::new();
    let path = directory.0.join("empty-raw");
    let roots = {
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&path, memory.clone(), PageCompression::Raw).unwrap();
        let mut world = source();
        let proposal = world
            .governance_proposals
            .get_mut(" outer proposal ")
            .unwrap();
        proposal.finalized_at = Some(0);
        proposal.winning_option = None;
        proposal.options.clear();
        proposal.vote_weight_snapshot.clear();
        let vote = world.governance_votes.get_mut("").unwrap();
        vote.votes_by_agent.clear();
        vote.tallies.clear();
        seed(&store, &memory, &world)
    };
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let store = IncrementalPageStore::open(
        &path,
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let family = qualify(&store, &memory, &roots, MemoryClass::Recovery);
    assert_eq!(family.record_counts(), [1, 1, 1]);
    let state = store
        .state_session(physical_scope(), MemoryClass::Recovery)
        .unwrap();
    let values = store.value_session(physical_scope());
    let mut ctx = WorldFieldReadContextV1 {
        root: roots
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Authorization)
            .unwrap(),
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut logical(),
        },
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        },
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    use GovernanceCellV1 as C;
    use GovernanceCollectionV1 as G;
    use GovernanceColumnV1 as F;
    use GovernanceRecordKindV1 as K;
    for (kind, id, cell, expected) in [
        (
            K::Proposal,
            " outer proposal ",
            C::Count(G::Options),
            GovernanceValueV1::Unsigned(0),
        ),
        (
            K::Proposal,
            " outer proposal ",
            C::Count(G::Snapshot),
            GovernanceValueV1::Unsigned(0),
        ),
        (
            K::Vote,
            "",
            C::Count(G::Ballots),
            GovernanceValueV1::Unsigned(0),
        ),
        (
            K::Vote,
            "",
            C::Count(G::Tallies),
            GovernanceValueV1::Unsigned(0),
        ),
        (
            K::Proposal,
            " outer proposal ",
            C::Column(F::FinalizedAt),
            GovernanceValueV1::Unsigned(0),
        ),
        (
            K::Proposal,
            " outer proposal ",
            C::Column(F::WinningOption),
            GovernanceValueV1::Unit,
        ),
    ] {
        let key = GovernanceRecordKeyV1 { kind, id };
        let proof = open_qualified_governance_record_v1(key, &roots, &family, &mut ctx)
            .unwrap()
            .unwrap();
        let got = read_qualified_governance_cell_v1(
            GovernanceCellRequestV1 {
                key,
                cell,
                qualification: &proof,
            },
            &mut ctx,
            165,
            &mut std::io::sink(),
        )
        .unwrap();
        assert_eq!(got, expected);
    }
}
