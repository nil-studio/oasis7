//! Paid persistent response-envelope ordinals. This is only resident metadata;
//! actual point/raw/schema and full artifact equality remain mandatory.
use super::*;
use crate::world::incremental_normal_cognition_service_evidence::index::tree;
pub(super) struct ResponseEnvelopeIndexV1 {
    root: tree::Link,
}
impl ResponseEnvelopeIndexV1 {
    pub(super) fn empty() -> Self {
        Self { root: None }
    }
    pub(super) fn share(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, BodyMutationError> {
        common.consume_steps(1).map_err(original)?;
        Ok(Self {
            root: self.root.as_ref().map(std::sync::Arc::clone),
        })
    }
    fn prefix(
        envelope: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<tree::Key, BodyMutationError> {
        let len = u64::try_from(envelope.len())
            .map_err(|_| bad("response envelope key length"))?
            .to_be_bytes();
        tree::key(&[&[8], &len, envelope.as_bytes()], common)
    }
    pub(super) fn replace_row(
        &mut self,
        ordinal: u64,
        envelope: Option<&str>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let row = tree::key(&[&[9], &ordinal.to_be_bytes()], common)?;
        if let Some(old) = tree::get(&self.root, &row, common)? {
            let tree::Data::Pair(old) = old else {
                return Err(bad("response ordinal reverse key kind"));
            };
            self.root = tree::remove(&self.root, &old, common)?;
            self.root = tree::remove(&self.root, &row, common)?;
        }
        if let Some(envelope) = envelope {
            let prefix = Self::prefix(envelope, common)?;
            let entry = tree::key(&[prefix.as_slice(), &ordinal.to_be_bytes()], common)?;
            self.root = tree::put(&self.root, &entry, tree::Data::Count(ordinal), common)?;
            self.root = tree::put(&self.root, &row, tree::Data::Pair(entry), common)?;
        }
        Ok(())
    }
    pub(super) fn next_ordinal(
        &self,
        envelope: &str,
        after: Option<u64>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<u64>, BodyMutationError> {
        let prefix = Self::prefix(envelope, common)?;
        let key = match after {
            None => std::sync::Arc::clone(&prefix),
            Some(u64::MAX) => return Ok(None),
            Some(at) => tree::key(&[prefix.as_slice(), &(at + 1).to_be_bytes()], common)?,
        };
        let Some((key, data)) = tree::lower_bound(&self.root, &key, common)? else {
            return Ok(None);
        };
        common.charge(1, prefix.len() as u64).map_err(original)?;
        if key.len()
            != prefix
                .len()
                .checked_add(8)
                .ok_or_else(|| bad("response envelope ordinal layout"))?
            || !key.starts_with(prefix.as_slice())
        {
            return Ok(None);
        }
        let tree::Data::Count(ordinal) = data else {
            return Err(bad("response envelope ordinal kind"));
        };
        if key[prefix.len()..] != ordinal.to_be_bytes() {
            return Err(bad("response envelope ordinal suffix"));
        }
        Ok(Some(ordinal))
    }
}
