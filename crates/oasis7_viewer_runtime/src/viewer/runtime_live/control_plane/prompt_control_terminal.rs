use super::*;

impl ViewerRuntimeLiveServer {
    #[expect(
        clippy::too_many_arguments,
        reason = "Prompt-control terminal error recording preserves the signed protocol context"
    )]
    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn record_enhanced_prompt_control_error(
        &mut self,
        verified_player_id: &str,
        verified_nonce: u64,
        player_id: &str,
        request_id: &str,
        operation_digest: String,
        mut error: PromptControlError,
        nonce_already_consumed: bool,
    ) -> Result<PromptControlError, PromptControlError> {
        let operation = error.operation.unwrap_or(PromptControlOperation::Apply);
        let preview = error.preview.unwrap_or(false);
        error.authority_epoch = Some(self.prompt_control_authority.authority_epoch.clone());
        error.operation_digest = Some(operation_digest.clone());
        self.prompt_control_authority
            .result_ledger
            .validate_error(&error)
            .map_err(|ledger_error| prompt_control_ledger_error(ledger_error, request_id))?;
        if !nonce_already_consumed {
            self.llm_sidecar
                .consume_player_auth_nonce(verified_player_id, verified_nonce)
                .map_err(|_message| {
                    prompt_control_enhanced_error(
                        "auth_invalid",
                        "prompt control authentication failed",
                        Some(request_id.to_string()),
                        operation,
                        preview,
                        None,
                        None,
                        PromptControlResultStatus::Blocked,
                    )
                })?;
        }
        if error.value_visibility == Some(PromptControlValueVisibility::Hidden) {
            error.operation_digest = None;
        }
        self.prompt_control_authority
            .result_ledger
            .insert_error(
                self.prompt_control_authority.authority_epoch.as_str(),
                player_id,
                request_id,
                operation_digest,
                error.clone(),
            )
            .map_err(|ledger_error| prompt_control_ledger_error(ledger_error, request_id))?;
        Ok(error)
    }
}
