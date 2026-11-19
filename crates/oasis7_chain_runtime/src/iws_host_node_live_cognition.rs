//! Retain the original Cognition startup accounts across the service lifetime.
//! Ordinary accepted residents use promotion. Only an actual Built child from
//! authenticated cold recovery can repeat the bounded cold admission.
use super::*;

pub struct IwsLiveNodeCognitionV1 {
    pub startup: PaidNormalHostCognitionParentSourcesV1,
    pub io: HostColdIoWorkV1,
    pub work: HostCognitionStartupWorkV1,
}

pub(super) struct CognitionState {
    startup: Option<PaidNormalHostCognitionParentSourcesV1>,
    startup_record: Option<[u8; 32]>,
    io: HostColdIoWorkV1,
    work: HostCognitionStartupWorkV1,
}

impl CognitionState {
    pub(super) fn new(input: IwsLiveNodeCognitionV1, record: [u8; 32]) -> Self {
        Self {
            startup: Some(input.startup),
            startup_record: Some(record),
            io: input.io,
            work: input.work,
        }
    }

    pub(super) fn startup(&self) -> Option<&PaidNormalHostCognitionParentSourcesV1> {
        self.startup.as_ref()
    }

    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(super) fn work(&self) -> (&HostColdIoWorkV1, &HostCognitionStartupWorkV1) {
        (&self.io, &self.work)
    }

    pub(super) fn prepare_next_parent(
        &mut self,
        primary: &IwsHostRuntimeV1,
        applied: &mut PaidNodeAppliedNormalV1,
        context: &NodeExecutionCommitContext,
        environment: &NodeNormalExecutionEnvironmentV1<'_>,
    ) -> Result<(), String> {
        let e = environment;
        let current = primary
            .current
            .current()
            .ok_or_else(|| invalid("live Node next Cognition accepted owner missing"))?;
        let record = current.source.record_id();
        let needs_startup = {
            let mut stop = || e.cancelled.load(Ordering::Acquire);
            let mut common = e.archive.bounded_context(e.memory, &mut stop);
            current
                .source
                .validate_context(&mut common)
                .map_err(|error| error.to_string())?;
            current
                .resident
                .as_ref()
                .ok_or_else(|| invalid("live Node next Cognition resident missing"))?
                .requires_cognition_startup_bounded_v1(record, &current.source.fence(), &mut common)
                .map_err(|error| error.to_string())?
        };
        if !needs_startup {
            // Only the actual Warm variant authorizes ordinary resident use.
            // Old startup evidence is no longer borrowed after publication.
            self.startup = None;
            self.startup_record = None;
            return Ok(());
        }
        if self.startup_record == Some(record) {
            let startup = self
                .startup
                .as_ref()
                .ok_or_else(|| invalid("live Node retained Cold Cognition owner missing"))?;
            primary
                .cognition_parent_sources_bounded(startup, e.memory, e.archive, e.cancelled)
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        // Retry failures keep these exact remaining accounts. Once the child
        // admission succeeds, later completion/hint-copy retries revalidate the
        // resulting paid owner instead of running the admission again.
        let startup = applied
            .prepare_cold_cognition_bounded(
                primary,
                context,
                &mut self.io,
                &mut self.work,
                environment,
            )
            .map_err(|error| error.to_string())?;
        self.startup = Some(startup);
        self.startup_record = Some(record);
        Ok(())
    }
}
