//! One explicit composition; original child operations remain the effect authority.
use super::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StartWorkRecord {
    pub schema_version: u32,
    pub intent_id: String,
    pub project_id: String,
    pub environment: String,
    pub basis_revision: String,
    pub fingerprint: String,
    pub state: String,
    pub thread_id: Option<String>,
    pub operation_ids: Vec<String>,
    pub updated_at: String,
}
pub(super) fn settled_intent(state: &str) -> bool {
    [
        "inputAcknowledged",
        "inputRejected",
        "sourceResolved",
        "inputNotSentConfirmed",
    ]
    .contains(&state)
}
impl Runtime {
    pub fn start_work(
        &mut self,
        project: &Project,
        intent_id: &str,
        config: SessionConfig,
        input: &str,
    ) -> Result<RuntimeSnapshot> {
        if intent_id.is_empty()
            || intent_id.len() > 128
            || !intent_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(Error::Invalid("invalid start intent identity".into()));
        }
        Self::input(input)?;
        self.gate(project, true)?;
        if config.basis_revision != crate::index::project(Path::new(&project.path))?.head {
            return Err(Error::Invalid("工作基线已变化，请重新核对发送预览".into()));
        }
        let scope = key(project);
        if self
            .store
            .all_start_intents(&scope)?
            .iter()
            .any(|r| !settled_intent(&r.state))
        {
            return Err(Error::Invalid(
                "原发起意图仍未核实；请核对原会话和子操作，不得使用新身份重发".into(),
            ));
        }
        let before = self.snapshot(project)?;
        let mut record = StartWorkRecord {
            schema_version: 1,
            intent_id: intent_id.into(),
            project_id: project.id.clone(),
            environment: project.environment.clone(),
            basis_revision: config.basis_revision.clone(),
            fingerprint: crate::index::digest(&serde_json::to_vec(&(&config, input))?),
            state: "pending".into(),
            thread_id: None,
            operation_ids: vec![],
            updated_at: crate::now(),
        };
        // Insert-only: restart, repeated clicks and lost receipts cannot replay this intent.
        self.store.reserve_start_intent(&scope, &record)?;
        let created = self.create_session(project, config);
        let after = self.snapshot(project)?;
        record.operation_ids = after
            .operations
            .iter()
            .filter(|op| !before.operations.iter().any(|old| old.id == op.id))
            .map(|op| op.id.clone())
            .collect();
        let new_threads: Vec<_> = after
            .sessions
            .iter()
            .filter(|s| {
                !before
                    .sessions
                    .iter()
                    .any(|old| old.thread_id == s.thread_id)
                    && s.owned
                    && Some(&s.instance_id) == after.instance_id.as_ref()
            })
            .collect();
        if created.is_err() || new_threads.len() != 1 {
            record.state = "unknown".into();
            self.store.save_start_intent(&scope, &record)?;
            return self.public_snapshot(project);
        }
        let thread = new_threads[0].thread_id.clone();
        record.thread_id = Some(thread.clone());
        record.state = "threadCreatedInputNotSent".into();
        self.store.save_start_intent(&scope, &record)?;
        record.state = "inputPending".into();
        self.store.save_start_intent(&scope, &record)?;
        let sent = self.start_turn(project, &thread, input);
        let final_snapshot = self.snapshot(project)?;
        record.operation_ids = final_snapshot
            .operations
            .iter()
            .filter(|op| !before.operations.iter().any(|old| old.id == op.id))
            .map(|op| op.id.clone())
            .collect();
        let input_operations: Vec<_> = final_snapshot
            .operations
            .iter()
            .filter(|op| op.method == "turn/start" && record.operation_ids.contains(&op.id))
            .collect();
        record.state = if input_operations
            .iter()
            .any(|op| ["pending", "awaiting", "unknown"].contains(&op.state.as_str()))
        {
            "unknown"
        } else if sent.is_ok()
            && input_operations
                .iter()
                .any(|op| ["acknowledged", "confirmed"].contains(&op.state.as_str()))
        {
            "inputAcknowledged"
        } else if input_operations.is_empty() {
            "threadCreatedInputNotSent"
        } else {
            "inputRejected"
        }
        .into();
        record.updated_at = crate::now();
        self.store.save_start_intent(&scope, &record)?;
        self.public_snapshot(project)
    }
    pub fn start_work_records(&self, project: &Project) -> Result<Vec<StartWorkRecord>> {
        self.store.start_intents(&key(project))
    }
    pub fn public_start_work_records(&mut self, project: &Project) -> Result<Vec<StartWorkRecord>> {
        let public = self.public_snapshot(project)?;
        Ok(self
            .start_work_records(project)?
            .into_iter()
            .filter(|r| {
                r.thread_id
                    .as_ref()
                    .is_none_or(|id| public.sessions.iter().any(|s| &s.thread_id == id))
            })
            .collect())
    }
}
