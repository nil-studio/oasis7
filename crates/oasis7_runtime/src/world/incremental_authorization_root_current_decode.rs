//! Untrusted cold-hint DTOs for the three closed capability cell grammars.
//! Decoding grants no source qualification; the original full cell visitor and
//! real Current matcher must subsequently validate every field and wire byte.
use super::*;
use oasis7_wasm_abi::CapabilitySubject;
use oasis7_world_state::incremental_runtime_capability_records::{
    RuntimeCapabilityRecordKindV1, RuntimeCapabilityRecordRefV1,
};
use oasis7_world_state::{
    CapabilityAuthorizationNonceRecord, CapabilityBudgetAccount, CapabilityEffectReceiptLink,
};
use std::mem::size_of;
pub(super) enum HintCapabilityRecordV1 {
    Nonce(CapabilityAuthorizationNonceRecord),
    Budget(CapabilityBudgetAccount),
    Link(CapabilityEffectReceiptLink),
}
impl HintCapabilityRecordV1 {
    pub(super) fn borrowed(&self) -> RuntimeCapabilityRecordRefV1<'_> {
        match self {
            Self::Nonce(v) => RuntimeCapabilityRecordRefV1::Nonce(v),
            Self::Budget(v) => RuntimeCapabilityRecordRefV1::Budget(v),
            Self::Link(v) => RuntimeCapabilityRecordRefV1::EffectLink(v),
        }
    }
}
// Exhaustive field/type destructuring pins the owned heap closure. New DTO or
// subject fields/variants cannot silently inherit this admission model.
#[allow(dead_code)]
fn audited_shapes(
    n: &CapabilityAuthorizationNonceRecord,
    b: &CapabilityBudgetAccount,
    l: &CapabilityEffectReceiptLink,
) {
    let CapabilityAuthorizationNonceRecord {
        request_hash,
        outcome_hash,
        committed_receipt_id,
        state,
    } = n;
    let _: (&String, &String, &Option<String>, &String) =
        (request_hash, outcome_hash, committed_receipt_id, state);
    let CapabilityBudgetAccount {
        subject,
        grant_id,
        remaining_units,
        reserved_units,
        spent_units,
    } = b;
    let _: (&CapabilitySubject, &String, &i64, &i64, &i64) = (
        subject,
        grant_id,
        remaining_units,
        reserved_units,
        spent_units,
    );
    match subject {
        CapabilitySubject::Agent {
            agent_id,
            owner_binding,
            generation,
        } => {
            let _: (&String, &String, &u64) = (agent_id, owner_binding, generation);
        }
        CapabilitySubject::Module {
            module_id,
            module_version,
            instance_id,
        } => {
            let _: (&String, &String, &String) = (module_id, module_version, instance_id);
        }
        CapabilitySubject::System { system_id, epoch } => {
            let _: (&String, &u64) = (system_id, epoch);
        }
    }
    let CapabilityEffectReceiptLink {
        authorization_receipt_id,
    } = l;
    let _: &String = authorization_receipt_id;
}
fn depth(
    node: &ValueRef,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, AuthorizationRootCacheErrorV1> {
    common.consume_steps(1)?;
    match &***node {
        CanonicalValueV1::Atom(_) => Ok(0),
        CanonicalValueV1::Tag { value, .. } => {
            depth(value, common)?.checked_add(1).ok_or_else(limited)
        }
        CanonicalValueV1::Array { entries, .. } | CanonicalValueV1::Map { entries, .. } => {
            let mut deepest = 0;
            tree::visit::<AuthorizationRootCacheErrorV1>(
                entries,
                common,
                &mut |_, value, common| {
                    deepest = deepest.max(depth(value, common)?);
                    Ok(())
                },
            )?;
            deepest.checked_add(1).ok_or_else(limited)
        }
    }
}
pub(super) fn decode(
    node: &ValueRef,
    kind: RuntimeCapabilityRecordKindV1,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<ReservedValue<HintCapabilityRecordV1>, AuthorizationRootCacheErrorV1> {
    let phases = depth(node, common)?.checked_add(4).ok_or_else(limited)?;
    let raw = cbor::encode_node(node, limits, common, class)?;
    // Every CBOR token occupies >=1 byte. Content's Vec growth and adjacent /
    // internally tagged buffering are paid for each reachable nesting phase.
    // The audited output types above contain only Strings, Option<String>,
    // fixed integers and the three closed subject variants; no opaque map DTO.
    let content = size_of::<serde::__private229::de::Content<'static>>();
    let pair = size_of::<(
        serde::__private229::de::Content<'static>,
        serde::__private229::de::Content<'static>,
    )>();
    let output = size_of::<CapabilityAuthorizationNonceRecord>()
        .max(size_of::<CapabilityBudgetAccount>())
        .max(size_of::<CapabilityEffectReceiptLink>())
        .max(size_of::<CapabilitySubject>());
    let per = content
        .max(pair)
        .max(output)
        .checked_mul(4)
        .and_then(|n| n.checked_add(2))
        .ok_or_else(limited)?;
    let heap = raw
        .len()
        .checked_mul(per)
        .and_then(|n| n.checked_mul(phases))
        .and_then(|n| n.checked_add(size_of::<HintCapabilityRecordV1>() + 512))
        .ok_or_else(limited)?;
    let fee = common.memory().reserve(heap, class)?;
    let visits = raw
        .len()
        .checked_mul(phases)
        .and_then(|n| n.checked_mul(8))
        .ok_or_else(limited)?;
    common.charge(visits as u64, visits as u64)?;
    struct Input<'a, 'c, 'm> {
        raw: &'a [u8],
        at: usize,
        common: &'c mut FormalBoundedContextV1<'m>,
        failure: Option<AuthorizationRootCacheErrorV1>,
    }
    impl io::Read for Input<'_, '_, '_> {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            let n = b.len().min(16 * 1024).min(self.raw.len() - self.at);
            if let Err(error) = self.common.charge(1, n as u64) {
                self.failure = Some(error.into());
                return Err(io::Error::other("authorization hint decode stopped"));
            }
            b[..n].copy_from_slice(&self.raw[self.at..self.at + n]);
            self.at += n;
            Ok(n)
        }
    }
    let mut input = Input {
        raw: &raw,
        at: 0,
        common,
        failure: None,
    };
    let result = match kind {
        RuntimeCapabilityRecordKindV1::Nonce => {
            serde_cbor::from_reader(&mut input).map(HintCapabilityRecordV1::Nonce)
        }
        RuntimeCapabilityRecordKindV1::Budget => {
            serde_cbor::from_reader(&mut input).map(HintCapabilityRecordV1::Budget)
        }
        RuntimeCapabilityRecordKindV1::EffectLink => {
            serde_cbor::from_reader(&mut input).map(HintCapabilityRecordV1::Link)
        }
    };
    if let Some(error) = input.failure.take() {
        return Err(error);
    }
    let out = result.map_err(AuthorizationRootCacheErrorV1::Decode)?;
    if input.at != raw.len() {
        return Err(invalid("authorization hint DTO trailing bytes"));
    }
    input.common.consume_steps(0)?;
    Ok(fee.retain(out))
}
