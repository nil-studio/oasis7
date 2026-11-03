//! Candidate and admission payloads share the same entries, byte reservoir,
//! idle eviction, immutable nonce identity and original deadline.
use super::super::admission_input::{NodeBftAdmissionInputBytesV1, NodeBftAdmissionInputSourceV1};
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputRefV1 as InputRef,
    BftPrivateRetentionAdmissionInputV1 as Input,
};

#[derive(Clone)]
pub(super) struct AdmissionCacheMetadata {
    reference: InputRef,
    retained_owner: Option<Arc<Input>>,
}
impl Cache {
    fn admission_entry(
        &self,
        key: &(String, String),
        reference: &InputRef,
    ) -> Result<Option<Arc<NodeBftAdmissionInputBytesV1>>, String> {
        let Some(entry) = self.entries.get(key) else {
            return Ok(None);
        };
        let (Some(bytes), Some(metadata)) = (&entry.bytes, &entry.admission) else {
            return Err(
                "private admission cache build in progress or original kind differs".into(),
            );
        };
        if metadata.reference != *reference {
            return Err("private cached admission original differs".into());
        }
        Ok(Some(Arc::new(NodeBftAdmissionInputBytesV1 {
            reference: metadata.reference.clone(),
            bytes: bytes.clone(),
            retained_owner: metadata.retained_owner.clone(),
        })))
    }
    fn forget_admission(
        &mut self,
        key: &(String, String),
        original: &NodeBftAdmissionInputBytesV1,
    ) {
        if self
            .entries
            .get(key)
            .and_then(|entry| entry.bytes.as_ref())
            .is_some_and(|bytes| Arc::ptr_eq(bytes, &original.bytes))
        {
            if let Some(entry) = self.entries.remove(key) {
                self.bytes -= entry.reserved;
            }
        }
    }
    fn admission_deadline_for_entry(
        &self,
        key: &(String, String),
        requested: Instant,
    ) -> Result<Instant, String> {
        let deadline = self
            .seen
            .get(key)
            .ok_or("private admission original nonce missing")?
            .deadline
            .min(requested);
        if deadline <= Instant::now() {
            return Err("private admission original deadline expired".into());
        }
        Ok(deadline)
    }
}
impl NodeBftPrivateTransportScopeV1 {
    /// The caller still performs fresh per-chunk issuer/Noise/current-view
    /// authorization and provider response charging around this material lookup.
    pub(crate) fn admission_original(
        &self,
        peer: &str,
        nonce: &str,
        reference: &InputRef,
        source: &dyn NodeBftAdmissionInputSourceV1,
        requested: Instant,
    ) -> Result<Arc<NodeBftAdmissionInputBytesV1>, String> {
        if !self.release_guard().load(Ordering::Acquire) {
            return Err("private admission scope stopped".into());
        }
        let key = (peer.to_owned(), nonce.to_owned());
        let (deadline, cached) = {
            let cache = self.cache.lock().map_err(|_| "private cache poisoned")?;
            (
                cache.admission_deadline_for_entry(&key, requested)?,
                cache.admission_entry(&key, reference)?,
            )
        };
        if source.retains_encoded_original() {
            let original = source.original(reference, deadline)?;
            if original.reference != *reference
                || !original.bytes.belongs_to(&self.buffers())
                || !source.revalidate_original(&original, deadline)?
                || Instant::now() >= deadline
                || !self.release_guard().load(Ordering::Acquire)
            {
                return Err("private publisher original owner/pool/deadline differs".into());
            }
            return Ok(original);
        }
        if let Some(cached) = cached {
            // No cache mutex is held while touching the independently owned
            // backend. Missing/busy/invalid originals cannot use encoded fallback.
            let fresh = source.revalidate_original(&cached, deadline);
            if matches!(&fresh, Ok(true)) {
                if Instant::now() >= deadline
                    || !self.release_guard().load(Ordering::Acquire)
                    || !cached.bytes.belongs_to(&self.buffers())
                {
                    self.cache
                        .lock()
                        .map_err(|_| "private cache poisoned")?
                        .forget_admission(&key, &cached);
                    return Err("private cached admission deadline/pool differs".into());
                }
                return Ok(cached);
            }
            self.cache
                .lock()
                .map_err(|_| "private cache poisoned")?
                .forget_admission(&key, &cached);
            drop(cached);
            // A changed immutable owner is a rebuild, not a claim that the
            // formerly encoded bytes still represent current backend ownership.
            fresh?;
        }
        {
            let mut cache = self.cache.lock().map_err(|_| "private cache poisoned")?;
            cache.admission_deadline_for_entry(&key, deadline)?;
            if cache.entries.contains_key(&key) {
                return Err("private admission cache concurrent build".into());
            }
            // Encoding needs the existing peak reservoir. Reclaim idle payloads
            // from both protocols; active borrows remain backpressure. The source
            // itself owns its real permit before allocation, so never reserve a
            // second peak here (publisher originals are already encoded/charged).
            while cache.entries.values().any(|entry| {
                entry
                    .bytes
                    .as_ref()
                    .is_some_and(|bytes| Arc::strong_count(bytes) == 1)
            }) {
                cache.evict_idle()?;
            }
            cache.make_room()?;
            cache.bytes += MAX_PACKAGE;
            cache.entries.insert(
                key.clone(),
                Cached {
                    bytes: None,
                    manifest: None,
                    reserved: MAX_PACKAGE,
                    admission: None,
                },
            );
        }
        let built: Result<Arc<NodeBftAdmissionInputBytesV1>, String> = (|| {
            let original = source.original(reference, deadline)?;
            if original.reference != *reference
                || !original.bytes.belongs_to(&self.buffers())
                || original.bytes.is_empty()
                || original.bytes.len() > MAX_PACKAGE
            {
                return Err("private admission encoded original/pool differs".into());
            }
            if !source.revalidate_original(&original, deadline)? || Instant::now() >= deadline {
                return Err("private admission original owner changed during encoding".into());
            }
            Ok(original)
        })();
        let mut cache = self.cache.lock().map_err(|_| "private cache poisoned")?;
        let placeholder = cache
            .entries
            .remove(&key)
            .ok_or("private admission cache reservation disappeared")?;
        cache.bytes -= placeholder.reserved;
        let original = built?;
        cache.admission_deadline_for_entry(&key, deadline)?;
        if !self.release_guard().load(Ordering::Acquire) {
            return Err("private admission scope stopped".into());
        }
        cache.bytes += original.bytes.len();
        cache.entries.insert(
            key,
            Cached {
                reserved: original.bytes.len(),
                bytes: Some(original.bytes.clone()),
                manifest: None,
                admission: Some(AdmissionCacheMetadata {
                    reference: original.reference.clone(),
                    retained_owner: original.retained_owner.clone(),
                }),
            },
        );
        Ok(original)
    }
}
#[cfg(test)]
#[path = "admission_input_cache_tests.rs"]
mod tests;
