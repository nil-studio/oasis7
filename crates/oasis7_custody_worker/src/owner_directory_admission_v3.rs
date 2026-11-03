//! Same-transaction adapter for every original Directory SQL operation.
use super::*;
use crate::directory_admission_policy_v3::DirectoryAdmissionPolicyV3;
#[wasm_bindgen(module = "/platform/read_directory_admission_sql_v3.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = ordinaryDirectoryAdmissionV3)]
    fn guarded_sql(
        state: &JsValue,
        execution_capacity: u32,
        read_capacity: u32,
        row_bound: f64,
        validate: &js_sys::Function,
        operation: &js_sys::Function,
    ) -> std::result::Result<JsValue, JsValue>;
}
impl ManagerAssociations {
    pub(super) fn guarded_directory_sql<T>(
        &self,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let policy = DirectoryAdmissionPolicyV3::load(&self.env)?;
        let mut validate = |raw: String| -> std::result::Result<(), JsValue> {
            policy
                .verify(&self.env, &self.object_id, &raw)
                .map(|_| ())
                .map_err(|_| JsValue::from_str("Directory historical inventory rejected"))
        };
        let mut result = None;
        let mut operation = Some(operation);
        let mut execute = || -> std::result::Result<JsValue, JsValue> {
            let next = operation
                .take()
                .ok_or_else(|| JsValue::from_str("Directory operation repeated"))?;
            result = Some(next().map_err(|_| JsValue::from_str("Directory operation rejected"))?);
            Ok(JsValue::UNDEFINED)
        };
        let validation = Closure::borrow_mut(&mut validate);
        let execution = Closure::borrow_mut(&mut execute);
        guarded_sql(
            &self.state,
            policy.execution_capacity,
            policy.read_capacity(),
            policy.row_bound as f64,
            validation.as_ref().unchecked_ref(),
            execution.as_ref().unchecked_ref(),
        )
        .map_err(|_| denied())?;
        drop(execution);
        result.ok_or_else(denied)
    }
}
