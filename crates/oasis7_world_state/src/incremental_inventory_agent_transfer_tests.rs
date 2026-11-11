use super::*;
use crate::{
    AgentCell,
    agent_state::AgentState,
    incremental_agent_body::{AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_fields::{
        AgentCellFieldVisitor, AgentFieldV1, AgentScalarRefV1, visit_agent_cell,
    },
    incremental_event_fields::{MailboxFrontierV1, MailboxKeyV1},
    incremental_field_member::MemberRecordLimitsV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory::{
        InventoryFamilyWorkV1, InventoryMaterialScopeWorkV1,
        build_parent_inventory_material_scope_v1, open_current_inventory_material_scope_v1,
        qualify_inventory_family_v1,
    },
    incremental_inventory_genesis::{InventoryGenesisContext, InventoryWriteSink},
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer, io},
};
use oasis7_agent_api::world_action::{GeoPos, MaterialLedgerId};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
    },
    incremental_state::{StateDeltaV1, StateNamespaceV1, StateScanBudget, StateWriteV1},
    incremental_value::{ValueNodeWriter, ValueReadContext, ValueTreeError, write_value},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

fn no_error() -> Option<ValueTreeError> {
    None
}

fn not_cancelled() -> bool {
    false
}

fn io_budget() -> oasis7_formal_protocol::incremental_value::ValueIoBudget {
    io()
}

struct AgentRows<'a> {
    agent: &'a str,
    pages: &'a Pages,
    memory: &'a MemoryBudget,
    output: &'a mut Vec<(StateNamespaceV1, Vec<u8>, [u8; 32])>,
}
impl AgentRows<'_> {
    fn value(&mut self, bytes: &[u8]) -> Result<[u8; 32], ValueTreeError> {
        let mut writer = Writer(self.pages);
        let mut source = bytes;
        write_value(
            &mut source,
            bytes.len() as u64,
            &mut writer,
            self.memory,
            &mut io_budget(),
            not_cancelled,
        )?
        .digest()
    }
    fn emit(
        &mut self,
        namespace: StateNamespaceV1,
        key: &[u8],
        bytes: &[u8],
    ) -> Result<(), ValueTreeError> {
        if self.output.len() >= 512 {
            return Err(ValueTreeError::ResourceLimited(
                "agent transfer fixture rows",
            ));
        }
        let value = self.value(bytes)?;
        self.output.push((namespace, key.to_vec(), value));
        Ok(())
    }
}
impl AgentCellFieldVisitor for AgentRows<'_> {
    type Error = ValueTreeError;

    fn scalar(
        &mut self,
        field: AgentFieldV1,
        ordinal: u64,
        value: AgentScalarRefV1<'_>,
    ) -> Result<(), Self::Error> {
        let length = value.encoded_len().map_err(ValueTreeError::Invalid)? as usize;
        let _fee = self.memory.reserve(length, MemoryClass::Ordinary)?;
        let mut bytes = Vec::with_capacity(length);
        value.write_to(&mut bytes)?;
        let key = crate::incremental_agent_fields::AgentFieldKeyV1::new(self.agent, field, ordinal)
            .map_err(ValueTreeError::Invalid)?;
        self.emit(field.namespace(), key.as_bytes(), &bytes)
    }

    fn body(&mut self, value: &AgentBodyValueV1) -> Result<(), Self::Error> {
        let key = AgentBodyKeyV1::new(self.agent).map_err(ValueTreeError::Invalid)?;
        let bytes = value.canonical_bytes();
        self.emit(StateNamespaceV1::Business, key.as_bytes(), &bytes)
    }

    fn mailbox_frontier(&mut self, count: u64) -> Result<(), Self::Error> {
        let key = MailboxKeyV1::new(self.agent, None).map_err(ValueTreeError::Invalid)?;
        let bytes = MailboxFrontierV1 {
            head: 0,
            tail: count,
        }
        .canonical_bytes()
        .map_err(ValueTreeError::Invalid)?;
        self.emit(StateNamespaceV1::Scheduler, key.as_bytes(), &bytes)
    }

    fn mailbox(&mut self, _: u64, _: &crate::DomainEvent) -> Result<(), Self::Error> {
        Err(ValueTreeError::Invalid("unexpected fixture mailbox event"))
    }
}

struct InventoryRows(Vec<(StateNamespaceV1, StateWriteV1)>);
impl InventoryWriteSink for InventoryRows {
    fn put_write(&mut self, write: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError> {
        let namespace = crate::incremental_world_field_key::WorldFieldKeyV1::from_bytes(&write.key)
            .map_err(ValueTreeError::Invalid)?
            .namespace;
        self.0.push((namespace, (*write).clone()));
        Ok(())
    }
}

struct Fixture {
    memory: MemoryBudget,
    pages: Pages,
    roots: RootSetV1,
    agents: ReservedValue<AgentSubtreeStructureQualificationV1>,
    inventory_family: ReservedValue<crate::incremental_inventory::InventoryFamilyQualificationV1>,
    inventory_parent: ReservedValue<crate::incremental_inventory::ParentInventoryMaterialScopeV1>,
    _input: oasis7_proto::memory_budget::MemoryReservation,
}

fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        leaves_remaining: 100_000,
        read_bytes_remaining: 128 << 20,
    }
}

fn fixture() -> Fixture {
    fixture_with_recipient_balance(None)
}

fn fixture_with_recipient_balance(recipient_balance: Option<i64>) -> Fixture {
    let memory = MemoryBudget::new(512 << 20, 16 << 20).unwrap();
    let input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::new(&memory);
    let mut all = Vec::<(StateNamespaceV1, Vec<u8>, [u8; 32])>::new();

    let mut world = crate::WorldState::default();
    world.material_ledgers.clear();
    world.material_ledgers.insert(
        MaterialLedgerId::Agent("sender".into()),
        [("ore".into(), 5)].into(),
    );
    if let Some(balance) = recipient_balance {
        world.material_ledgers.insert(
            MaterialLedgerId::Agent("receiver".into()),
            [("ore".into(), balance)].into(),
        );
    }
    world.materials.clear();
    let mut inventory_rows = InventoryRows(Vec::new());
    let mut inventory_writer = Writer(&pages);
    crate::incremental_inventory_genesis::prepare_inventory_genesis_v1(
        &world,
        &mut InventoryGenesisContext {
            values: &mut inventory_writer,
            writes: &mut inventory_rows,
            memory: &memory,
            io: &mut io_budget(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            records_remaining: 10_000,
            cancelled: not_cancelled,
        },
    )
    .unwrap();
    for (namespace, write) in inventory_rows.0 {
        all.push((namespace, write.key, write.after.unwrap()));
    }

    for agent in ["sender", "receiver"] {
        let cell = AgentCell::new(AgentState::new(agent, GeoPos::new(0, 0, 0)), 0);
        let mut writer = AgentRows {
            agent,
            pages: &pages,
            memory: &memory,
            output: &mut all,
        };
        visit_agent_cell(agent, &cell, &mut writer).unwrap();
    }

    let mut roots = RootSetV1::empty("agent-ledger-transfer".into(), [0x3a; 32]).unwrap();
    for namespace in [
        StateNamespaceV1::Business,
        StateNamespaceV1::Scheduler,
        StateNamespaceV1::Authorization,
    ] {
        let mut writes: Vec<StateWriteV1> = all
            .iter()
            .filter(|row| row.0 == namespace)
            .map(|(_, key, value)| StateWriteV1 {
                key: key.clone(),
                before: None,
                after: Some(*value),
            })
            .collect();
        writes.sort_by(|left, right| left.key.cmp(&right.key));
        let charge = memory
            .reserve(
                writes.len() * (512 << 10) + (128 << 10),
                MemoryClass::Ordinary,
            )
            .unwrap();
        let delta = charge.retain(
            StateDeltaV1::build(
                roots.state_root(namespace).clone(),
                writes,
                &pages,
                16 << 20,
            )
            .unwrap(),
        );
        for node in &delta.new_nodes {
            let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
            Writer(&pages)
                .put_node(
                    id,
                    memory
                        .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        *roots
            .state
            .iter_mut()
            .find(|root| root.namespace == namespace)
            .unwrap() = delta.after.clone();
    }

    let agents = crate::incremental_agent_subtree::qualify_agent_subtree_structure_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io_budget(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: no_error,
            cancelled: not_cancelled,
        },
        &mut scan(),
        &mut crate::incremental_agent_subtree::AgentSubtreeScanWorkV1 {
            max_agents: 4,
            cells_remaining: 100_000,
            max_scalar_bytes: 1 << 20,
        },
        &mut |_| Err(ValueTreeError::Invalid("unexpected agent original")),
    )
    .unwrap();

    let mut family_work = InventoryFamilyWorkV1 {
        records_remaining: 100_000,
        index_steps_remaining: 1_000_000,
        max_semantic_bytes: 4096,
    };
    let inventory_family = qualify_inventory_family_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io_budget(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: no_error,
            cancelled: not_cancelled,
        },
        &mut scan(),
        &mut family_work,
    )
    .unwrap();
    let mut scope_work = InventoryMaterialScopeWorkV1 {
        max_entries_per_map: 32,
        max_material_bytes: 256,
        pending_entries_remaining: 100_000,
        comparison_bytes_remaining: 64 << 20,
    };
    let inventory_parent = build_parent_inventory_material_scope_v1(
        &inventory_family,
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io_budget(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: no_error,
            cancelled: not_cancelled,
        },
        &mut scan(),
        &mut family_work,
        &mut scope_work,
    )
    .unwrap();
    Fixture {
        memory,
        pages,
        roots,
        agents,
        inventory_family,
        inventory_parent,
        _input: input,
    }
}

fn transaction(fixture: &Fixture) -> ExecutionTransaction<'_, Pages> {
    ExecutionTransaction::begin(
        ExecutionParent {
            roots: fixture.roots.clone(),
            commit: [0x21; 32],
            authority: AuthorityBinding {
                token: [0x32; 32],
                epoch: 7,
            },
        },
        [0x43; 32],
        &fixture.pages,
        &fixture.memory,
        TransactionLimits {
            max_reads: 4096,
            max_writes: 128,
            max_facts: 8,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap()
}

fn transfer(
    fixture: &Fixture,
    amount: i64,
) -> (
    ExecutionTransaction<'_, Pages>,
    AgentLedgerTransferOutcomeV1,
) {
    let mut tx = transaction(fixture);
    let live = tx.parent_fence();
    let outcome = {
        let mut family_work = InventoryFamilyWorkV1 {
            records_remaining: 100_000,
            index_steps_remaining: 1_000_000,
            max_semantic_bytes: 4096,
        };
        let mut scope_work = InventoryMaterialScopeWorkV1 {
            max_entries_per_map: 32,
            max_material_bytes: 256,
            pending_entries_remaining: 100_000,
            comparison_bytes_remaining: 64 << 20,
        };
        let mut current_io = io_budget();
        let mut current_keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut current = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &fixture.pages,
                memory: &fixture.memory,
                class: MemoryClass::Ordinary,
                io: &mut current_io,
            },
            key_work: &mut current_keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: no_error,
            cancelled: not_cancelled,
        };
        let inventory_guard = current.transaction.guard();
        let inventory = open_current_inventory_material_scope_v1(
            &fixture.inventory_parent,
            &fixture.roots,
            &fixture.pages,
            &mut current,
            &mut family_work,
            &mut scope_work,
            &inventory_guard,
            &live,
        )
        .unwrap();

        let mut parent_read = WorldFieldReadContextV1 {
            root: fixture.roots.state_root(StateNamespaceV1::Business),
            state: &fixture.pages,
            values: ValueReadContext {
                source: &fixture.pages,
                memory: &fixture.memory,
                class: MemoryClass::Ordinary,
                io: &mut io_budget(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: no_error,
            cancelled: not_cancelled,
        };
        let mut writer = Writer(&fixture.pages);
        let mut transfer_io = io_budget();
        let mut transfer_keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &fixture.memory,
            io: &mut transfer_io,
            key_work: &mut transfer_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 8,
                record_bytes: 2 << 20,
            },
            cancelled: not_cancelled,
        };
        let mut work = AgentLedgerTransferWorkV1 {
            steps_remaining: 10_000,
            bytes_remaining: 16 << 20,
            pending_entries_remaining: 100_000,
        };
        let guard = current.transaction.guard();
        transfer_current_agent_material_overlay_v1(
            "sender",
            &MaterialLedgerId::Agent("sender".into()),
            &MaterialLedgerId::Agent("receiver".into()),
            "ore",
            amount,
            &inventory,
            &fixture.agents,
            &fixture.roots,
            &mut current,
            &mut parent_read,
            &mut write,
            &mut work,
            &guard,
            &live,
        )
        .unwrap()
    };
    (tx, outcome)
}

#[test]
fn current_agent_transfer_prepares_both_balances_and_creates_only_existing_recipient_group() {
    let fixture = fixture();
    let (mut tx, outcome) = transfer(&fixture, 2);
    assert_eq!(outcome, AgentLedgerTransferOutcomeV1::Applied);

    let mut read_io = io_budget();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut balance_reader =
        crate::incremental_inventory_overlay_read::InventoryOverlayReadContext {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &fixture.pages,
                memory: &fixture.memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: no_error,
            cancelled: not_cancelled,
        };
    assert_eq!(
        balance_reader
            .ledger(&MaterialLedgerId::Agent("sender".into()), "ore")
            .unwrap(),
        crate::incremental_inventory_read::LedgerQuantityV1::Value(3)
    );
    assert_eq!(
        balance_reader
            .ledger(&MaterialLedgerId::Agent("receiver".into()), "ore")
            .unwrap(),
        crate::incremental_inventory_read::LedgerQuantityV1::Value(2)
    );
    assert!(fixture.inventory_family.uses_budget(&fixture.memory));
}

#[test]
fn current_agent_transfer_reports_destination_overflow_without_applying_either_balance() {
    let fixture = fixture_with_recipient_balance(Some(i64::MAX));
    let (mut tx, outcome) = transfer(&fixture, 1);
    assert_eq!(
        outcome,
        AgentLedgerTransferOutcomeV1::DestinationOverflow { current: i64::MAX }
    );

    let mut read_io = io_budget();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut balance_reader =
        crate::incremental_inventory_overlay_read::InventoryOverlayReadContext {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &fixture.pages,
                memory: &fixture.memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: no_error,
            cancelled: not_cancelled,
        };
    assert_eq!(
        balance_reader
            .ledger(&MaterialLedgerId::Agent("sender".into()), "ore")
            .unwrap(),
        crate::incremental_inventory_read::LedgerQuantityV1::Value(5)
    );
    assert_eq!(
        balance_reader
            .ledger(&MaterialLedgerId::Agent("receiver".into()), "ore")
            .unwrap(),
        crate::incremental_inventory_read::LedgerQuantityV1::Value(i64::MAX)
    );
}
