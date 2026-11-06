//! Full closed authorization proof without constructing business DTOs.
use super::*;
use crate::incremental_record_tree::RecordTreeLabelsV1;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_columns_read::{ColumnCellV1, ColumnReadProfileV1, read_column_row_v1},
    incremental_record_tree as tree,
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
};
use oasis7_formal_protocol::incremental_state::StateEntryV1;
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "auth stream read",
    pending: "auth stream pending",
    tree: "auth stream tree",
    digest: "auth stream digest",
    read_allowance: "auth stream read allowance",
    admission: "auth stream admission",
    bound: "auth stream bound",
    producer_pool: "auth stream pool",
    page: "auth stream page",
    write_allowance: "auth stream write allowance",
};
fn value_profile(codec: MemberPayloadCodecV1) -> RecordReadProfileV1 {
    RecordReadProfileV1 {
        codec,
        nullable: false,
        integer_max: u64::MAX,
        labels: RecordReadLabelsV1 {
            utf8: "auth stream UTF8",
            ledger_utf8: "auth stream unused",
            ledger_frame: "auth stream unused",
            boolean: "auth stream boolean",
            fixed: "auth stream fixed",
            profile: "auth stream profile",
            integer: "auth stream integer",
        },
    }
}
struct Probe<'a, 'b, 'v, R, C> {
    root: &'a StateRootV1,
    kind: AuthorizationBodyKindV1,
    records: &'a mut InventoryValueReader<'b, 'v, R, C>,
    work: &'a mut JsonReadWorkV1,
    known: Vec<Option<[u8; 32]>>,
}
impl<R: ValueNodeReader, C: FnMut() -> bool> Probe<'_, '_, '_, R, C> {
    fn spend(&mut self) -> Result<(), ValueTreeError> {
        if (self.records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        self.work.cells_remaining = self
            .work
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("auth stream cells"))?;
        Ok(())
    }
    fn point<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
    ) -> Result<(WorldFieldKeyV1, u64, [u8; 32]), ValueTreeError> {
        self.spend()?;
        let (key, n) =
            self.records
                .key(self.kind.descriptor(), WorldFieldRecordKindV1::Entry, key)?;
        let id = tree::lookup(self.root, &key, self.records, LABELS)?.ok_or_else(invalid)?;
        if self.known.len() == self.known.capacity() {
            return Err(invalid());
        }
        self.known.push(
            WorldFieldKeyV1::from_bytes(key.as_bytes())
                .map_err(ValueTreeError::Invalid)?
                .semantic_key,
        );
        Ok((key, n, id))
    }
    fn scalar<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
        codec: MemberPayloadCodecV1,
        max: u64,
        out: &mut impl std::io::Write,
    ) -> Result<RecordValueV1, ValueTreeError> {
        let (key, n, id) = self.point(key)?;
        self.records.values.verify_closure(
            &id,
            self.records.record_bytes,
            &mut *self.records.cancelled,
        )?;
        read_record_value_v1(&id, &(key, n), self.records, max, out, value_profile(codec))
    }
    fn text(&mut self, key: &str) -> Result<(), ValueTreeError> {
        let max = self.records.record_bytes;
        match self.scalar(&key, MemberPayloadCodecV1::Text, max, &mut std::io::sink())? {
            RecordValueV1::Text { .. } => Ok(()),
            _ => Err(invalid()),
        }
    }
    fn unsigned<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
    ) -> Result<u64, ValueTreeError> {
        match self.scalar(key, MemberPayloadCodecV1::U64, 8, &mut std::io::sink())? {
            RecordValueV1::Unsigned(n) => Ok(n),
            _ => Err(invalid()),
        }
    }
    fn signed(&mut self, key: &str) -> Result<(), ValueTreeError> {
        match self.scalar(&key, MemberPayloadCodecV1::I64, 8, &mut std::io::sink())? {
            RecordValueV1::Signed(_) => Ok(()),
            _ => Err(invalid()),
        }
    }
    fn present(&mut self, key: &str) -> Result<bool, ValueTreeError> {
        match self.scalar(
            &(key, "present"),
            MemberPayloadCodecV1::Bool,
            1,
            &mut std::io::sink(),
        )? {
            RecordValueV1::Bool(v) => Ok(v),
            _ => Err(invalid()),
        }
    }
    fn optional(&mut self, key: &str, codec: MemberPayloadCodecV1) -> Result<(), ValueTreeError> {
        if self.present(key)? {
            let max = if codec == MemberPayloadCodecV1::Text {
                self.records.record_bytes
            } else {
                8
            };
            self.scalar(&(key, "value"), codec, max, &mut std::io::sink())?;
        } else {
            self.spend()?;
            let (key, _) = self.records.key(
                self.kind.descriptor(),
                WorldFieldRecordKindV1::Entry,
                &(key, "value"),
            )?;
            if tree::lookup(self.root, &key, self.records, LABELS)?.is_some() {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn json(&mut self, key: &str) -> Result<(), ValueTreeError> {
        let (key, _, id) = self.point(&key)?;
        let entry = StateEntryV1 {
            namespace: self.root.namespace,
            key: key.as_bytes().to_vec(),
            value: id,
        };
        let row = read_column_row_v1(
            ColumnReadProfileV1 {
                field: self.kind.descriptor(),
                schema: self.kind.schema(),
                groups: &[],
                reference_schema: None,
            },
            &entry,
            self.records,
            16 << 10,
        )?;
        let ColumnCellV1::Root(root) = &row.cell else {
            return Err(invalid());
        };
        crate::incremental_json_records::qualify_json_subtree_records_v1(
            crate::incremental_json_records::JsonRecordProfileV1 {
                field: self.kind.descriptor(),
                schema: self.kind.schema(),
            },
            root,
            self.records,
            self.work,
        )?;
        Ok(())
    }
    fn audit(&mut self) -> Result<u64, ValueTreeError> {
        for key in [
            "receipt_id",
            "scope_hash",
            "decision",
            "branch_id",
            "finality_status",
            "state_hash_before",
            "canonical_request_hash",
            "canonical_result_hash",
        ] {
            self.text(key)?;
        }
        for key in [
            "root_receipt_id",
            "grant_id",
            "module_id",
            "module_version",
            "manifest_hash",
            "catalog_snapshot_id",
            "response_nonce",
            "authorization_nonce_key_hash",
            "denial_code",
            "finality_block_hash",
            "state_hash_after",
            "committed_effect_receipt_id",
        ] {
            self.optional(key, MemberPayloadCodecV1::Text)?;
        }
        self.signed("budget_before")?;
        self.optional("budget_after", MemberPayloadCodecV1::I64)?;
        self.unsigned(&"world_head_before")?;
        self.optional("world_head_after", MemberPayloadCodecV1::U64)?;
        self.unsigned(&"finality_epoch")?;
        self.json("subject")?;
        self.json("audience")?;
        if self.present("presenter")? {
            self.json("presenter")?;
        }
        self.unsigned(&("committed_effect_receipt_ids", "count"))
    }
    fn subject(&mut self) -> Result<(), ValueTreeError> {
        let start = self.known.len();
        let mut bytes = [0u8; 8];
        let mut cursor = std::io::Cursor::new(bytes.as_mut_slice());
        self.scalar(&"subject.kind", MemberPayloadCodecV1::Text, 8, &mut cursor)?;
        let n = cursor.position() as usize;
        match &bytes[..n] {
            b"agent" => {
                self.text("subject.agent_id")?;
                self.text("subject.owner_binding")?;
                self.unsigned(&"subject.generation")?;
            }
            b"module" => {
                self.text("subject.module_id")?;
                self.text("subject.module_version")?;
                self.text("subject.instance_id")?;
            }
            b"system" => {
                self.text("subject.system_id")?;
                self.unsigned(&"subject.epoch")?;
            }
            _ => return Err(invalid()),
        }
        for name in [
            "subject.agent_id",
            "subject.owner_binding",
            "subject.generation",
            "subject.module_id",
            "subject.module_version",
            "subject.instance_id",
            "subject.system_id",
            "subject.epoch",
        ] {
            self.spend()?;
            let (key, _) =
                self.records
                    .key(self.kind.descriptor(), WorldFieldRecordKindV1::Entry, &name)?;
            if !self.known[start..].contains(
                &WorldFieldKeyV1::from_bytes(key.as_bytes())
                    .map_err(ValueTreeError::Invalid)?
                    .semantic_key,
            ) && tree::lookup(self.root, &key, self.records, LABELS)?.is_some()
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn invocation(&mut self) -> Result<(), ValueTreeError> {
        self.subject()?;
        for key in [
            "grant_id",
            "presenter.presenter_id",
            "presenter.presenter_kind",
            "audience.world_id",
            "audience.branch_id",
            "audience.target_kind",
            "catalog_snapshot_id",
            "module_id",
            "module_version",
            "response_nonce",
        ] {
            self.text(key)?;
        }
        self.unsigned(&"audience.finality_epoch")?;
        for key in [
            "presenter.session_id",
            "presenter.attestation_ref",
            "audience.target_id",
        ] {
            self.optional(key, MemberPayloadCodecV1::Text)?;
        }
        Ok(())
    }
}
#[path = "incremental_runtime_authorization_stream_set.rs"]
mod set;
pub(super) fn qualify<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: AuthorizationBodyKindV1,
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    json: &mut JsonReadWorkV1,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    root.validate().map_err(|_| invalid())?;
    if root.namespace != StateNamespaceV1::Authorization {
        return Err(invalid());
    }
    // The known column set has a fixed schema bound; no source-sized staging.
    let _scratch = ctx.values.memory.reserve(64 << 10, ctx.values.class)?;
    let mut records = ctx.records();
    let metadata = WorldFieldKeyV1::new(kind.descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let id = tree::lookup(root, &metadata, &mut records, LABELS)?.ok_or_else(invalid)?;
    records
        .values
        .verify_closure(&id, records.record_bytes, &mut *records.cancelled)?;
    records.presence(&id, &metadata, 0)?;
    let mut p = Probe {
        root,
        kind,
        records: &mut records,
        work: json,
        known: Vec::with_capacity(96),
    };
    p.known.push(None);
    let set_count = match kind {
        AuthorizationBodyKindV1::Audit => p.audit()?,
        AuthorizationBodyKindV1::Invocation => {
            p.invocation()?;
            0
        }
    };
    let known = p.known;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut actual = 0u64;
    let mut dynamic = 0u64;
    let result = StateScanContext {
        source: ctx.state,
        memory: ctx.values.memory,
        class: ctx.values.class,
        io: scan,
    }
    .visit_entries(
        root,
        |entry| {
            json.cells_remaining = json
                .cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("auth stream rows"))?;
            let key = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            let canonical = WorldFieldKeyV1::new(kind.descriptor(), key.kind, key.semantic_key)
                .map_err(ValueTreeError::Invalid)?;
            if canonical.as_bytes() != entry.key
                || entry.namespace != root.namespace
                || !matches!(
                    key.kind,
                    WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Entry
                )
            {
                return Err(invalid());
            }
            actual = actual.checked_add(1).ok_or_else(invalid)?;
            if known.contains(&key.semantic_key) {
                return Ok(());
            }
            if !matches!(kind, AuthorizationBodyKindV1::Audit) {
                return Err(invalid());
            }
            let mut stop = || (cancel.borrow_mut())();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut stop,
            };
            set::validate(entry, &mut records)?;
            dynamic = dynamic.checked_add(1).ok_or_else(invalid)?;
            Ok(())
        },
        || (cancel.borrow_mut())(),
    );
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(e) = (ctx.take_state_error)() {
        return Err(e);
    }
    match result {
        Ok(_) => (),
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(e.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited("auth stream scan"));
        }
        Err(_) => return Err(invalid()),
    }
    if dynamic != set_count
        || actual != root.leaf_count
        || actual
            != (known.len() as u64)
                .checked_add(dynamic)
                .ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    ctx.check()
}

pub(crate) enum ChangedColumn {
    Required(&'static str, MemberPayloadCodecV1),
    Optional(&'static str, MemberPayloadCodecV1),
    Subject,
    Json(&'static str),
    Presenter,
    Effects,
    EffectMember,
}
pub(super) fn validate_changed_group<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: AuthorizationBodyKindV1,
    root: &StateRootV1,
    column: &ChangedColumn,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
) -> Result<(), ValueTreeError> {
    let _scratch = records
        .values
        .memory
        .reserve(64 << 10, records.values.class)?;
    let mut p = Probe {
        root,
        kind,
        records,
        work,
        known: Vec::with_capacity(96),
    };
    match column {
        ChangedColumn::Required(name, codec) => {
            let max = if *codec == MemberPayloadCodecV1::Text {
                p.records.record_bytes
            } else {
                8
            };
            p.scalar(name, *codec, max, &mut std::io::sink())?;
        }
        ChangedColumn::Optional(name, codec) => p.optional(name, *codec)?,
        ChangedColumn::Subject => p.subject()?,
        ChangedColumn::Presenter => {
            let present = p.present("presenter")?;
            let (key, _) = p.records.key(
                kind.descriptor(),
                WorldFieldRecordKindV1::Entry,
                &"presenter",
            )?;
            if tree::lookup(root, &key, p.records, LABELS)?.is_some() != present {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    }
    Ok(())
}
#[path = "incremental_runtime_authorization_stream_classify.rs"]
mod classify;
pub(super) use classify::classify;
pub(super) fn effect_count<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
) -> Result<u64, ValueTreeError> {
    let _scratch = records
        .values
        .memory
        .reserve(64 << 10, records.values.class)?;
    Probe {
        root,
        kind: AuthorizationBodyKindV1::Audit,
        records,
        work,
        known: Vec::with_capacity(96),
    }
    .unsigned(&("committed_effect_receipt_ids", "count"))
}
pub(super) use set::validate as validate_effect_member;
