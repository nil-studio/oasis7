//! GEN-only auxiliary rows from the same immutable World borrow as physical
//! preparation. Each original typed field uses the original JSON codec; no
//! Snapshot, executable World clone or aggregate World serialization is made.
use super::*;
use oasis7_world_projection::current_commitment::ProjectionDataPathRefV1 as Path;
fn graph_error<E: std::error::Error + Send + Sync + 'static>(
    e: ProjectionColumnErrorV1<E>,
) -> BodyMutationError {
    match e {
        ProjectionColumnErrorV1::Work(e) => paid(e),
        ProjectionColumnErrorV1::Memory(e) => ValueTreeError::Memory(e).into(),
        ProjectionColumnErrorV1::Invalid(e) => invalid(e),
    }
}
#[derive(Debug)]
struct CodecCause {
    error: serde_json::Error,
    _phase: MemoryReservation,
}
impl std::fmt::Display for CodecCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for CodecCause {}
fn data<T: serde::Serialize + ?Sized>(
    value: &T,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ProjectionColumnDataV1, BodyMutationError> {
    let bytes = capture(common, |out| {
        serde_json::to_writer(out, value).map_err(paid)
    })?;
    let n = bytes
        .len()
        .checked_mul(1024)
        .and_then(|n| n.checked_add(32768))
        .ok_or_else(|| invalid("GEN auxiliary original graph layout"))?;
    common.charge(bytes.len() as u64, n as u64).map_err(paid)?;
    let phase = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let parsed = serde_json::from_slice(&bytes);
    common.consume_steps(0).map_err(paid)?;
    let parsed = match parsed {
        Ok(value) => value,
        Err(error) => {
            return Err(paid(CodecCause {
                error,
                _phase: phase,
            }));
        }
    };
    let memory = common.memory().clone();
    ProjectionColumnDataV1::from_json_data(parsed, &memory, &mut |s, b| common.charge(s, b))
        .map_err(graph_error)
}
impl NormalProjectionColumnInitializerV1<'_, '_> {
    pub(in crate::world) fn capture_original_auxiliary_fields_v1(
        &mut self,
        world: &crate::World,
    ) -> Result<(), BodyMutationError> {
        self.common.consume_steps(1).map_err(paid)?;
        if self.auxiliary.is_some() {
            return Err(invalid("duplicate GEN auxiliary capture"));
        }
        let memory = self.common.memory().clone();
        let mut graph = ProjectionColumnDataV1::from_json_data(
            serde_json::Value::Object(Default::default()),
            &memory,
            &mut |s, b| self.common.charge(s, b),
        )
        .map_err(graph_error)?;
        macro_rules! field {
            ($name:literal,$value:expr) => {{
                let value = data($value, self.common)?;
                graph = graph
                    .with_object_member_data($name, Some(&value), &memory, &mut |s, b| {
                        self.common.charge(s, b)
                    })
                    .map_err(graph_error)?;
            }};
        }
        field!("manifest", &world.manifest);
        field!("chain_resource_manifest", &world.chain_resource_manifest);
        field!("module_registry", &world.module_registry);
        field!("capability_grants_v2", &world.capability_grants_v2);
        field!(
            "capability_revocation_state",
            &world.capability_revocation_state
        );
        field!(
            "capability_invocation_contexts",
            &world.capability_invocation_contexts
        );
        field!(
            "capability_authorization_root",
            &world.capability_authorization_root
        );
        field!("policies", &world.policies);
        field!("next_event_id", &world.next_event_id);
        field!("event_id_era", &world.next_event_id_era);
        field!(
            "governance_execution_policy",
            &world.governance_execution_policy
        );
        field!(
            "governance_finality_epoch_snapshots",
            &world.governance_finality_epoch_snapshots
        );
        field!(
            "governance_identity_penalties",
            &world.governance_identity_penalties
        );
        field!(
            "governance_emergency_brake_until_tick",
            &world.governance_emergency_brake_until_tick
        );
        field!(
            "tick_consensus_authority_source",
            &world.tick_consensus_authority_source
        );
        // Internal service DATA is keyed by the real outer proposal key. It is
        // not a substitute Proposal, authorization root, or Snapshot field.
        let mut statuses = ProjectionColumnDataV1::from_json_data(
            serde_json::Value::Object(Default::default()),
            &memory,
            &mut |s, b| self.common.charge(s, b),
        )
        .map_err(graph_error)?;
        for (id, proposal) in &world.proposals {
            self.common.charge(1, 20).map_err(paid)?;
            let _key_fee = memory
                .reserve(20, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let key = id.to_string();
            let hash = match &proposal.status {
                oasis7_world_state::ProposalStatus::Approved { manifest_hash, .. }
                | oasis7_world_state::ProposalStatus::Applied { manifest_hash } => {
                    Some(manifest_hash.as_str())
                }
                _ => None,
            };
            let value = data(&hash, self.common)?;
            statuses = statuses
                .with_object_member_data(&key, Some(&value), &memory, &mut |s, b| {
                    self.common.charge(s, b)
                })
                .map_err(graph_error)?;
        }
        graph = graph
            .with_object_member_data(
                "proposal_statuses",
                Some(&statuses),
                &memory,
                &mut |s, b| self.common.charge(s, b),
            )
            .map_err(graph_error)?;
        field!("world_journal", &world.journal.events);
        // Cognition's original envelope names are retained. Metadata enabled/
        // revision are added later from the actual accepted History manifest.
        field!("cognition", &world.cognition);
        graph=crate::world::incremental_normal_projection_runtime::initialize_revocation_set_projection_v1(&graph,self.common)?;
        self.auxiliary = Some(graph);
        Ok(())
    }
}
impl NormalProjectionColumnSeedDataV1 {
    /// Borrowed actual GEN data only. Cold producers have to emit their own
    /// authenticated full auxiliary graph; absence cannot become an empty one.
    pub(in crate::world) fn auxiliary_columns_data_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ProjectionColumnDataV1, BodyMutationError> {
        self.validate_context(common)?;
        let source = self
            .auxiliary
            .as_ref()
            .ok_or_else(|| invalid("actual bootstrap auxiliary columns missing"))?;
        let memory = common.memory().clone();
        source
            .share_data(&memory, &mut |s, b| common.charge(s, b))
            .map_err(graph_error)
    }
    pub(in crate::world) fn observation_columns_data_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ProjectionColumnDataV1, BodyMutationError> {
        self.validate_context(common)?;
        if self.original_state_declared_fields().is_none() {
            return Err(invalid("actual bootstrap State emissions missing"));
        }
        let memory = common.memory().clone();
        let mut graph = ProjectionColumnDataV1::from_json_data(
            serde_json::Value::Object(Default::default()),
            &memory,
            &mut |s, b| common.charge(s, b),
        )
        .map_err(graph_error)?;
        for field in self.original_state_emissions() {
            common
                .charge(
                    VISIBLE_STATE_COLUMNS_V1.len() as u64,
                    (field.name().len() * VISIBLE_STATE_COLUMNS_V1.len()) as u64,
                )
                .map_err(paid)?;
            if VISIBLE_STATE_COLUMNS_V1.contains(&field.name()) {
                graph = graph
                    .with_path_data_v1(
                        &[Path::Field(field.name())],
                        Some(field.value()),
                        &memory,
                        &mut |s, b| common.charge(s, b),
                    )
                    .map_err(graph_error)?;
            }
        }
        Ok(graph)
    }
}
