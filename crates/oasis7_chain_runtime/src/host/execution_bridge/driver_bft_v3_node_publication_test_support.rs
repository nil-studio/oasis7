//! Narrow fixtures around actual production publication consumers.
use super::*;
impl FormalBftHeaderNodeDriverV3 {
    /// Test-only failure metadata. Never print actions, World state or arbitrary
    /// error text; known capacity classes and a digest identify opaque failures.
    pub(crate) fn diagnose_candidate_capacity_for_test(
        &mut self,
        context: &NodeBftBuildContextV3,
    ) -> String {
        let preflight = match self.preflight_bft_v3_capacity(context) {
            Ok(admission) => format!("{admission:?}"),
            Err(reason) => diagnostic_error(&reason),
        };
        let plan = match self.exact_parent(&context.execution) {
            Err(reason) => format!("parent:{}", diagnostic_error(&reason)),
            Ok(()) => match serde_cbor::to_vec(&context.actions) {
                Err(_) => "actions_encoding".into(),
                Ok(actions) => match self.execution.origin.plan_candidate(&actions) {
                    Ok((_, capacity)) => format!("{capacity:?}"),
                    Err(reason) => diagnostic_error(&reason),
                },
            },
        };
        format!(
            "cursor={} records={} preflight={preflight} plan={plan}",
            self.cursor_height,
            self.execution.ledger.records().count()
        )
    }

    /// Actual preview material only: no candidate retention, CAS/pin publish,
    /// protected reservation, signature or canonical append.
    pub(crate) fn preview_actual_package_for_test(
        &mut self,
        context: NodeBftBuildContextV3,
    ) -> Result<NodeBftPrivateCandidatePackageV3, String> {
        let candidate = self.build_bft_v3_candidate(context)?;
        let dependency_nodes = self
            .execution
            .origin
            .retained_nodes
            .accessed_nodes()?
            .into_iter()
            .collect();
        Ok(NodeBftPrivateCandidatePackageV3 {
            candidate,
            dependency_nodes,
        })
    }

    pub(crate) fn check_archive_publication_owner_for_test(&self) -> Result<(), String> {
        self.private.check_archive_publication_owner_for_test()
    }

    #[cfg(test)]
    pub(crate) fn retain_publication_integrity_fixture_for_test(
        &mut self,
        expected: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
        reject_budget: bool,
    ) -> Result<String, String> {
        self.validate_candidate(expected, candidate)?;
        let executed = self.execution.origin.execute(&candidate.actions_cbor)?;
        let nodes = self.execution.origin.retained_nodes.accessed_nodes()?;
        let prefix = self
            .execution
            .ledger
            .records()
            .map(|record| record.input().certificate)
            .collect::<Vec<_>>();
        let plan = super::super::super::publication_plan::PublicationPlanV1::from_actual(
            &self.execution.origin.schedule,
            &executed,
            &candidate.actions_cbor,
            &nodes,
            &prefix,
            storage::RECORD_DOMAIN,
        )?;
        self.private
            .retain_planned(candidate, &plan, &nodes, &mut |bytes, future| {
                if reject_budget {
                    return Err("injected publication reserve rejection".into());
                }
                self.execution.ledger.reserve_private_material(
                    bytes,
                    future.clone(),
                    &self.execution.store,
                )
            })?;
        self.private
            .publication_reference(&candidate.public_value)?
            .ok_or("fixture publication reference missing".into())
    }

    #[cfg(test)]
    pub(crate) fn publication_bounds_for_test(
        &mut self,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<(usize, usize, usize, FormalBftCertifiedHeaderV3), String> {
        self.validate_candidate(context, candidate)?;
        let executed = self.execution.origin.execute(&candidate.actions_cbor)?;
        let nodes = self.execution.origin.retained_nodes.accessed_nodes()?;
        let prefix = self
            .execution
            .ledger
            .records()
            .map(|record| record.input().certificate)
            .collect::<Vec<_>>();
        let plan = super::super::super::publication_plan::PublicationPlanV1::from_actual(
            &self.execution.origin.schedule,
            &executed,
            &candidate.actions_cbor,
            &nodes,
            &prefix,
            storage::RECORD_DOMAIN,
        )?;
        Ok((
            plan.record_upper_bytes,
            plan.prefix_upper_bytes,
            plan.publication_bytes,
            plan.encoded_qc,
        ))
    }

    #[cfg(test)]
    pub(crate) fn publication_prefix_codec_bound_for_test(
        prefix: &[FormalBftCertifiedHeaderV3],
        next: &FormalBftCertifiedHeaderV3,
    ) -> Result<usize, String> {
        super::super::super::publication_plan::encoded_prefix_upper_bytes(prefix, next)
    }

    #[cfg(test)]
    pub(crate) fn publication_usage_for_test(
        &self,
    ) -> Result<(usize, usize, usize, usize), String> {
        let (total, external, physical) = self.execution.ledger.publication_usage_for_test();
        let consumed = self
            .execution
            .ledger
            .publication_accounting(&self.execution.store)?
            .total_consumed()?;
        Ok((total, external, physical, consumed))
    }

    #[cfg(test)]
    pub(crate) fn reject_live_capacity_parent_for_test(&self) -> Result<(), String> {
        self.execution
            .origin
            .capacity_in_new_layer(&[0x80])
            .map(|_| ())
    }

    pub(crate) fn replace_record_with_foreign_publication_for_test(
        &self,
        height: u64,
    ) -> Result<(), String> {
        let original = self
            .execution
            .ledger
            .record_at(height)
            .ok_or("test Record missing")?;
        let reference = original
            .publication_reservation_ref()
            .ok_or("test original promise missing")?;
        let bytes = common_storage::read_cas_bounded(
            &self.execution.store,
            reference,
            super::super::super::publication_reservation::MAX_PUBLICATION_METADATA,
        )?;
        let mut fields: serde_cbor::Value =
            serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
        let serde_cbor::Value::Map(ref mut fields) = fields else {
            return Err("test metadata map missing".into());
        };
        fields.insert(
            serde_cbor::Value::Text("instance_id".into()),
            serde_cbor::Value::Text("ff".repeat(32)),
        );
        let metadata: super::super::super::publication_reservation::PublicationReservationV1 =
            serde_cbor::value::from_value(serde_cbor::Value::Map(fields.clone()))
                .map_err(|e| e.to_string())?;
        let bytes = metadata.bytes()?;
        let reference = blake3_hex(&bytes);
        common_storage::put_cas_durable(
            &self.execution.store,
            &bytes,
            super::super::super::publication_reservation::MAX_PUBLICATION_METADATA,
        )?;
        let foreign = original
            .clone()
            .with_publication_reservation(Some(reference))?;
        let bytes = serde_cbor::to_vec(&foreign).map_err(|e| e.to_string())?;
        std::fs::write(
            self.records_root
                .join(storage::DIRECTORY)
                .join(format!("record-{height:020}.cbor")),
            bytes,
        )
        .map_err(|e| e.to_string())?;
        let shard = format!(
            "origin-{}-height-{height:020}",
            self.execution.ledger.origin_digest()?
        );
        self.execution
            .store
            .replace_pin_scope_shard(storage::DIRECTORY, &shard, &foreign.references())
            .map_err(|e| format!("test foreign pin {e:?}"))?;
        Ok(())
    }
}

fn diagnostic_error(reason: &str) -> String {
    let class = if reason.contains("retained index node count exceeds bound") {
        "retained_index_node_capacity"
    } else if reason.contains("retained index bytes exceed bound") {
        "retained_index_byte_capacity"
    } else if reason.contains("requires another dependency window") {
        "content_plan_exceeded"
    } else if reason.starts_with("isolated BFT V2 World execution failed:") {
        "world_execution"
    } else if reason.contains("capacity") || reason.contains("exceeds bound") {
        "other_capacity"
    } else {
        "other_execution"
    };
    format!(
        "{class}(error_bytes={},error_hash={})",
        reason.len(),
        blake3_hex(reason.as_bytes())
    )
}

impl OriginalHeaderIdentityV1 {
    #[cfg(test)]
    pub(crate) fn persist_auto_prepare_for_test(
        &mut self,
        records: &Path,
        binding: &BftSignerBindingV3,
        pin: &str,
        body: oasis7_distfs::protected_bft_v3::BftSuccessorWindowInstallIntentBodyV1,
        manifest: oasis7_distfs::protected_bft_v3::BftSuccessorWindowArchiveManifestV1,
    ) -> Result<(), String> {
        let owner = auto_prepare::AutoPrepareOwnerV1::from_identity(self, pin)?;
        let intent = auto_prepare::AutoPrepareIntentV1::new(owner, binding, body, manifest)?;
        self.verify_unchanged()?;
        let store = self.execution.store.clone();
        intent.persist(records, &store, &mut |prospective_wal| {
            let usage = self
                .private
                .usage_without_auxiliary()?
                .checked_add(prospective_wal)
                .ok_or("auto Prepare same-ledger reserve overflow")?;
            self.execution.ledger.reserve_private_material(
                usage,
                self.private.publication_promises()?,
                &store,
            )
        })?;
        self.verify_unchanged()
    }

    #[cfg(test)]
    pub(crate) fn auto_prepare_accounting_for_test(
        &mut self,
        records: &Path,
    ) -> Result<(usize, usize), String> {
        self.execution.ledger.reserve_private_material(
            self.private.usage()?,
            self.private.publication_promises()?,
            &self.execution.store,
        )?;
        Ok((
            self.execution.ledger.publication_usage_for_test().0,
            auto_prepare::retained_usage(
                records,
                &self.execution.store,
                &self.material_identity,
                self.private.instance_id(),
            )?,
        ))
    }
}

// Test-only remote package construction from the actual authenticated parent.
// It neither selects a local proposal nor persists any received material.
impl FormalBftHeaderNodeDriverV3 {
    pub(crate) fn preview_received_package_for_test(
        &mut self,
        context: &NodeBftExecutionContextV3,
        actions_cbor: &[u8],
    ) -> Result<oasis7_node::NodeBftPrivateCandidatePackageV3, String> {
        self.exact_parent(context)?;
        let actual = self.execution.origin.build_candidate(actions_cbor)?;
        let candidate = oasis7_node::NodeBftPrivateCandidateV3 {
            public_value: oasis7_consensus::tendermint_v3::BftHeaderValueV3 {
                header: actual.header,
                execution_commitment: actual.commitment,
            },
            actions_cbor: actions_cbor.to_vec(),
        };
        // Restore/re-execute exactly as receiver preview does; do not reuse a
        // preview from another candidate or make its nodes a parent authority.
        self.validate_candidate(context, &candidate)?;
        let dependency_nodes =
            self.execution
                .origin
                .retained_nodes
                .with_accessed_nodes(|nodes| {
                    Ok(nodes
                        .iter()
                        .map(|(digest, bytes)| (digest.clone(), bytes.clone()))
                        .collect())
                })?;
        Ok(oasis7_node::NodeBftPrivateCandidatePackageV3 {
            candidate,
            dependency_nodes,
        })
    }
}
