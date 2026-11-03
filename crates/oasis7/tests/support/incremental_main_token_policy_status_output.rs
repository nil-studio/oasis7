use super::*;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::{
    cell::Cell,
    io::{self, Write},
};
struct Output<'a> {
    cancel: &'a Cell<bool>,
    attempts: &'a Cell<usize>,
    mode: usize,
    accepted: Vec<u8>,
}
impl Write for Output<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.attempts.get() + 1;
        self.attempts.set(n);
        if self.mode < 2 {
            if n > 1 {
                return Err(io::Error::other("output retried after cancellation"));
            }
            self.cancel.set(true);
            if self.mode == 0 {
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.accepted.push(bytes[0]);
            return Ok(1);
        }
        if n <= 2 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.accepted.push(bytes[0]);
        Ok(1)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn status_output_checks_cancel_before_retry_and_poisons_prior_success() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(512 << 10, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("status-output"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = seed(&store, &memory, &source());
    let key = MainTokenPolicyKeyV1::Grant(" raw grant ");
    let base = {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = ctx!(
            roots
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .unwrap(),
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let child = inspect_main_token_policy_record_root_v1(key, &mut ctx)
            .unwrap()
            .unwrap();
        let mut semantic = b"oasis7.iws.semantic-key.v1".to_vec();
        semantic.push(0);
        semantic.push(1);
        semantic.extend_from_slice(&11u64.to_be_bytes());
        semantic.extend_from_slice(b" raw grant ");
        ctx.root = &child;
        qualify_main_token_policy_body_v1(
            MainTokenPolicyKindV1::Grant,
            *blake3::hash(&semantic).as_bytes(),
            &mut ctx,
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap()
    };
    for mode in 0..3 {
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut tx = ExecutionTransaction::begin(
            parent,
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 32,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let stop = Cell::new(false);
        let attempts = Cell::new(0);
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || stop.get(),
        };
        let mut backend = store.value_session(physical());
        let mut wio = logical();
        let mut wkeys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut backend,
            memory: &memory,
            io: &mut wio,
            key_work: &mut wkeys,
            limits: limits(),
            cancelled: || false,
        };
        let guard = overlay.transaction.guard();
        let proof = change_qualified_main_token_policy_cell_v1(
            MainTokenPolicyCellChangeV1 {
                key,
                cell: MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::StatusReason),
                qualification: &base,
                expected: MainTokenPolicyValueRefV1::Text(""),
                after: MainTokenPolicyValueRefV1::Text("prefix"),
                max_current_payload_bytes: 165,
                max_pending_node_bytes: 2 << 20,
            },
            &mut overlay,
            &mut write,
            &guard,
            &live,
        )
        .unwrap()
        .unwrap();
        let guard = overlay.transaction.guard();
        let mut output = Output {
            cancel: &stop,
            attempts: &attempts,
            mode,
            accepted: Vec::new(),
        };
        let result = read_qualified_main_token_policy_cell_overlay_v1(
            MainTokenPolicyCellRequestV1 {
                key,
                cell: MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::Status),
                qualification: &proof,
            },
            &mut overlay,
            165,
            &mut output,
            &guard,
            &live,
        );
        if mode < 2 {
            let guard = overlay.transaction.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
            assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                ),
                "mode{mode}, attempts={}: {result:?}",
                attempts.get()
            );
            assert_eq!(attempts.get(), 1);
            assert_eq!(
                output.accepted.as_slice(),
                if mode == 0 { &b""[..] } else { &b"r"[..] }
            );
        } else {
            assert_eq!(
                result.unwrap(),
                MainTokenPolicyValueV1::Text { utf8_bytes: 7 }
            );
            assert_eq!(output.accepted, b"revoked");
            assert_eq!(attempts.get(), 9);
            let guard = overlay.transaction.guard();
            let sealed = tx.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.writes.len(), 1);
        }
    }
}
