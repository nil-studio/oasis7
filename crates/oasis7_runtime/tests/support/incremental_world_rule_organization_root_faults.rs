use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StateArcMode {
    Good,
    Foreign,
    Undercharged,
}
pub(super) struct StateArcReader<'a, R> {
    pub(super) source: &'a R,
    pub(super) foreign: &'a MemoryBudget,
    pub(super) mode: StateArcMode,
    pub(super) supplied: std::cell::Cell<u64>,
}
impl<R: oasis7_formal_protocol::incremental_value::ValueNodeReader>
    oasis7_formal_protocol::incremental_value::ValueNodeReader for StateArcReader<'_, R>
{
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let bytes = self.source.read_node(id, max, memory, class)?;
        if self.mode == StateArcMode::Good || !bytes.starts_with(b"oasis7.iws.state-page.v1\0") {
            return Ok(bytes);
        }
        self.supplied.set(self.supplied.get() + 1);
        let charge = match self.mode {
            StateArcMode::Foreign => self.foreign.reserve(bytes.len(), class),
            StateArcMode::Undercharged => memory.reserve(0, class),
            StateArcMode::Good => unreachable!(),
        }
        .map_err(ValueTreeError::Memory)?;
        Ok(std::sync::Arc::new(
            charge.retain(bytes.as_slice().to_vec()),
        ))
    }
}
pub(super) fn relative_field_key(field: &str) -> WorldFieldKeyV1 {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.extend(pair(vec![0], text(field)));
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&bytes).as_bytes()),
    )
    .unwrap()
}
fn outer_case_key() -> WorldFieldKeyV1 {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.extend(pair(text("organization_cases"), text(CASE)));
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&bytes).as_bytes()),
    )
    .unwrap()
}
pub(super) fn verify_missing_original_pages(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    path: &std::path::Path,
    roots: &RootSetV1,
    inner: &StateRootV1,
    max_payload: u64,
    max_record: u64,
) {
    use oasis7_formal_protocol::incremental_value::{
        VALUE_MAX_NODE_BYTES, ValueNodeReader, ValueNodeWriter, ValuePageLinksV1,
        verified_value_links,
    };
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let auth = roots
        .state
        .iter()
        .find(|root| root.namespace == Ns::Authorization)
        .unwrap();
    let outer_value = auth
        .get(outer_case_key().as_bytes(), &state)
        .unwrap()
        .unwrap();
    let unit_value = inner
        .get(relative_member_key("new-member").as_bytes(), &state)
        .unwrap()
        .unwrap();
    let text_value = inner
        .get(relative_field_key("founder_id").as_bytes(), &state)
        .unwrap()
        .unwrap();
    let mut logical_io = logical();
    let mut context = ValueReadContext {
        source: &values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut logical_io,
    };
    let outer = context.load_root(&outer_value).unwrap().0;
    let unit = context.load_root(&unit_value).unwrap().0;
    let long = context.load_root(&text_value).unwrap().0;
    assert_eq!((outer.depth, unit.depth, long.depth), (0, 0, 1));
    let index_page = values
        .read_node(
            &long.id,
            VALUE_MAX_NODE_BYTES,
            memory,
            MemoryClass::Ordinary,
        )
        .unwrap();
    let ValuePageLinksV1::Index(children) = verified_value_links(&long.id, &index_page).unwrap()
    else {
        panic!("long Text must have data children")
    };
    assert_eq!(children.len(), 3);
    let last = children.last().unwrap();
    assert_eq!(last.depth, 0);
    for (label, id) in [
        ("outer RootRef data", outer.id),
        ("Unit member data", unit.id),
        ("last long Text data", last.id),
    ] {
        let backup = values
            .read_node(&id, VALUE_MAX_NODE_BYTES, memory, MemoryClass::Ordinary)
            .unwrap();
        let recipe = path
            .join("recipes")
            .join(blake3::Hash::from_bytes(id).to_hex().as_str());
        std::fs::remove_file(&recipe).unwrap();
        assert!(!recipe.exists());
        let mut read = WorldFieldReadContextV1 {
            root: auth,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 1024,
            record_bytes: max_record,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let result = qualify_organization_family_v1(
            roots,
            &mut read,
            &mut scan(),
            &mut scan(),
            &mut 100_000,
            max_payload,
        );
        assert!(
            matches!(result,Err(ValueTreeError::Io(ref error)) if error.kind()==std::io::ErrorKind::NotFound),
            "missing {label} must reject with actual NotFound, got {result:?}"
        );
        store
            .value_session(physical())
            .put_node(
                id,
                memory
                    .reserve(backup.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(backup.as_slice().to_vec()),
            )
            .unwrap();
        assert!(recipe.exists());
    }
}

pub(super) struct BatchControl {
    pub cancelled: std::sync::atomic::AtomicBool,
    pub cancel_once: std::sync::atomic::AtomicBool,
    pub direct_puts: std::sync::atomic::AtomicU64,
    pub state_pages: std::sync::atomic::AtomicU64,
}
pub(super) struct BatchObserver<W> {
    pub inner: W,
    pub control: std::sync::Arc<BatchControl>,
}
impl<W> BatchObserver<W> {
    pub fn new(inner: W, cancel: bool) -> Self {
        Self {
            inner,
            control: std::sync::Arc::new(BatchControl {
                cancelled: std::sync::atomic::AtomicBool::new(false),
                cancel_once: std::sync::atomic::AtomicBool::new(cancel),
                direct_puts: std::sync::atomic::AtomicU64::new(0),
                state_pages: std::sync::atomic::AtomicU64::new(0),
            }),
        }
    }
}
impl<W: StatePageWriter> oasis7_formal_protocol::incremental_value::ValueNodeWriter
    for BatchObserver<W>
{
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, bytes)?;
        self.control
            .direct_puts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}
impl<W: StatePageWriter> StatePageWriter for BatchObserver<W> {
    fn put_state_pages_cancelled(
        &mut self,
        delta: &ReservedValue<StateDeltaV1>,
        memory: &MemoryBudget,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        self.inner
            .put_state_pages_cancelled(delta, memory, cancelled)?;
        self.control.state_pages.fetch_add(
            delta.new_nodes.len() as u64,
            std::sync::atomic::Ordering::SeqCst,
        );
        // The real bounded sink has completed durable work. Simulate an
        // external cancellation immediately after its final callback; the
        // owner consumer still has to recheck before publishing its outer ref.
        if self
            .control
            .cancel_once
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            self.control
                .cancelled
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }
}

pub(super) struct DenyValue<'a, R> {
    pub source: &'a R,
    pub denied: [u8; 32],
    pub touched: std::cell::Cell<u64>,
}
impl<R: oasis7_formal_protocol::incremental_value::ValueNodeReader>
    oasis7_formal_protocol::incremental_value::ValueNodeReader for DenyValue<'_, R>
{
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if *id == self.denied {
            self.touched.set(self.touched.get() + 1);
            return Err(ValueTreeError::Invalid("unrelated Text must not be read"));
        }
        self.source.read_node(id, max, memory, class)
    }
}

pub(super) fn change_policy_without_text<
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    proof: &ReservedValue<OrganizationRecordQualificationV1>,
    state: &M,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    max_record: u64,
) -> ReservedValue<OrganizationRecordQualificationV1> {
    let memory = context.values.memory;
    let denied = proof
        .root()
        .get(relative_field_key("founder_id").as_bytes(), state)
        .unwrap()
        .unwrap();
    let deny = DenyValue {
        source: context.values.source,
        denied,
        touched: std::cell::Cell::new(0),
    };
    let result = {
        let mut point_read = WorldFieldReadContextV1 {
            root: proof.root(),
            state,
            values: ValueReadContext {
                source: &deny,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: 1024,
            record_bytes: max_record,
            take_state_error: &mut context.take_state_error,
            cancelled: &mut context.cancelled,
        };
        assert_eq!(
            read_qualified_organization_field_v1(
                OrganizationFieldRequestV1 {
                    owner: OrganizationRootKeyV1::Case(CASE),
                    record: OrganizationRelativeRecordV1::Case,
                    field: OrganizationScalarFieldV1::PolicyVersion,
                    qualification: proof
                },
                &mut point_read,
                0,
                &mut std::io::sink()
            )
            .unwrap(),
            OrganizationFieldReadV1::Value(OrganizationFieldValueV1::U32(u32::MAX))
        );
        let guard = context.transaction.guard();
        let mut point = WorldFieldOverlayContextV1 {
            transaction: &mut *context.transaction,
            values: ValueReadContext {
                source: &deny,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: 1024,
            record_bytes: max_record,
            take_state_error: &mut context.take_state_error,
            cancelled: &mut context.cancelled,
        };
        change_qualified_organization_field_v1(
            OrganizationFieldChangeV1 {
                request: OrganizationFieldRequestV1 {
                    owner: OrganizationRootKeyV1::Case(CASE),
                    record: OrganizationRelativeRecordV1::Case,
                    field: OrganizationScalarFieldV1::PolicyVersion,
                    qualification: proof,
                },
                expected: OrganizationFieldRefV1::U32(u32::MAX),
                after: OrganizationFieldRefV1::U32(0),
                max_current_payload_bytes: 0,
                max_pending_node_bytes: 4 << 20,
            },
            &mut point,
            write,
            &guard,
            live,
        )
        .unwrap()
        .unwrap()
    };
    assert_eq!(
        deny.touched.get(),
        0,
        "numeric field read/write must not scan founder Text"
    );
    result
}
pub(super) fn assert_cold_policy<
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    parent: &ReservedValue<OrganizationFamilyQualificationV1>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) {
    let opened = open_qualified_organization_record_v1(
        OrganizationRootKeyV1::Case(CASE),
        roots,
        parent,
        context,
    )
    .unwrap()
    .unwrap();
    let mut point = WorldFieldReadContextV1 {
        root: opened.root(),
        state: context.state,
        values: ValueReadContext {
            source: context.values.source,
            memory: context.values.memory,
            class: context.values.class,
            io: &mut *context.values.io,
        },
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        take_state_error: &mut context.take_state_error,
        cancelled: &mut context.cancelled,
    };
    assert_eq!(
        read_qualified_organization_field_v1(
            OrganizationFieldRequestV1 {
                owner: OrganizationRootKeyV1::Case(CASE),
                record: OrganizationRelativeRecordV1::Case,
                field: OrganizationScalarFieldV1::PolicyVersion,
                qualification: &opened
            },
            &mut point,
            0,
            &mut std::io::sink()
        )
        .unwrap(),
        OrganizationFieldReadV1::Value(OrganizationFieldValueV1::U32(0))
    );
}
