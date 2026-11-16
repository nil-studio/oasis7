//! Reborrow a paid Root meter for the factory phase external lane adapter.
use super::*;

impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn with_factory_prologue_external_meter_v1<T, X, Run>(
        &mut self,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
        external: &mut X,
        run: Run,
    ) -> Result<T, RootModuleExecutionErrorV1<O::Error>>
    where
        T: 'static,
        X: FactoryPhaseMeterLanesV1,
        Run: for<'b> FnOnce(
            &mut ReborrowedRoot<'b, 's, M, R, W, O, C>,
            &mut (dyn FnMut() -> ExecutionFence + 'b),
            &mut X,
        ) -> Result<T, RootModuleExecutionErrorV1<O::Error>>,
    {
        paid_inner_external(
            self,
            common,
            live_owner,
            None,
            None,
            None,
            Some(external),
            |root, live, _, _, _, external| {
                run(
                    root,
                    live,
                    external.ok_or_else(|| root_invalid("factory phase external lanes missing"))?,
                )
            },
        )
    }
}
