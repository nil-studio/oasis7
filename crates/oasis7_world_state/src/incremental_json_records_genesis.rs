//! Charged iterative postorder JSON nodes; no record/JSON aggregate blob.
use super::*;
use serde_json::Value;
enum Edge<'a> {
    Index(u64),
    Name(&'a str),
}
enum Children<'a> {
    None,
    Array(std::iter::Enumerate<std::slice::Iter<'a, Value>>),
    Object(serde_json::map::Iter<'a>),
}
struct Frame<'a> {
    root: StateRootV1,
    children: Children<'a>,
    edge: Option<Edge<'a>>,
    expected: u64,
}
fn frame<'a, W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    value: &'a Value,
    edge: Option<Edge<'a>>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut JsonWriteWorkV1,
) -> Result<Frame<'a>, MemberRecordError> {
    work.json_node()?;
    let (label, payload, length, children) = match value {
        Value::Null => (
            "null",
            MemberPayloadRefV1::Unit,
            MemberPayloadRefV1::Unit,
            Children::None,
        ),
        Value::Bool(v) => (
            "bool",
            MemberPayloadRefV1::Bool(*v),
            MemberPayloadRefV1::Unit,
            Children::None,
        ),
        Value::String(v) => (
            "string",
            MemberPayloadRefV1::Text(v),
            MemberPayloadRefV1::Unit,
            Children::None,
        ),
        Value::Number(v) if v.is_f64() => (
            "f64",
            MemberPayloadRefV1::U64(
                v.as_f64()
                    .ok_or(MemberRecordError::Invalid("JSON float"))?
                    .to_bits(),
            ),
            MemberPayloadRefV1::Unit,
            Children::None,
        ),
        Value::Number(v) => {
            if let Some(v) = v.as_u64() {
                (
                    "u64",
                    MemberPayloadRefV1::U64(v),
                    MemberPayloadRefV1::Unit,
                    Children::None,
                )
            } else {
                (
                    "i64",
                    MemberPayloadRefV1::I64(
                        v.as_i64()
                            .ok_or(MemberRecordError::Invalid("JSON number"))?,
                    ),
                    MemberPayloadRefV1::Unit,
                    Children::None,
                )
            }
        }
        Value::Array(v) => (
            "array",
            MemberPayloadRefV1::Unit,
            MemberPayloadRefV1::U64(v.len() as u64),
            Children::Array(v.iter().enumerate()),
        ),
        Value::Object(v) => (
            "object",
            MemberPayloadRefV1::Unit,
            MemberPayloadRefV1::U64(v.len() as u64),
            Children::Object(v.iter()),
        ),
    };
    let expected = 5 + match value {
        Value::Array(v) => v.len() as u64,
        Value::Object(v) => v.len() as u64,
        _ => 0,
    };
    work.cell()?;
    let meta = metadata(profile, ctx)?;
    let mut root = tree::append(
        StateRootV1::empty(profile.namespace()?)
            .map_err(|_| MemberRecordError::Invalid("effect JSON namespace"))?,
        &meta.field_key,
        meta.value_root,
        ctx,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: LABELS,
        },
    )?;
    work.cell()?;
    let marker = row(profile, &(), MemberPayloadRefV1::Unit, ctx)?;
    root = append(root, &marker, ctx, work)?;
    for (name, payload) in [
        ("json.kind", MemberPayloadRefV1::Text(label)),
        ("json.value", payload),
        ("json.length", length),
    ] {
        work.cell()?;
        let cell = row(profile, &((), name), payload, ctx)?;
        root = append(root, &cell, ctx, work)?;
    }
    Ok(Frame {
        root,
        children,
        edge,
        expected,
    })
}
pub fn prepare_json_subtree_v1<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    value: &Value,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut JsonWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    profile.namespace()?;
    let capacity = work
        .max_json_depth
        .checked_add(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "effect JSON depth admission",
        ))?;
    // Includes each root's fixed-size digest String allocation as well as Vec.
    let bytes = capacity
        .checked_mul(std::mem::size_of::<Frame<'_>>() + 128)
        .ok_or(ValueTreeError::ResourceLimited(
            "effect JSON stack admission",
        ))?;
    let _stack = ctx
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut stack = Vec::with_capacity(capacity);
    stack.push(frame(profile, value, None, ctx, work)?);
    loop {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let current = stack
            .last_mut()
            .ok_or(MemberRecordError::Invalid("JSON stack"))?;
        let next = match &mut current.children {
            Children::None => None,
            Children::Array(v) => v.next().map(|(i, value)| (Edge::Index(i as u64), value)),
            Children::Object(v) => v
                .next()
                .map(|(key, value)| (Edge::Name(key.as_str()), value)),
        };
        if let Some((edge, value)) = next {
            if stack.len() > work.max_json_depth {
                return Err(ValueTreeError::ResourceLimited("effect JSON depth").into());
            }
            stack.push(frame(profile, value, Some(edge), ctx, work)?);
            continue;
        }
        let complete = stack.pop().ok_or(MemberRecordError::Invalid("JSON pop"))?;
        if complete.root.leaf_count != complete.expected {
            return Err(MemberRecordError::Invalid("JSON exhaustive members"));
        }
        let Some(parent) = stack.last_mut() else {
            return Ok(complete.root);
        };
        work.cell()?;
        let edge = match complete
            .edge
            .ok_or(MemberRecordError::Invalid("JSON parent edge"))?
        {
            Edge::Index(index) => root_row(profile, &(((), "members"), index), complete.root, ctx)?,
            Edge::Name(name) => root_row(profile, &(((), "members"), name), complete.root, ctx)?,
        };
        parent.root = append(parent.root.clone(), &edge, ctx, work)?;
    }
}
