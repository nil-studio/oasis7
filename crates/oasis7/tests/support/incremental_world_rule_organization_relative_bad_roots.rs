use super::*;
pub(super) fn exercise() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("bad-roots"),
            memory.clone(),
            compression,
        )
        .unwrap();
        let outer = build(&store, &memory, &produce_all(&store, &memory, &source()));
        let mut cases: Vec<_> = [
            (Object::Grant(CASE, GRANT), Part::Record),
            (Object::Grant(CASE, GRANT), Part::Field("delegate_id")),
            (Object::Grant(CASE, GRANT), Part::Collection("scopes")),
            (Object::Recovery(CASE, REQUEST), Part::Record),
            (Object::Recovery(CASE, REQUEST), Part::Flag("approved")),
            (Object::Estate(ESTATE), Part::Field("material")),
        ]
        .into_iter()
        .map(|(object, part)| (object, part, None))
        .collect();
        cases.extend([
            (
                Object::Case(CASE),
                Part::Field("policy_version"),
                Some((2, (u64::from(u32::MAX) + 1).to_be_bytes().to_vec())),
            ),
            (
                Object::Recovery(CASE, REQUEST),
                Part::Option("approved"),
                Some((4, vec![2])),
            ),
            (
                Object::Case(CASE),
                Part::Flag("notified_at"),
                Some((4, vec![1])),
            ),
            (
                Object::Case(CASE),
                Part::Flag("protected_at"),
                Some((4, vec![0])),
            ),
            (
                Object::Grant(CASE, GRANT),
                Part::Field("delegate_id"),
                Some((5, vec![0xff])),
            ),
            (
                Object::Case(CASE),
                Part::Field("phase"),
                Some((5, b"unknown".to_vec())),
            ),
            (
                Object::Case(CASE),
                Part::Field("disposition"),
                Some((5, b"unknown".to_vec())),
            ),
            (
                Object::Case(CASE),
                Part::Length,
                Some((2, 2u64.to_be_bytes().to_vec())),
            ),
            (
                Object::Case(CASE),
                Part::Field("unknown"),
                Some((1, Vec::new())),
            ),
            (
                Object::Grant(CASE, GRANT),
                Part::Record,
                Some((5, Vec::new())),
            ),
        ]);
        for (object, field) in [
            (Object::Case(CASE), "id"),
            (Object::Estate(ESTATE), "material"),
            (Object::Grant(CASE, GRANT), "delegate_id"),
            (Object::Recovery(CASE, REQUEST), "requester_id"),
        ] {
            cases.push((object, Part::Record, None));
            cases.push((object, Part::Field(field), None));
        }
        for (object, field) in [
            (Object::Case(CASE), "protected_at"),
            (Object::Estate(ESTATE), "effect_receipt"),
            (Object::Grant(CASE, GRANT), "replaces"),
            (Object::Recovery(CASE, REQUEST), "approved"),
            (Object::Recovery(CASE, REQUEST), "resolved_by"),
        ] {
            cases.push((object, Part::Option(field), None));
            cases.push((object, Part::Flag(field), Some((4, vec![0]))));
        }
        cases.extend([
            (
                Object::Case(CASE),
                Part::Option("notified_at"),
                Some((2, 0u64.to_be_bytes().to_vec())),
            ),
            (
                Object::Estate(ESTATE),
                Part::Field("held_in_trust"),
                Some((4, vec![2])),
            ),
            (
                Object::Recovery(CASE, REQUEST),
                Part::Field("appeal"),
                Some((4, vec![2])),
            ),
            (
                Object::Grant(CASE, GRANT),
                Part::Field("id"),
                Some((5, vec![0xff])),
            ),
            (
                Object::Estate(ESTATE),
                Part::Field("amount"),
                Some((2, 0u64.to_be_bytes().to_vec())),
            ),
            (
                Object::Case(CASE),
                Part::Member("members", ""),
                Some((4, vec![1])),
            ),
            (
                Object::Grant(CASE, GRANT),
                Part::Scope("", "preserve"),
                Some((4, vec![1])),
            ),
            (
                Object::Case(CASE),
                Part::Length,
                Some((2, 4u64.to_be_bytes().to_vec())),
            ),
        ]);
        cases.extend([
            (Object::Case(CASE), Part::Ordinal(0), Some((5, vec![0xff]))),
            (
                Object::Case(CASE),
                Part::Length,
                Some((3, 3i64.to_be_bytes().to_vec())),
            ),
        ]);
        for (object, part) in [
            (Object::Case(CASE), Part::Collection("members")),
            (Object::Case(CASE), Part::Collection("grants")),
            (Object::Grant(CASE, GRANT), Part::Collection("scopes")),
            (Object::Case(CASE), Part::Collection("requests")),
            (Object::Case(CASE), Part::Collection("stage_receipts")),
            (Object::Case(CASE), Part::Length),
            (Object::Case(CASE), Part::Ordinal(1)),
        ] {
            cases.push((object, part, None));
        }
        let mut wrong_semantic = frame(
            Object::Grant(CASE, GRANT),
            Part::Scope("", "preserve"),
            1,
            &[],
        );
        let length = wrong_semantic.len();
        assert_eq!(&wrong_semantic[length - 8..], b"preserve");
        wrong_semantic[length - 8..].copy_from_slice(b"unknown?");
        // 255 is an independent fixture sentinel for an already encoded frame,
        // never a production payload codec.
        cases.push((
            Object::Grant(CASE, GRANT),
            Part::Scope("", "preserve"),
            Some((255, wrong_semantic)),
        ));
        cases.sort();
        cases.dedup();
        for (object, part, replacement) in cases {
            let estate = matches!(object, Object::Estate(_));
            let owner = if estate {
                OrganizationRootKeyV1::Estate(ESTATE)
            } else {
                OrganizationRootKeyV1::Case(CASE)
            };
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical());
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let inner = inspect_organization_root_v1(
                owner,
                &mut WorldFieldReadContextV1 {
                    root: &outer,
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut io,
                    },
                    key_work: &mut keys,
                    semantic_bytes: 1024,
                    record_bytes: 8192,
                    take_state_error: || state.take_read_error().map(state_error),
                    cancelled: || false,
                },
            )
            .unwrap()
            .unwrap();
            let target = key(object, part);
            let before = inner.get(target.as_bytes(), &state).unwrap();
            assert!(before.is_some() || replacement.is_some());
            let after = replacement.map(|(codec, payload)| {
                let bytes = if codec == 255 {
                    payload
                } else {
                    frame(object, part, codec, &payload)
                };
                write_value(
                    &mut bytes.as_slice(),
                    bytes.len() as u64,
                    &mut store.value_session(physical()),
                    &memory,
                    &mut logical(),
                    || false,
                )
                .unwrap()
                .digest()
                .unwrap()
            });
            let _workspace = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
            let delta = _workspace.retain(
                StateDeltaV1::build(
                    (*inner).clone(),
                    vec![StateWriteV1 {
                        key: target.as_bytes().to_vec(),
                        before,
                        after,
                    }],
                    &state,
                    4 << 20,
                )
                .unwrap(),
            );
            store
                .value_session(physical())
                .put_state_pages(&delta, &memory)
                .unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut read = WorldFieldReadContextV1 {
                root: &delta.after,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io,
                },
                key_work: &mut keys,
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let digest = WorldFieldKeyV1::from_bytes(literal_outer_key(object).as_bytes())
                .unwrap()
                .semantic_key
                .unwrap();
            let result =
                qualify_organization_body_v1(owner, digest, &mut read, &mut scan(), &mut 1024, 165);
            assert!(
                matches!(result, Err(ValueTreeError::Invalid(_))),
                "malformed marker/column/Option/enum/UTF8/ordinal must be a schema rejection, not budget/IO failure: {result:?}"
            );
        }
    }
}
