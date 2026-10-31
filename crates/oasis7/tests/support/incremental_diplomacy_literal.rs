//! Independent source literals: no production profile/visitor supplies expectations.
use super::*;
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader, incremental_value::ValueNodeReader,
};
enum Expected {
    Unit,
    Bool(bool),
    Unsigned(u64),
    Signed(i64),
    Text(&'static str),
    Absent,
}
use Expected::*;
fn cell<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    token: &[u8],
    expected: Expected,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) {
    let (key, semantic) = fixture::key(
        field.name(),
        FieldShape::Map,
        WorldFieldRecordKindV1::Entry,
        Some(token),
    );
    let _scratch = ctx
        .values
        .memory
        .reserve(512 << 10, ctx.values.class)
        .unwrap();
    let root = ctx.root.get(key.as_bytes(), ctx.state).unwrap();
    if matches!(expected, Absent) {
        assert_eq!(root, None);
        return;
    }
    let value = ctx.values.load_root(&root.unwrap()).unwrap();
    let mut bytes = Vec::with_capacity(8192);
    ctx.values
        .read_range(
            &value,
            0,
            value.0.logical_len,
            &mut bytes,
            &mut ctx.cancelled,
        )
        .unwrap();
    let offset = FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len() + semantic.len();
    assert_eq!(
        &bytes[FIELD_MEMBER_HEADER_BYTES..FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()],
        key.as_bytes()
    );
    assert_eq!(
        &bytes[FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()..offset],
        semantic
    );
    assert_eq!(&bytes[..FIELD_MEMBER_DOMAIN.len()], FIELD_MEMBER_DOMAIN);
    let codec = match expected {
        Unit => 1,
        Unsigned(_) => 2,
        Signed(_) => 3,
        Bool(_) => 4,
        Text(_) => 5,
        Absent => unreachable!(),
    };
    assert_eq!(bytes[FIELD_MEMBER_DOMAIN.len() + 2], codec);
    let mut integer = [0; 8];
    let payload = match expected {
        Unit => &integer[..0],
        Bool(v) => {
            integer[0] = u8::from(v);
            &integer[..1]
        }
        Unsigned(v) => {
            integer = v.to_be_bytes();
            &integer[..]
        }
        Signed(v) => {
            integer = v.to_be_bytes();
            &integer[..]
        }
        Text(v) => v.as_bytes(),
        Absent => unreachable!(),
    };
    assert_eq!(
        &bytes[offset..],
        payload,
        "{} semantic={token:?}",
        field.name()
    );
}
pub(super) fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) {
    cell(field, &[0], Unit, ctx);
    let columns: &[(&str, Expected)] = match field {
        DiplomacyFieldV1::Alliances => &[
            ("alliance_id", Text("internal-alliance")),
            ("charter", Text("")),
            ("formed_by_agent_id", Text("creator")),
            ("formed_at", Unsigned(u64::MAX)),
        ],
        DiplomacyFieldV1::Wars => &[
            ("war_id", Text("internal-war")),
            ("initiator_agent_id", Text("i")),
            ("aggressor_alliance_id", Text("a")),
            ("defender_alliance_id", Text("d")),
            ("objective", Text("")),
            ("intensity", Unsigned(u64::from(u32::MAX))),
            ("active", Bool(false)),
            ("declared_mobilization_electricity_cost", Signed(i64::MIN)),
            ("declared_mobilization_data_cost", Signed(i64::MAX)),
            ("max_duration_ticks", Unsigned(u64::MAX)),
            ("aggressor_score", Signed(-1)),
            ("defender_score", Signed(0)),
            ("declared_at", Unsigned(0)),
        ],
        DiplomacyFieldV1::Crises => &[
            ("crisis_id", Text("internal-crisis")),
            ("kind", Text("")),
            ("severity", Unsigned(u64::from(u32::MAX))),
            ("status", Text("timed_out")),
            ("opened_at", Unsigned(0)),
            ("expires_at", Unsigned(u64::MAX)),
            ("impact", Signed(i64::MIN)),
        ],
        DiplomacyFieldV1::EconomicContracts => &[
            ("contract_id", Text("internal-contract")),
            ("creator_agent_id", Text("creator")),
            ("counterparty_agent_id", Text("")),
            ("fulfillment_kind", Text("service")),
            ("settlement_kind", Text("data")),
            ("settlement_amount", Signed(i64::MIN)),
            ("reputation_stake", Signed(i64::MAX)),
            ("expires_at", Unsigned(u64::MAX)),
            ("description", Text("")),
            ("status", Text("expired")),
            ("transfer_amount", Signed(-1)),
            ("tax_amount", Signed(0)),
        ],
    };
    for (name, value) in columns {
        cell(
            field,
            &fixture::pair_token(&[0], &fixture::text_token(name)),
            copy(value),
            ctx,
        );
    }
    let options: &[(&str, bool, Expected)] = match field {
        DiplomacyFieldV1::Alliances => &[],
        DiplomacyFieldV1::Wars => &[
            ("concluded_at", true, Unsigned(0)),
            ("winner_alliance_id", true, Text("")),
            ("loser_alliance_id", false, Absent),
            ("settlement_summary", true, Text("summary")),
        ],
        DiplomacyFieldV1::Crises => &[
            ("resolver_agent_id", true, Text("")),
            ("strategy", false, Absent),
            ("success", true, Bool(false)),
            ("resolved_at", true, Unsigned(0)),
        ],
        DiplomacyFieldV1::EconomicContracts => &[
            ("accepted_at", true, Unsigned(0)),
            ("settled_at", false, Absent),
            ("settlement_success", true, Bool(false)),
            ("settlement_notes", true, Text("")),
        ],
    };
    for (name, present, value) in options {
        let base = fixture::pair_token(&[0], &fixture::text_token(name));
        cell(
            field,
            &fixture::pair_token(&base, &[0]),
            Bool(*present),
            ctx,
        );
        cell(
            field,
            &fixture::pair_token(&base, &fixture::text_token("value")),
            copy(value),
            ctx,
        );
    }
    let (collection, count) = match field {
        DiplomacyFieldV1::Alliances => ("members", 3),
        DiplomacyFieldV1::Wars => ("participant_outcomes", 2),
        _ => return,
    };
    let base = fixture::pair_token(&[0], &fixture::text_token(collection));
    cell(field, &fixture::pair_token(&base, &[0]), Unit, ctx);
    cell(
        field,
        &fixture::pair_token(&base, &fixture::text_token("length")),
        Unsigned(count),
        ctx,
    );
    for ordinal in 0..count {
        let mut number = vec![2];
        number.extend_from_slice(&ordinal.to_be_bytes());
        let member = fixture::pair_token(&base, &number);
        if field == DiplomacyFieldV1::Alliances {
            cell(
                field,
                &member,
                Text(if ordinal == 2 { "" } else { "same\0é" }),
                ctx,
            );
        } else {
            cell(field, &member, Unit, ctx);
            for (name, value) in [
                ("agent_id", Text("")),
                ("electricity_delta", Signed(i64::MIN)),
                ("data_delta", Signed(i64::MAX)),
                ("reputation_delta", Signed(0)),
            ] {
                cell(
                    field,
                    &fixture::pair_token(&member, &fixture::text_token(name)),
                    value,
                    ctx,
                );
            }
        }
    }
}
fn copy(value: &Expected) -> Expected {
    match value {
        Unit => Unit,
        Bool(v) => Bool(*v),
        Unsigned(v) => Unsigned(*v),
        Signed(v) => Signed(*v),
        Text(v) => Text(v),
        Absent => Absent,
    }
}
