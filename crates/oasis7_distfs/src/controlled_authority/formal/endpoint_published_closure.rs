//! Qualification and independent local closure are distinct facts.
use super::super::coordinator::FormalPublishedClosureReceiptV1;
use super::*;

impl FileFormalEndpoint {
    pub fn release_published_closure(&mut self, candidate_digest: &str) -> Result<(), FormalError> {
        hash_hex(candidate_digest)?;
        let _guard = self.guard()?;
        self.published_closure_jobs.remove(candidate_digest);
        Ok(())
    }
    /// Query authenticates the immutable Qualified entry under a short provider
    /// lease. The paged typed walk retains a live physical pin across calls,
    /// uses registered-local objects only, and reacquires the provider lease to
    /// rebind the exact Qualified entry before issuing a completed receipt.
    pub fn verify_published_closure(
        &mut self,
        request: &str,
        candidate_digest: &str,
    ) -> Result<FormalPublishedClosureReceiptV1, FormalError> {
        #[cfg(test)]
        let started = std::time::Instant::now();
        #[cfg(test)]
        let diagnostics = std::env::var_os("OASIS7_FORMAL_CLOSURE_DIAGNOSTICS").is_some();
        #[cfg(test)]
        let timing = |stage: &str| {
            if diagnostics {
                eprintln!(
                    "published_closure_timing stage={stage} elapsed_us={}",
                    started.elapsed().as_micros()
                );
            }
        };
        hash_hex(candidate_digest)?;
        let FormalQueryOutcomeV1::Recorded(state) = self.query(request)? else {
            return Err(invalid("published closure request unavailable"));
        };
        #[cfg(test)]
        timing("authenticated_query");
        let FormalRequestStateV1::Qualified { candidate, proof } = *state else {
            return Err(invalid("published closure requires exact Qualified state"));
        };
        if candidate.candidate.record.request_digest != request
            || candidate.candidate.record != proof.decision.record
            || blake3::hash(
                &serde_cbor::to_vec(&candidate.candidate)
                    .map_err(|error| invalid(error.to_string()))?,
            )
            .to_hex()
            .as_str()
                != candidate_digest
        {
            return Err(invalid("published closure candidate/proof identity"));
        }
        let proof_digest =
            blake3::hash(&serde_cbor::to_vec(&proof).map_err(|error| invalid(error.to_string()))?)
                .to_hex()
                .to_string();
        if !self.published_closure_jobs.contains_key(candidate_digest) {
            if self.published_closure_jobs.len() >= 8 {
                // Completed receipts remain pinned until evicted. Pending work
                // is never displaced by a newer publication.
                if let Some(done) = self
                    .published_closure_jobs
                    .iter()
                    .find(|(_, job)| job.complete())
                    .map(|(id, _)| id.clone())
                {
                    self.published_closure_jobs.remove(&done);
                } else {
                    return Err(FormalError::Conflict(
                        "published closure job capacity; attempt retained".into(),
                    ));
                }
            }
            let job = store::published_closure::ClosureJob::new(
                &self.store,
                &candidate,
                self.authority.definition().profile_version,
                candidate_digest,
                &proof_digest,
            )?;
            self.published_closure_jobs
                .insert(candidate_digest.into(), job);
        }
        // Acquire this run's physical pin and fresh scratch before any durable
        // local checks. Markers amortize fsync, never content verification.
        let sync_markers = self
            .published_closure_jobs
            .get(candidate_digest)
            .and_then(|job| job.sync_markers());
        let checks = (|| {
            let local = self
                .store
                .read_registered_local_request_with_sync(request, sync_markers.as_deref())?;
            if local
                != (FormalRequestStateV1::Qualified {
                    candidate: candidate.clone(),
                    proof: proof.clone(),
                })
            {
                return Err(invalid(
                    "published local request differs from authenticated query",
                ));
            }
            self.store
                .verify_published_raw_aliases_with_sync(&candidate, sync_markers.as_deref())
        })();
        if let Err(error) = checks {
            self.published_closure_jobs.remove(candidate_digest);
            return Err(error);
        }
        #[cfg(test)]
        timing("raw_aliases");
        let result = self
            .published_closure_jobs
            .get_mut(candidate_digest)
            .ok_or_else(|| invalid("published closure job missing after insertion"))?
            .advance(&self.store, &proof_digest);
        #[cfg(test)]
        timing("advance");
        if result.is_ok() {
            self.published_closure_jobs
                .get(candidate_digest)
                .ok_or_else(|| invalid("published closure job missing after advancement"))?
                .persist_diagnostic(&self.store)?;
        }
        #[cfg(test)]
        timing("diagnostic_persist");
        match result {
            Ok(Some(receipt)) => {
                // Do not hold a provider lease across the potentially long
                // readback job, and do not nest query() under this final lease.
                let _guard = self.guard()?;
                let markers = self
                    .published_closure_jobs
                    .get(candidate_digest)
                    .ok_or_else(|| invalid("published closure final job missing"))?
                    .sync_markers();
                if self
                    .store
                    .read_registered_local_request_with_sync(request, markers.as_deref())?
                    != (FormalRequestStateV1::Qualified { candidate, proof })
                {
                    return Err(invalid("published closure final Qualified binding changed"));
                }
                Ok(receipt)
            }
            Ok(None) => Err(FormalError::Conflict(
                "published closure verification pending; attempt retained".into(),
            )),
            Err(error) => {
                self.published_closure_jobs.remove(candidate_digest);
                Err(error)
            }
        }
    }
}
