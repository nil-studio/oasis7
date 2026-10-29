//! Bounded retained rotation inputs. Shape validation grants no authority.
use oasis7_agent_api::world_action::{
    Action, FinalityKeyRotationBodyV1, FinalityKeyRotationProofV1, FinalityKeyRotationRoleV1,
    GovernanceAuthorizedActionV1,
};
use serde::{Deserialize, Serialize, de};
use std::collections::BTreeMap;

pub const MAX_FINALITY_KEY_ROTATION_RECORDS_V1: usize = 256;

/// Preserve the exact authorized action once, rather than duplicating its body
/// and proofs. Runtime verifies the original parent and signatures separately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalityKeyRotationRecordV1 {
    pub authorized_action: GovernanceAuthorizedActionV1,
    pub applied_at_tick: crate::WorldTime,
}
impl FinalityKeyRotationRecordV1 {
    pub fn typed_parts(
        &self,
    ) -> Result<
        (
            &FinalityKeyRotationBodyV1,
            &FinalityKeyRotationProofV1,
            &FinalityKeyRotationProofV1,
        ),
        String,
    > {
        match self.authorized_action.action.as_ref() {
            Action::RotateGovernanceValidatorFinalityKeyV1 {
                body,
                old_key_proof,
                new_key_proof,
                ..
            } => Ok((body, old_key_proof, new_key_proof)),
            _ => Err("rotation record does not retain a rotation action".into()),
        }
    }
    pub fn body(&self) -> Result<&FinalityKeyRotationBodyV1, String> {
        self.typed_parts().map(|(body, _, _)| body)
    }
    pub fn validate(&self) -> Result<(), String> {
        let (body, old, new) = self.typed_parts()?;
        body.validate_authorization(&self.authorized_action.authorization)?;
        if self.applied_at_tick != body.decision_height {
            return Err("rotation record applied tick differs from decision height".into());
        }
        let Action::RotateGovernanceValidatorFinalityKeyV1 {
            controller_account_id,
            ..
        } = self.authorized_action.action.as_ref()
        else {
            unreachable!("typed_parts checked rotation action")
        };
        if controller_account_id != &body.authorization.controller_account_id {
            return Err("rotation action controller differs from its body".into());
        }
        old.validate_for(body, FinalityKeyRotationRoleV1::OldKeyAuthorization)?;
        new.validate_for(body, FinalityKeyRotationRoleV1::NewKeyPossession)?;
        Ok(())
    }
}

pub(crate) trait RotationRecord {
    fn validate_record(&self) -> Result<(), String>;
    fn rotation_id(&self) -> Result<&str, String>;
}
impl RotationRecord for FinalityKeyRotationRecordV1 {
    fn validate_record(&self) -> Result<(), String> {
        self.validate()
    }
    fn rotation_id(&self) -> Result<&str, String> {
        self.body().map(|b| b.rotation_id.as_str())
    }
}
/// Both retained sources share one budget; callers check before reserving a slot.
pub fn validate_finality_key_rotation_history_capacity(
    admitted: usize,
    bootstrap: usize,
) -> Result<(), String> {
    if admitted
        .checked_add(bootstrap)
        .is_none_or(|n| n > MAX_FINALITY_KEY_ROTATION_RECORDS_V1)
    {
        return Err("combined rotation history count exceeds bound".into());
    }
    Ok(())
}
pub(crate) fn deserialize_finality_key_rotations<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, FinalityKeyRotationRecordV1>, D::Error> {
    deserialize_rotation_records(d)
}
pub(crate) fn deserialize_rotation_records<
    'de,
    R: RotationRecord + Deserialize<'de>,
    D: serde::Deserializer<'de>,
>(
    d: D,
) -> Result<BTreeMap<String, R>, D::Error> {
    struct Rotations<R>(std::marker::PhantomData<R>);
    impl<'de, R: RotationRecord + Deserialize<'de>> de::Visitor<'de> for Rotations<R> {
        type Value = BTreeMap<String, R>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 256 unique exact finality rotation records")
        }
        fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            if map
                .size_hint()
                .is_some_and(|n| n > MAX_FINALITY_KEY_ROTATION_RECORDS_V1)
            {
                return Err(de::Error::custom(
                    "rotation history declared count exceeds bound",
                ));
            }
            let mut records = BTreeMap::new();
            while let Some(key) = map.next_key::<String>()? {
                if records.len() == MAX_FINALITY_KEY_ROTATION_RECORDS_V1
                    || records.contains_key(&key)
                {
                    return Err(de::Error::custom("rotation history count or duplicate ID"));
                }
                if key.is_empty() || key.len() > 256 || key.trim() != key {
                    return Err(de::Error::custom(
                        "rotation history ID exceeds identity bounds",
                    ));
                }
                let record: R = map.next_value()?;
                record.validate_record().map_err(de::Error::custom)?;
                if record.rotation_id().map_err(de::Error::custom)? != key {
                    return Err(de::Error::custom(
                        "rotation history key differs from original body",
                    ));
                }
                records.insert(key, record);
            }
            Ok(records)
        }
    }
    d.deserialize_map(Rotations(std::marker::PhantomData))
}

pub(crate) fn serialize_rotation_records<R: RotationRecord + Serialize, S: serde::Serializer>(
    records: &BTreeMap<String, R>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if records.len() > MAX_FINALITY_KEY_ROTATION_RECORDS_V1 {
        return Err(serde::ser::Error::custom(
            "rotation history count exceeds bound",
        ));
    }
    for (key, record) in records {
        record
            .validate_record()
            .map_err(serde::ser::Error::custom)?;
        if record.rotation_id().map_err(serde::ser::Error::custom)? != key {
            return Err(serde::ser::Error::custom(
                "rotation history key differs from original body",
            ));
        }
    }
    records.serialize(serializer)
}
pub(crate) struct FinalityKeyRotationHistoryRef<'a>(
    pub &'a BTreeMap<String, FinalityKeyRotationRecordV1>,
);
impl Serialize for FinalityKeyRotationHistoryRef<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serialize_rotation_records(self.0, s)
    }
}
#[cfg(test)]
#[path = "governance_finality_rotation_tests.rs"]
pub(super) mod tests;
