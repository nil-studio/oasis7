//! Publication-bound, retryable reclamation under the writer lease.
use super::*;
enum AttemptReclaimProgress {
    Complete,
    Pending,
    Yield,
}
impl<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint> FormalWriter<P, R> {
    pub(crate) fn advance_storage_maintenance_result(&mut self) -> Result<bool, String> {
        if !self.maintenance_session_started {
            self.attempt_scan = None;
            self.attempt_scan_blocked = None;
            self.maintenance_session_started = true;
        }
        if self.attempt_scan.is_none() && self.attempt_reclaim_current.is_none() {
            self.attempt_scan_blocked = None;
        }
        self.advance_storage_maintenance_inner();
        if let Some(error) = &self.attempt_scan_blocked {
            return Err(error.clone());
        }
        Ok(self.attempt_scan.is_none() && self.attempt_reclaim_current.is_none())
    }
    pub(crate) fn advance_storage_maintenance(&mut self) {
        // A live polling step ends the previous stopped observation session.
        self.maintenance_session_started = false;
        self.advance_storage_maintenance_inner();
    }
    fn advance_storage_maintenance_inner(&mut self) {
        let position = self.anchor.head().record().position;
        let timing = super::super::formal_execution_timing::FormalExecutionTiming::begin(
            "maintenance",
            position,
        );
        match timing.step("read_current_publication", None, || {
            read_published_candidate_at(&self.root, position)
        }) {
            Ok(Some((candidate, proof))) => self.reclaim_published_attempts(&Published {
                schema_version: 1,
                candidate,
                proof,
            }),
            Ok(None) => {
                if !matches!(
                    self.anchor.head().record().kind,
                    FormalRecordKindV1::InitialActivation
                ) {
                    self.attempt_scan_blocked = Some(
                        "current publication missing; maintenance cannot prove convergence".into(),
                    );
                }
            }
            Err(error) => {
                self.attempt_scan_blocked = Some(error.clone());
                eprintln!("formal-attempt maintenance blocked reason={error}");
            }
        }
    }
    /// Retention failure does not change the already qualified result. The exact
    /// source delete list stays pinned and the next open/resume/ack retries it.
    pub(super) fn reclaim_published_attempts(&mut self, expected: &Published) {
        // Incremental discovery is only a scheduling cursor, never deletion
        // authority. Reopening restarts the directory scan and every selected
        // attempt is rebound to its original durable publication and both P/R.
        // Finish one live paged job before discovering more work. Continuous
        // business publications must not dilute an older job's progress. This
        // path is scheduling only; every page rebinds durable source authority.
        if let Some(path) = self.attempt_reclaim_current.take() {
            self.advance_attempt_path(expected, &path);
            return;
        }
        if self.attempt_scan.is_none() {
            match std::fs::read_dir(self.root.join("attempts")) {
                Ok(scan) => self.attempt_scan = Some(scan),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
                Err(e) => {
                    self.attempt_scan_blocked = Some(e.to_string());
                    eprintln!("formal-attempt inventory blocked reason={e}");
                    return;
                }
            }
        }
        for _ in 0..16 {
            let Some(next) = self.attempt_scan.as_mut().and_then(Iterator::next) else {
                self.attempt_scan = None;
                break;
            };
            let path = match next {
                Ok(entry) => entry.path(),
                Err(e) => {
                    self.attempt_scan_blocked = Some(e.to_string());
                    eprintln!("formal-attempt inventory blocked reason={e}");
                    break;
                }
            };
            if self.advance_attempt_path(expected, &path) {
                break;
            }
        }
    }
    // Return whether this path consumed the tick's single closure page.
    fn advance_attempt_path(&mut self, expected: &Published, path: &Path) -> bool {
        let position = match super::super::formal_attempt_lifecycle::pending_position(path) {
            Ok(Some(position)) if position <= expected.candidate.candidate.record.position => {
                position
            }
            Ok(_) => return false,
            Err(error) => {
                self.attempt_scan_blocked = Some(error.clone());
                eprintln!("formal-attempt inventory blocked reason={error}");
                return false;
            }
        };
        let original = if position == expected.candidate.candidate.record.position {
            None
        } else {
            match read_published_candidate_at(&self.root, position) {
                Ok(Some((candidate, proof))) => Some(Published {
                    schema_version: 1,
                    candidate,
                    proof,
                }),
                Ok(None) => {
                    self.attempt_scan_blocked =
                        Some("attempt original publication unavailable".into());
                    return false;
                }
                Err(error) => {
                    self.attempt_scan_blocked = Some(error.clone());
                    eprintln!(
                        "formal-attempt publication blocked position={position} reason={error}"
                    );
                    return false;
                }
            }
        };
        let publication = original.as_ref().unwrap_or(expected);
        if self.reclaim_one_attempt_publication(publication, path) {
            self.attempt_reclaim_current = Some(path.to_owned());
        }
        true
    }
    fn reclaim_one_attempt_publication(
        &mut self,
        expected: &Published,
        attempt_path: &Path,
    ) -> bool {
        let timing = super::super::formal_execution_timing::FormalExecutionTiming::begin(
            "reclaim_attempt",
            expected.candidate.candidate.record.position,
        );
        let error = match timing.step(
            "attempt_page",
            Some(&expected.candidate.candidate.record.request_digest),
            || self.try_reclaim_published_attempts(expected, attempt_path),
        ) {
            Ok(progress) => {
                if matches!(progress, AttemptReclaimProgress::Yield) {
                    self.attempt_scan_blocked =
                        Some("attempt closure capacity; retry required".into());
                }
                return matches!(progress, AttemptReclaimProgress::Pending);
            }
            Err(error) => error,
        };
        self.attempt_scan_blocked = Some(error.clone());
        if let Ok(digest) =
            super::super::formal_attempt_lifecycle::candidate_digest(&expected.candidate)
        {
            let _ = super::super::formal_attempt_lifecycle::mark(
                attempt_path,
                &digest,
                super::super::formal_attempt_lifecycle::Phase::RecoveryPinned,
                &error,
            );
        }
        eprintln!(
            "formal-attempt-reclamation blocked position={} reason={error}",
            expected.candidate.candidate.record.position
        );
        false // A failed/unknown original yields to other durable attempts.
    }
    fn try_reclaim_published_attempts(
        &mut self,
        expected: &Published,
        attempt_path: &Path,
    ) -> Result<AttemptReclaimProgress, String> {
        let position = expected.candidate.candidate.record.position;
        let timing = super::super::formal_execution_timing::FormalExecutionTiming::begin(
            "closure_page",
            position,
        );
        let path = self
            .root
            .join("published")
            .join(format!("{position:020}.cbor"));
        let bytes = timing.step("read_publication", None, || {
            crate::controlled_history_cli::read_bounded(&path, 64 * 1024 * 1024)
        })?;
        // No attempt path or cached deserialization participates in this read.
        let actual: Published =
            timing.step("decode_publication", None, || published::decode(&bytes))?;
        timing.material(
            "published_material",
            &actual.candidate.candidate.record.request_digest,
            || {
                (
                    actual.candidate.material.objects.len(),
                    actual
                        .candidate
                        .material
                        .objects
                        .iter()
                        .map(|object| object.bytes.len())
                        .sum(),
                )
            },
        );
        if to_cbor(&actual)? != to_cbor(expected)?
            || actual.candidate.candidate.record.position > self.anchor.head().record().position
            || actual.proof.decision.record != actual.candidate.candidate.record
        {
            return Err("attempt destination publication identity mismatch".into());
        }
        timing
            .step(
                "verify_record_material",
                Some(&actual.candidate.candidate.record.request_digest),
                || {
                    verify_record_material(
                        &actual.candidate.candidate.record,
                        &actual.candidate.material,
                    )
                },
            )
            .map_err(|e| e.to_string())?;
        let digest = super::super::formal_attempt_lifecycle::candidate_digest(&actual.candidate)?;
        let request = &actual.candidate.candidate.record.request_digest;
        let publication_digest = blake3::hash(&bytes).to_hex().to_string();
        if super::super::formal_attempt_lifecycle::release_pending(
            attempt_path,
            &actual.candidate,
            &publication_digest,
        )? {
            #[cfg(all(test, unix))]
            super::super::formal_crash_test_boundary::attempt_release_boundary(
                "release_before_primary",
                &actual.candidate,
            )?;
            let (primary, replica) = self.coordinator.endpoints_mut();
            primary
                .release_published_closure(&digest)
                .map_err(|e| e.to_string())?;
            #[cfg(all(test, unix))]
            super::super::formal_crash_test_boundary::attempt_release_boundary(
                "release_after_primary",
                &actual.candidate,
            )?;
            replica
                .release_published_closure(&digest)
                .map_err(|e| e.to_string())?;
            #[cfg(all(test, unix))]
            super::super::formal_crash_test_boundary::attempt_release_boundary(
                "release_replica_reply_lost",
                &actual.candidate,
            )?;
            super::super::formal_attempt_lifecycle::finish_release(
                attempt_path,
                &actual.candidate,
                &publication_digest,
            )?;
            return Ok(AttemptReclaimProgress::Complete);
        }
        super::super::formal_attempt_lifecycle::mark(
            attempt_path,
            &digest,
            super::super::formal_attempt_lifecycle::Phase::QualifiedVerified,
            "qualified proof and exact published envelope durable; P/R closure verification pending",
        )?;
        let (primary, replica) = self.coordinator.endpoints_mut();
        let p = match timing.step("primary_verify_closure", Some(&digest), || {
            primary.verify_published_closure(request, &digest)
        }) {
            Ok(receipt) => receipt,
            Err(FormalError::Conflict(reason))
                if reason.starts_with("published closure verification pending") =>
            {
                return Ok(AttemptReclaimProgress::Pending);
            }
            Err(FormalError::Conflict(reason))
                if reason.starts_with("published closure job capacity") =>
            {
                return Ok(AttemptReclaimProgress::Yield);
            }
            Err(error) => return Err(error.to_string()),
        };
        let r = match timing.step("replica_verify_closure", Some(&digest), || {
            replica.verify_published_closure(request, &digest)
        }) {
            Ok(receipt) => receipt,
            Err(FormalError::Conflict(reason))
                if reason.starts_with("published closure verification pending") =>
            {
                return Ok(AttemptReclaimProgress::Pending);
            }
            Err(FormalError::Conflict(reason))
                if reason.starts_with("published closure job capacity") =>
            {
                return Ok(AttemptReclaimProgress::Yield);
            }
            Err(error) => return Err(error.to_string()),
        };
        if p != r
            || p.candidate_digest != digest
            || p.qualified_proof_digest != blake3::hash(&to_cbor(&actual.proof)?).to_hex().as_str()
        {
            return Err("attempt independent destination closures differ".into());
        }
        let complete = timing.step("reclaim_files", Some(&digest), || {
            super::super::formal_attempt_lifecycle::reclaim(
                attempt_path,
                &actual.candidate,
                blake3::hash(&bytes).to_hex().as_str(),
                &[p.inventory_digest, r.inventory_digest],
            )
        })?;
        if !complete {
            return Ok(AttemptReclaimProgress::Pending);
        }
        #[cfg(all(test, unix))]
        super::super::formal_crash_test_boundary::attempt_release_boundary(
            "release_before_primary",
            &actual.candidate,
        )?;
        timing
            .step("primary_release_closure", Some(&digest), || {
                primary.release_published_closure(&digest)
            })
            .map_err(|e| e.to_string())?;
        #[cfg(all(test, unix))]
        super::super::formal_crash_test_boundary::attempt_release_boundary(
            "release_after_primary",
            &actual.candidate,
        )?;
        timing
            .step("replica_release_closure", Some(&digest), || {
                replica.release_published_closure(&digest)
            })
            .map_err(|e| e.to_string())?;
        #[cfg(all(test, unix))]
        super::super::formal_crash_test_boundary::attempt_release_boundary(
            "release_replica_reply_lost",
            &actual.candidate,
        )?;
        super::super::formal_attempt_lifecycle::finish_release(
            attempt_path,
            &actual.candidate,
            &publication_digest,
        )?;
        Ok(AttemptReclaimProgress::Complete)
    }
}
