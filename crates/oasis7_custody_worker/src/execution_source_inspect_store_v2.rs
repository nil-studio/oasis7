//! Complete finite inventory for the new dedicated Identity object only.
use super::*;
use config::Server;
use serde::{Deserialize, Serialize};
const BIRTH: &str = "execution-source-inspect-birth-v2";
const META: &str = "table:custody_source_inspect_admission_v2";
const ONCE: &str = "table:custody_source_inspect_once_v2";
const ROW_MAX: usize = 4 * 32768;
fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> core::result::Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Original {
    request: PendingSourceInspectDispatchRequestV2,
    claimed_at: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    original: Original,
    #[serde(deserialize_with = "nullable")]
    native_leaf: Option<SignedPendingSourceNativeLeafV2>,
    #[serde(deserialize_with = "nullable")]
    native_at: Option<DecimalU64>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Birth {
    schema: u8,
    server: Server,
    object_id: String,
    first: Original,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    schema: u8,
    server: Server,
    object_id: String,
    count: DecimalU64,
    birth_digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    id: String,
    payload: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    tables: Vec<String>,
    #[serde(deserialize_with = "nullable")]
    records: Option<Vec<Row>>,
    #[serde(deserialize_with = "nullable")]
    meta: Option<Vec<Row>>,
    #[serde(deserialize_with = "nullable")]
    count: Option<String>,
    #[serde(deserialize_with = "nullable")]
    claims: Option<Vec<Row>>,
}
fn json<T: Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|_| rejected())
}
fn decode<T: serde::de::DeserializeOwned + Serialize>(raw: &str, max: usize) -> Result<T> {
    checked(raw.len() <= max)?;
    let value: T = serde_json::from_str(raw).map_err(|_| rejected())?;
    checked(json(&value)? == raw)?;
    Ok(value)
}
fn original(server: &Server, object: &str, q: &Original) -> Result<()> {
    q.request.wire_bytes().map_err(|_| rejected())?;
    let b = &q.request.proof.binding;
    checked(
        b.expected_service_epoch == server.service_epoch
            && q.request.request.expected_audience == server.native_audience
            && b.native_binding.identity_namespace == server.native_namespace
            && b.native_binding.identity_instance_epoch == server.native_epoch
            && b.issuer.native_audience_origin == server.native_origin
            && q.claimed_at.0 >= b.issued_at.0
            && q.claimed_at.0 < b.expires_at.0
            && q.claimed_at.0 < q.request.request.approval.proof.expires_at.0,
    )?;
    q.request
        .proof
        .verify(&server.dispatch_pin, &server.floor, object, q.claimed_at.0)
        .map_err(|_| rejected())?;
    q.request
        .verify_native_approval(&server.native_key_id, &server.native_public_key)
        .map_err(|_| rejected())
}
fn validate_claim(server: &Server, object: &str, c: &Claim) -> Result<()> {
    original(server, object, &c.original)?;
    match (&c.native_leaf, c.native_at) {
        (None, None) => Ok(()),
        (Some(leaf), Some(at)) => {
            checked(
                leaf.claim.dispatch == c.original.request.proof
                    && at.0 >= c.original.claimed_at.0
                    && at.0 >= leaf.claim.claimed_at.0
                    && at.0 < leaf.claim.expires_at.0,
            )?;
            leaf.verify(&server.leaf_pin, &server.floor, &server.native_origin, at.0)
                .map_err(|_| rejected())
        }
        _ => Err(rejected()),
    }
}
struct Installed {
    header: Header,
    birth_raw: String,
    claims: std::collections::BTreeMap<String, (String, Claim)>,
}
fn inventory(server: &Server, object: &str, raw: &str) -> Result<Option<Installed>> {
    let inv: Inventory = serde_json::from_str(raw).map_err(|_| rejected())?;
    checked(
        inv.tables.iter().all(|t| {
            matches!(t.as_str(), "table:_cf_METADATA" | "table:custody_records")
                || t == META
                || t == ONCE
        }) && inv.tables.len() <= 4,
    )?;
    if inv.records.is_none() && inv.meta.is_none() && inv.claims.is_none() && inv.count.is_none() {
        checked(inv.tables.iter().all(|t| t == "table:_cf_METADATA"))?;
        return Ok(None);
    }
    let records = inv.records.ok_or_else(rejected)?;
    let meta = inv.meta.ok_or_else(rejected)?;
    let count = inv.count.ok_or_else(rejected)?;
    let count_num = count.parse::<u64>().map_err(|_| rejected())?;
    checked(
        count_num.to_string() == count && count_num > 0 && count_num <= u64::from(server.capacity),
    )?;
    let rows = inv.claims.ok_or_else(rejected)?;
    checked(
        records.len() == 1
            && records[0].id == BIRTH
            && meta.len() == 1
            && meta[0].id == "singleton"
            && u64::try_from(rows.len()).map_err(|_| rejected())? == count_num,
    )?;
    let birth: Birth = decode(&records[0].payload, ROW_MAX)?;
    let header: Header = decode(&meta[0].payload, 16384)?;
    checked(
        birth.schema == 2
            && header.schema == 2
            && birth.server == *server
            && header.server == *server
            && birth.object_id == object
            && header.object_id == object
            && header.count.0 == count_num
            && header.birth_digest
                == oasis7_custody_core::approval::bytes_digest(records[0].payload.as_bytes()),
    )?;
    original(server, object, &birth.first)?;
    let mut claims = std::collections::BTreeMap::new();
    for row in rows {
        let c: Claim = decode(&row.payload, ROW_MAX)?;
        validate_claim(server, object, &c)?;
        checked(
            row.id == c.original.request.proof.binding.dispatcher_attempt_id
                && claims.insert(row.id, (row.payload, c)).is_none(),
        )?;
    }
    let first = claims
        .get(&birth.first.request.proof.binding.dispatcher_attempt_id)
        .ok_or_else(rejected)?;
    checked(first.1.original == birth.first)?;
    Ok(Some(Installed {
        header,
        birth_raw: records[0].payload.clone(),
        claims,
    }))
}
/// First-return only; no serde/Clone/history constructor.
pub(super) struct First {
    original: Original,
    server: Server,
    object_id: String,
}
impl First {
    pub(super) fn request(&self) -> &PendingSourceInspectDispatchRequestV2 {
        &self.original.request
    }
}
/// Construct this token only after the actual storage transaction returns success.
pub(super) fn claim_transition(
    server: &Server,
    object: &str,
    request: &PendingSourceInspectDispatchRequestV2,
    at: u64,
    raw: &str,
) -> Result<Vec<String>> {
    let new = Original {
        request: request.clone(),
        claimed_at: DecimalU64(at),
    };
    original(server, object, &new)?;
    let installed = inventory(server, object, raw)?;
    let (mut header, birth_raw) = if let Some(old) = installed {
        checked(
            old.header.count.0 < u64::from(server.capacity)
                && !old
                    .claims
                    .contains_key(&request.proof.binding.dispatcher_attempt_id),
        )?;
        (old.header, old.birth_raw)
    } else {
        let birth = Birth {
            schema: 2,
            server: server.clone(),
            object_id: object.into(),
            first: new.clone(),
        };
        let raw = json(&birth)?;
        checked(raw.len() <= ROW_MAX)?;
        (
            Header {
                schema: 2,
                server: server.clone(),
                object_id: object.into(),
                count: DecimalU64(0),
                birth_digest: oasis7_custody_core::approval::bytes_digest(raw.as_bytes()),
            },
            raw,
        )
    };
    header.count.0 = header.count.0.checked_add(1).ok_or_else(rejected)?;
    let claim = Claim {
        original: new,
        native_leaf: None,
        native_at: None,
    };
    let claim_raw = json(&claim)?;
    checked(claim_raw.len() <= ROW_MAX)?;
    Ok(vec![
        json(&header)?,
        request.proof.binding.dispatcher_attempt_id.clone(),
        claim_raw,
        birth_raw,
    ])
}
pub(super) fn first_after_commit(
    server: Server,
    object: String,
    request: PendingSourceInspectDispatchRequestV2,
    at: u64,
    committed: &str,
) -> Result<First> {
    let c: Claim = decode(committed, ROW_MAX)?;
    checked(
        c.original.request == request
            && c.original.claimed_at.0 == at
            && c.native_leaf.is_none()
            && c.native_at.is_none(),
    )?;
    Ok(First {
        original: c.original,
        server,
        object_id: object,
    })
}
/// This capture contains only immutable public comparison data, never a sender token.
pub(super) fn native_comparison(
    first: &First,
    leaf: SignedPendingSourceNativeLeafV2,
    at: u64,
) -> impl Fn(&str) -> Result<Vec<String>> + 'static {
    let server = first.server.clone();
    let object = first.object_id.clone();
    let original = first.original.clone();
    move |raw| {
        let installed = inventory(&server, &object, raw)?.ok_or_else(rejected)?;
        let id = &original.request.proof.binding.dispatcher_attempt_id;
        let (old_raw, old) = installed.claims.get(id).ok_or_else(rejected)?;
        checked(
            old.original == original
                && old.native_leaf.is_none()
                && old.native_at.is_none()
                && leaf.claim.dispatch == original.request.proof,
        )?;
        let next = Claim {
            original: old.original.clone(),
            native_leaf: Some(leaf.clone()),
            native_at: Some(DecimalU64(at)),
        };
        validate_claim(&server, &object, &next)?;
        Ok(vec![id.clone(), old_raw.clone(), json(&next)?])
    }
}
pub(super) struct Authorized {
    request: PendingSourceNativeRequestV2,
}
impl Authorized {
    pub(super) fn request(&self) -> &PendingSourceNativeRequestV2 {
        &self.request
    }
}
pub(super) fn native_after_commit(
    first: First,
    leaf: SignedPendingSourceNativeLeafV2,
    at: u64,
    committed: &str,
) -> Result<Authorized> {
    let c: Claim = decode(committed, ROW_MAX)?;
    validate_claim(&first.server, &first.object_id, &c)?;
    checked(
        c.original == first.original
            && c.native_leaf.as_ref() == Some(&leaf)
            && c.native_at == Some(DecimalU64(at)),
    )?;
    let request = PendingSourceNativeRequestV2 {
        leaf,
        request: first.original.request.request,
    };
    request.wire_bytes().map_err(|_| rejected())?;
    Ok(Authorized { request })
}
