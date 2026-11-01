use super::*;
fn rows(value: GovernanceRecordRefV1<'_>) -> Vec<(String, String)> {
    let mut result = Vec::new();
    visit_governance_record_v1(value, |cell, payload| -> Result<(), ()> {
        let value = match payload {
            MemberPayloadRefV1::Unit => "unit".into(),
            MemberPayloadRefV1::Text(v) => format!("text:{v}"),
            MemberPayloadRefV1::U64(v) => format!("u:{v}"),
            MemberPayloadRefV1::I64(v) => format!("i:{v}"),
            _ => panic!("original governance primitive"),
        };
        result.push((format!("{cell:?}"), value));
        Ok(())
    })
    .unwrap();
    result
}
#[test]
fn original_identity_primitive_domains_and_borrowing_are_exact() {
    let source = GovernanceIdentityProfileState {
        agent_id: " raw\0界 ".into(),
        stake_locked: u64::MAX,
        warmup_until_tick: 0,
        status: GovernanceIdentityStatus::Expelled,
        slash_count: u32::MAX,
        updated_at: u64::MAX,
    };
    assert_eq!(
        rows(GovernanceRecordRefV1::Identity(&source)),
        vec![
            ("Record".into(), "unit".into()),
            ("Column(AgentId)".into(), "text: raw\0界 ".into()),
            ("Column(StakeLocked)".into(), format!("u:{}", u64::MAX)),
            ("Column(WarmupUntilTick)".into(), "u:0".into()),
            ("Column(IdentityStatus)".into(), "u:2".into()),
            ("Column(SlashCount)".into(), "u:4294967295".into()),
            ("Column(UpdatedAt)".into(), format!("u:{}", u64::MAX))
        ]
    );
    visit_governance_record_v1(
        GovernanceRecordRefV1::Identity(&source),
        |cell, payload| -> Result<(), ()> {
            if cell == GovernanceCellV1::Column(GovernanceColumnV1::AgentId) {
                let MemberPayloadRefV1::Text(text) = payload else {
                    panic!("borrowed")
                };
                assert_eq!(text.as_ptr(), source.agent_id.as_ptr());
            }
            Ok(())
        },
    )
    .unwrap();
}
#[test]
fn original_proposal_and_vote_collections_keep_keys_order_duplicates_and_options() {
    let proposal:GovernanceProposalState=serde_json::from_value(serde_json::json!({"proposal_key":"embedded proposal","proposer_agent_id":"raw agent","title":"","description":" raw description ","options":["z","","z"],"voting_window_ticks":u64::MAX,"quorum_weight":0,"pass_threshold_bps":u16::MAX,"opened_at":0,"closes_at":u64::MAX,"status":"rejected","finalized_at":0,"winning_option":"","winning_weight":u64::MAX,"total_weight_at_finalize":0,"snapshot_at_tick":0,"vote_weight_snapshot":{" raw outer voter ":{"agent_id":"different embedded voter","reputation_score":i64::MIN,"stake_locked":u64::MAX,"status":"frozen","vote_weight_cap":u32::MAX}}})).unwrap();
    let projection = rows(GovernanceRecordRefV1::Proposal(&proposal));
    assert_eq!(projection.len(), 26);
    for (key, value) in [
        ("Count(Options)", "u:3"),
        ("Option(0)", "text:z"),
        ("Option(1)", "text:"),
        ("Option(2)", "text:z"),
        ("Column(PassThresholdBps)", "u:65535"),
        ("Column(ProposalStatus)", "u:2"),
        ("Column(FinalizedAt)", "u:0"),
        ("Column(WinningOption)", "text:"),
        ("Count(Snapshot)", "u:1"),
        (
            "Snapshot(\" raw outer voter \", AgentId)",
            "text:different embedded voter",
        ),
        (
            "Snapshot(\" raw outer voter \", ReputationScore)",
            "i:-9223372036854775808",
        ),
        (
            "Snapshot(\" raw outer voter \", VoteWeightCap)",
            "u:4294967295",
        ),
    ] {
        assert!(
            projection.contains(&(key.into(), value.into())),
            "{key}: {projection:?}"
        );
    }
    let mut absent = proposal.clone();
    absent.options.clear();
    absent.vote_weight_snapshot.clear();
    absent.finalized_at = None;
    absent.winning_option = None;
    let projection = rows(GovernanceRecordRefV1::Proposal(&absent));
    assert_eq!(projection.len(), 18);
    for key in ["Column(FinalizedAt)", "Column(WinningOption)"] {
        assert!(projection.contains(&(key.into(), "unit".into())));
    }
    for key in ["Count(Options)", "Count(Snapshot)"] {
        assert!(projection.contains(&(key.into(), "u:0".into())));
    }
    let vote:GovernanceVoteState=serde_json::from_value(serde_json::json!({"proposal_key":"other embedded","votes_by_agent":{"":{"option":" raw option ","weight":u32::MAX,"voted_at":u64::MAX}},"tallies":{"":u64::MAX," raw option ":0},"total_weight":0,"last_updated_at":u64::MAX})).unwrap();
    let projection = rows(GovernanceRecordRefV1::Vote(&vote));
    assert_eq!(projection.len(), 11);
    for (key, value) in [
        ("Count(Ballots)", "u:1"),
        ("Ballot(\"\", Weight)", "u:4294967295"),
        ("Ballot(\"\", Option)", "text: raw option "),
        ("Count(Tallies)", "u:2"),
        ("Tally(\" raw option \")", "u:0"),
    ] {
        assert!(
            projection.contains(&(key.into(), value.into())),
            "{key}: {projection:?}"
        );
    }
}

#[test]
fn closed_relative_tuple_parser_preserves_original_member_keys_and_rejects_other_shapes() {
    fn text(v: &str) -> Vec<u8> {
        let mut b = vec![1];
        b.extend_from_slice(&(v.len() as u64).to_be_bytes());
        b.extend_from_slice(v.as_bytes());
        b
    }
    fn pair(a: &[u8], b: &[u8]) -> Vec<u8> {
        let mut out = vec![5];
        out.extend_from_slice(a);
        out.extend_from_slice(b);
        out
    }
    fn semantic(b: Vec<u8>) -> Vec<u8> {
        let mut out = b"oasis7.iws.semantic-key.v1\0".to_vec();
        out.extend(b);
        out
    }
    let record = pair(&[0], &text("record"));
    let snapshot = pair(&record, &text("vote_weight_snapshot"));
    let key = semantic(pair(
        &pair(&snapshot, &text(" raw\0界 ")),
        &text("agent_id"),
    ));
    assert_eq!(
        key_read::parse(&key, GovernanceRecordKindV1::Proposal).unwrap(),
        GovernanceCellV1::Snapshot(" raw\0界 ", GovernanceSnapshotColumnV1::AgentId)
    );
    assert!(key_read::parse(&key, GovernanceRecordKindV1::Vote).is_err());
    let options = pair(&record, &text("options"));
    let mut index = vec![2];
    index.extend_from_slice(&u64::MAX.to_be_bytes());
    assert_eq!(
        key_read::parse(
            &semantic(pair(&options, &index)),
            GovernanceRecordKindV1::Proposal
        )
        .unwrap(),
        GovernanceCellV1::Option(u64::MAX)
    );
    assert!(
        key_read::parse(
            &semantic(pair(&options, &text("0"))),
            GovernanceRecordKindV1::Proposal
        )
        .is_err()
    );
    assert!(
        key_read::parse(
            &semantic(pair(&pair(&snapshot, &text("k")), &text("unknown"))),
            GovernanceRecordKindV1::Proposal
        )
        .is_err()
    );
    let mut trailing = key;
    trailing.push(0);
    assert!(key_read::parse(&trailing, GovernanceRecordKindV1::Proposal).is_err());
}
