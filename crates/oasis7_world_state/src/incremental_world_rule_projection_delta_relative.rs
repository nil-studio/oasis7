//! Selected legacy relative records retain their original Spec qualification.
//! This closed typed DTO reader uses original member payload/semantic codecs;
//! it never decodes a legacy relative root with the typed-composite Codec.
use super::*;
use crate::incremental_semantic_key::{
    CanonicalSemanticKeyRefV1, SemanticKeyRefV1, SemanticKeyV1, read_semantic_key_v1,
    write_semantic_key_v1,
};
use oasis7_formal_protocol::incremental_state::{StateScanContext, StateScanError};
use std::collections::BTreeSet;
pub(super) struct Reader<'a, 'p, 'v, M, R, E, C> {
    pub(super) ctx: &'a mut WorldFieldReadContextV1<'v, M, R, E, C>,
    pub(super) scan: &'a mut StateScanBudget,
    pub(super) p: &'a Processing<'p>,
    fee: MemoryReservation,
}
impl<
    'a,
    'p,
    'v,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
> Reader<'a, 'p, 'v, M, R, E, C>
{
    pub(super) fn new(
        ctx: &'a mut WorldFieldReadContextV1<'v, M, R, E, C>,
        scan: &'a mut StateScanBudget,
        p: &'a Processing<'p>,
    ) -> Result<Self, ValueTreeError> {
        let fee = ctx
            .values
            .memory
            .reserve(size_of::<Self>(), MemoryClass::Ordinary)?;
        Ok(Self { ctx, scan, p, fee })
    }
    fn admit(&mut self, n: usize) -> Result<(), ValueTreeError> {
        spend(self.p, n, &mut self.ctx.cancelled)?;
        let mut fee = self.ctx.values.memory.reserve(n, MemoryClass::Ordinary)?;
        self.fee.merge(&mut fee).map_err(ValueTreeError::Invalid)
    }
    fn bytes<K: SemanticKeyV1 + ?Sized>(&mut self, key: &K) -> Result<Vec<u8>, ValueTreeError> {
        let mut count = Output {
            p: self.p,
            stop: &mut self.ctx.cancelled,
            bytes: None,
            n: 0,
            max: usize::MAX,
            first: None,
        };
        let result = write_semantic_key_v1(key, &mut count);
        if let Some(e) = count.first.take() {
            return Err(e);
        }
        result.map_err(|_| bad())?;
        let n = count.n;
        drop(count);
        self.admit(n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?)?;
        let mut out = Vec::new();
        out.try_reserve_exact(n).map_err(|_| limit())?;
        if out.capacity() > n {
            return Err(limit());
        }
        let mut output = Output {
            p: self.p,
            stop: &mut self.ctx.cancelled,
            bytes: Some(out),
            n: 0,
            max: n,
            first: None,
        };
        let result = write_semantic_key_v1(key, &mut output);
        if let Some(e) = output.first.take() {
            return Err(e);
        }
        result.map_err(|_| bad())?;
        if output.n != n {
            return Err(bad());
        }
        output.bytes.take().ok_or_else(bad)
    }
    fn named(&mut self, prefix: &[u8], field: &str) -> Result<Vec<u8>, ValueTreeError> {
        self.bytes(&(
            CanonicalSemanticKeyRefV1::new(prefix).map_err(ValueTreeError::Invalid)?,
            field,
        ))
    }
    fn key(&mut self, key: &[u8]) -> Result<(WorldFieldKeyV1, u64, [u8; 32]), ValueTreeError> {
        let (field, n) = self.ctx.key(
            crate::incremental_world_rule_parent::descriptor()?,
            WorldFieldRecordKindV1::Entry,
            &CanonicalSemanticKeyRefV1::new(key).map_err(ValueTreeError::Invalid)?,
        )?;
        let value = self.ctx.lookup(&field)?.ok_or_else(bad)?;
        Ok((field, n, value))
    }
    fn fixed<const N: usize>(
        &mut self,
        key: &[u8],
        codec: MemberPayloadCodecV1,
    ) -> Result<[u8; N], ValueTreeError> {
        let (field, n, id) = self.key(key)?;
        let mut out = [0; N];
        read_member_record_profile_v1(
            &id,
            &field,
            &mut self.ctx.values,
            MemberReadProfileV1 {
                codec,
                semantic_bytes: n,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: n,
                    payload_bytes: N as u64,
                    record_bytes: self.ctx.record_bytes,
                },
            },
            &mut io::sink(),
            &mut io::Cursor::new(out.as_mut_slice()),
            &mut self.ctx.cancelled,
        )?;
        spend(self.p, N, &mut self.ctx.cancelled)?;
        Ok(out)
    }
    fn text(&mut self, key: &[u8]) -> Result<String, ValueTreeError> {
        let (field, n, id) = self.key(key)?;
        let limits = MemberRecordLimitsV1 {
            semantic_bytes: n,
            payload_bytes: self.ctx.record_bytes,
            record_bytes: self.ctx.record_bytes,
        };
        let h = read_member_record_header_v1(
            &id,
            &field,
            &mut self.ctx.values,
            limits,
            &mut self.ctx.cancelled,
        )?;
        if h.codec != MemberPayloadCodecV1::Text {
            return Err(bad());
        }
        let len = usize::try_from(h.payload_bytes).map_err(|_| limit())?;
        self.admit(len.checked_add(size_of::<String>()).ok_or_else(limit)?)?;
        let mut out = Vec::new();
        out.try_reserve_exact(len).map_err(|_| limit())?;
        if out.capacity() > len {
            return Err(limit());
        }
        crate::incremental_utf8_output::read_text_value(
            &id,
            &field,
            &mut self.ctx.values,
            MemberReadProfileV1 {
                codec: MemberPayloadCodecV1::Text,
                semantic_bytes: n,
                limits,
            },
            &mut out,
            "projection relative UTF-8",
            &mut self.ctx.cancelled,
        )?;
        spend(self.p, out.len(), &mut self.ctx.cancelled)?;
        String::from_utf8(out).map_err(|_| bad())
    }
    fn members(&mut self, prefix: &[u8]) -> Result<Vec<Vec<u8>>, ValueTreeError> {
        let cap = usize::try_from(self.ctx.root.leaf_count).map_err(|_| limit())?;
        self.admit(cap.checked_mul(size_of::<Vec<u8>>()).ok_or_else(limit)?)?;
        let mut members = Vec::new();
        members.try_reserve_exact(cap).map_err(|_| limit())?;
        if members.capacity() > cap {
            return Err(limit());
        }
        let p = self.p;
        let stop = std::cell::RefCell::new(&mut self.ctx.cancelled);
        let errors = std::cell::RefCell::new(&mut self.ctx.take_state_error);
        let memory = self.ctx.values.memory;
        let max = self.ctx.record_bytes;
        let semantic_max = self.ctx.semantic_bytes;
        let mut admitted = memory.reserve(0, MemoryClass::Ordinary)?;
        let result = StateScanContext {
            source: self.ctx.state,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut *self.scan,
        }
        .visit_entries(
            self.ctx.root,
            |entry| {
                spend(p, entry.key.len(), &mut || (stop.borrow_mut())())?;
                if let Some(e) = (errors.borrow_mut())() {
                    return Err(e);
                }
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if parsed.kind == WorldFieldRecordKindV1::Metadata {
                    return Ok(());
                }
                let field = WorldFieldKeyV1::new(
                    crate::incremental_world_rule_parent::descriptor()?,
                    parsed.kind,
                    parsed.semantic_key,
                )
                .map_err(ValueTreeError::Invalid)?;
                let limits = MemberRecordLimitsV1 {
                    semantic_bytes: semantic_max,
                    payload_bytes: max,
                    record_bytes: max,
                };
                let header = read_member_record_header_v1(
                    &entry.value,
                    &field,
                    &mut self.ctx.values,
                    limits,
                    &mut || (stop.borrow_mut())(),
                )?;
                let n = usize::try_from(header.semantic_bytes).map_err(|_| limit())?;
                spend(p, n, &mut || (stop.borrow_mut())())?;
                let mut scratch = memory.reserve(
                    n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
                    MemoryClass::Ordinary,
                )?;
                let mut raw = Vec::new();
                raw.try_reserve_exact(n).map_err(|_| limit())?;
                if raw.capacity() > n {
                    return Err(limit());
                }
                read_member_record_key_v1(
                    &entry.value,
                    &field,
                    &mut self.ctx.values,
                    limits,
                    &mut raw,
                    &mut || (stop.borrow_mut())(),
                )?;
                let semantic = read_semantic_key_v1(&raw).map_err(ValueTreeError::Invalid)?;
                if let SemanticKeyRefV1::Pair { left, right } = semantic {
                    spend(
                        p,
                        prefix.len().checked_add(raw.len()).ok_or_else(limit)?,
                        &mut || (stop.borrow_mut())(),
                    )?;
                    if left.decode().map_err(ValueTreeError::Invalid)?
                        == read_semantic_key_v1(prefix).map_err(ValueTreeError::Invalid)?
                        && !matches!(
                            right.decode().map_err(ValueTreeError::Invalid)?,
                            SemanticKeyRefV1::Unit
                        )
                    {
                        if members.len() == cap {
                            return Err(limit());
                        }
                        admitted
                            .merge(&mut scratch)
                            .map_err(ValueTreeError::Invalid)?;
                        members.push(raw);
                    }
                }
                Ok(())
            },
            || (stop.borrow_mut())(),
        );
        if let Some(e) = (errors.borrow_mut())() {
            return Err(e);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => return Err(limit()),
            Err(StateScanError::Invalid(e)) => return Err(ValueTreeError::Invalid(e)),
            Err(StateScanError::Tree(_)) => return Err(bad()),
        }
        self.fee
            .merge(&mut admitted)
            .map_err(ValueTreeError::Invalid)?;
        Ok(members)
    }
    fn field<T: Value>(&mut self, prefix: &[u8], field: &str) -> Result<T, ValueTreeError> {
        let key = self.named(prefix, field)?;
        T::read(&key, self)
    }
    pub(super) fn value<T: Value>(&mut self, key: &[u8]) -> Result<T, ValueTreeError> {
        T::read(key, self)
    }
}
pub(super) trait Value: Sized {
    fn read<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        key: &[u8],
        r: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError>;
}
impl Value for String {
    fn read<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        key: &[u8],
        r: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        r.text(key)
    }
}
macro_rules! number {
    ($t:ty,$codec:ident,$n:literal) => {
        impl Value for $t {
            fn read<
                M: MerkleNodeReader,
                R: ValueNodeReader,
                E: FnMut() -> Option<ValueTreeError>,
                C: FnMut() -> bool,
            >(
                key: &[u8],
                r: &mut Reader<'_, '_, '_, M, R, E, C>,
            ) -> Result<Self, ValueTreeError> {
                Ok(<$t>::from_be_bytes(
                    r.fixed::<$n>(key, MemberPayloadCodecV1::$codec)?,
                ))
            }
        }
    };
}
number!(u64, U64, 8);
number!(i64, I64, 8);
impl Value for u32 {
    fn read<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        key: &[u8],
        r: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        u32::try_from(u64::read(key, r)?).map_err(|_| bad())
    }
}
impl Value for bool {
    fn read<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        key: &[u8],
        r: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        match r.fixed::<1>(key, MemberPayloadCodecV1::Bool)? {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err(bad()),
        }
    }
}
impl<T: Value> Value for Option<T> {
    fn read<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        key: &[u8],
        r: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let present = r.bytes(&(
            CanonicalSemanticKeyRefV1::new(key).map_err(ValueTreeError::Invalid)?,
            (),
        ))?;
        if bool::read(&present, r)? {
            let value = r.named(key, "value")?;
            Ok(Some(T::read(&value, r)?))
        } else {
            Ok(None)
        }
    }
}
trait Key: Ord + Sized {
    fn key<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        raw: &[u8],
        r: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError>;
}
fn token(raw: &[u8]) -> Result<SemanticKeyRefV1<'_>, ValueTreeError> {
    let SemanticKeyRefV1::Pair { right, .. } =
        read_semantic_key_v1(raw).map_err(ValueTreeError::Invalid)?
    else {
        return Err(bad());
    };
    right.decode().map_err(ValueTreeError::Invalid)
}
impl Key for String {
    fn key<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        raw: &[u8],
        r: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let SemanticKeyRefV1::Text(text) = token(raw)? else {
            return Err(bad());
        };
        r.admit(
            text.len()
                .checked_add(size_of::<String>())
                .ok_or_else(limit)?,
        )?;
        let mut out = String::new();
        out.try_reserve_exact(text.len()).map_err(|_| limit())?;
        if out.capacity() > text.len() {
            return Err(limit());
        }
        out.push_str(text);
        Ok(out)
    }
}
impl Key for u64 {
    fn key<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        raw: &[u8],
        _: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let SemanticKeyRefV1::Unsigned(n) = token(raw)? else {
            return Err(bad());
        };
        Ok(n)
    }
}
impl<K: Key, V: Value> Value for BTreeMap<K, V> {
    fn read<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        key: &[u8],
        r: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let marker = r.bytes(&(
            CanonicalSemanticKeyRefV1::new(key).map_err(ValueTreeError::Invalid)?,
            (),
        ))?;
        r.fixed::<0>(&marker, MemberPayloadCodecV1::Unit)?;
        let keys = r.members(key)?;
        let mut out = BTreeMap::new();
        for raw in &keys {
            let k = K::key(raw, r)?;
            let v = V::read(raw, r)?;
            r.admit(11 * (size_of::<(K, V)>() + size_of::<usize>()) + 128)?;
            spend(
                r.p,
                raw.len()
                    .checked_mul(11)
                    .and_then(|n| n.checked_mul((out.len() + 1).ilog2() as usize + 1))
                    .ok_or_else(limit)?,
                &mut r.ctx.cancelled,
            )?;
            if out.insert(k, v).is_some() {
                return Err(bad());
            }
        }
        Ok(out)
    }
}
impl<K: Key> Value for BTreeSet<K> {
    fn read<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        key: &[u8],
        r: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let marker = r.bytes(&(
            CanonicalSemanticKeyRefV1::new(key).map_err(ValueTreeError::Invalid)?,
            (),
        ))?;
        r.fixed::<0>(&marker, MemberPayloadCodecV1::Unit)?;
        let keys = r.members(key)?;
        let mut out = BTreeSet::new();
        for raw in &keys {
            r.fixed::<0>(raw, MemberPayloadCodecV1::Unit)?;
            let k = K::key(raw, r)?;
            r.admit(11 * (size_of::<K>() + size_of::<usize>()) + 128)?;
            spend(
                r.p,
                raw.len()
                    .checked_mul(11)
                    .and_then(|n| n.checked_mul((out.len() + 1).ilog2() as usize + 1))
                    .ok_or_else(limit)?,
                &mut r.ctx.cancelled,
            )?;
            if !out.insert(k) {
                return Err(bad());
            }
        }
        Ok(out)
    }
}
impl<T: Value> Value for Vec<T> {
    fn read<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        key: &[u8],
        r: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let marker = r.bytes(&(
            CanonicalSemanticKeyRefV1::new(key).map_err(ValueTreeError::Invalid)?,
            (),
        ))?;
        r.fixed::<0>(&marker, MemberPayloadCodecV1::Unit)?;
        let keys = r.members(key)?;
        r.admit(keys.len().checked_mul(size_of::<T>()).ok_or_else(limit)?)?;
        let mut ordered = BTreeMap::new();
        for raw in keys {
            let n = u64::key(&raw, r)?;
            r.admit(11 * (size_of::<(u64, Vec<u8>)>() + size_of::<usize>()) + 128)?;
            if ordered.insert(n, raw).is_some() {
                return Err(bad());
            }
        }
        let mut out = Vec::new();
        out.try_reserve_exact(ordered.len()).map_err(|_| limit())?;
        if out.capacity() > ordered.len() {
            return Err(limit());
        }
        for (n, raw) in ordered {
            spend(r.p, raw.len(), &mut r.ctx.cancelled)?;
            if n != out.len() as u64 {
                return Err(bad());
            }
            out.push(T::read(&raw, r)?);
        }
        Ok(out)
    }
}
macro_rules! record{($ty:ty,{$($field:ident),+$(,)?})=>{impl Value for $ty{fn read<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(key:&[u8],r:&mut Reader<'_,'_,'_,M,R,E,C>)->Result<Self,ValueTreeError>{r.admit(size_of::<Self>())?;Ok(Self{$($field:r.field(key,stringify!($field))?),+})}}};}
record!(crate::world_rules::SupplyAuthorizationV1,{id,provider_id,crisis_id,crisis_evidence,material,beneficiaries,policy_version,starts_at,expires_at,review_proposal,remaining_escrow,requests,revision,active});
record!(crate::world_rules::SupplyBatchV1,{id,authorization_id,frozen_at,authorization_revision,frozen_escrow,frozen_lineages,demands,allocation,allocated});
record!(crate::world_rules::SupplyLineageV1,{target,fulfilled,reserved});
record!(crate::CrisisState,{crisis_id,kind,severity,status,opened_at,expires_at,resolver_agent_id,strategy,success,impact,resolved_at});
record!(crate::world_rules::OrganizationEstateDepositV1,{id,organization_id,contributor_id,material,amount,held_in_trust,disposed,effect_receipt});
record!(crate::world_rules::OrganizationCaseV1,{id,organization_id,founder_id,members,policy_version,revision,phase,disposition,notified_at,protected_at,grants,requests,stage_receipts});
record!(crate::world_rules::OrganizationGrantV1,{id,delegate_id,scopes,revision,expires_at,source_proposal,replaces});
record!(crate::world_rules::OrganizationRecoveryRequestV1,{requester_id,appeal,approved,resolved_by});
macro_rules! enumeration {
    ($ty:ty) => {
        impl Value for $ty {
            fn read<
                M: MerkleNodeReader,
                R: ValueNodeReader,
                E: FnMut() -> Option<ValueTreeError>,
                C: FnMut() -> bool,
            >(
                key: &[u8],
                r: &mut Reader<'_, '_, '_, M, R, E, C>,
            ) -> Result<Self, ValueTreeError> {
                let text = r.text(key)?;
                spend(r.p, text.len(), &mut r.ctx.cancelled)?;
                serde::Deserialize::deserialize(serde::de::value::StrDeserializer::<
                    serde::de::value::Error,
                >::new(&text))
                .map_err(|_| bad())
            }
        }
    };
}
enumeration!(crate::CrisisStatus);
enumeration!(crate::world_rules::OrganizationProtectionPhaseV1);
enumeration!(crate::world_rules::OrganizationDispositionPhaseV1);
impl Key for oasis7_agent_api::world_action::OrganizationEstateScopeV1 {
    fn key<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        raw: &[u8],
        r: &mut Reader<'_, '_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let SemanticKeyRefV1::Pair { left, right } = token(raw)? else {
            return Err(bad());
        };
        let SemanticKeyRefV1::Text(object) = left.decode().map_err(ValueTreeError::Invalid)? else {
            return Err(bad());
        };
        let SemanticKeyRefV1::Text(right) = right.decode().map_err(ValueTreeError::Invalid)? else {
            return Err(bad());
        };
        r.admit(
            object
                .len()
                .checked_add(size_of::<Self>())
                .ok_or_else(limit)?,
        )?;
        let mut object_id = String::new();
        object_id
            .try_reserve_exact(object.len())
            .map_err(|_| limit())?;
        if object_id.capacity() > object.len() {
            return Err(limit());
        }
        object_id.push_str(object);
        let right = serde::Deserialize::deserialize(serde::de::value::StrDeserializer::<
            serde::de::value::Error,
        >::new(right))
        .map_err(|_| bad())?;
        Ok(Self { object_id, right })
    }
}
pub(super) fn original_json<
    T: Value + serde::Serialize,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    p: &Processing<'_>,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let memory = ctx.values.memory.clone();
    let mut r = Reader::new(ctx, scan, p)?;
    let key = r.bytes(&())?;
    let value = r.value::<T>(&key)?;
    json(&value, &memory, r.p, &mut r.ctx.cancelled)
}
