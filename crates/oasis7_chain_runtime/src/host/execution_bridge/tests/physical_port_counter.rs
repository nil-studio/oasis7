//! Opt-in receiver-side evidence for an isolated process fixture, never a World route.
use std::{fs::{File,OpenOptions},io::Write,path::Path};
pub(super) struct PhysicalPortCounter { writer: Option<File>, rows: usize }
impl PhysicalPortCounter {
    pub(super) fn new(path: Option<&Path>) -> Self {
        Self { writer: path.map(|p|OpenOptions::new().write(true).create_new(true).open(p).expect("create isolated physical port evidence")), rows:0 }
    }
    pub(super) fn record(&mut self, method: &str, path: &str) {
        let Some(writer)=self.writer.as_mut() else {return;};
        assert!(self.rows<4096,"bounded physical receiver evidence exceeded");
        let method=match method {"POST"=>"post","GET"=>"get",_=>"other"};
        let port=match path {
            oasis7_client_api::world_service::SUBMIT_PATH=>"submit",
            oasis7_client_api::world_service::LOOKUP_PATH=>"lookup",
            oasis7_client_api::world_service::VIEW_JSON_PATH=>"view",
            "/v1/world/participant-policy"=>"policy",
            "/v1/world/participant-owner-view"=>"owner_view",
            "/v1/world/participant-starter-eligibility"=>"starter",
            "/v1/world/private-goal/prepare"=>"goal_prepare",
            "/v1/world/private-goal/current"=>"goal_current",
            _=>"other",
        };
        writeln!(writer,"{{\"method\":\"{method}\",\"port\":\"{port}\"}} ").expect("write fixed physical receiver event");
        self.rows+=1;
    }
}
