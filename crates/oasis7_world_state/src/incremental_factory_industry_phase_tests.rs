//! Real GEN -> Recovery qualifier -> independently sealed ordinary no-op.
use crate::{
    WorldState,
    incremental_factory_records::*,
    incremental_industry_jobs::*,
    incremental_typed_record_test_support::{Pages, Writer, io},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
    },
    incremental_state::{MerkleNodeReader, StateDeltaV1, StateScanBudget, StateWriteV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::sync::Arc;
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1000000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 10000,
    }
}
fn fw() -> FactoryWorkBudgetV1 {
    FactoryWorkBudgetV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 8 << 20,
        scan: scan(),
    }
}
fn iw() -> IndustryWorkBudgetV1 {
    IndustryWorkBudgetV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 8 << 20,
        scan: scan(),
    }
}
struct Recovery<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
}
struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl ValueNodeReader for Recovery<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Recovery || !memory.same_pool(self.memory) {
            return Err(ValueTreeError::Invalid("phase recovery source"));
        }
        let bytes = ValueNodeReader::read_node(self.pages, id, max, memory, MemoryClass::Ordinary)?;
        let charge = memory.reserve(bytes.len() + 128, class)?;
        Ok(Arc::new(charge.retain(bytes.to_vec())))
    }
}
impl MerkleNodeReader for Recovery<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        ValueNodeReader::read_node(self, &id, max, self.memory, MemoryClass::Recovery)
            .map(|n| Some(Node(n)))
            .map_err(|e| e.to_string())
    }
}
fn roots(memory: &MemoryBudget, pages: &Pages) -> RootSetV1 {
    let mut rows = Vec::new();
    let mut writer = Writer(pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = MemberRecordWriteContext {
        sink: &mut writer,
        memory,
        io: &mut quota,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let mut records = 10000;
    let source = WorldState::default();
    prepare_factory_records_genesis_v1(&source, &mut ctx, &mut records, &mut fw(), &mut |row| {
        rows.push(row);
        Ok(())
    })
    .unwrap();
    prepare_industry_jobs_genesis_v1(&source, &mut ctx, &mut records, &mut iw(), &mut |row| {
        rows.push(row);
        Ok(())
    })
    .unwrap();
    let mut roots = RootSetV1::empty("factory-industry-phase".into(), [9; 32]).unwrap();
    for root in &mut roots.state {
        let mut writes: Vec<StateWriteV1> = rows
            .iter()
            .filter(|r| r.namespace == root.namespace)
            .map(|r| r.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        if writes.is_empty() {
            continue;
        }
        let delta = StateDeltaV1::build(root.clone(), writes, pages, 8 << 20).unwrap();
        for node in &delta.new_nodes {
            Writer(pages)
                .put_node(
                    *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                    memory
                        .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        *root = delta.after;
    }
    roots
}
macro_rules! phase_case{($name:ident,$qualify:ident,$context:ident,$verify:ident,$receiver:ident,$work:ident,$readmit:ident)=>{
 #[test]fn $name(){let memory=MemoryBudget::new(128<<20,16<<20).unwrap();let _source=memory.reserve(8<<20,MemoryClass::Ordinary).unwrap();let pages=Pages::with_limit(&memory,16384);let roots=roots(&memory,&pages);let recovery=Recovery{pages:&pages,memory:&memory};let mut quota=io();let mut keys=SemanticKeyBudget{bytes_remaining:8<<20};let mut outer=scan();
 let proof=$qualify(&roots,&mut $context{state:&recovery,values:ValueReadContext{source:&recovery,memory:&memory,class:MemoryClass::Recovery,io:&mut quota},key_work:&mut keys,semantic_bytes:4096,record_bytes:2<<20,max_payload_bytes:1<<20,outer_scan:&mut outer,take_state_error:||None,cancelled:||false},&mut $work()).unwrap();
 assert_eq!(proof.allocation_class(),MemoryClass::Recovery);
 let parent=ExecutionParent{roots:roots.clone(),commit:[1;32],authority:AuthorityBinding{token:[2;32],epoch:3}};let live=parent.fence().unwrap();let tx=ExecutionTransaction::begin(parent.clone(),[3;32],&pages,&memory,TransactionLimits{max_reads:8,max_writes:8,max_facts:8,max_pending_node_bytes:8<<20}).unwrap();let guard=tx.guard();let prepared=tx.seal(&guard,&live,1).unwrap();
 let result=$verify(&parent,[3;32],&prepared.record,&proof,&mut $receiver{values:ValueReadContext{source:&pages,memory:&memory,class:MemoryClass::Ordinary,io:&mut quota},key_work:&mut keys,semantic_bytes:4096,record_bytes:2<<20,max_payload_bytes:1<<20,cancelled:||false},&mut $work());
 assert!(result.is_err(),"Recovery-qualified family must not authorize an Ordinary no-op receiver without explicit readmission");
 let promoted=$readmit(&proof,&roots,&memory,||false).unwrap();
 assert_eq!(promoted.allocation_class(),MemoryClass::Ordinary);
 assert_eq!(proof.record_counts(),promoted.record_counts());
 let result=$verify(&parent,[3;32],&prepared.record,&promoted,&mut $receiver{values:ValueReadContext{source:&pages,memory:&memory,class:MemoryClass::Ordinary,io:&mut quota},key_work:&mut keys,semantic_bytes:4096,record_bytes:2<<20,max_payload_bytes:1<<20,cancelled:||false},&mut $work());
 result.unwrap();
 assert!($readmit(&promoted,&roots,&memory,||false).is_err());
 let mut wrong=roots.clone();wrong.simulation_tick=wrong.simulation_tick.wrapping_add(1);
 assert!($readmit(&proof,&wrong,&memory,||false).is_err());
 let foreign=MemoryBudget::new(128<<20,16<<20).unwrap();
 assert!($readmit(&proof,&roots,&foreign,||false).is_err());
 assert!(matches!($readmit(&proof,&roots,&memory,||true),Err(ValueTreeError::Cancelled)));
 let used=memory.used();
 let exhausted=memory.reserve((112<<20)-used,MemoryClass::Ordinary).unwrap();
 assert!(matches!($readmit(&proof,&roots,&memory,||false),Err(ValueTreeError::Memory(_))));
 drop(exhausted);
 let mut polls=0;
 assert!(matches!($readmit(&proof,&roots,&memory,||{polls+=1;polls>1}),Err(ValueTreeError::Cancelled)));

 }
}}
phase_case!(
    factory_recovery_token_cannot_enter_ordinary_noop,
    qualify_factory_family_v1,
    FactoryFamilyReadContextV1,
    verify_factory_transition_v1,
    FactoryTransitionReadContextV1,
    fw,
    readmit_factory_family_ordinary_v1
);
phase_case!(
    industry_recovery_token_cannot_enter_ordinary_noop,
    qualify_industry_family_v1,
    IndustryFamilyReadContextV1,
    verify_industry_transition_v1,
    IndustryTransitionReadContextV1,
    iw,
    readmit_industry_family_ordinary_v1
);
