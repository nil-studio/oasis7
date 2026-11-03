//! Actual test hook forwarding; diagnostics never replace driver results.
use super::*;

impl ActualHook {
    pub(super) fn new(driver: Arc<Mutex<FormalBftHeaderNodeDriverV3>>) -> Self {
        Self(driver, hook_trace::current())
    }
}

impl NodeExecutionHook for ActualHook {
    fn on_commit(
        &mut self,
        _: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        let call = hook_trace::Call::enter(&self.1, hook_trace::Method::LegacyCommit, None);
        let result = Err("test actual header hook refuses legacy".into());
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn configure_bft_v3_execution(
        &mut self,
        b: &BftSignerBindingV3,
    ) -> Result<Option<NodeBftExecutionHeadV3>, String> {
        let call = hook_trace::Call::enter(
            &self.1,
            hook_trace::Method::Configure,
            Some(b.initial_schedule().height()),
        );
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.configure_bft_v3_execution(b)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn current_bft_v3_execution_context(
        &mut self,
        s: &BftHeaderHeightScheduleV3,
    ) -> Result<NodeBftExecutionContextV3, String> {
        let call = hook_trace::Call::enter(
            &self.1,
            hook_trace::Method::CurrentContext,
            Some(s.height()),
        );
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.current_bft_v3_execution_context(s)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn build_bft_v3_candidate(
        &mut self,
        c: NodeBftBuildContextV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        let call =
            hook_trace::Call::enter(&self.1, hook_trace::Method::Build, Some(c.execution.height));
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.build_bft_v3_candidate(c)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn preview_bft_v3_candidate(
        &mut self,
        c: &NodeBftExecutionContextV3,
        p: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        let call = hook_trace::Call::enter(&self.1, hook_trace::Method::Preview, Some(c.height));
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.preview_bft_v3_candidate(c, p)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn retain_bft_v3_private_candidate(
        &mut self,
        c: &NodeBftExecutionContextV3,
        p: &NodeBftPrivateCandidateV3,
    ) -> Result<String, String> {
        let call = hook_trace::Call::enter(&self.1, hook_trace::Method::Retain, Some(c.height));
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.retain_bft_v3_private_candidate(c, p)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn pending_bft_v3_private_candidate(
        &mut self,
        c: &NodeBftExecutionContextV3,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        let call = hook_trace::Call::enter(&self.1, hook_trace::Method::Pending, Some(c.height));
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.pending_bft_v3_private_candidate(c)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn export_bft_v3_private_candidate(
        &mut self,
        value: &BftHeaderValueV3,
    ) -> Result<oasis7_node::NodeBftPrivateCandidatePackageV3, String> {
        let call = hook_trace::Call::enter(
            &self.1,
            hook_trace::Method::Export,
            Some(value.execution_commitment.height),
        );
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.export_bft_v3_private_candidate(value)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn enable_bft_v3_membership_execution(&mut self) -> Result<(), String> {
        let call = hook_trace::Call::enter(&self.1, hook_trace::Method::EnableMembership, None);
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.enable_bft_v3_membership_execution()
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn lookup_bft_v3_public_value(
        &mut self,
        height: u64,
        header_id: &str,
    ) -> Result<Option<BftHeaderValueV3>, String> {
        let call = hook_trace::Call::enter(&self.1, hook_trace::Method::LookupPublic, Some(height));
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.lookup_bft_v3_public_value(height, header_id)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn retain_received_bft_v3_private_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
        package: &oasis7_node::NodeBftPrivateCandidatePackageV3,
    ) -> Result<String, String> {
        let call = hook_trace::Call::enter(
            &self.1,
            hook_trace::Method::RetainReceived,
            Some(context.height),
        );
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.retain_received_bft_v3_private_candidate(context, package)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn load_bft_v3_private_candidate(
        &mut self,
        v: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        let call = hook_trace::Call::enter(
            &self.1,
            hook_trace::Method::Load,
            Some(v.execution_commitment.height),
        );
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.load_bft_v3_private_candidate(v)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn validate_bft_v3_action_payload(&mut self, p: &[u8]) -> Result<(), String> {
        let call = hook_trace::Call::enter(&self.1, hook_trace::Method::ValidatePayload, None);
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.validate_bft_v3_action_payload(p)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn validate_bft_v3_action_batch(
        &mut self,
        s: &BftHeaderHeightScheduleV3,
        a: &[oasis7_node::NodeConsensusAction],
    ) -> Result<(), String> {
        let call =
            hook_trace::Call::enter(&self.1, hook_trace::Method::ValidateBatch, Some(s.height()));
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.validate_bft_v3_action_batch(s, a)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn load_bft_v3_commit_history_page(
        &mut self,
        b: &BftSignerBindingV3,
        a: u64,
        m: usize,
    ) -> Result<Vec<FormalBftCertifiedHeaderV3>, String> {
        let call = hook_trace::Call::enter(&self.1, hook_trace::Method::HistoryPage, Some(a));
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.load_bft_v3_commit_history_page(b, a, m)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
    fn commit_bft_v3_certificate(
        &mut self,
        t: VerifiedBftHeaderCertificateV3,
    ) -> Result<NodeBftExecutionCommitResultV3, String> {
        let call = hook_trace::Call::enter(
            &self.1,
            hook_trace::Method::CommitCertificate,
            Some(t.schedule().height()),
        );
        let result = {
            let mut driver = self.0.lock().unwrap();
            hook_trace::Call::acquired(&call);
            let _context = hook_trace::CallContext::bind(&call);
            driver.commit_bft_v3_certificate(t)
        };
        hook_trace::Call::returned(&call, result.is_ok());
        result
    }
}
